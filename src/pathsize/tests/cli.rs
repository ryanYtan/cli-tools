use testutil::Tool;

const TOOL: Tool = Tool::new(env!("CARGO_BIN_EXE_pathsize"));

#[test]
fn default_prints_raw_byte_count_of_a_file() {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    use std::io::Write;
    f.write_all(&[0u8; 4096]).unwrap();
    let out = TOOL.stdout(&[f.path().to_str().unwrap()]);
    assert_eq!(out.trim(), "4096");
}

#[test]
fn kilo_flag_divides_by_1024() {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    use std::io::Write;
    f.write_all(&[0u8; 4096]).unwrap();
    let out = TOOL.stdout(&["-k", f.path().to_str().unwrap()]);
    assert_eq!(out.trim(), "4.0K");
}

#[test]
fn human_readable_sums_a_directory_recursively() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.bin"), vec![0u8; 1_500_000]).unwrap();
    let out = TOOL.stdout(&["-h", dir.path().to_str().unwrap()]);
    assert_eq!(out.trim(), "1.4M");
}

#[test]
fn conflicting_unit_flags_fail() {
    let out = TOOL.run(&["-k", "-m", "."]);
    assert!(!out.status.success());
}

#[test]
fn missing_path_fails() {
    let out = TOOL.run(&["/no/such/path/at/all"]);
    assert!(!out.status.success());
}
