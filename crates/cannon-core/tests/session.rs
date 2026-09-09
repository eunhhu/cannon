use std::ops::ControlFlow;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use cannon_core::{CancellationToken, InputBindings, InputSpec, InputType, Limits,
    ObservationOptions, Value, MAX_INT, MAX_SOURCE_UNITS};
use cannon_core::policy_review::{review_policies_cancellable, PolicyCase, PolicyReview, ReviewLimits, SuiteStatus};
use cannon_core::session::{Author, CheckState, HistoricalPolicy, Phase, PolicyDocument,
    PolicySession, RunReceipt, SessionError};

fn case(id: &str, expected: Value) -> PolicyCase {
    PolicyCase { id: id.into(), name: id.into(), inputs: InputBindings::new(), expected }
}
fn document(source: &str, expected: Option<Value>) -> PolicyDocument {
    PolicyDocument { source: source.into(), inputs: vec![], cases: expected.into_iter().map(|v| case("case", v)).collect() }
}
fn session(source: &str, expected: Value) -> PolicySession {
    PolicySession::new("memory:policy", document(source, Some(expected))).unwrap()
}
fn run(s: &PolicySession, id: &str) -> RunReceipt {
    s.prepare_case(id, Limits::default(), ObservationOptions::default()).unwrap().execute()
}
fn inventory() -> PolicyDocument {
    let inputs = vec![
        InputSpec::new("stock.total", "onHand", InputType::NonNegativeInt),
        InputSpec::new("stock.held", "reserved", InputType::NonNegativeInt),
        InputSpec::new("quantity", "quantity", InputType::PositiveInt),
    ];
    let cases = [("exact", 2), ("spare", 1)].into_iter().map(|(id, quantity)| PolicyCase {
        id: id.into(), name: id.into(), inputs: InputBindings::from([
            ("stock.total".into(), Value::Int(10)), ("stock.held".into(), Value::Int(8)),
            ("quantity".into(), Value::Int(quantity)),
        ]), expected: Value::Bool(true),
    }).collect();
    PolicyDocument { source: "reserved + quantity <= onHand".into(), inputs, cases }
}
fn changed_inventory() -> PolicyDocument {
    let mut d = inventory();
    d.source = "reserved + quantity < onHand".into();
    d.cases[0].expected = Value::Bool(false);
    d
}
fn propose(s: &PolicySession, d: PolicyDocument) -> cannon_core::session::ChangeReview {
    s.propose(&s.snapshot(), d, "explicit test proposal", Author::Human).unwrap().preview(ReviewLimits::default())
}

#[test]
fn receipt_keeps_exact_document_case_settings_and_observations() {
    let s = PolicySession::new("memory:재고", inventory()).unwrap();
    let r = run(&s, "exact");
    assert_eq!(r.snapshot().uri(), "memory:재고");
    assert_eq!(r.snapshot().revision(), 1);
    assert_eq!(r.snapshot().source(), "reserved + quantity <= onHand");
    assert_eq!(r.snapshot().inputs(), inventory().inputs.as_slice());
    assert_eq!(r.case().inputs["stock.total"], Value::Int(10));
    assert_eq!(r.case().expected, Value::Bool(true));
    assert_eq!(r.phase(), Phase::Execution);
    assert_eq!(r.limits().max_steps, Limits::default().max_steps);
    assert_eq!(r.check(), CheckState::Passed);
    assert!(s.receipt_is_current(&r));
    assert!(r.outcome().trace.iter().any(|e| e.kind == "binary" && e.value == Some(Value::Bool(true))));
}
#[test]
fn source_edits_do_not_rewrite_inputs_or_expected_answers() {
    let mut s = PolicySession::new("memory:inventory", inventory()).unwrap();
    s.edit_source(&s.snapshot(), "reserved + quantity < onHand").unwrap();
    assert_eq!(s.snapshot().inputs(), inventory().inputs.as_slice());
    assert_eq!(run(&s, "exact").check(), CheckState::Mismatch);
    assert_eq!(run(&s, "spare").check(), CheckState::Passed);
}
#[test]
fn input_renames_keep_ids_data_and_historical_replay() {
    let mut s = PolicySession::new("memory:inventory", inventory()).unwrap();
    let mut d = inventory();
    d.inputs[0].name = "total".into();
    d.source = "reserved + quantity <= total".into();
    let review = propose(&s, d);
    assert!(review.report().unwrap().passed());
    assert!(!review.report().unwrap().logic_changed);
    assert!(review.report().unwrap().input_changes.iter().any(|c| c.id == "stock.total" && c.kind == "renamed"));
    s.apply(&review, HistoricalPolicy::Preserve).unwrap();
    assert_eq!(run(&s, "exact").check(), CheckState::Passed);
}
#[test]
fn direct_schema_edit_is_atomic_and_preserves_cases() {
    let mut s = PolicySession::new("memory:inventory", inventory()).unwrap();
    let mut specs = inventory().inputs;
    specs[0].name = "total".into();
    s.edit_policy(&s.snapshot(), "reserved + quantity <= total", &specs).unwrap();
    assert_eq!(run(&s, "exact").check(), CheckState::Passed);
    assert_eq!(s.snapshot().cases()[0].expected, Value::Bool(true));
}
#[test]
fn completed_receipt_becomes_historical_without_losing_its_result() {
    let mut s = session("1 + 1", Value::Int(2));
    let old = run(&s, "case");
    s.edit_source(&s.snapshot(), "1 + 2").unwrap();
    assert!(!s.receipt_is_current(&old));
    assert_eq!(old.outcome().result, Ok(Value::Int(2)));
    assert_eq!(old.snapshot().source(), "1 + 1");
}
#[test]
fn queued_execution_is_cancelled_after_an_edit() {
    let mut s = session("1 + 1", Value::Int(2));
    let request = s.prepare_case("case", Limits::default(), ObservationOptions::default()).unwrap();
    s.edit_source(&s.snapshot(), "1 + 2").unwrap();
    let old = request.execute();
    assert_eq!(old.outcome().result.as_ref().unwrap_err().code, "CANCELLED");
    assert_eq!(old.outcome().steps, 0);
    assert!(!s.receipt_is_current(&old));
}
#[test]
fn editing_during_worker_observation_cancels_the_exact_old_revision() {
    let mut s = session("1 + 2 + 3", Value::Int(6));
    let request = s.prepare_case("case", Limits::default(), ObservationOptions::default()).unwrap();
    let (seen_tx, seen_rx) = mpsc::sync_channel(0);
    let (resume_tx, resume_rx) = mpsc::sync_channel(0);
    let worker = thread::spawn(move || request.execute_observed(|origin, _| {
        assert_eq!(origin.source(), "1 + 2 + 3");
        seen_tx.send(()).unwrap();
        resume_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        ControlFlow::Continue(())
    }));
    seen_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    s.edit_source(&s.snapshot(), "3 + 3").unwrap();
    resume_tx.send(()).unwrap();
    let old = worker.join().unwrap();
    assert_eq!(old.outcome().result.as_ref().unwrap_err().code, "CANCELLED");
    assert_eq!(old.observation().emitted_events, 1);
    assert!(!s.receipt_is_current(&old));
    assert_eq!(run(&s, "case").check(), CheckState::Passed);
}
#[test]
fn tickets_disambiguate_same_revision_runs_with_different_limits() {
    let s = session("1 + 2", Value::Int(3));
    let a = s.prepare_case("case", Limits::default(), ObservationOptions::default()).unwrap();
    let b = s.prepare_case("case", Limits { max_steps: 1, ..Limits::default() }, ObservationOptions::default()).unwrap();
    let ta = a.ticket(); let tb = b.ticket();
    let rb = b.execute(); let ra = a.execute();
    assert!(ta.matches(&ra)); assert!(!ta.matches(&rb));
    assert!(tb.matches(&rb)); assert!(!tb.matches(&ra));
    assert!(s.receipt_is_current(&ra)); assert!(s.receipt_is_current(&rb));
    assert_eq!(ra.check(), CheckState::Passed); assert_eq!(rb.check(), CheckState::ExecutionError);
}
#[test]
fn identical_text_and_aba_edits_never_resurrect_old_handles() {
    let mut s = session("1", Value::Int(1));
    let old = s.snapshot();
    s.edit_source(&old, "1").unwrap();
    assert_eq!(s.edit_source(&old, "2").unwrap_err(), SessionError::StaleBase);
    s.edit_source(&s.snapshot(), "2").unwrap();
    s.edit_source(&s.snapshot(), "1").unwrap();
    assert!(!s.is_current(&old));
    assert_eq!(s.snapshot().revision(), 4);
}
#[test]
fn same_uri_source_and_revision_in_different_sessions_are_not_identity() {
    let first = session("1", Value::Int(1));
    let mut second = session("1", Value::Int(1));
    assert_eq!(first.snapshot().revision(), second.snapshot().revision());
    assert!(!second.is_current(&first.snapshot()));
    assert_eq!(second.edit_source(&first.snapshot(), "2").unwrap_err(), SessionError::StaleBase);
    let review = propose(&first, document("1", Some(Value::Int(1))));
    assert_eq!(second.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::StaleBase);
}
#[test]
fn incomplete_source_retains_diagnostic_and_can_be_repaired() {
    let mut s = session("1 +", Value::Int(2));
    assert!(s.snapshot().diagnostic().is_some());
    assert_eq!(run(&s, "case").phase(), Phase::Compilation);
    assert_eq!(run(&s, "case").check(), CheckState::CompilationError);
    s.edit_source(&s.snapshot(), "1 + 1").unwrap();
    assert_eq!(run(&s, "case").check(), CheckState::Passed);
}
#[test]
fn invalid_bindings_are_execution_errors_not_assertion_mismatches() {
    let mut d = inventory(); d.cases[0].inputs.remove("quantity");
    let s = PolicySession::new("memory:x", d).unwrap();
    let r = run(&s, "exact");
    assert_eq!(r.check(), CheckState::ExecutionError);
    assert_eq!(r.outcome().result.as_ref().unwrap_err().code, "MISSING_INPUT");
    assert_eq!(r.outcome().steps, 0);
}
#[test]
fn invalid_limits_are_not_current_success_even_for_a_current_revision() {
    let s = session("1", Value::Int(1));
    let r = s.prepare_case("case", Limits { max_steps: 0, ..Limits::default() }, ObservationOptions::default()).unwrap().execute();
    assert_eq!(r.outcome().result.as_ref().unwrap_err().code, "INVALID_LIMIT");
    assert!(s.receipt_is_current(&r));
    assert_eq!(r.check(), CheckState::ExecutionError);
}
#[test]
fn streaming_receipt_reports_delivery_and_retention_separately() {
    let s = session("1 + 2", Value::Int(3));
    let options = ObservationOptions { retain_trace: false, ..ObservationOptions::default() };
    let mut events = Vec::new();
    let r = s.prepare_case("case", Limits::default(), options).unwrap().execute_observed(|_, event| {
        events.push(event.clone()); ControlFlow::Continue(())
    });
    assert_eq!(events.len(), 3);
    assert_eq!(r.observation().emitted_events, 3);
    assert!(r.outcome().trace.is_empty());
    assert!(!r.observation().options.retain_trace);
    assert_eq!(r.check(), CheckState::Passed);
}
#[test]
fn observer_can_cancel_one_request_without_poisoning_the_revision() {
    let s = session("1 + 2", Value::Int(3));
    let r = s.prepare_case("case", Limits::default(), ObservationOptions::default()).unwrap()
        .execute_observed(|_, _| ControlFlow::Break(()));
    assert_eq!(r.outcome().result.as_ref().unwrap_err().code, "CANCELLED");
    assert_eq!(run(&s, "case").check(), CheckState::Passed);
}
#[test]
fn rejected_source_edit_preserves_currentness_and_pending_work() {
    let mut s = session("1", Value::Int(1));
    let base = s.snapshot();
    let pending = s.prepare_case("case", Limits::default(), ObservationOptions::default()).unwrap();
    assert_eq!(s.edit_source(&base, &" ".repeat(MAX_SOURCE_UNITS + 1)).unwrap_err(), SessionError::SourceLimit);
    assert!(s.is_current(&base));
    assert_eq!(pending.execute().check(), CheckState::Passed);
}
#[test]
fn source_budget_uses_utf16_units_not_utf8_bytes() {
    let mut s = session("1", Value::Int(1));
    let oversized = format!("//{}", "😀".repeat(MAX_SOURCE_UNITS / 2));
    assert_eq!(s.edit_source(&s.snapshot(), &oversized).unwrap_err(), SessionError::SourceLimit);
    s.edit_source(&s.snapshot(), "\"😀\"").unwrap();
    assert_eq!(run(&s, "case").outcome().result, Ok(Value::Text(vec![0xd83d, 0xde00])));
}
#[test]
fn invalid_uri_and_schema_are_rejected_before_creation() {
    assert_eq!(PolicySession::new(" ", document("1", None)).unwrap_err(), SessionError::InvalidUri);
    assert_eq!(PolicySession::new(&"x".repeat(4097), document("1", None)).unwrap_err(), SessionError::InvalidUri);
    let mut d = inventory(); d.inputs[1].id = d.inputs[0].id.clone();
    assert!(matches!(PolicySession::new("memory:x", d), Err(SessionError::InvalidMetadata(_))));
}
#[test]
fn rejected_schema_and_case_edits_do_not_invalidate_current() {
    let mut s = session("1", Value::Int(1));
    let base = s.snapshot();
    assert!(s.edit_policy(&base, "x", &[InputSpec::new("x", "if", InputType::Int)]).is_err());
    let bad = vec![case("duplicate", Value::Int(1)), case("duplicate", Value::Int(1))];
    assert!(s.revise_cases(&base, &bad).is_err());
    assert!(s.is_current(&base));
    assert_eq!(run(&s, "case").check(), CheckState::Passed);
}
#[test]
fn invalid_expected_integer_and_aggregate_case_text_are_rejected() {
    assert!(matches!(PolicySession::new("memory:x", document("1", Some(Value::Int(MAX_INT + 1)))), Err(SessionError::InvalidMetadata(_))));
    let cases = (0..5).map(|i| case(&format!("c{i}"), Value::Text(vec![65; MAX_SOURCE_UNITS]))).collect();
    let d = PolicyDocument { source: "\"x\"".into(), inputs: vec![], cases };
    assert_eq!(PolicySession::new("memory:x", d).unwrap_err(), SessionError::CaseTextLimit);
}
#[test]
fn case_edits_are_independent_and_cancel_old_requests() {
    let mut s = session("1", Value::Int(2));
    let old = s.prepare_case("case", Limits::default(), ObservationOptions::default()).unwrap();
    s.revise_cases(&s.snapshot(), &[case("case", Value::Int(1))]).unwrap();
    assert_eq!(s.snapshot().source(), "1");
    assert_eq!(old.execute().outcome().result.as_ref().unwrap_err().code, "CANCELLED");
    assert_eq!(run(&s, "case").check(), CheckState::Passed);
}
#[test]
fn unknown_cases_and_empty_documents_do_not_fabricate_a_passing_run() {
    let s = PolicySession::new("memory:x", document("1", None)).unwrap();
    assert_eq!(s.prepare_case("case", Limits::default(), ObservationOptions::default()).unwrap_err(), SessionError::UnknownCase);
    let review = propose(&s, document("1", Some(Value::Int(1))));
    assert!(!review.report().unwrap().passed());
}
#[test]
fn close_is_idempotent_cancels_pending_work_and_rejects_edits() {
    let mut s = session("1", Value::Int(1));
    let base = s.snapshot();
    let request = s.prepare_case("case", Limits::default(), ObservationOptions::default()).unwrap();
    s.close(); s.close();
    assert!(s.is_closed()); assert!(!s.is_current(&base));
    assert_eq!(s.edit_source(&base, "2").unwrap_err(), SessionError::Closed);
    assert_eq!(s.prepare_case("case", Limits::default(), ObservationOptions::default()).unwrap_err(), SessionError::Closed);
    assert_eq!(request.execute().outcome().result.as_ref().unwrap_err().code, "CANCELLED");
}
#[test]
fn dropping_owner_cancels_queued_requests_and_proposal_previews() {
    let s = session("1", Value::Int(1));
    let request = s.prepare_case("case", Limits::default(), ObservationOptions::default()).unwrap();
    let p = s.propose(&s.snapshot(), document("1", Some(Value::Int(1))), "retain", Author::Human).unwrap();
    drop(s);
    assert_eq!(request.execute().outcome().result.as_ref().unwrap_err().code, "CANCELLED");
    assert_eq!(p.preview(ReviewLimits::default()).report().unwrap_err().code, "CANCELLED");
}
#[test]
fn changed_implementation_and_expectation_still_replay_historical_case() {
    let mut s = PolicySession::new("memory:inventory", inventory()).unwrap();
    let review = propose(&s, changed_inventory());
    let report = review.report().unwrap();
    assert_eq!(PolicyReview::status(&report.candidate_results), SuiteStatus::Passed);
    assert_eq!(report.regressions(), ["exact"]);
    assert!(report.case_changes.iter().any(|c| c.id == "exact" && c.kind == "expectation-changed"));
    assert_eq!(s.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::HistoricalExpectationFailed);
    assert_eq!(run(&s, "exact").check(), CheckState::Passed);
}
#[test]
fn acknowledged_value_change_commits_exact_candidate_with_fresh_identity() {
    let mut s = PolicySession::new("memory:inventory", inventory()).unwrap();
    let old_receipt = run(&s, "exact");
    let review = propose(&s, changed_inventory());
    let staged = review.proposal().candidate().clone();
    let installed = s.apply(&review, HistoricalPolicy::AcknowledgeChangedValues).unwrap();
    assert_eq!(installed.source(), staged.source());
    assert_eq!(installed.revision(), staged.revision());
    assert!(!installed.same_revision(&staged)); assert!(!s.is_current(&staged));
    assert!(!s.receipt_is_current(&old_receipt));
    assert_eq!(run(&s, "exact").outcome().result, Ok(Value::Bool(false)));
    assert_eq!(run(&s, "exact").check(), CheckState::Passed);
    assert_eq!(review.report().unwrap().regressions(), ["exact"]);
}
#[test]
fn deleting_a_candidate_case_never_deletes_its_historical_replay() {
    let s = PolicySession::new("memory:inventory", inventory()).unwrap();
    let mut d = changed_inventory(); d.cases.remove(0);
    let review = propose(&s, d);
    let report = review.report().unwrap();
    assert_eq!(report.candidate_results.len(), 1);
    assert_eq!(report.historical_results.len(), 2);
    assert_eq!(report.regressions(), ["exact"]);
    assert!(report.case_changes.iter().any(|c| c.id == "exact" && c.kind == "removed"));
}
#[test]
fn empty_candidate_suite_cannot_be_acknowledged_into_verification() {
    let mut s = session("1", Value::Int(1));
    let review = propose(&s, document("1", None));
    assert_eq!(s.apply(&review, HistoricalPolicy::AcknowledgeChangedValues).unwrap_err(), SessionError::CandidateNotVerified);
}
#[test]
fn failed_baseline_cannot_be_hidden_by_a_passing_candidate() {
    let mut s = session("2", Value::Int(1));
    let review = propose(&s, document("1", Some(Value::Int(1))));
    assert_eq!(s.apply(&review, HistoricalPolicy::AcknowledgeChangedValues).unwrap_err(), SessionError::BaselineNotVerified);
}
#[test]
fn empty_baseline_cannot_become_a_passing_review_gate() {
    let mut s = PolicySession::new("memory:x", document("1", None)).unwrap();
    let review = propose(&s, document("1", Some(Value::Int(1))));
    assert_eq!(s.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::BaselineNotVerified);
}
#[test]
fn historical_execution_error_is_not_waived_as_a_value_change() {
    let mut d = document("x", Some(Value::Int(1)));
    d.inputs.push(InputSpec::new("x", "x", InputType::Int));
    d.cases[0].inputs.insert("x".into(), Value::Int(1));
    let mut s = PolicySession::new("memory:x", d).unwrap();
    let review = propose(&s, document("1", Some(Value::Int(1))));
    assert_eq!(review.report().unwrap().historical_results[0].outcome.result.as_ref().unwrap_err().code, "UNKNOWN_INPUT");
    assert_eq!(s.apply(&review, HistoricalPolicy::AcknowledgeChangedValues).unwrap_err(), SessionError::HistoricalExecutionFailed);
}
#[test]
fn invalid_candidate_stays_inspectable_but_is_never_applied() {
    let mut s = session("1", Value::Int(1));
    let review = propose(&s, document("1 +", Some(Value::Int(1))));
    assert!(review.proposal().candidate().diagnostic().is_some());
    assert!(review.report().is_err());
    assert_eq!(s.apply(&review, HistoricalPolicy::AcknowledgeChangedValues).unwrap_err(), SessionError::InvalidCandidate);
    assert_eq!(run(&s, "case").check(), CheckState::Passed);
}
#[test]
fn overflow_and_wrong_candidate_expectations_block_application() {
    let mut s = session("1", Value::Int(1));
    for d in [document("9007199254740991 + 1", Some(Value::Int(1))), document("2", Some(Value::Int(1)))] {
        let review = propose(&s, d);
        assert_eq!(s.apply(&review, HistoricalPolicy::AcknowledgeChangedValues).unwrap_err(), SessionError::CandidateNotVerified);
    }
}
#[test]
fn an_incomplete_or_invalid_limit_review_cannot_authorize_application() {
    let mut s = session("1 + 1", Value::Int(2));
    let p = s.propose(&s.snapshot(), document("1 + 1", Some(Value::Int(2))), "retain", Author::Human).unwrap();
    for limits in [ReviewLimits { max_events: 1, ..ReviewLimits::default() }, ReviewLimits { max_events: 0, ..ReviewLimits::default() }] {
        let review = p.preview(limits);
        assert!(review.report().is_err());
        assert_eq!(s.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::ReviewUnavailable);
    }
}
#[test]
fn subsequent_edit_invalidates_review_and_cancels_late_preview() {
    let mut s = session("1", Value::Int(1));
    let p = s.propose(&s.snapshot(), document("1", Some(Value::Int(1))), "retain", Author::Human).unwrap();
    let review = p.preview(ReviewLimits::default());
    s.edit_source(&s.snapshot(), "1 // new revision").unwrap();
    assert_eq!(s.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::StaleBase);
    assert_eq!(p.preview(ReviewLimits::default()).report().unwrap_err().code, "CANCELLED");
}
#[test]
fn case_only_edit_also_invalidates_previous_review() {
    let mut s = session("1", Value::Int(1));
    let review = propose(&s, document("1", Some(Value::Int(1))));
    s.revise_cases(&s.snapshot(), &[case("case", Value::Int(2))]).unwrap();
    assert_eq!(s.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::StaleBase);
}
#[test]
fn one_proposal_cannot_apply_another_candidate_or_be_applied_twice() {
    let mut s = session("1", Value::Int(1));
    let a = propose(&s, document("1 // reviewed", Some(Value::Int(1))));
    let b = propose(&s, document("2", Some(Value::Int(2))));
    s.apply(&a, HistoricalPolicy::Preserve).unwrap();
    assert_eq!(s.snapshot().source(), "1 // reviewed");
    assert_eq!(s.apply(&a, HistoricalPolicy::Preserve).unwrap_err(), SessionError::StaleBase);
    assert_eq!(s.apply(&b, HistoricalPolicy::AcknowledgeChangedValues).unwrap_err(), SessionError::StaleBase);
}
#[test]
fn report_copies_cannot_mutate_the_private_review_used_by_apply() {
    let mut s = session("1", Value::Int(1));
    let review = propose(&s, document("2", Some(Value::Int(1))));
    let mut detached = review.report().unwrap().clone();
    for r in &mut detached.candidate_results { r.passed = true; r.outcome.result = Ok(Value::Int(1)); }
    assert_eq!(PolicyReview::status(&detached.candidate_results), SuiteStatus::Passed);
    assert_eq!(s.apply(&review, HistoricalPolicy::AcknowledgeChangedValues).unwrap_err(), SessionError::CandidateNotVerified);
}
#[test]
fn human_and_assistant_follow_the_same_review_and_apply_rules() {
    for author in [Author::Human, Author::Assistant] {
        let mut s = PolicySession::new("memory:inventory", inventory()).unwrap();
        let p = s.propose(&s.snapshot(), changed_inventory(), "keep one item", author).unwrap();
        assert_eq!(p.author(), author); assert_eq!(p.reason(), "keep one item");
        let review = p.preview(ReviewLimits::default());
        assert_eq!(s.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::HistoricalExpectationFailed);
    }
}
#[test]
fn invalid_rationale_does_not_mutate_or_cancel_current_document() {
    let s = session("1", Value::Int(1));
    for reason in [" ".to_owned(), "x".repeat(8193)] {
        assert_eq!(s.propose(&s.snapshot(), document("1", Some(Value::Int(1))), &reason, Author::Human).unwrap_err(), SessionError::InvalidReason);
    }
    assert_eq!(run(&s, "case").check(), CheckState::Passed);
}
#[test]
fn cancelled_review_api_is_distinct_from_a_completed_failed_policy() {
    let p = cannon_core::compile_expression("1").unwrap();
    let cases = [case("case", Value::Int(1))];
    let token = CancellationToken::new(); token.cancel();
    assert_eq!(review_policies_cancellable(&p, &p, &cases, &cases, ReviewLimits::default(), &token).unwrap_err().code, "CANCELLED");
    assert!(cannon_core::policy_review::review_policies(&p, &p, &cases, &cases, ReviewLimits::default()).unwrap().passed());
}
#[test]
fn formatting_only_changes_still_advance_revision_but_not_policy_logic() {
    let mut s = session("1 + 1", Value::Int(2));
    let review = propose(&s, document(" 1 + 1 // comment\n", Some(Value::Int(2))));
    assert!(!review.report().unwrap().logic_changed);
    let prior = s.snapshot();
    s.apply(&review, HistoricalPolicy::Preserve).unwrap();
    assert!(!s.is_current(&prior));
}
#[test]
fn changing_inputs_is_reported_independently_of_expected_values() {
    let s = PolicySession::new("memory:inventory", inventory()).unwrap();
    let mut d = inventory(); d.cases[0].inputs.insert("quantity".into(), Value::Int(1));
    let review = propose(&s, d);
    assert!(review.report().unwrap().case_changes.iter().any(|c| c.kind == "input-changed"));
    assert!(!review.report().unwrap().case_changes.iter().any(|c| c.kind == "expectation-changed"));
    assert_eq!(review.report().unwrap().historical_results[0].case.inputs["quantity"], Value::Int(2));
}
#[test]
fn case_text_limits_cover_input_values_and_expected_values() {
    let mut d = inventory();
    d.cases[0].inputs.insert("quantity".into(), Value::Text(vec![65; MAX_SOURCE_UNITS]));
    d.cases[0].expected = Value::Text(vec![65]);
    assert!(matches!(PolicySession::new("memory:x", d), Err(SessionError::InvalidMetadata(_))));
}
