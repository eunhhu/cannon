use std::ops::ControlFlow;
use cannon_core::*;

fn spec(id: &str, name: &str, ty: InputType) -> InputSpec { InputSpec::new(id, name, ty) }
fn values(pairs: &[(&str, Value)]) -> InputBindings { pairs.iter().map(|(k,v)| ((*k).to_owned(), v.clone())).collect() }
fn run(source: &str, specs: &[InputSpec], bindings: &InputBindings) -> Outcome {
    evaluate_bound(&compile_with_inputs(source, specs).unwrap(), bindings, Limits::default())
}
#[test]
fn existing_empty_input_api_matches_the_original_path() {
    let p = compile_expression("if true then 4 else 5").unwrap();
    let a = evaluate(&p, Limits::default()); let b = evaluate_bound(&p, &InputBindings::new(), Limits::default());
    assert_eq!(a.result, b.result); assert_eq!(a.trace, b.trace); assert_eq!(a.steps, b.steps);
}
#[test]
fn one_compilation_can_evaluate_multiple_inputs_without_source_substitution() {
    let p = compile_with_inputs("x + x", &[spec("id.x", "x", InputType::Int)]).unwrap();
    for n in [-2, 0, 8] {
        let out = evaluate_bound(&p, &values(&[("id.x", Value::Int(n))]), Limits::default());
        assert_eq!(out.result, Ok(Value::Int(n*2))); assert_eq!(p.source(), "x + x");
        assert_eq!(out.trace.iter().filter(|e| e.kind == "input").count(), 2);
    }
}
#[test]
fn stable_ids_not_names_bind_values() {
    let inputs = [spec("stable", "count", InputType::Int)];
    assert_eq!(run("count", &inputs, &values(&[("count", Value::Int(4))])).result.unwrap_err().code, "UNKNOWN_INPUT");
    assert_eq!(run("count", &inputs, &values(&[("stable", Value::Int(4))])).result, Ok(Value::Int(4)));
}
#[test]
fn type_checking_uses_declared_inputs_and_checks_unselected_branches() {
    let inputs = [spec("b", "flag", InputType::Bool)];
    assert_eq!(compile_with_inputs("flag + 1", &inputs).unwrap_err().code, "TYPE_MISMATCH");
    assert_eq!(compile_with_inputs("if true then 0 else flag", &inputs).unwrap_err().code, "TYPE_MISMATCH");
    assert_eq!(compile_with_inputs("if true then 0 else absent", &inputs).unwrap_err().code, "UNKNOWN_NAME");
}
#[test]
fn missing_and_extra_bindings_do_not_produce_execution_events() {
    let p = compile_with_inputs("x", &[spec("x", "x", InputType::Int)]).unwrap();
    for (inputs, code) in [(values(&[]), "MISSING_INPUT"), (values(&[("x", Value::Int(1)),("y",Value::Int(2))]), "UNKNOWN_INPUT")] {
        let out = evaluate_bound(&p, &inputs, Limits::default());
        assert_eq!(out.result.unwrap_err().code, code); assert_eq!(out.steps,0); assert!(out.trace.is_empty());
    }
}
#[test]
fn unused_inputs_are_still_required_and_validated() {
    let inputs = [spec("x", "x", InputType::PositiveInt)];
    assert_eq!(run("false", &inputs, &values(&[])).result.unwrap_err().code, "MISSING_INPUT");
    assert_eq!(run("false and x > 0", &inputs, &values(&[("x", Value::Int(0))])).result.unwrap_err().code,"REFINEMENT_VIOLATION");
}
#[test]
fn numeric_boundaries_and_refinements_are_explicit() {
    for (ty,n,code) in [(InputType::Int,i64::MAX,"INTEGER_RANGE"),(InputType::Int,i64::MIN,"INTEGER_RANGE"),
        (InputType::PositiveInt,0,"REFINEMENT_VIOLATION"),(InputType::NonNegativeInt,-1,"REFINEMENT_VIOLATION")] {
        assert_eq!(run("x", &[spec("x","x",ty)], &values(&[("x",Value::Int(n))])).result.unwrap_err().code,code);
    }
    for n in [-MAX_INT,MAX_INT] { assert_eq!(run("x", &[spec("x","x",InputType::Int)], &values(&[("x",Value::Int(n))])).result,Ok(Value::Int(n))); }
    assert_eq!(run("x", &[spec("x","x",InputType::Int)], &values(&[("x",Value::Bool(true))])).result.unwrap_err().code,"TYPE_ERROR");
}
#[test]
fn schema_rejects_duplicate_invalid_reserved_and_oversized_names() {
    let bad = vec![vec![spec("a","x",InputType::Int),spec("a","y",InputType::Int)],
        vec![spec("a","x",InputType::Int),spec("b","x",InputType::Int)], vec![spec("","x",InputType::Int)],
        vec![spec("a b","x",InputType::Int)],vec![spec("a","if",InputType::Int)],vec![spec("a","1x",InputType::Int)],
        vec![spec("a","x+y",InputType::Int)],vec![spec(&"a".repeat(129),"x",InputType::Int)],
        vec![spec("a",&"x".repeat(129),InputType::Int)]];
    for inputs in bad { assert_eq!(compile_with_inputs("0",&inputs).unwrap_err().code,"INPUT_SCHEMA"); }
    let inputs: Vec<_>=(0..=MAX_INPUTS).map(|i|spec(&format!("id{i}"),&format!("x{i}"),InputType::Int)).collect();
    assert_eq!(compile_with_inputs("0",&inputs).unwrap_err().code,"INPUT_SCHEMA");
}
#[test]
fn unicode_and_surrogate_values_remain_exact() {
    let units=vec![0xd800,0x41,0xdc00]; let bindings=values(&[("id",Value::Text(units.clone()))]);
    let p=compile_with_inputs("// 😀\r\nword",&[spec("id","word",InputType::String)]).unwrap();
    let out=evaluate_bound(&p,&bindings,Limits::default()); assert_eq!(out.result,Ok(Value::Text(units)));
    let r=&p.input_references()[0]; assert_eq!(r.id,"id"); assert_eq!(r.span.start,7); assert_eq!(r.span.line,2); assert_eq!(r.span.column,1);
    assert_eq!(out.trace[0].span,r.span);
}
#[test]
fn cumulative_input_text_budget_includes_unused_inputs() {
    let specs=[spec("a","a",InputType::String),spec("b","b",InputType::String)];
    let mut bindings=values(&[("a",Value::Text(vec![65;MAX_INPUT_TEXT_UNITS])),("b",Value::Text(vec![]))]);
    assert_eq!(run("true",&specs,&bindings).result,Ok(Value::Bool(true)));
    bindings.insert("b".into(),Value::Text(vec![65]));
    assert_eq!(run("true",&specs,&bindings).result.unwrap_err().code,"INPUT_LIMIT");
}
#[test]
fn input_reads_respect_branching_streaming_and_cancellation() {
    let p=compile_with_inputs("if flag then x else y",&[spec("f","flag",InputType::Bool),spec("x","x",InputType::Int),spec("y","y",InputType::Int)]).unwrap();
    let bindings=values(&[("f",Value::Bool(true)),("x",Value::Int(4)),("y",Value::Int(9))]);
    let mut events=Vec::new();
    let out=evaluate_bound_observed(&p,&bindings,Limits::default(),ObservationOptions{retain_trace:false,..ObservationOptions::default()},&CancellationToken::new(),|e|{events.push(e.clone());ControlFlow::Continue(())});
    assert_eq!(out.outcome.result,Ok(Value::Int(4))); assert!(out.outcome.trace.is_empty());
    assert_eq!(events.iter().filter(|e|e.kind=="input").count(),2); assert_eq!(out.emitted_events,events.len());
    let cancelled=evaluate_bound_observed(&p,&bindings,Limits::default(),ObservationOptions::default(),&CancellationToken::new(), |_|ControlFlow::Break(()));
    assert_eq!(cancelled.outcome.result.unwrap_err().code,"CANCELLED"); assert_eq!(cancelled.emitted_events,1);
}
#[test]
fn string_input_reads_obey_observation_payload_budget() {
    let p=compile_with_inputs("s == s",&[spec("s","s",InputType::String)]).unwrap();
    let out=evaluate_bound_observed(&p,&values(&[("s",Value::Text(vec![65,66]))]),Limits::default(),
        ObservationOptions{max_trace_text_units:3,..ObservationOptions::default()},&CancellationToken::new(), |_|ControlFlow::Continue(()));
    assert_eq!(out.outcome.result.unwrap_err().code,"TRACE_VALUE_LIMIT"); assert_eq!(out.emitted_events,1);
}
#[test]
fn renamed_reordered_schemas_preserve_logic_but_reidentified_inputs_do_not() {
    let a=compile_with_inputs("x + y",&[spec("a","x",InputType::Int),spec("b","y",InputType::Int)]).unwrap();
    let b=compile_with_inputs(" renamed + y // unchanged",&[spec("b","y",InputType::Int),spec("a","renamed",InputType::Int)]).unwrap();
    let c=compile_with_inputs("x + y",&[spec("different","x",InputType::Int),spec("b","y",InputType::Int)]).unwrap();
    assert!(a.same_logic(&b)); assert!(!a.same_logic(&c));
    assert!(compile_expression("true && false").unwrap().same_logic(&compile_expression("true and false").unwrap()));
}
#[test]
fn literal_input_parser_does_not_evaluate_computations() {
    assert_eq!(parse_input_literal("-4").unwrap(),Value::Int(-4));
    assert_eq!(parse_input_literal("\"\\ud800\"").unwrap(),Value::Text(vec![0xd800]));
    for text in ["1+2","if true then 1 else 2","--1","x","null","[1]"] { assert!(parse_input_literal(text).is_err(),"{text}"); }
}
#[test]
fn exhaustive_inventory_bound_policy_checks_3060_inputs() {
    let p=compile_with_inputs("reserved + quantity <= onHand",&[spec("t","onHand",InputType::NonNegativeInt),
        spec("r","reserved",InputType::NonNegativeInt),spec("q","quantity",InputType::PositiveInt)]).unwrap();
    let mut count=0;
    for on_hand in 0..=16 { for reserved in 0..=on_hand { for quantity in 1..=20 {
        let out=evaluate_bound(&p,&values(&[("t",Value::Int(on_hand)),("r",Value::Int(reserved)),("q",Value::Int(quantity))]),Limits::default());
        assert_eq!(out.result,Ok(Value::Bool(quantity<=on_hand-reserved))); count+=1;
    }}}
    assert_eq!(count,3060);
}
#[test]
fn malformed_calls_and_undeclared_identifiers_do_not_fall_back_to_another_engine() {
    let inputs=[spec("x","x",InputType::Int)];
    for source in ["x(1)","x.y","unknown", "module Foo"] { assert!(compile_with_inputs(source,&inputs).is_err()); }
    assert!(compile_expression("x").is_err());
}
#[test]
fn caller_mutations_do_not_change_compiled_input_schema() {
    let mut specs=vec![spec("a","x",InputType::Int)];
    let p=compile_with_inputs("x",&specs).unwrap(); specs[0].id="changed".into();
    assert_eq!(p.inputs()[0].id,"a");
}
