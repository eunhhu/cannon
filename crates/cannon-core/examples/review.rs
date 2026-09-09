//! A complete in-memory review cycle. No files, providers, or deployments change.
use cannon_core::{InputBindings, InputSpec, InputType, Limits, ObservationOptions, Value};
use cannon_core::session::{Author, CheckState, ExpressionSession, HistoricalPolicy, SessionError};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = ExpressionSession::new("memory:inventory-example", "8 + 2 <= 10", Some(Value::Bool(true)))?;
    let baseline = session.snapshot();
    let old_run = session.prepare_run(Limits::default(), ObservationOptions::default())?.execute();
    println!("Baseline: {:?}", old_run.check());

    // The assistant may suggest a policy AND an expected-answer change. Both stay visible.
    let proposal = session.propose(&baseline, "8 + 2 < 10", Some(Value::Bool(false)),
        "Proposal: keep at least one item available (caller-supplied rationale)", Author::Assistant)?;
    let review = proposal.preview(Limits::default());
    println!("Candidate's own expectation: {:?}", review.candidate().check());
    println!("Original expectation against candidate: {:?}", review.historical_check());
    println!("Facts: {:?}", review.facts());
    assert_eq!(review.candidate().check(), CheckState::Passed);
    assert_eq!(review.historical_check(), CheckState::Mismatch);
    assert!(review.introduces_regression());

    let blocked = session.apply(&review, HistoricalPolicy::Preserve).unwrap_err();
    assert_eq!(blocked, SessionError::HistoricalExpectationFailed);
    println!("Preserving original intent: {blocked}");
    assert!(session.is_current(&baseline));

    // A human can intentionally approve the changed policy. This is explicit and
    // does not rewrite the immutable review or declare it an equivalence proof.
    let applied = session.apply(&review, HistoricalPolicy::AcknowledgeChange)?;
    println!("Applied revision {}: {}", applied.revision(), applied.source());
    assert!(!session.receipt_is_current(&old_run));
    assert!(!session.receipt_is_current(review.candidate()));
    let current = session.prepare_run(Limits::default(), ObservationOptions::default())?.execute();
    assert!(session.receipt_is_current(&current));
    assert_eq!(current.check(), CheckState::Passed);
    println!("Re-executed committed revision: {:?}", current.check());

    // A subsequent edit invalidates queued work as well as already displayed results.
    let queued = session.prepare_run(Limits::default(), ObservationOptions::default())?;
    session.edit(&applied, "8 + 2 < 10 // human is editing")?;
    let cancelled = queued.execute();
    assert_eq!(cancelled.outcome().result.as_ref().unwrap_err().code, "CANCELLED");
    assert_eq!(session.apply(&review, HistoricalPolicy::AcknowledgeChange).unwrap_err(), SessionError::StaleBase);
    println!("Queued old work: CANCELLED; reused review: STALE_BASE");
    // Candidate data cannot hide a policy change: replay the original data by ID.
    let inputs = [InputSpec::new("quantity.id", "quantity", InputType::PositiveInt)];
    let bindings = InputBindings::from([("quantity.id".into(), Value::Int(10))]);
    let typed = ExpressionSession::new_with_inputs("memory:typed", "quantity <= 10", &inputs, &bindings, Some(Value::Bool(true)))?;
    let base = typed.snapshot();
    let mut draft = base.draft();
    draft.source = "quantity < 10".into();
    draft.bindings.insert("quantity.id".into(), Value::Int(9));
    let review = typed.propose_draft(&base, &draft, "Tighten the policy and change the example", Author::Assistant)?.preview(Limits::default());
    assert_eq!(review.candidate().check(), CheckState::Passed);
    assert_eq!(review.historical_check(), CheckState::Mismatch);
    println!("Changed candidate input: PASSED; original input replay: MISMATCH");
    println!("Candidate inputs: {:?}; original inputs: {:?}", review.candidate().bindings(), review.historical().bindings());
    Ok(())
}
