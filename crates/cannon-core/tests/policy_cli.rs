use std::process::{Command,Output};
fn run(args:&[&str])->Output {Command::new(env!("CARGO_BIN_EXE_cannon-policy")).args(args).output().unwrap()}
#[test]
fn standalone_cli_evaluates_typed_inputs_as_json() {
    let out=run(&["eval","x + 2","--input","id:x:Int=8","--json"]);
    assert!(out.status.success()); let text=String::from_utf8(out.stdout).unwrap();
    for part in ["bound-expression/1","inputReferences","\"id\":\"id\"","\"value\":10"] {assert!(text.contains(part),"{part}: {text}");}
}
#[test]
fn malformed_cli_input_is_rejected_without_evaluation() {
    let inputs:Vec<Vec<&str>>=vec![vec!["eval","x","--input"],vec!["eval","x","--input","id:x:Float=2"],
        vec!["eval","x","--input","id:x:Int=1+2"],vec!["eval","x","--input","id:x:Int=1","--input","id:y:Int=2"],
        vec!["eval","0","--json","--json"],vec!["eval","0","--surprise"],vec!["eval","0","1"],vec!["demo","--surprise"]];
    for args in inputs {let out=run(&args);assert_eq!(out.status.code(),Some(2),"{args:?}");assert!(out.stdout.is_empty());}
}
#[test]
fn native_demo_exposes_hidden_expectation_change() {
    let out=run(&["demo","--json"]); assert!(out.status.success());
    let text=String::from_utf8(out.stdout).unwrap();
    for part in ["\"candidateStatus\":\"passed\"","\"historicalStatus\":\"failed\"","expectation-changed","\"regressions\":[\"case.exact\"]"] {assert!(text.contains(part),"{part}");}
}
#[test]
fn invalid_bindings_and_compilation_have_distinct_exit_codes() {
    let run_error=run(&["eval","x","--input","id:x:PositiveInt=0","--json"]);
    assert_eq!(run_error.status.code(),Some(1)); assert!(String::from_utf8_lossy(&run_error.stdout).contains("REFINEMENT_VIOLATION"));
    assert_eq!(run(&["eval","absent","--json"]).status.code(),Some(2));
    assert_eq!(run(&["eval","-2","--json"]).status.code(),Some(0));
}
#[test]
fn string_inputs_keep_equals_and_colons_inside_the_literal() {
    let out=run(&["eval","s","--input","id:s:String=\"a=b:c\"","--json"]);
    assert!(out.status.success()); assert!(String::from_utf8_lossy(&out.stdout).contains("a=b:c"));
}
