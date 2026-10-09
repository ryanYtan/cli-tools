use std::sync::atomic::{AtomicUsize, Ordering};

use regex::{Regex, RegexBuilder};

const MIN_WIDTH: usize = 8;

/// Formats output lines; shared by every reader thread.
pub struct Printer {
    color: bool,
    width: AtomicUsize,
    grep: Option<Regex>,
}

impl Printer {
    /// `grep` is a case-insensitive regex that lines must match to be printed.
    pub fn new(color: bool, grep: Option<&str>) -> Result<Self, regex::Error> {
        let grep = grep
            .map(|g| RegexBuilder::new(g).case_insensitive(true).build())
            .transpose()?;
        Ok(Self {
            color,
            width: AtomicUsize::new(MIN_WIDTH),
            grep,
        })
    }

    /// Makes the label column at least wide enough for `label`.
    pub fn widen(&self, label: &str) {
        self.width
            .fetch_max(label.chars().count(), Ordering::Relaxed);
    }

    pub fn prefix(&self, label: &str, color: &str) -> String {
        let width = self.width.load(Ordering::Relaxed);
        if self.color {
            format!("\x1b[{color}m{label:<width$}\x1b[0m │ ")
        } else {
            format!("{label:<width$} │ ")
        }
    }

    pub fn status(&self, msg: &str) {
        if self.color {
            eprintln!("\x1b[2m{msg}\x1b[0m");
        } else {
            eprintln!("{msg}");
        }
    }

    pub fn wants(&self, line: &[u8]) -> bool {
        self.grep
            .as_ref()
            .is_none_or(|re| re.is_match(&String::from_utf8_lossy(line)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grep_is_case_insensitive() {
        let p = Printer::new(false, Some("error")).unwrap();
        assert!(p.wants(b"Unhandled ERROR here\n"));
        assert!(!p.wants(b"all good\n"));
    }

    #[test]
    fn prefix_pads_to_the_widest_label_seen() {
        let p = Printer::new(false, None).unwrap();
        assert_eq!(p.prefix("iam", "31"), "iam      │ ");
        p.widen("0123456789");
        p.widen("short");
        assert_eq!(p.prefix("iam", "31"), "iam        │ ");
    }
}
