use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use clap::Parser;

/// Maps lines from stdin using a mapping file
#[derive(Parser)]
struct Args {
    /// File containing the mapping
    #[arg(short = 'i', long)]
    input_mapping: String,

    /// Delimiter used in the mapping file
    #[arg(short = 'd', long, default_value = ":")]
    map_delimiter: String,

    /// Default value to use if no mapping is found
    #[arg(short = 'e', long, default_value = "")]
    default_value: String,
}

fn build_mapping(contents: &str, delimiter: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(delimiter).map(|x| x.trim()).collect();
        if parts.len() < 2 {
            continue;
        }
        m.insert(parts[0].to_string(), parts[1].to_string());
    }
    m
}

fn main() -> ExitCode {
    common::run(env!("CARGO_PKG_NAME"), run)
}

fn run() -> common::Result {
    let args = Args::parse();

    let contents = fs::read_to_string(&args.input_mapping)
        .map_err(|e| format!("failed to read {}: {e}", args.input_mapping))?;
    let m = build_mapping(&contents, &args.map_delimiter);

    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| format!("failed to read from stdin: {e}"))?;
        let line = line.trim();
        let value = m.get(line).unwrap_or(&args.default_value);
        writeln!(out, "{value}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_basic_key_value_pairs() {
        let m = build_mapping("a:1\nb:2\n", ":");
        assert_eq!(m.get("a"), Some(&"1".to_string()));
        assert_eq!(m.get("b"), Some(&"2".to_string()));
    }

    #[test]
    fn trims_whitespace_around_keys_and_values() {
        let m = build_mapping("  a  :  1  \n", ":");
        assert_eq!(m.get("a"), Some(&"1".to_string()));
    }

    #[test]
    fn skips_blank_lines() {
        let m = build_mapping("a:1\n\n   \nb:2\n", ":");
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn skips_lines_without_the_delimiter() {
        let m = build_mapping("a:1\nnodelimiterhere\nb:2\n", ":");
        assert_eq!(m.len(), 2);
        assert!(!m.contains_key("nodelimiterhere"));
    }

    #[test]
    fn ignores_extra_fields_past_the_second() {
        let m = build_mapping("a:1:extra:stuff\n", ":");
        assert_eq!(m.get("a"), Some(&"1".to_string()));
    }

    #[test]
    fn supports_multi_character_delimiters() {
        let m = build_mapping("a::1\nb::2\n", "::");
        assert_eq!(m.get("a"), Some(&"1".to_string()));
        assert_eq!(m.get("b"), Some(&"2".to_string()));
    }

    #[test]
    fn last_value_for_a_repeated_key_wins() {
        let m = build_mapping("a:1\na:2\n", ":");
        assert_eq!(m.get("a"), Some(&"2".to_string()));
    }
}
