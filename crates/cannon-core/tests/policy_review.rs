use cannon_core::*;
use cannon_core::policy_review::*;
fn program(source: &str, name: &str, ty: InputType) -> CompiledExpression {
    compile_with_inputs(source,&[InputSpec::new("quantity",name,ty)]).unwrap()
}
fn case(n: i64, expected: bool) -> PolicyCase {
    PolicyCase { id:"boundary".into(),name:"Exact boundary".into(),
        inputs:InputBindings::from([("quantity".into(),Value::Int(n))]),expected:Value::Bool(expected) }
}
fn review(a:&CompiledExpression,b:&CompiledExpression,old:&[PolicyCase],new:&[PolicyCase])->PolicyReview {
    review_policies(a,b,old,new,ReviewLimits::default()).unwrap()
}
#[test]
fn changing_expected_answer_does_not_hide_historical_regression() {
    let a=program("x <= 10","x",InputType::Int); let b=program("x < 10","x",InputType::Int);
    let r=review(&a,&b,&[case(10,true)],&[case(10,false)]);
    assert_eq!(PolicyReview::status(&r.candidate_results),SuiteStatus::Passed);
    assert_eq!(r.regressions(),["boundary"]); assert!(!r.passed()); assert!(r.logic_changed);
    assert!(r.case_changes.iter().any(|c|c.kind=="expectation-changed"));
    assert_eq!(r.historical_results[0].case.expected,Value::Bool(true));
}
#[test]
fn deleted_cases_still_replay_and_empty_candidate_is_not_success() {
    let a=program("x <= 10","x",InputType::Int); let b=program("x < 10","x",InputType::Int);
    let r=review(&a,&b,&[case(10,true)],&[]);
    assert_eq!(r.historical_results.len(),1); assert_eq!(r.regressions(),["boundary"]);
    assert_eq!(PolicyReview::status(&r.candidate_results),SuiteStatus::NoCases);
    assert!(r.case_changes.iter().any(|c|c.kind=="removed")); assert!(!r.passed());
}
#[test]
fn changed_inputs_do_not_replace_the_historical_inputs() {
    let a=program("x <= 10","x",InputType::Int); let b=program("x < 10","x",InputType::Int);
    let r=review(&a,&b,&[case(10,true)],&[case(9,true)]);
    assert!(r.candidate_results[0].passed); assert!(!r.historical_results[0].passed);
    assert!(r.case_changes.iter().any(|c|c.kind=="input-changed"));
}
#[test]
fn stable_rename_preserves_replay_and_is_separate_from_logic() {
    let a=program("x <= 10","x",InputType::Int); let b=program("count <= 10","count",InputType::Int);
    let r=review(&a,&b,&[case(10,true)],&[case(10,true)]);
    assert!(r.passed()); assert!(!r.logic_changed); assert_eq!(r.input_changes[0].kind,"renamed");
    assert_eq!(r.baseline.source,"x <= 10"); assert_eq!(r.candidate.source,"count <= 10");
    assert_eq!(r.baseline_results[0].outcome.trace[0].span.end,1);
    assert_eq!(r.historical_results[0].outcome.trace[0].span.end,5);
}
#[test]
fn replacing_an_id_does_not_infer_a_mapping_from_the_old_name() {
    let a=program("x <= 10","x",InputType::Int);
    let b=compile_with_inputs("x <= 10",&[InputSpec::new("different","x",InputType::Int)]).unwrap();
    let r=review(&a,&b,&[case(10,true)],&[]);
    assert_eq!(r.historical_results[0].outcome.result.as_ref().unwrap_err().code,"UNKNOWN_INPUT");
    assert!(r.logic_changed); assert_eq!(r.input_changes.len(),2);
}
#[test]
fn refining_an_input_can_break_old_data_even_if_logic_is_identical() {
    let a=program("x <= 10","x",InputType::Int); let b=program("x <= 10","x",InputType::PositiveInt);
    let r=review(&a,&b,&[case(0,true)],&[case(1,true)]);
    assert!(!r.logic_changed); assert_eq!(r.input_changes[0].kind,"type-changed");
    assert_eq!(r.historical_results[0].outcome.result.as_ref().unwrap_err().code,"REFINEMENT_VIOLATION");
}
#[test]
fn no_examples_is_not_a_successful_review() {
    let a=program("true","x",InputType::Int); let r=review(&a,&a,&[],&[]);
    assert!(!r.passed()); assert_eq!(PolicyReview::status(&r.baseline_results),SuiteStatus::NoCases);
}
#[test]
fn a_preexisting_baseline_failure_is_visible_but_not_a_new_regression() {
    let a=program("false","x",InputType::Int); let r=review(&a,&a,&[case(2,true)],&[case(2,true)]);
    assert!(!r.passed()); assert!(r.regressions().is_empty()); assert!(!r.baseline_results[0].passed);
}
#[test]
fn duplicate_or_invalid_case_metadata_is_rejected() {
    let a=program("true","x",InputType::Int);
    let c=case(2,true);
    assert_eq!(review_policies(&a,&a,&[c.clone(),c],&[],ReviewLimits::default()).unwrap_err().code,"CASE_SCHEMA");
    let mut invalid=case(2,true); invalid.id="".into();
    assert!(review_policies(&a,&a,&[invalid],&[],ReviewLimits::default()).is_err());
    let mut invalid=case(2,true); invalid.expected=Value::Int(i64::MAX);
    assert!(review_policies(&a,&a,&[invalid],&[],ReviewLimits::default()).is_err());
    assert!(review_policies(&a,&a,&vec![case(2,true);129],&[],ReviewLimits::default()).is_err());
}
#[test]
fn cumulative_budgets_cannot_return_a_partial_successful_report() {
    let a=program("x <= 10","x",InputType::Int);
    let limits=ReviewLimits{max_events:8,..ReviewLimits::default()};
    assert_eq!(review_policies(&a,&a,&[case(2,true)],&[case(2,true)],limits).unwrap_err().code,"REVIEW_LIMIT");
    let limits=ReviewLimits{max_events:9,..ReviewLimits::default()};
    assert!(review_policies(&a,&a,&[case(2,true)],&[case(2,true)],limits).unwrap().passed());
}
#[test]
fn cumulative_text_budget_is_not_reset_between_suites() {
    let a=compile_with_inputs("s",&[InputSpec::new("s","s",InputType::String)]).unwrap();
    let c=PolicyCase{id:"s".into(),name:"Text".into(),inputs:InputBindings::from([("s".into(),Value::Text(vec![65]))]),expected:Value::Text(vec![65])};
    let limits=ReviewLimits{max_text_units:2,..ReviewLimits::default()};
    assert_eq!(review_policies(&a,&a,&[c.clone()],&[c],limits).unwrap_err().code,"REVIEW_LIMIT");
}
#[test]
fn execution_errors_never_match_a_value_expectation() {
    let a=program("x + 1 > 0","x",InputType::Int);
    let r=review(&a,&a,&[case(MAX_INT,true)],&[case(MAX_INT,true)]);
    assert!(!r.passed()); assert_eq!(r.historical_results[0].outcome.result.as_ref().unwrap_err().code,"INTEGER_RANGE");
}
#[test]
fn invalid_limits_are_rejected_even_for_empty_suites() {
    let a=program("true","x",InputType::Int);
    let limits=ReviewLimits{max_events:0,..ReviewLimits::default()};
    assert_eq!(review_policies(&a,&a,&[],&[],limits).unwrap_err().code,"INVALID_LIMIT");
}
#[test]
fn report_is_deterministic_and_embeds_both_sources_and_expectations() {
    let a=program("x <= 10","x",InputType::Int); let b=program("x < 10","x",InputType::Int);
    let one=review(&a,&b,&[case(10,true)],&[case(10,false)]);
    let two=review(&a,&b,&[case(10,true)],&[case(10,false)]);
    let text=bound_report::render_review(&one);
    assert_eq!(text,bound_report::render_review(&two));
    for part in ["policy-review/1","baseline.policy","candidate.policy","expectation-changed","historicalResults","quantity"] { assert!(text.contains(part),"{part}"); }
    assert!(one.baseline.compile().unwrap().same_logic(&a));
}
