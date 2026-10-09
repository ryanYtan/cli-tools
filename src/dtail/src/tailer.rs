use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use crate::docker::{self, Container, History};
use crate::filter::Filter;
use crate::printer::Printer;

const COLORS: [&str; 12] = [
    "31", "32", "33", "34", "35", "36", "91", "92", "93", "94", "95", "96",
];

pub struct Options {
    pub timestamps: bool,
    /// Keep watching for new, restarted, or recreated containers.
    pub watch: bool,
    pub full_names: bool,
    /// Time between checks for new containers.
    pub interval: Duration,
}

/// A running `docker logs` and the threads copying its output.
pub struct Tail {
    child: Child,
    readers: Vec<JoinHandle<()>>,
}

/// The running tails by container ID, shared so they can be stopped from the
/// signal handler and the reader threads.
pub type Tails = Arc<Mutex<HashMap<String, Tail>>>;

fn kill_all(tails: &Tails) {
    if let Ok(mut tails) = tails.lock() {
        for tail in tails.values_mut() {
            let _ = tail.child.kill();
            let _ = tail.child.wait();
        }
    }
}

/// Stops every tail and exits successfully: the user asked to stop (Ctrl-C)
/// or stopped reading (stdout closed).
pub fn shutdown(tails: &Tails) -> ! {
    kill_all(tails);
    std::process::exit(0);
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
                shutdown(&tails);
            }
        }
    })
}

pub struct Tailer {
    options: Options,
    filter: Filter,
    printer: Arc<Printer>,
    tails: Tails,
    /// When each container's previous tail ended, so a restarted container
    /// (same ID) resumes from there instead of replaying its history.
    ended: HashMap<String, SystemTime>,
    colors: HashMap<String, &'static str>,
}

impl Tailer {
    pub fn new(options: Options, filter: Filter, printer: Printer, tails: Tails) -> Self {
        Self {
            options,
            filter,
            printer: Arc::new(printer),
            tails,
            ended: HashMap::new(),
            colors: HashMap::new(),
        }
    }

    fn color_for(&mut self, label: &str) -> &'static str {
        let next = COLORS[self.colors.len() % COLORS.len()];
        self.colors.entry(label.to_string()).or_insert(next)
    }

    fn start(&mut self, c: Container, history: &History) -> Result<(), String> {
        let color = self.color_for(&c.label);
        self.printer.widen(&c.label);
        self.printer.status(&format!("+ {} ({})", c.name, c.id));

        let mut child = docker::follow_logs(&c.id, history, self.options.timestamps)?;

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

    /// Tails the matching containers until their logs end, or forever when
    /// watching, showing `history` for those already running. Nothing is left
    /// running if this fails.
    pub fn run(mut self, history: History) -> Result<(), String> {
        let result = self.tail(&history);
        if result.is_err() {
            kill_all(&self.tails);
        }
        result
    }

    fn tail(&mut self, history: &History) -> Result<(), String> {
        let initial: Vec<Container> = docker::list_containers(self.options.full_names)?
            .into_iter()
            .filter(|c| self.filter.matches(&c.label))
            .collect();
        if initial.is_empty() {
            if !self.options.watch {
                return Err("no running containers match".into());
            }
            self.printer.status("no matching containers yet — waiting…");
        }

        // Size the label column for the whole batch up front: the first
        // tails start printing while the later ones are still being spawned.
        for c in &initial {
            self.printer.widen(&c.label);
        }
        for c in initial {
            self.start(c, history)?;
        }

        if !self.options.watch {
            while self.reap() > 0 {
                thread::sleep(Duration::from_millis(200));
            }
            return Ok(());
        }

        let mut ps_failing = false;
        loop {
            thread::sleep(self.options.interval);
            self.reap();
            let containers = match docker::list_containers(self.options.full_names) {
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
                    Some(t) => History::since_time(*t),
                    None => History::All,
                };
                if let Err(e) = self.start(c, &history) {
                    self.printer.status(&e);
                }
            }
        }
    }
}
