use cannon_core::{compile_expression, evaluate, report, Limits, Value, MAX_INT, MAX_SOURCE_UNITS};

fn value(source: &str) -> Value {
    evaluate(&compile_expression(source).unwrap(), Limits::default()).result.unwrap()
}
#[test]
fn arithmetic_and_precedence() {
    for (source, expected) in [("1 + 2 * 3", 7), ("(1 + 2) * 3", 9), ("10 - 3 - 2", 5), ("-4 * -3", 12), ("0000012", 12)] {
        assert_eq!(value(source), Value::Int(expected), "{source}");
    }
}
#[test]
fn inventory_boundary_change_is_observable() {
    assert_eq!(value("8 + 2 <= 10"), Value::Bool(true));
    assert_eq!(value("8 + 2 < 10"), Value::Bool(false));
}
#[test]
fn safe_integer_domain_is_not_widened_to_i64() {
    assert_eq!(value("9007199254740991"), Value::Int(MAX_INT));
    assert_eq!(value("-9007199254740991"), Value::Int(-MAX_INT));
    for source in ["9007199254740991 + 1", "-9007199254740991 - 1", "9007199254740991 * 9007199254740991"] {
        let run = evaluate(&compile_expression(source).unwrap(), Limits::default());
        assert_eq!(run.result.unwrap_err().code, "INTEGER_RANGE");
    }
    for source in ["9007199254740992", "999999999999999999999999999"] {
        assert_eq!(compile_expression(source).unwrap_err().code, "PARSE_ERROR");
    }
}
#[test]
fn skipped_branches_are_checked_but_not_executed() {
    assert_eq!(value("false and (9007199254740991 + 1 == 0)"), Value::Bool(false));
    assert_eq!(value("true or (9007199254740991 + 1 == 0)"), Value::Bool(true));
    assert_eq!(value("if true then 7 else 9007199254740991 + 1"), Value::Int(7));
    for source in ["if true then 7 else false", "false and 3", "if false then 1 + true else 8", "1 == true", "\"1\" + \"2\""] {
        assert_eq!(compile_expression(source).unwrap_err().code, "TYPE_MISMATCH", "{source}");
    }
}
#[test]
fn trace_is_actual_evaluation_not_an_explanation() {
    let run = evaluate(&compile_expression("if true then 7 else 9").unwrap(), Limits::default());
    assert_eq!(run.steps, 3);
    assert_eq!(run.trace.iter().map(|s| (s.kind, s.label)).collect::<Vec<_>>(), vec![
        ("literal", "literal"), ("branch", "then branch selected"), ("literal", "literal"), ("if", "if")]);
    assert!(!run.trace.iter().any(|step| step.value == Some(Value::Int(9))));
}
#[test]
fn short_circuit_note_has_no_invented_value() {
    let run = evaluate(&compile_expression("false and true").unwrap(), Limits::default());
    assert_eq!(run.steps, 2);
    assert_eq!(run.trace[1].kind, "short-circuit");
    assert_eq!(run.trace[1].value, None);
}
#[test]
fn strings_preserve_utf16_code_units_including_lone_surrogates() {
    assert_eq!(value(r#""\ud800""#), Value::Text(vec![0xd800]));
    assert_eq!(value(r#""\ud83d\ude80" == "🚀""#), Value::Bool(true));
    assert_eq!(value(r#""\ud800" == "\udfff""#), Value::Bool(false));
    assert_eq!(report::value_json(&Value::Text(vec![0xd800, 10, 34, 92])), r#""\ud800\u000a\"\\""#);
}
#[test]
fn source_positions_use_utf16_not_utf8_bytes() {
    let program = compile_expression("\"🚀\" == \"🚀\"").unwrap();
    let run = evaluate(&program, Limits::default());
    assert_eq!((run.trace[0].span.start, run.trace[0].span.end), (0, 4));
    assert_eq!((run.trace[1].span.start, run.trace[1].span.end, run.trace[1].span.column), (8, 12, 9));
    assert_eq!(program.span().end, 12);
}
#[test]
fn crlf_and_unicode_comment_positions_match_reference() {
    let run = evaluate(&compile_expression("// 🚀\r\n  1 + 2").unwrap(), Limits::default());
    let s = run.trace[0].span;
    assert_eq!((s.start, s.line, s.column), (9, 2, 3));
}
#[test]
fn only_ecmascript_whitespace_is_accepted() {
    assert_eq!(value("\u{feff}1\u{a0}+\u{2007}2"), Value::Int(3));
    assert_eq!(compile_expression("1\u{85}+2").unwrap_err().code, "PARSE_ERROR");
}
#[test]
fn json_string_escapes_are_validated() {
    for source in [r#""\q""#, r#""\u12xz""#, "\"unterminated", "\"raw\nnewline\""] {
        assert_eq!(compile_expression(source).unwrap_err().code, "PARSE_ERROR");
    }
    assert_eq!(value(r#""\b\f\n\r\t\/""#), Value::Text(vec![8,12,10,13,9,47]));
}
#[test]
fn limits_fail_instead_of_returning_a_clipped_success() {
    let program = compile_expression("1 + 2").unwrap();
    for (limits, code) in [
        (Limits { max_steps: 1, ..Limits::default() }, "STEP_LIMIT"),
        (Limits { max_depth: 1, ..Limits::default() }, "DEPTH_LIMIT"),
        (Limits { max_trace: 1, ..Limits::default() }, "TRACE_LIMIT"),
        (Limits { max_steps: 0, ..Limits::default() }, "INVALID_LIMIT"),
        (Limits { max_depth: 129, ..Limits::default() }, "INVALID_LIMIT"),
    ] {
        assert_eq!(evaluate(&program, limits).result.unwrap_err().code, code);
    }
}
#[test]
fn parsing_is_bounded_in_depth_width_and_source_size() {
    for source in [format!("{}1{}", "(".repeat(130), ")".repeat(130)), "1+".repeat(3000) + "1", " ".repeat(MAX_SOURCE_UNITS + 1), "1 ".repeat(20_001)] {
        assert_eq!(compile_expression(&source).unwrap_err().code, "PARSE_ERROR");
    }
}
#[test]
fn unknown_names_and_trailing_tokens_are_not_silently_accepted() {
    assert_eq!(compile_expression("quantity").unwrap_err().code, "UNKNOWN_NAME");
    for source in ["", "1 2", "1 / 2", "1;", "(1", "1)"] {
        assert_eq!(compile_expression(source).unwrap_err().code, "PARSE_ERROR");
    }
}
#[test]
fn arena_remains_reusable_across_runs() {
    let program = compile_expression("1 + 2 * 3").unwrap();
    let one = evaluate(&program, Limits { max_steps: 1, ..Limits::default() });
    assert!(one.result.is_err());
    let two = evaluate(&program, Limits::default());
    assert_eq!(two.result.unwrap(), Value::Int(7));
    assert_eq!(two.steps, 5);
    assert_eq!(two.trace.len(), 5);
}
#[test]
fn report_contains_exact_source_and_distinct_protocol_identity() {
    let source = "\"🚀\"";
    let run = evaluate(&compile_expression(source).unwrap(), Limits::default());
    let text = report::render(source, "test.expression", Limits::default(), "execution", &run);
    assert!(text.contains("cannon.native.expression/1"));
    assert!(text.contains("\"implementation\":\"rust\""));
    assert!(text.contains(&format!("\"text\":{}", report::quote(source))));
    assert!(!text.contains("engineDigest")); // Do not forge full-engine evidence compatibility.
}
