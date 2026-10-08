use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Parser;
use regex::{Regex, RegexBuilder};

const COLORS: [&str; 12] = [
    "31", "32", "33", "34", "35", "36", "91", "92", "93", "94", "95", "96",
];
const MIN_WIDTH: usize = 8;
const PS_FORMAT: &str = "{{.ID}}\t{{.Names}}\t{{.Label \"com.docker.compose.service\"}}\t{{.Label \"com.docker.compose.container-number\"}}";

/// Tails logs from all running Docker containers matching any pattern, like
/// kubetail. Patterns are regexes matched against the compose service name
/// (e.g. `kyberlifeorder`), or the container name for non-compose containers
/// or with --full-names. No pattern = every running container. Containers
/// that start, restart, or are recreated later are picked up automatically.
#[derive(Parser)]
#[command(
    about = "Tails logs from all running Docker containers matching any pattern, like kubetail",
    long_about = None
)]
struct Args {
    /// Regexes to match containers against (any match is tailed)
    patterns: Vec<String>,

    /// Show logs since a time or duration (e.g. 10m, 1h, 2026-10-08T09:00); overrides --tail
    #[arg(short, long)]
    since: Option<String>,

    /// Lines of history to show per container at startup
    #[arg(short = 'n', long, default_value_t = 10)]
    tail: u64,

    /// Show Docker timestamps
    #[arg(short, long)]
    timestamps: bool,

    /// Skip containers matching this regex (repeatable)
    #[arg(short = 'x', long)]
    exclude: Vec<String>,

    /// Only print lines matching this regex (case-insensitive)
    #[arg(short, long)]
    grep: Option<String>,

    /// Don't watch for new, restarted, or recreated containers; exit when all logs end
    #[arg(short = '1', long)]
    no_watch: bool,

    /// Use full container names (for prefix and matching), not compose service names
    #[arg(short, long)]
    full_names: bool,

    /// Seconds between checks for new containers
    #[arg(long, default_value_t = 2)]
    interval: u64,
}

#[derive(Debug, PartialEq)]
struct Container {
    id: String,
    name: String,
    label: String,
}

/// Parses one line of `docker ps --format PS_FORMAT`. Compose containers are
/// labelled by service name, with the replica number appended past the first.
fn parse_ps_line(line: &str, full_names: bool) -> Option<Container> {
    let mut fields = line.split('\t');
    let id = fields.next().filter(|s| !s.is_empty())?.to_string();
    let name = fields.next().filter(|s| !s.is_empty())?.to_string();
    let service = fields.next().unwrap_or("");
    let number = fields.next().unwrap_or("");

    let label = if full_names || service.is_empty() {
        name.clone()
    } else if number.is_empty() || number == "1" {
        service.to_string()
    } else {
        format!("{service}-{number}")
    };
    Some(Container { id, name, label })
}

struct Filter {
    include: Vec<Regex>,
    exclude: Vec<Regex>,
}

impl Filter {
    fn new(include: &[String], exclude: &[String]) -> Result<Self, regex::Error> {
        let compile = |ps: &[String]| ps.iter().map(|p| Regex::new(p)).collect::<Result<_, _>>();
        Ok(Self {
            include: compile(include)?,
            exclude: compile(exclude)?,
        })
    }

    fn matches(&self, label: &str) -> bool {
        (self.include.is_empty() || self.include.iter().any(|r| r.is_match(label)))
            && !self.exclude.iter().any(|r| r.is_match(label))
    }
}

/// Lists running containers via the docker CLI, so the active Docker context
/// (Docker Desktop, colima, ...) is respected.
fn list_containers(full_names: bool) -> Result<Vec<Container>, String> {
    let out = Command::new("docker")
        .args(["ps", "--format", PS_FORMAT])
        .output()
        .map_err(|e| format!("failed to run docker: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| parse_ps_line(l, full_names))
        .collect())
}

/// How much history to request when starting to tail a container.
#[derive(Clone)]
enum History {
    Tail(u64),
    Since(String),
    All,
}

impl History {
    fn args(&self) -> Vec<String> {
        match self {
            History::Tail(n) => vec!["--tail".into(), n.to_string()],
            History::Since(s) => vec!["--since".into(), s.clone()],
            History::All => vec![],
        }
    }
}

fn unix_timestamp(t: SystemTime) -> String {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{}.{:09}", d.as_secs(), d.subsec_nanos())
}

struct Tail {
    child: Child,
    readers: Vec<JoinHandle<()>>,
}

type Tails = Arc<Mutex<HashMap<String, Tail>>>;

fn kill_all(tails: &Tails) {
    if let Ok(mut tails) = tails.lock() {
        for tail in tails.values_mut() {
            let _ = tail.child.kill();
            let _ = tail.child.wait();
        }
    }
}

struct Printer {
    color: bool,
    width: AtomicUsize,
    grep: Option<Regex>,
}

impl Printer {
    fn prefix(&self, label: &str, color: &str) -> String {
        let width = self.width.load(Ordering::Relaxed);
        if self.color {
            format!("\x1b[{color}m{label:<width$}\x1b[0m │ ")
        } else {
            format!("{label:<width$} │ ")
        }
    }

    fn status(&self, msg: &str) {
        if self.color {
            eprintln!("\x1b[2m{msg}\x1b[0m");
        } else {
            eprintln!("{msg}");
        }
    }

    fn wants(&self, line: &[u8]) -> bool {
        self.grep
            .as_ref()
            .is_none_or(|re| re.is_match(&String::from_utf8_lossy(line)))
    }
}

/// Copies `stream` to stdout line by line, each line prefixed. A whole line is
/// written under one stdout lock so lines from different containers never interleave.
fn spawn_reader(
    stream: impl Read + Send + 'static,
    label: String,
    color: &'static str,
    printer: Arc<Printer>,
    tails: Tails,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            if !printer.wants(&line) {
                continue;
            }
            if !line.ends_with(b"\n") {
                line.push(b'\n');
            }
            let prefix = printer.prefix(&label, color);
            let mut out = io::stdout().lock();
            let written = out
                .write_all(prefix.as_bytes())
                .and_then(|_| out.write_all(&line))
                .and_then(|_| out.flush());
            if written.is_err() {
                // stdout closed (e.g. `dtail | head`): stop everything.
                drop(out);
                kill_all(&tails);
                std::process::exit(0);
            }
        }
    })
}

struct Tailer {
    args: Args,
    filter: Filter,
    printer: Arc<Printer>,
    tails: Tails,
    /// When each container's previous tail ended, so a restarted container
    /// (same ID) resumes from there instead of replaying its history.
    ended: HashMap<String, SystemTime>,
    colors: HashMap<String, &'static str>,
}

impl Tailer {
    fn color_for(&mut self, label: &str) -> &'static str {
        let next = COLORS[self.colors.len() % COLORS.len()];
        self.colors.entry(label.to_string()).or_insert(next)
    }

    fn start(&mut self, c: Container, history: History) -> Result<(), String> {
        let color = self.color_for(&c.label);
        self.printer
            .width
            .fetch_max(c.label.chars().count(), Ordering::Relaxed);
        self.printer.status(&format!("+ {} ({})", c.name, c.id));

        let mut cmd = Command::new("docker");
        cmd.args(["logs", "--follow"]);
        if self.args.timestamps {
            cmd.arg("--timestamps");
        }
        let mut child = cmd
            .args(history.args())
            .arg(&c.id)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to run docker logs: {e}"))?;

        // docker logs replays the container's stdout and stderr on its own.
        let streams: [Box<dyn Read + Send>; 2] = [
            Box::new(child.stdout.take().unwrap()),
            Box::new(child.stderr.take().unwrap()),
        ];
        let readers = streams
            .into_iter()
            .map(|s| {
                spawn_reader(
                    s,
                    c.label.clone(),
                    color,
                    self.printer.clone(),
                    self.tails.clone(),
                )
            })
            .collect();
        self.tails
            .lock()
            .unwrap()
            .insert(c.id, Tail { child, readers });
        Ok(())
    }

    /// Removes tails whose `docker logs` has exited (the container stopped)
    /// and waits for their output to drain. Returns how many are still running.
    fn reap(&mut self) -> usize {
        let (finished, remaining) = {
            let mut tails = self.tails.lock().unwrap();
            let done: Vec<String> = tails
                .iter_mut()
                .filter_map(|(id, t)| (!matches!(t.child.try_wait(), Ok(None))).then(|| id.clone()))
                .collect();
            let finished: Vec<Tail> = done.iter().filter_map(|id| tails.remove(id)).collect();
            let now = SystemTime::now();
            for id in done {
                self.ended.insert(id, now);
            }
            (finished, tails.len())
        };
        for tail in finished {
            for reader in tail.readers {
                let _ = reader.join();
            }
        }
        remaining
    }

    fn run(mut self) -> Result<(), String> {
        let initial: Vec<Container> = list_containers(self.args.full_names)?
            .into_iter()
            .filter(|c| self.filter.matches(&c.label))
            .collect();
        if initial.is_empty() {
            if self.args.no_watch {
                return Err("no running containers match".into());
            }
            self.printer.status("no matching containers yet — waiting…");
        }

        let history = match &self.args.since {
            Some(s) => History::Since(s.clone()),
            None => History::Tail(self.args.tail),
        };
        for c in initial {
            self.start(c, history.clone())?;
        }

        if self.args.no_watch {
            while self.reap() > 0 {
                thread::sleep(Duration::from_millis(200));
            }
            return Ok(());
        }

        let mut ps_failing = false;
        loop {
            thread::sleep(Duration::from_secs(self.args.interval));
            self.reap();
            let containers = match list_containers(self.args.full_names) {
                Ok(cs) => {
                    ps_failing = false;
                    cs
                }
                Err(e) => {
                    if !ps_failing {
                        self.printer.status(&format!("docker ps failed: {e}"));
                    }
                    ps_failing = true;
                    continue;
                }
            };
            for c in containers {
                if !self.filter.matches(&c.label) || self.tails.lock().unwrap().contains_key(&c.id)
                {
                    continue;
                }
                // New containers are tailed from their first line, so startup
                // output isn't lost; restarted ones resume where they stopped.
                let history = match self.ended.get(&c.id) {
                    Some(t) => History::Since(unix_timestamp(*t)),
                    None => History::All,
                };
                if let Err(e) = self.start(c, history) {
                    self.printer.status(&e);
                }
            }
        }
    }
}

fn main() {
    let args = Args::parse();

    let fail = |msg: String| -> ! {
        eprintln!("dtail: {msg}");
        std::process::exit(1);
    };
    let filter = Filter::new(&args.patterns, &args.exclude)
        .unwrap_or_else(|e| fail(format!("invalid pattern: {e}")));
    let grep = args.grep.as_deref().map(|g| {
        RegexBuilder::new(g)
            .case_insensitive(true)
            .build()
            .unwrap_or_else(|e| fail(format!("invalid --grep: {e}")))
    });

    let printer = Arc::new(Printer {
        color: io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        width: AtomicUsize::new(MIN_WIDTH),
        grep,
    });
    let tails: Tails = Arc::new(Mutex::new(HashMap::new()));

    let handler_tails = tails.clone();
    ctrlc::set_handler(move || {
        kill_all(&handler_tails);
        std::process::exit(0);
    })
    .unwrap_or_else(|e| fail(format!("failed to set signal handler: {e}")));

    let tailer = Tailer {
        args,
        filter,
        printer,
        tails: tails.clone(),
        ended: HashMap::new(),
        colors: HashMap::new(),
    };
    if let Err(e) = tailer.run() {
        kill_all(&tails);
        fail(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(include: &[&str], exclude: &[&str]) -> Filter {
        let s = |v: &[&str]| v.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        Filter::new(&s(include), &s(exclude)).unwrap()
    }

    #[test]
    fn compose_container_is_labelled_by_service() {
        let c = parse_ps_line("abc\tproj-kyberlifeorder-1\tkyberlifeorder\t1", false).unwrap();
        assert_eq!(c.label, "kyberlifeorder");
        assert_eq!(c.name, "proj-kyberlifeorder-1");
        assert_eq!(c.id, "abc");
    }

    #[test]
    fn extra_compose_replicas_get_their_number() {
        let c = parse_ps_line("abc\tproj-web-2\tweb\t2", false).unwrap();
        assert_eq!(c.label, "web-2");
    }

    #[test]
    fn plain_container_is_labelled_by_name() {
        let c = parse_ps_line("abc\tmy-postgres\t\t", false).unwrap();
        assert_eq!(c.label, "my-postgres");
    }

    #[test]
    fn full_names_uses_container_name() {
        let c = parse_ps_line("abc\tproj-web-1\tweb\t1", true).unwrap();
        assert_eq!(c.label, "proj-web-1");
    }

    #[test]
    fn blank_ps_line_is_skipped() {
        assert_eq!(parse_ps_line("", false), None);
    }

    #[test]
    fn no_patterns_matches_everything() {
        assert!(filter(&[], &[]).matches("anything"));
    }

    #[test]
    fn any_include_pattern_matches() {
        let f = filter(&["order", "^iam$"], &[]);
        assert!(f.matches("kyberlifeorder"));
        assert!(f.matches("iam"));
        assert!(!f.matches("kyberlifeiam"));
    }

    #[test]
    fn exclude_wins_over_include() {
        let f = filter(&["kyberlife"], &["analytics"]);
        assert!(f.matches("kyberlifeorder"));
        assert!(!f.matches("kyberlifeanalytics"));
    }

    #[test]
    fn grep_is_case_insensitive() {
        let p = Printer {
            color: false,
            width: AtomicUsize::new(MIN_WIDTH),
            grep: Some(
                RegexBuilder::new("error")
                    .case_insensitive(true)
                    .build()
                    .unwrap(),
            ),
        };
        assert!(p.wants(b"Unhandled ERROR here\n"));
        assert!(!p.wants(b"all good\n"));
    }

    #[test]
    fn prefix_pads_to_current_width() {
        let p = Printer {
            color: false,
            width: AtomicUsize::new(10),
            grep: None,
        };
        assert_eq!(p.prefix("iam", "31"), "iam        │ ");
    }
}
