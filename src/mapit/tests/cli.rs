use std::io::Write;
use std::process::{Command, Stdio};

fn run_with_stdin(args: &[&str], input: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mapit"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn mapit");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().expect("failed to wait on mapit")
}

fn write_mapping(contents: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("failed to create temp mapping file");
    f.write_all(contents.as_bytes()).unwrap();
    f
}

#[test]
fn maps_known_keys_and_uses_default_for_unknown() {
    let mapping = write_mapping("a:1\nb:2\n");
    let out = run_with_stdin(
        &[
            "-i",
            mapping.path().to_str().unwrap(),
            "-e",
            "MISSING",
        ],
        "a\nb\nc\n",
    );
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "1\n2\nMISSING\n"
    );
}

#[test]
fn default_value_is_empty_line_when_unset() {
    let mapping = write_mapping("a:1\n");
    let out = run_with_stdin(&["-i", mapping.path().to_str().unwrap()], "z\n");
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "\n");
}

#[test]
fn custom_delimiter_is_respected() {
    let mapping = write_mapping("a=1\nb=2\n");
    let out = run_with_stdin(
        &["-i", mapping.path().to_str().unwrap(), "-d", "="],
        "a\nb\n",
    );
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "1\n2\n");
}

#[test]
fn missing_mapping_file_fails() {
    let out = run_with_stdin(&["-i", "/no/such/mapping/file"], "a\n");
    assert!(!out.status.success());
}

#[test]
fn stdin_lines_are_trimmed_before_lookup() {
    let mapping = write_mapping("a:1\n");
    let out = run_with_stdin(&["-i", mapping.path().to_str().unwrap()], "  a  \n");
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "1\n");
}
