use std::path::PathBuf;
use std::process::Command;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.xlsx")
}

fn stdout(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_colsof"))
        .args(args)
        .output()
        .expect("failed to run colsof");
    assert!(out.status.success(), "colsof {args:?} failed: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn reads_columns_by_index() {
    let path = fixture();
    let out = stdout(&[
        path.to_str().unwrap(),
        "-i",
        "Sheet1",
        "-c",
        "1",
        "2",
        "3",
        "-d",
        ",",
    ]);
    assert_eq!(
        out,
        "Alice,30,NYC\nBob,<EMPTY>,LA\nCarol,25,<EMPTY>\n"
    );
}

#[test]
fn reads_columns_by_letter() {
    let path = fixture();
    let out = stdout(&[
        path.to_str().unwrap(),
        "-i",
        "Sheet1",
        "-c",
        "A",
        "B",
        "C",
        "-d",
        ",",
    ]);
    assert_eq!(
        out,
        "Alice,30,NYC\nBob,<EMPTY>,LA\nCarol,25,<EMPTY>\n"
    );
}

#[test]
fn sheet_can_be_selected_by_numeric_index() {
    let path = fixture();
    let out = stdout(&[
        path.to_str().unwrap(),
        "-i",
        "1",
        "-c",
        "1",
        "2",
        "-d",
        ",",
    ]);
    assert_eq!(out, "1,2\n3,4\n");
}

#[test]
fn custom_default_value_for_empty_cells() {
    let path = fixture();
    let out = stdout(&[
        path.to_str().unwrap(),
        "-i",
        "Sheet1",
        "-c",
        "2",
        "-f",
        "N/A",
    ]);
    assert_eq!(out, "30\nN/A\n25\n");
}

#[test]
fn default_columns_argument_matches_nothing_and_prints_blank_lines() {
    // `-c` defaults to "0", which never matches a real (1-based) column -
    // this mirrors the original tool's documented quirk.
    let path = fixture();
    let out = stdout(&[path.to_str().unwrap(), "-i", "Sheet1"]);
    assert_eq!(out, "\n\n\n");
}

#[test]
fn missing_file_fails() {
    let out = Command::new(env!("CARGO_BIN_EXE_colsof"))
        .args(["/no/such/file.xlsx", "-i", "Sheet1"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn missing_sheet_name_fails() {
    let path = fixture();
    let out = Command::new(env!("CARGO_BIN_EXE_colsof"))
        .args([path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success());
}
