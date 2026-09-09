use cannon_core::{InputBindings, InputSpec, InputType, Limits, ObservationOptions, Value, MAX_INPUT_TEXT_UNITS, MAX_INT};
use cannon_core::session::{Author, CheckState, ExpressionSession, HistoricalPolicy, Phase, SessionError};

fn schema() -> Vec<InputSpec> { vec![InputSpec::new("quantity.id", "quantity", InputType::NonNegativeInt)] }
fn values(n: i64) -> InputBindings { InputBindings::from([("quantity.id".into(), Value::Int(n))]) }
fn document() -> ExpressionSession {
    ExpressionSession::new_with_inputs("memory:typed-policy", "quantity <= 10", &schema(), &values(10), Some(Value::Bool(true))).unwrap()
}
fn execute(s: &ExpressionSession) -> cannon_core::session::RunReceipt {
    s.prepare_run(Limits::default(), ObservationOptions::default()).unwrap().execute()
}

#[test]
fn typed_snapshot_freezes_schema_data_and_expected_value() {
    let s = document();
    let r = execute(&s);
    assert_eq!(r.snapshot().inputs(), schema());
    assert_eq!(r.bindings(), &values(10));
    assert_eq!(r.expected(), Some(&Value::Bool(true)));
    assert_eq!(r.check(), CheckState::Passed);
    assert!(s.receipt_is_current(&r));
    assert!(r.outcome().trace.iter().any(|t| t.kind == "input" && t.value == Some(Value::Int(10))));
}

#[test]
fn text_edit_preserves_typed_inputs_and_does_not_change_the_answer() {
    let mut s = document();
    s.edit(&s.snapshot(), "quantity < 10").unwrap();
    assert_eq!(s.snapshot().inputs(), schema());
    assert_eq!(s.snapshot().bindings(), &values(10));
    assert_eq!(execute(&s).check(), CheckState::Mismatch);
}

#[test]
fn input_edit_invalidates_receipts_and_cancels_queued_work() {
    let mut s = document();
    let old = execute(&s);
    let queued = s.prepare_run(Limits::default(), ObservationOptions::default()).unwrap();
    s.revise_inputs(&s.snapshot(), &schema(), &values(11)).unwrap();
    assert!(!s.receipt_is_current(&old));
    assert_eq!(old.check(), CheckState::Passed);
    assert_eq!(queued.execute().outcome().result.as_ref().unwrap_err().code, "CANCELLED");
    assert_eq!(s.snapshot().source(), "quantity <= 10");
    assert_eq!(execute(&s).check(), CheckState::Mismatch);
}

#[test]
fn changed_candidate_data_cannot_hide_the_original_input_regression() {
    let mut s = document();
    let base = s.snapshot();
    let mut draft = base.draft();
    draft.source = "quantity < 10".into();
    draft.bindings = values(9);
    let review = s.propose_draft(&base, &draft, "Tighten policy and change the example", Author::Assistant).unwrap().preview(Limits::default());
    assert_eq!(review.candidate().check(), CheckState::Passed);
    assert_eq!(review.candidate().bindings(), &values(9));
    assert_eq!(review.historical().bindings(), &values(10));
    assert_eq!(review.historical().snapshot().bindings(), &values(9));
    assert_eq!(review.historical_check(), CheckState::Mismatch);
    assert!(review.introduces_regression());
    assert!(review.facts().input_values_changed);
    assert!(!review.facts().expected_value_changed);
    assert_eq!(s.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::HistoricalExpectationFailed);
    let applied = s.apply(&review, HistoricalPolicy::AcknowledgeChange).unwrap();
    assert_eq!(applied.bindings(), &values(9));
    assert!(!s.receipt_is_current(review.candidate()));
    assert!(!s.receipt_is_current(review.historical()));
    assert_eq!(execute(&s).check(), CheckState::Passed);
}

#[test]
fn renamed_input_with_same_id_replays_historical_data() {
    let mut s = document();
    let base = s.snapshot();
    let mut draft = base.draft();
    draft.inputs[0].name = "count".into();
    draft.source = "count <= 10".into();
    let review = s.propose_draft(&base, &draft, "Rename only", Author::Human).unwrap().preview(Limits::default());
    assert_eq!(review.historical_check(), CheckState::Passed);
    assert!(review.facts().input_schema_changed);
    assert!(!review.facts().input_values_changed);
    s.apply(&review, HistoricalPolicy::Preserve).unwrap();
    assert_eq!(execute(&s).outcome().result, Ok(Value::Bool(true)));
}

#[test]
fn identity_changes_are_not_silently_remapped_by_name() {
    let mut s = document();
    let base = s.snapshot();
    let mut draft = base.draft();
    draft.inputs[0].id = "new.id".into();
    draft.bindings = InputBindings::from([("new.id".into(), Value::Int(10))]);
    let review = s.propose_draft(&base, &draft, "Explicit identity change", Author::Human).unwrap().preview(Limits::default());
    assert_eq!(review.candidate().check(), CheckState::Passed);
    assert_eq!(review.historical().outcome().result.as_ref().unwrap_err().code, "UNKNOWN_INPUT");
    assert_eq!(review.historical_check(), CheckState::ExecutionError);
    assert_eq!(s.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::HistoricalExpectationFailed);
}

#[test]
fn narrowed_refinement_checks_original_not_replacement_data() {
    let mut s = ExpressionSession::new_with_inputs("memory:refinement", "quantity >= 0", &schema(), &values(0), Some(Value::Bool(true))).unwrap();
    let base = s.snapshot();
    let mut draft = base.draft();
    draft.inputs[0].input_type = InputType::PositiveInt;
    draft.bindings = values(1);
    let review = s.propose_draft(&base, &draft, "Require a positive quantity", Author::Assistant).unwrap().preview(Limits::default());
    assert_eq!(review.candidate().check(), CheckState::Passed);
    assert_eq!(review.historical().outcome().result.as_ref().unwrap_err().code, "REFINEMENT_VIOLATION");
    assert_eq!(review.historical().outcome().steps, 0);
    assert!(review.historical().outcome().trace.is_empty());
    assert_eq!(s.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::HistoricalExpectationFailed);
}

#[test]
fn newly_required_unused_inputs_still_fail_historical_replay() {
    let s = document();
    let base = s.snapshot();
    let mut draft = base.draft();
    draft.inputs.push(InputSpec::new("extra.id", "unused", InputType::Bool));
    draft.bindings.insert("extra.id".into(), Value::Bool(true));
    let review = s.propose_draft(&base, &draft, "Add a required input", Author::Human).unwrap().preview(Limits::default());
    assert_eq!(review.candidate().check(), CheckState::Passed);
    assert_eq!(review.historical().outcome().result.as_ref().unwrap_err().code, "MISSING_INPUT");
    assert!(review.introduces_regression());
}

#[test]
fn invalid_binding_is_inspectable_and_cannot_be_acknowledged_away() {
    let mut s = document();
    let base = s.snapshot();
    let mut draft = base.draft();
    draft.bindings.insert("quantity.id".into(), Value::Bool(true));
    let review = s.propose_draft(&base, &draft, "Invalid input example", Author::Assistant).unwrap().preview(Limits::default());
    assert_eq!(review.candidate().phase(), Phase::Execution);
    assert_eq!(review.candidate().outcome().result.as_ref().unwrap_err().code, "TYPE_ERROR");
    assert_eq!(s.apply(&review, HistoricalPolicy::AcknowledgeChange).unwrap_err(), SessionError::CandidateExecutionFailed);
    assert!(s.is_current(&base));
}

#[test]
fn overlarge_input_payload_does_not_mutate_or_cancel_valid_work() {
    let mut s = document();
    let base = s.snapshot();
    let queued = s.prepare_run(Limits::default(), ObservationOptions::default()).unwrap();
    let too_large = InputBindings::from([("quantity.id".into(), Value::Text(vec![65; MAX_INPUT_TEXT_UNITS + 1]))]);
    assert_eq!(s.revise_inputs(&base, &schema(), &too_large).unwrap_err(), SessionError::InvalidInputPayload);
    let too_long = [InputSpec::new("x".repeat(129), "quantity", InputType::Int)];
    assert_eq!(s.revise_inputs(&base, &too_long, &values(10)).unwrap_err(), SessionError::InvalidInputPayload);
    assert!(s.is_current(&base));
    assert_eq!(queued.execute().check(), CheckState::Passed);
}

#[test]
fn bounded_invalid_schema_is_not_silently_replaced_by_last_good_program() {
    let mut s = document();
    let mut invalid = schema();
    invalid.push(InputSpec::new("quantity.id", "duplicate", InputType::Int));
    s.revise_inputs(&s.snapshot(), &invalid, &values(10)).unwrap();
    assert_eq!(s.snapshot().diagnostic().unwrap().code, "INPUT_SCHEMA");
    assert_eq!(execute(&s).phase(), Phase::Compilation);
    s.revise_inputs(&s.snapshot(), &schema(), &values(10)).unwrap();
    assert_eq!(execute(&s).check(), CheckState::Passed);
}

#[test]
fn modifying_draft_after_proposal_does_not_change_reviewed_data() {
    let s = document();
    let base = s.snapshot();
    let mut draft = base.draft();
    let proposal = s.propose_draft(&base, &draft, "Same policy", Author::Human).unwrap();
    draft.source = "false".into();
    draft.bindings.clear();
    draft.inputs.clear();
    draft.expected = Some(Value::Bool(false));
    let review = proposal.preview(Limits::default());
    assert_eq!(review.candidate().bindings(), &values(10));
    assert_eq!(review.candidate().snapshot().source(), "quantity <= 10");
    assert_eq!(review.candidate().check(), CheckState::Passed);
}

#[test]
fn historical_receipt_retains_its_own_expected_value_and_source() {
    let s = document();
    let review = s.propose(&s.snapshot(), "quantity < 10", Some(Value::Bool(false)), "Keep one", Author::Human).unwrap().preview(Limits::default());
    assert_eq!(review.candidate().expected(), Some(&Value::Bool(false)));
    assert_eq!(review.historical().expected(), Some(&Value::Bool(true)));
    assert_eq!(review.historical().snapshot().source(), "quantity < 10");
    assert_eq!(review.historical().check(), CheckState::Mismatch);
    assert_eq!(review.historical().outcome().trace.last().unwrap().label, "<");
}

#[test]
fn preview_observation_limits_cannot_be_hidden_by_unretained_traces() {
    let mut s = document();
    let proposal = s.propose(&s.snapshot(), "quantity <= 10", Some(Value::Bool(true)), "Limited check", Author::Human).unwrap();
    let review = proposal.preview_with_options(Limits { max_trace: 1, ..Limits::default() }, ObservationOptions { retain_trace: false, ..ObservationOptions::default() });
    assert_eq!(review.candidate().outcome().result.as_ref().unwrap_err().code, "TRACE_LIMIT");
    assert_eq!(review.candidate().observation().emitted_events, 1);
    assert!(review.candidate().outcome().trace.is_empty());
    assert_eq!(s.apply(&review, HistoricalPolicy::AcknowledgeChange).unwrap_err(), SessionError::CandidateExecutionFailed);
}

#[test]
fn out_of_domain_input_values_are_execution_errors_not_rounded_or_repaired() {
    let s = ExpressionSession::new_with_inputs("memory:range", "quantity", &schema(), &values(MAX_INT + 1), None).unwrap();
    assert_eq!(execute(&s).outcome().result.as_ref().unwrap_err().code, "INTEGER_RANGE");
}

#[test]
fn closed_sessions_refuse_schema_and_data_changes() {
    let mut s = document();
    let base = s.snapshot();
    s.close();
    assert_eq!(s.revise_inputs(&base, &schema(), &values(9)).unwrap_err(), SessionError::Closed);
    assert_eq!(s.propose_draft(&base, &base.draft(), "Closed proposal", Author::Human).unwrap_err(), SessionError::Closed);
}
