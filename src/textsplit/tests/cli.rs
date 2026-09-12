use std::io::Write;
use std::process::{Command, Stdio};

fn run_with_stdin(args: &[&str], input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_textsplit"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn textsplit");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("failed to wait on textsplit");
    assert!(out.status.success(), "textsplit {args:?} failed: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn default_delimiter_is_a_comma() {
    let out = run_with_stdin(&[], "a,b,c\n");
    assert_eq!(out, "a\nb\nc\n");
}

#[test]
fn custom_delimiter() {
    let out = run_with_stdin(&["-d", ";"], "a;b;c\n");
    assert_eq!(out, "a\nb\nc\n");
}

#[test]
fn empty_parts_are_dropped_by_default() {
    let out = run_with_stdin(&[], "a,,b\n");
    assert_eq!(out, "a\nb\n");
}

#[test]
fn whitespace_only_lines_are_skipped_with_ignore_empty() {
    let out = run_with_stdin(&["-n"], "a,b\n   \nc,d\n");
    assert_eq!(out, "a\nb\nc\nd\n");
}

#[test]
fn multiple_input_lines_are_all_processed() {
    let out = run_with_stdin(&[], "a,b\nc,d\n");
    assert_eq!(out, "a\nb\nc\nd\n");
}
