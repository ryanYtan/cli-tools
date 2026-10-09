mod docker;
mod filter;
mod printer;
mod tailer;

use std::io::{self, IsTerminal};
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;

use docker::History;
use filter::Filter;
use printer::Printer;
use tailer::{Options, Tailer, Tails};

// Patterns are regexes matched against the compose service name (e.g.
// `kyberlifeorder`), or the container name for non-compose containers or with
// --full-names. No pattern = every running container. Containers that start,
// restart, or are recreated later are picked up automatically.

/// Tails logs from all running Docker containers matching any pattern, like kubetail
#[derive(Parser)]
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

fn main() -> ExitCode {
    common::run(env!("CARGO_PKG_NAME"), run)
}

fn run() -> common::Result {
    let args = Args::parse();

    let filter = Filter::new(&args.patterns, &args.exclude)
        .map_err(|e| format!("invalid pattern: {e}"))?;
    let color = io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let printer =
        Printer::new(color, args.grep.as_deref()).map_err(|e| format!("invalid --grep: {e}"))?;

    let tails = Tails::default();
    let handler_tails = tails.clone();
    ctrlc::set_handler(move || tailer::shutdown(&handler_tails))
        .map_err(|e| format!("failed to set signal handler: {e}"))?;

    let options = Options {
        timestamps: args.timestamps,
        watch: !args.no_watch,
        full_names: args.full_names,
        interval: Duration::from_secs(args.interval),
    };
    let history = match args.since {
        Some(since) => History::Since(since),
        None => History::Tail(args.tail),
    };
    Tailer::new(options, filter, printer, tails).run(history)?;
    Ok(())
}
