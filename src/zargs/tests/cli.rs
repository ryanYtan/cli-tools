use std::io::Write;
use std::process::{Command, Stdio};

fn run_with_stdin(args: &[&str], input: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_zargs"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn zargs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().expect("failed to wait on zargs")
}

#[test]
fn runs_the_command_once_per_input_line() {
    let out = run_with_stdin(&["--", "echo", "hi"], "one\ntwo\nthree\n");
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "hi\nhi\nhi\n");
}

#[test]
fn substitutes_positional_arguments_from_each_line() {
    let out = run_with_stdin(&["--", "echo", "{1}-{0}"], "a b\nc d\n");
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "b-a\nd-c\n");
}

#[test]
fn multiple_template_arguments_are_each_substituted() {
    let out = run_with_stdin(&["--", "echo", "{0}", "{1}"], "x y\n");
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "x y\n");
}

#[test]
fn invalid_template_fails_before_running_anything() {
    let out = run_with_stdin(&["--", "echo", "{a}"], "x\n");
    assert!(!out.status.success());
}
