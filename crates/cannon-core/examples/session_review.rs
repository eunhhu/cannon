//! End-to-end in-memory editing/review; does not write files or call an AI provider.
use cannon_core::{InputBindings, InputSpec, InputType, Limits, ObservationOptions, Value};
use cannon_core::policy_review::{PolicyCase, PolicyReview, ReviewLimits, SuiteStatus};
use cannon_core::session::{Author, CheckState, HistoricalPolicy, PolicyDocument, PolicySession, SessionError};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let document = PolicyDocument {
        source: "quantity <= 2".into(),
        inputs: vec![InputSpec::new("request.quantity", "quantity", InputType::PositiveInt)],
        cases: vec![PolicyCase {
            id: "boundary".into(), name: "The remaining quantity can be reserved".into(),
            inputs: InputBindings::from([("request.quantity".into(), Value::Int(2))]), expected: Value::Bool(true),
        }],
    };
    let mut session = PolicySession::new("memory:inventory", document)?;
    let baseline = session.prepare_case("boundary", Limits::default(), ObservationOptions::default())?.execute();
    assert_eq!(baseline.check(), CheckState::Passed);
    println!("Revision {}: {} -> {:?}", baseline.snapshot().revision(), baseline.snapshot().source(), baseline.outcome().result);

    let mut candidate = session.snapshot().document().clone();
    candidate.source = "quantity < 2".into();
    candidate.cases[0].expected = Value::Bool(false);
    let proposal = session.propose(&session.snapshot(), candidate, "Keep one item available", Author::Assistant)?;
    let review = proposal.preview(ReviewLimits::default());
    let report = review.report().expect("static demonstration has a complete review");
    assert_eq!(PolicyReview::status(&report.candidate_results), SuiteStatus::Passed);
    assert_eq!(report.regressions(), ["boundary"]);
    println!("Candidate cases: PASSED; historical regressions: {:?}", report.regressions());
    assert_eq!(session.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::HistoricalExpectationFailed);
    println!("Default application: HISTORICAL_EXPECTATION_FAILED");

    let queued = session.prepare_case("boundary", Limits::default(), ObservationOptions::default())?;
    let installed = session.apply(&review, HistoricalPolicy::AcknowledgeChangedValues)?;
    assert!(!installed.same_revision(proposal.candidate()));
    assert!(!session.receipt_is_current(&baseline));
    assert_eq!(queued.execute().outcome().result.as_ref().unwrap_err().code, "CANCELLED");
    let current = session.prepare_case("boundary", Limits::default(), ObservationOptions::default())?.execute();
    assert_eq!(current.check(), CheckState::Passed);
    assert_eq!(current.outcome().result, Ok(Value::Bool(false)));
    println!("Acknowledged revision {}: {:?}; old queued run: CANCELLED", installed.revision(), current.outcome().result);
    assert_eq!(session.apply(&review, HistoricalPolicy::Preserve).unwrap_err(), SessionError::StaleBase);
    println!("Reusing the old review: STALE_BASE");
    println!("Only in-memory state changed. No file, repository, database or deployment was modified.");
    Ok(())
}
