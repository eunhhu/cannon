use std::ops::ControlFlow;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use cannon_core::{Limits, ObservationOptions, Value, MAX_INT, MAX_SOURCE_UNITS};
use cannon_core::session::{
    Author, CheckState, ExpressionSession, HistoricalPolicy, Phase, SessionError,
};

fn session(source: &str, expected: Option<Value>) -> ExpressionSession {
    ExpressionSession::new("memory:policy", source, expected).unwrap()
}
fn execute(session: &ExpressionSession) -> cannon_core::session::RunReceipt {
    session.prepare_run(Limits::default(), ObservationOptions::default()).unwrap().execute()
}

#[test]
fn receipt_binds_exact_source_uri_revision_and_options() {
    let document = session("8 + 2 <= 10", Some(Value::Bool(true)));
    let receipt = execute(&document);
    assert_eq!(receipt.snapshot().source(), "8 + 2 <= 10");
    assert_eq!(receipt.snapshot().uri(), "memory:policy");
    assert_eq!(receipt.snapshot().revision(), 1);
    assert_eq!(receipt.phase(), Phase::Execution);
    assert_eq!(receipt.limits().max_steps, Limits::default().max_steps);
    assert_eq!(receipt.outcome().result, Ok(Value::Bool(true)));
    assert_eq!(receipt.check(), CheckState::Passed);
    assert!(document.receipt_is_current(&receipt));
}

#[test]
fn direct_text_edit_keeps_independent_expected_value() {
    let mut document = session("8 + 2 <= 10", Some(Value::Bool(true)));
    let base = document.snapshot();
    document.edit(&base, "8 + 2 < 10").unwrap();
    let receipt = execute(&document);
    assert_eq!(receipt.snapshot().expected(), Some(&Value::Bool(true)));
    assert_eq!(receipt.check(), CheckState::Mismatch);
}

#[test]
fn completed_receipt_becomes_stale_but_keeps_historical_result() {
    let mut document = session("1 + 1", Some(Value::Int(2)));
    let old = execute(&document);
    document.edit(&document.snapshot(), "1 + 2").unwrap();
    assert!(!document.receipt_is_current(&old));
    assert_eq!(old.outcome().result, Ok(Value::Int(2)));
    assert_eq!(old.snapshot().source(), "1 + 1");
}

#[test]
fn queued_execution_is_cancelled_after_edit() {
    let mut document = session("1 + 1", None);
    let request = document.prepare_run(Limits::default(), ObservationOptions::default()).unwrap();
    document.edit(&document.snapshot(), "1 + 2").unwrap();
    let old = request.execute();
    assert_eq!(old.outcome().result.as_ref().unwrap_err().code, "CANCELLED");
    assert_eq!(old.outcome().steps, 0);
    assert!(old.outcome().trace.is_empty());
    assert!(!document.receipt_is_current(&old));
}

#[test]
fn editing_while_a_worker_observes_cancels_that_exact_revision() {
    let mut document = session("1 + 2 + 3", None);
    let request = document.prepare_run(Limits::default(), ObservationOptions::default()).unwrap();
    let (observed_tx, observed_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let worker = thread::spawn(move || request.execute_observed(|origin, _| {
        assert_eq!(origin.source(), "1 + 2 + 3");
        observed_tx.send(()).unwrap();
        resume_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        ControlFlow::Continue(())
    }));
    observed_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    document.edit(&document.snapshot(), "4 + 5").unwrap();
    resume_tx.send(()).unwrap();
    let old = worker.join().unwrap();
    assert_eq!(old.outcome().result.as_ref().unwrap_err().code, "CANCELLED");
    assert_eq!(old.observation().emitted_events, 1);
    assert!(!document.receipt_is_current(&old));
    assert_eq!(execute(&document).outcome().result, Ok(Value::Int(9)));
}

#[test]
fn identical_text_edit_still_rejects_old_handles() {
    let mut document = session("1", None);
    let old = document.snapshot();
    let current = document.edit(&old, "1").unwrap();
    assert_eq!(current.revision(), 2);
    assert!(!current.same_revision(&old));
    assert_eq!(document.edit(&old, "2").unwrap_err(), SessionError::StaleBase);
}

#[test]
fn changing_back_to_old_text_does_not_resurrect_old_evidence() {
    let mut document = session("1", None);
    let old = execute(&document);
    document.edit(&document.snapshot(), "2").unwrap();
    document.edit(&document.snapshot(), "1").unwrap();
    assert_eq!(document.snapshot().revision(), 3);
    assert!(!document.receipt_is_current(&old));
}

#[test]
fn different_sessions_with_same_text_and_number_are_not_interchangeable() {
    let first = session("1", None);
    let mut second = session("1", None);
    assert_eq!(first.snapshot().revision(), second.snapshot().revision());
    assert!(!second.is_current(&first.snapshot()));
    assert_eq!(second.edit(&first.snapshot(), "2").unwrap_err(), SessionError::StaleBase);
    let proposal = first.propose(&first.snapshot(), "2", None, "new value", Author::Human).unwrap();
    assert_eq!(second.apply(&proposal.preview(Limits::default()), HistoricalPolicy::Preserve).unwrap_err(), SessionError::StaleBase);
}

#[test]
fn incomplete_source_has_a_diagnostic_and_can_be_repaired() {
    let mut document = session("1 +", Some(Value::Int(2)));
    assert!(document.snapshot().diagnostic().is_some());
    let failed = execute(&document);
    assert_eq!(failed.phase(), Phase::Compilation);
    assert_eq!(failed.check(), CheckState::CompilationError);
    assert_eq!(failed.outcome().steps, 0);
    document.edit(&document.snapshot(), "1 + 1").unwrap();
    assert_eq!(execute(&document).check(), CheckState::Passed);
}

#[test]
fn execution_failure_is_not_an_assertion_mismatch_or_missing_assertion() {
    let document = session("9007199254740991 + 1", None);
    let receipt = execute(&document);
    assert_eq!(receipt.phase(), Phase::Execution);
    assert_eq!(receipt.check(), CheckState::ExecutionError);
    assert_eq!(receipt.outcome().result.as_ref().unwrap_err().code, "INTEGER_RANGE");
}

#[test]
fn no_expected_value_is_not_a_passing_verification() {
    assert_eq!(execute(&session("2", None)).check(), CheckState::NotSpecified);
}

#[test]
fn invalid_execution_limits_remain_errors() {
    let document = session("1", Some(Value::Int(1)));
    let limits = Limits { max_steps: 0, ..Limits::default() };
    let receipt = document.prepare_run(limits, ObservationOptions::default()).unwrap().execute();
    assert_eq!(receipt.outcome().result.as_ref().unwrap_err().code, "INVALID_LIMIT");
    assert_eq!(receipt.check(), CheckState::ExecutionError);
    assert!(document.receipt_is_current(&receipt));
    assert_eq!(execute(&document).check(), CheckState::Passed);
}

#[test]
fn streamed_receipt_distinguishes_delivered_events_from_retained_trace() {
    let document = session("1 + 2", Some(Value::Int(3)));
    let options = ObservationOptions { retain_trace: false, ..ObservationOptions::default() };
    let request = document.prepare_run(Limits::default(), options).unwrap();
    let mut delivered = Vec::new();
    let receipt = request.execute_observed(|_, event| { delivered.push(event.clone()); ControlFlow::Continue(()) });
    assert_eq!(delivered.len(), 3);
    assert_eq!(receipt.observation().emitted_events, 3);
    assert!(!receipt.observation().options.retain_trace);
    assert!(receipt.outcome().trace.is_empty());
    assert_eq!(receipt.check(), CheckState::Passed);
}

#[test]
fn an_observer_cancels_one_run_without_poisoning_other_runs_of_current_source() {
    let document = session("1 + 2", None);
    let stopped = document.prepare_run(Limits::default(), ObservationOptions::default()).unwrap()
        .execute_observed(|_, _| ControlFlow::Break(()));
    assert_eq!(stopped.check(), CheckState::ExecutionError);
    assert_eq!(execute(&document).outcome().result, Ok(Value::Int(3)));
}

#[test]
fn source_budgets_are_checked_before_mutating_the_document() {
    let mut document = session("1", None);
    let old = document.snapshot();
    let pending = document.prepare_run(Limits::default(), ObservationOptions::default()).unwrap();
    let oversized = " ".repeat(MAX_SOURCE_UNITS + 1);
    assert_eq!(document.edit(&old, &oversized).unwrap_err(), SessionError::SourceTooLarge);
    assert!(document.is_current(&old));
    assert_eq!(pending.execute().outcome().result, Ok(Value::Int(1)));
}

#[test]
fn source_budget_counts_utf16_not_bytes() {
    let mut document = session("1", None);
    // Each non-BMP scalar uses two UTF-16 code units.
    let oversized = format!("//{}", "😀".repeat(MAX_SOURCE_UNITS / 2));
    assert_eq!(document.edit(&document.snapshot(), &oversized).unwrap_err(), SessionError::SourceTooLarge);
    document.edit(&document.snapshot(), "\"😀\"").unwrap();
    assert_eq!(execute(&document).outcome().result, Ok(Value::Text(vec![0xd83d, 0xde00])));
}

#[test]
fn uri_and_expected_values_are_validated() {
    assert_eq!(ExpressionSession::new(" ", "1", None).unwrap_err(), SessionError::InvalidUri);
    assert_eq!(ExpressionSession::new(&"x".repeat(4097), "1", None).unwrap_err(), SessionError::InvalidUri);
    assert_eq!(ExpressionSession::new("memory:x", "1", Some(Value::Int(MAX_INT + 1))).unwrap_err(), SessionError::InvalidExpectedValue);
    assert_eq!(ExpressionSession::new("memory:x", "1", Some(Value::Text(vec![1; MAX_SOURCE_UNITS + 1]))).unwrap_err(), SessionError::InvalidExpectedValue);
}

#[test]
fn expectation_edits_are_independent_and_invalidate_old_runs() {
    let mut document = session("1", Some(Value::Int(2)));
    let old = execute(&document);
    document.revise_expectation(&document.snapshot(), Some(Value::Int(1))).unwrap();
    assert_eq!(document.snapshot().source(), "1");
    assert!(!document.receipt_is_current(&old));
    assert_eq!(old.check(), CheckState::Mismatch);
    assert_eq!(execute(&document).check(), CheckState::Passed);
}

#[test]
fn invalid_expectation_edit_is_transactional() {
    let mut document = session("1", Some(Value::Int(1)));
    let base = document.snapshot();
    assert_eq!(document.revise_expectation(&base, Some(Value::Int(i64::MIN))).unwrap_err(), SessionError::InvalidExpectedValue);
    assert!(document.is_current(&base));
    assert_eq!(execute(&document).check(), CheckState::Passed);
}

#[test]
fn preview_does_not_edit_the_session() {
    let document = session("1", Some(Value::Int(1)));
    let base = document.snapshot();
    let proposal = document.propose(&base, "2", Some(Value::Int(2)), "new policy", Author::Assistant).unwrap();
    let review = proposal.preview(Limits::default());
    assert!(document.is_current(&base));
    assert_eq!(document.snapshot().source(), "1");
    assert_eq!(review.proposal().author(), Author::Assistant);
    assert_eq!(review.proposal().reason(), "new policy");
    assert!(!document.receipt_is_current(review.candidate()));
}

#[test]
fn changed_expectation_cannot_hide_previous_expected_value() {
    let document = session("8 + 2 <= 10", Some(Value::Bool(true)));
    let proposal = document.propose(&document.snapshot(), "8 + 2 < 10", Some(Value::Bool(false)), "reserve one item", Author::Assistant).unwrap();
    let review = proposal.preview(Limits::default());
    assert_eq!(review.baseline().check(), CheckState::Passed);
    assert_eq!(review.candidate().check(), CheckState::Passed);
    assert_eq!(review.historical_check(), CheckState::Mismatch);
    assert!(review.introduces_regression());
    assert!(review.facts().source_text_changed);
    assert!(review.facts().expected_value_changed);
    assert!(!review.facts().result_type_changed);
}

#[test]
fn removed_expectation_cannot_hide_previous_expected_value() {
    let document = session("1", Some(Value::Int(1)));
    let review = document.propose(&document.snapshot(), "2", None, "remove assertion explicitly", Author::Human).unwrap().preview(Limits::default());
    assert_eq!(review.candidate().check(), CheckState::NotSpecified);
    assert_eq!(review.historical_check(), CheckState::Mismatch);
    assert!(review.facts().expected_value_changed);
    assert!(review.introduces_regression());
}

#[test]
fn historical_check_uses_expected_value_not_the_old_implementation_output() {
    let document = session("2", Some(Value::Int(3)));
    let review = document.propose(&document.snapshot(), "3", Some(Value::Int(3)), "repair implementation", Author::Human).unwrap().preview(Limits::default());
    assert_eq!(review.baseline().check(), CheckState::Mismatch);
    assert_eq!(review.historical_check(), CheckState::Passed);
    assert!(!review.introduces_regression());
}

#[test]
fn preserve_policy_rejects_a_historical_regression_without_mutation() {
    let mut document = session("1", Some(Value::Int(1)));
    let base = document.snapshot();
    let review = document.propose(&base, "2", Some(Value::Int(2)), "new policy", Author::Human).unwrap().preview(Limits::default());
    assert_eq!(document.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::HistoricalExpectationFailed);
    assert!(document.is_current(&base));
    assert_eq!(document.snapshot().expected(), Some(&Value::Int(1)));
}

#[test]
fn explicit_historical_acknowledgement_applies_exact_reviewed_candidate() {
    let mut document = session("1", Some(Value::Int(1)));
    let review = document.propose(&document.snapshot(), "2", Some(Value::Int(2)), "new policy", Author::Human).unwrap().preview(Limits::default());
    let current = document.apply(&review, HistoricalPolicy::AcknowledgeChange).unwrap();
    assert_eq!(current.source(), "2");
    assert_eq!(current.expected(), Some(&Value::Int(2)));
    assert!(document.is_current(&current));
    assert_eq!(execute(&document).check(), CheckState::Passed);
}

#[test]
fn preview_evidence_is_not_promoted_to_committed_evidence() {
    let mut document = session("1", Some(Value::Int(1)));
    let review = document.propose(&document.snapshot(), "1 + 0", Some(Value::Int(1)), "equivalent example", Author::Human).unwrap().preview(Limits::default());
    let candidate_revision = review.candidate().snapshot().revision();
    let committed = document.apply(&review, HistoricalPolicy::Preserve).unwrap();
    assert_eq!(candidate_revision, committed.revision());
    assert!(!document.receipt_is_current(review.candidate()));
    assert!(!document.receipt_is_current(review.baseline()));
    assert!(document.receipt_is_current(&execute(&document)));
}

#[test]
fn stale_review_is_rejected_even_after_comment_only_edit() {
    let mut document = session("1", Some(Value::Int(1)));
    let review = document.propose(&document.snapshot(), "1 + 0", Some(Value::Int(1)), "candidate", Author::Assistant).unwrap().preview(Limits::default());
    document.edit(&document.snapshot(), "1 // edited").unwrap();
    assert_eq!(document.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::StaleBase);
}

#[test]
fn two_reviews_cannot_be_mixed_or_applied_twice() {
    let mut document = session("1", None);
    let base = document.snapshot();
    let first = document.propose(&base, "2", None, "first", Author::Human).unwrap().preview(Limits::default());
    let second = document.propose(&base, "3", None, "second", Author::Assistant).unwrap().preview(Limits::default());
    document.apply(&second, HistoricalPolicy::Preserve).unwrap();
    assert_eq!(document.snapshot().source(), "3");
    assert_eq!(document.apply(&first, HistoricalPolicy::Preserve).unwrap_err(), SessionError::StaleBase);
    assert_eq!(document.apply(&second, HistoricalPolicy::Preserve).unwrap_err(), SessionError::StaleBase);
}

#[test]
fn acknowledgement_does_not_override_candidate_compilation_or_execution_failures() {
    let mut document = session("1", None);
    let syntax = document.propose(&document.snapshot(), "1 +", None, "broken", Author::Assistant).unwrap().preview(Limits::default());
    assert_eq!(syntax.candidate().phase(), Phase::Compilation);
    assert_eq!(document.apply(&syntax, HistoricalPolicy::AcknowledgeChange).unwrap_err(), SessionError::InvalidCandidate);
    let overflow = document.propose(&document.snapshot(), "9007199254740991 + 1", None, "broken runtime", Author::Assistant).unwrap().preview(Limits::default());
    assert_eq!(document.apply(&overflow, HistoricalPolicy::AcknowledgeChange).unwrap_err(), SessionError::CandidateExecutionFailed);
    assert_eq!(document.snapshot().revision(), 1);
}

#[test]
fn acknowledgement_does_not_override_a_failed_candidate_expectation() {
    let mut document = session("1", Some(Value::Int(1)));
    let review = document.propose(&document.snapshot(), "2", Some(Value::Int(99)), "wrong answer", Author::Assistant).unwrap().preview(Limits::default());
    assert_eq!(document.apply(&review, HistoricalPolicy::AcknowledgeChange).unwrap_err(), SessionError::CandidateExpectationFailed);
}

#[test]
fn failed_preview_resource_budget_cannot_be_applied() {
    let mut document = session("1", None);
    let review = document.propose(&document.snapshot(), "1 + 2", None, "limited preview", Author::Human).unwrap()
        .preview(Limits { max_steps: 1, ..Limits::default() });
    assert_eq!(review.candidate().outcome().result.as_ref().unwrap_err().code, "STEP_LIMIT");
    assert_eq!(document.apply(&review, HistoricalPolicy::AcknowledgeChange).unwrap_err(), SessionError::CandidateExecutionFailed);
}

#[test]
fn source_type_changes_are_computed_facts() {
    let document = session("1", None);
    let review = document.propose(&document.snapshot(), "true", None, "Boolean policy", Author::Human).unwrap().preview(Limits::default());
    assert!(review.facts().result_type_changed);
    assert!(!review.facts().expected_value_changed);
}

#[test]
fn missing_expectations_stay_unknown_in_review() {
    let document = session("1", None);
    let review = document.propose(&document.snapshot(), "2", None, "unverified example", Author::Human).unwrap().preview(Limits::default());
    assert_eq!(review.baseline().check(), CheckState::NotSpecified);
    assert_eq!(review.candidate().check(), CheckState::NotSpecified);
    assert_eq!(review.historical_check(), CheckState::NotSpecified);
    assert!(!review.introduces_regression());
}

#[test]
fn human_and_assistant_authorship_get_identical_checks() {
    for author in [Author::Human, Author::Assistant] {
        let mut document = session("1", Some(Value::Int(1)));
        let review = document.propose(&document.snapshot(), "2", Some(Value::Int(2)), "same policy", author).unwrap().preview(Limits::default());
        assert_eq!(document.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::HistoricalExpectationFailed);
    }
}

#[test]
fn rationale_and_candidate_validation_fail_without_mutating_base() {
    let document = session("1", None);
    let base = document.snapshot();
    for reason in ["".to_owned(), " ".to_owned(), "x".repeat(8193)] {
        assert_eq!(document.propose(&base, "2", None, &reason, Author::Human).unwrap_err(), SessionError::InvalidReason);
    }
    assert_eq!(document.propose(&base, "2", Some(Value::Int(i64::MAX)), "candidate", Author::Human).unwrap_err(), SessionError::InvalidExpectedValue);
    assert!(document.is_current(&base));
}

#[test]
fn stale_proposal_preview_cannot_run_as_a_current_revision() {
    let mut document = session("1", Some(Value::Int(1)));
    let proposal = document.propose(&document.snapshot(), "2", Some(Value::Int(2)), "proposal", Author::Assistant).unwrap();
    document.edit(&document.snapshot(), "3").unwrap();
    let review = proposal.preview(Limits::default());
    assert_eq!(review.baseline().outcome().result.as_ref().unwrap_err().code, "CANCELLED");
    assert_eq!(review.candidate().outcome().result.as_ref().unwrap_err().code, "CANCELLED");
    assert_eq!(document.apply(&review, HistoricalPolicy::AcknowledgeChange).unwrap_err(), SessionError::StaleBase);
}

#[test]
fn cloned_receipts_are_immutable_snapshots_not_new_authority() {
    let mut document = session("1", None);
    let receipt = execute(&document);
    let copied = receipt.clone();
    assert!(copied.snapshot().same_revision(receipt.snapshot()));
    document.edit(&document.snapshot(), "2").unwrap();
    assert!(!document.receipt_is_current(&copied));
    assert_eq!(copied.outcome().result, Ok(Value::Int(1)));
}

#[test]
fn close_is_idempotent_and_rejects_all_future_mutations_and_runs() {
    let mut document = session("1", None);
    let base = document.snapshot();
    let request = document.prepare_run(Limits::default(), ObservationOptions::default()).unwrap();
    let proposal = document.propose(&base, "2", None, "candidate", Author::Human).unwrap();
    let review = proposal.preview(Limits::default());
    document.close(); document.close();
    assert!(document.is_closed());
    assert!(!document.is_current(&base));
    assert_eq!(document.edit(&base, "3").unwrap_err(), SessionError::Closed);
    assert_eq!(document.revise_expectation(&base, None).unwrap_err(), SessionError::Closed);
    assert_eq!(document.prepare_run(Limits::default(), ObservationOptions::default()).unwrap_err(), SessionError::Closed);
    assert_eq!(document.propose(&base, "3", None, "candidate", Author::Human).unwrap_err(), SessionError::Closed);
    assert_eq!(document.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::Closed);
    assert_eq!(request.execute().outcome().result.as_ref().unwrap_err().code, "CANCELLED");
    assert_eq!(document.snapshot().source(), "1");
}

#[test]
fn dropping_the_session_cancels_an_outstanding_worker_request() {
    let document = session("1", None);
    let request = document.prepare_run(Limits::default(), ObservationOptions::default()).unwrap();
    drop(document);
    assert_eq!(request.execute().outcome().result.as_ref().unwrap_err().code, "CANCELLED");
}

#[test]
fn utf16_strings_and_independent_trace_origins_survive_review() {
    let document = session("\"😀\"", Some(Value::Text(vec![0xd83d, 0xde00])));
    let review = document.propose(&document.snapshot(), "\"😀\" // note", Some(Value::Text(vec![0xd83d, 0xde00])), "retain value", Author::Human).unwrap().preview(Limits::default());
    assert_eq!(review.historical_check(), CheckState::Passed);
    assert_eq!(review.baseline().outcome().trace[0].span.end, 4);
    assert_eq!(review.baseline().snapshot().source(), "\"😀\"");
    assert_eq!(review.candidate().snapshot().source(), "\"😀\" // note");
}

#[test]
fn each_observed_event_carries_its_immutable_source_snapshot() {
    let document = session("1 + 2", None);
    let origin = document.snapshot();
    let request = document.prepare_run(Limits::default(), ObservationOptions::default()).unwrap();
    let mut count = 0;
    let receipt = request.execute_observed(|snapshot, event| {
        assert!(snapshot.same_revision(&origin));
        assert_eq!(snapshot.source(), "1 + 2");
        assert!(event.span.end <= snapshot.source().encode_utf16().count());
        count += 1;
        ControlFlow::Continue(())
    });
    assert_eq!(count, 3);
    assert_eq!(receipt.outcome().result, Ok(Value::Int(3)));
}
