//! Helpers for the tools' CLI tests.

use std::io::Write;
use std::process::{Command, Output, Stdio};

/// A tool's binary, as given by `env!("CARGO_BIN_EXE_<name>")`.
pub struct Tool(&'static str);

impl Tool {
    pub const fn new(bin: &'static str) -> Self {
        Self(bin)
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.run_with_stdin(args, "")
    }

    pub fn run_with_stdin(&self, args: &[&str], input: &str) -> Output {
        let mut child = Command::new(self.0)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", self.0));
        // The tool may exit without reading its input, closing the pipe.
        let _ = child.stdin.take().unwrap().write_all(input.as_bytes());
        child
            .wait_with_output()
            .unwrap_or_else(|e| panic!("failed to wait on {}: {e}", self.0))
    }

    /// Runs the tool, asserts that it succeeded, and returns its stdout.
    pub fn stdout(&self, args: &[&str]) -> String {
        self.stdout_with_stdin(args, "")
    }

    /// Like [`Tool::stdout`], with `input` fed to the tool's stdin.
    pub fn stdout_with_stdin(&self, args: &[&str], input: &str) -> String {
        let out = self.run_with_stdin(args, input);
        assert!(out.status.success(), "{} {args:?} failed: {out:?}", self.0);
        String::from_utf8(out.stdout).expect("stdout was not utf8")
    }
}
