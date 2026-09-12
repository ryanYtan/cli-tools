use std::io::{self, BufRead};

use clap::Parser;

/// Splits text into lines based on a delimiter
#[derive(Parser)]
#[command(about = "Splits text into lines based on a delimiter")]
struct Args {
    /// Delimiter to split on
    #[arg(short, long, default_value = ",")]
    delimiter: String,

    /// Ignore empty lines
    #[arg(short = 'n', long)]
    ignore_empty: bool,

    /// Do not write empty lines
    #[arg(short = 'w', long, default_value_t = true)]
    no_empty_write: bool,

    /// Trim whitespace from lines
    #[arg(short, long, default_value_t = true)]
    trim: bool,
}

/// Splits one line per the given options, returning the parts that should
/// be printed (already trimmed if `trim` is set).
fn split_line<'a>(
    line: &'a str,
    delimiter: &str,
    ignore_empty: bool,
    no_empty_write: bool,
    trim: bool,
) -> Vec<&'a str> {
    if ignore_empty && line.trim().is_empty() {
        return Vec::new();
    }
    line.split(delimiter)
        .filter(|part| !(no_empty_write && part.trim().is_empty()))
        .map(|part| if trim { part.trim() } else { part })
        .collect()
}

fn main() {
    let args = Args::parse();

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.expect("failed to read from stdin");
        for part in split_line(
            &line,
            &args.delimiter,
            args.ignore_empty,
            args.no_empty_write,
            args.trim,
        ) {
            println!("{part}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_the_delimiter() {
        assert_eq!(split_line("a,b,c", ",", false, true, true), vec!["a", "b", "c"]);
    }

    #[test]
    fn drops_empty_parts_when_no_empty_write_is_set() {
        assert_eq!(split_line("a,,b", ",", false, true, true), vec!["a", "b"]);
    }

    #[test]
    fn keeps_empty_parts_when_no_empty_write_is_unset() {
        assert_eq!(split_line("a,,b", ",", false, false, true), vec!["a", "", "b"]);
    }

    #[test]
    fn trims_whitespace_from_each_part_when_trim_is_set() {
        assert_eq!(
            split_line(" a , b ", ",", false, true, true),
            vec!["a", "b"]
        );
    }

    #[test]
    fn keeps_whitespace_when_trim_is_unset() {
        assert_eq!(
            split_line(" a , b ", ",", false, false, false),
            vec![" a ", " b "]
        );
    }

    #[test]
    fn ignore_empty_skips_whitespace_only_lines_entirely() {
        assert!(split_line("   ", ",", true, true, true).is_empty());
        assert_eq!(
            split_line("a,b", ",", true, true, true),
            vec!["a", "b"]
        );
    }

    #[test]
    fn supports_multi_character_delimiters() {
        assert_eq!(
            split_line("a::b::c", "::", false, true, true),
            vec!["a", "b", "c"]
        );
    }
}
