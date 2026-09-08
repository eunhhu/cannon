use std::io::Write;
use std::process::{Command, Stdio};
const CLI: &str = env!("CARGO_BIN_EXE_cannon-native");
#[test]
fn help_and_demo_are_native_executables() {
    assert!(Command::new(CLI).arg("--help").output().unwrap().status.success());
    let demo = Command::new(CLI).arg("demo").output().unwrap();
    assert!(demo.status.success());
    let text = String::from_utf8(demo.stdout).unwrap();
    assert!(text.contains("\"value\":true"));
    assert!(text.contains("\"value\":false"));
}
#[test]
fn expression_stdin_is_not_executed_as_host_code() {
    let mut process = Command::new(CLI).args(["eval", "--stdin", "--json"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    process.stdin.take().unwrap().write_all(b"if true then 42 else 0").unwrap();
    let output = process.wait_with_output().unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("\"value\":42"));
    assert!(text.contains("\"uri\":\"stdin.expression\""));
}
#[test]
fn failure_exit_codes_distinguish_compilation_and_execution() {
    assert_eq!(Command::new(CLI).args(["eval", "1 + true", "--json"]).output().unwrap().status.code(), Some(2));
    assert_eq!(Command::new(CLI).args(["eval", "9007199254740991 + 1", "--json"]).output().unwrap().status.code(), Some(1));
}
#[test]
fn malformed_options_are_errors_not_ignored_preferences() {
    for args in [vec!["nope"], vec!["eval"], vec!["eval", "1", "--other"], vec!["eval", "1", "--stdin"], vec!["eval", "1", "2"], vec!["eval", "1", "--max-depth", "129"], vec!["eval", "1", "--max-trace", "0"], vec!["eval", "1", "--json", "--json"], vec!["eval", "1", "--max-steps", "2.5"]] {
        assert_eq!(Command::new(CLI).args(args).output().unwrap().status.code(), Some(2));
    }
}
#[test]
fn negative_expression_is_not_mistaken_for_an_option() {
    let output = Command::new(CLI).args(["eval", "-42", "--json"]).output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().contains("\"value\":-42"));
}
