use testutil::Tool;

const TOOL: Tool = Tool::new(env!("CARGO_BIN_EXE_zargs"));

#[test]
fn runs_the_command_once_per_input_line() {
    let out = TOOL.run_with_stdin(&["--", "echo", "hi"], "one\ntwo\nthree\n");
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "hi\nhi\nhi\n");
}

#[test]
fn substitutes_positional_arguments_from_each_line() {
    let out = TOOL.run_with_stdin(&["--", "echo", "{1}-{0}"], "a b\nc d\n");
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "b-a\nd-c\n");
}

#[test]
fn multiple_template_arguments_are_each_substituted() {
    let out = TOOL.run_with_stdin(&["--", "echo", "{0}", "{1}"], "x y\n");
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "x y\n");
}

#[test]
fn invalid_template_fails_before_running_anything() {
    let out = TOOL.run_with_stdin(&["--", "echo", "{a}"], "x\n");
    assert!(!out.status.success());
}
