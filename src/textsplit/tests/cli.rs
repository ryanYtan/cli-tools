use testutil::Tool;

const TOOL: Tool = Tool::new(env!("CARGO_BIN_EXE_textsplit"));

#[test]
fn default_delimiter_is_a_comma() {
    let out = TOOL.stdout_with_stdin(&[], "a,b,c\n");
    assert_eq!(out, "a\nb\nc\n");
}

#[test]
fn custom_delimiter() {
    let out = TOOL.stdout_with_stdin(&["-d", ";"], "a;b;c\n");
    assert_eq!(out, "a\nb\nc\n");
}

#[test]
fn empty_parts_are_written_by_default() {
    let out = TOOL.stdout_with_stdin(&[], "a,,b\n\nc\n");
    assert_eq!(out, "a\n\nb\n\nc\n");
}

#[test]
fn empty_parts_and_lines_are_skipped_with_ignore_empty() {
    let out = TOOL.stdout_with_stdin(&["-n"], "a,,b\n   \nc, ,d\n");
    assert_eq!(out, "a\nb\nc\nd\n");
}

#[test]
fn parts_are_trimmed_by_default() {
    let out = TOOL.stdout_with_stdin(&[], " a , b \n");
    assert_eq!(out, "a\nb\n");
}

#[test]
fn no_trim_keeps_whitespace() {
    let out = TOOL.stdout_with_stdin(&["--no-trim"], " a , b \n");
    assert_eq!(out, " a \n b \n");
    let out = TOOL.stdout_with_stdin(&["-T"], " a , b \n");
    assert_eq!(out, " a \n b \n");
}

#[test]
fn multiple_input_lines_are_all_processed() {
    let out = TOOL.stdout_with_stdin(&[], "a,b\nc,d\n");
    assert_eq!(out, "a\nb\nc\nd\n");
}
