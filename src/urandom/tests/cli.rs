use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_urandom"))
        .args(args)
        .output()
        .expect("failed to run urandom")
}

fn stdout(args: &[&str]) -> String {
    let out = run(args);
    assert!(out.status.success(), "urandom {args:?} failed: {out:?}");
    String::from_utf8(out.stdout).expect("stdout was not utf8")
}

#[test]
fn default_size_is_32_charset_characters() {
    let out = stdout(&[]);
    assert_eq!(out.trim_end_matches('\n').len(), 32);
}

#[test]
fn size_flag_controls_output_length() {
    let out = stdout(&["-i", "10"]);
    assert_eq!(out.trim_end_matches('\n').len(), 10);
}

#[test]
fn output_only_uses_the_default_charset() {
    let out = stdout(&["-i", "500"]);
    let line = out.trim_end_matches('\n');
    assert!(line.chars().all(|c| c.is_ascii_alphanumeric()));
}

#[test]
fn custom_charset_restricts_output_characters() {
    let out = stdout(&["-i", "200", "-c", "ab"]);
    let line = out.trim_end_matches('\n');
    assert_eq!(line.len(), 200);
    assert!(line.chars().all(|c| c == 'a' || c == 'b'));
}

#[test]
fn hex_output_is_twice_the_byte_size_and_valid_hex() {
    let out = stdout(&["-i", "16", "-x"]);
    let line = out.trim_end_matches('\n');
    assert_eq!(line.len(), 32);
    assert!(line.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(line, line.to_ascii_lowercase());
}

#[test]
fn bytes_output_writes_the_exact_raw_byte_count() {
    let out = run(&["-i", "37", "-b"]);
    assert!(out.status.success());
    assert_eq!(out.stdout.len(), 37);
}

#[test]
fn hex_and_bytes_conflict() {
    let out = run(&["-x", "-b"]);
    assert!(!out.status.success());
}

#[test]
fn zero_size_produces_empty_output() {
    let out = stdout(&["-i", "0"]);
    assert_eq!(out.trim_end_matches('\n'), "");
}

#[test]
fn empty_charset_fails_cleanly() {
    let out = run(&["-c", ""]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
}
