use std::io::{self, BufRead, Write};

use clap::Parser;

/// Splits text into lines based on a delimiter
#[derive(Parser)]
#[command(about = "Splits text into lines based on a delimiter")]
struct Args {
    /// Delimiter to split on
    #[arg(short, long, default_value = ",")]
    delimiter: String,

    /// Do not write empty lines
    #[arg(short = 'n', long)]
    ignore_empty: bool,

    /// Do not trim whitespace from lines
    #[arg(short = 'T', long)]
    no_trim: bool,
}

/// Splits one line per the given options, returning the parts that should
/// be printed (already trimmed if `trim` is set).
fn split_line<'a>(line: &'a str, delimiter: &str, ignore_empty: bool, trim: bool) -> Vec<&'a str> {
    line.split(delimiter)
        .map(|part| if trim { part.trim() } else { part })
        .filter(|part| !(ignore_empty && part.is_empty()))
        .collect()
}

/// Exits quietly when stdout is closed early (e.g. `textsplit | head`).
fn write_failed(e: io::Error) -> ! {
    if e.kind() == io::ErrorKind::BrokenPipe {
        std::process::exit(0);
    }
    eprintln!("failed to write to stdout: {e}");
    std::process::exit(1);
}

fn main() {
    let args = Args::parse();

    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.expect("failed to read from stdin");
        for part in split_line(&line, &args.delimiter, args.ignore_empty, !args.no_trim) {
            writeln!(out, "{part}").unwrap_or_else(|e| write_failed(e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_the_delimiter() {
        assert_eq!(split_line("a,b,c", ",", false, true), vec!["a", "b", "c"]);
    }

    #[test]
    fn drops_empty_parts_when_ignore_empty_is_set() {
        assert_eq!(split_line("a,,b", ",", true, true), vec!["a", "b"]);
    }

    #[test]
    fn keeps_empty_parts_when_ignore_empty_is_unset() {
        assert_eq!(split_line("a,,b", ",", false, true), vec!["a", "", "b"]);
    }

    #[test]
    fn trims_whitespace_from_each_part_when_trim_is_set() {
        assert_eq!(split_line(" a , b ", ",", false, true), vec!["a", "b"]);
    }

    #[test]
    fn keeps_whitespace_when_trim_is_unset() {
        assert_eq!(split_line(" a , b ", ",", false, false), vec![" a ", " b "]);
    }

    #[test]
    fn ignore_empty_drops_parts_that_are_empty_after_trimming() {
        assert_eq!(split_line("a, ,b", ",", true, true), vec!["a", "b"]);
        assert!(split_line("   ", ",", true, true).is_empty());
    }

    #[test]
    fn ignore_empty_keeps_whitespace_only_parts_when_trim_is_unset() {
        assert_eq!(split_line("a, ,,b", ",", true, false), vec!["a", " ", "b"]);
    }

    #[test]
    fn supports_multi_character_delimiters() {
        assert_eq!(split_line("a::b::c", "::", false, true), vec!["a", "b", "c"]);
    }
}
