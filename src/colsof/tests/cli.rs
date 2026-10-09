use std::path::PathBuf;

use testutil::Tool;

const TOOL: Tool = Tool::new(env!("CARGO_BIN_EXE_colsof"));

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.xlsx")
}

#[test]
fn reads_columns_by_index() {
    let path = fixture();
    let out = TOOL.stdout(&[
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
    let out = TOOL.stdout(&[
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
    let out = TOOL.stdout(&[
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
    let out = TOOL.stdout(&[
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
    let out = TOOL.stdout(&[path.to_str().unwrap(), "-i", "Sheet1"]);
    assert_eq!(out, "\n\n\n");
}

#[test]
fn missing_file_fails() {
    let out = TOOL.run(&["/no/such/file.xlsx", "-i", "Sheet1"]);
    assert!(!out.status.success());
}

#[test]
fn missing_sheet_name_fails() {
    let path = fixture();
    let out = TOOL.run(&[path.to_str().unwrap()]);
    assert!(!out.status.success());
}

#[test]
fn no_skip_header_prints_the_first_row_too() {
    let path = fixture();
    let with_header = TOOL.stdout(&[path.to_str().unwrap(), "-i", "Sheet1", "-c", "1", "-W"]);
    let without = TOOL.stdout(&[path.to_str().unwrap(), "-i", "Sheet1", "-c", "1", "-w"]);
    assert_eq!(without, "Alice\nBob\nCarol\n");
    assert_eq!(with_header.lines().count(), 4);
    assert!(with_header.ends_with(&without));
}

#[test]
fn invalid_column_fails_cleanly() {
    let path = fixture();
    for col in ["-", "A1", "ZZZZZZZZZZ"] {
        let out = TOOL.run(&[path.to_str().unwrap(), "-i", "Sheet1", "-c", col]);
        assert_eq!(out.status.code(), Some(2), "column {col:?}");
        assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"));
    }
}
