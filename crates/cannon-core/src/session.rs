//! Process-local document sessions over the existing typed policy engine.
//! Snapshots, requests and reviews are opaque handles, not signed wire evidence.
use std::fmt;
use std::ops::ControlFlow;
use std::sync::Arc;

use crate::inputs::check_schema;
use crate::policy_review::{
    review_policies_cancellable, validate_cases, PolicyCase, PolicyReview, ReviewLimits, SuiteStatus,
};
use crate::{
    compile_with_inputs, evaluate_bound_observed, CancellationToken, CompiledExpression,
    Diagnostic, InputSpec, Limits, ObservationOptions, ObservedOutcome, Outcome, TraceStep,
    Type, Value, MAX_SOURCE_UNITS,
};

pub const MAX_SESSION_TEXT_UNITS: usize = 1_048_576;
const MAX_URI_BYTES: usize = 4096;
const MAX_REASON_BYTES: usize = 8192;

/// Editable input, moved into an immutable snapshot after metadata validation.
#[derive(Clone, Debug)]
pub struct PolicyDocument {
    pub source: String,
    pub inputs: Vec<InputSpec>,
    pub cases: Vec<PolicyCase>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionError {
    Closed,
    StaleBase,
    InvalidUri,
    InvalidReason,
    SourceLimit,
    CaseTextLimit,
    InvalidMetadata(Diagnostic),
    UnknownCase,
    RevisionExhausted,
    InvalidCandidate,
    ReviewUnavailable,
    BaselineNotVerified,
    CandidateNotVerified,
    HistoricalExecutionFailed,
    HistoricalExpectationFailed,
}
impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Self::InvalidMetadata(error) = self {
            return write!(f, "{}: {}", error.code, error.message);
        }
        f.write_str(match self {
            Self::Closed => "SESSION_CLOSED: this session is closed",
            Self::StaleBase => "STALE_BASE: the exact document revision is no longer current",
            Self::InvalidUri => "INVALID_URI: supply a nonempty URI of at most 4096 bytes",
            Self::InvalidReason => "INVALID_REASON: supply a nonempty rationale of at most 8192 bytes",
            Self::SourceLimit => "SOURCE_LIMIT: source exceeds the UTF-16 budget",
            Self::CaseTextLimit => "CASE_TEXT_LIMIT: cases exceed the cumulative text budget",
            Self::UnknownCase => "UNKNOWN_CASE: case ID is not present in this snapshot",
            Self::RevisionExhausted => "REVISION_EXHAUSTED: revision numbers must never wrap",
            Self::InvalidCandidate => "INVALID_CANDIDATE: candidate source does not compile",
            Self::ReviewUnavailable => "REVIEW_UNAVAILABLE: a complete review is required",
            Self::BaselineNotVerified => "BASELINE_NOT_VERIFIED: baseline cases must be nonempty and passing",
            Self::CandidateNotVerified => "CANDIDATE_NOT_VERIFIED: candidate cases must be nonempty and passing",
            Self::HistoricalExecutionFailed => "HISTORICAL_EXECUTION_FAILED: an execution failure cannot be acknowledged as a changed value",
            Self::HistoricalExpectationFailed => "HISTORICAL_EXPECTATION_FAILED: separately acknowledge changed historical values",
            Self::InvalidMetadata(_) => unreachable!(),
        })
    }
}
impl std::error::Error for SessionError {}

/// Declared authorship is descriptive metadata, not authentication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Author { Human, Assistant }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoricalPolicy { Preserve, AcknowledgeChangedValues }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState { Passed, Mismatch, CompilationError, ExecutionError }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase { Compilation, Execution }

fn check_source(source: &str) -> Result<(), SessionError> {
    if source.encode_utf16().take(MAX_SOURCE_UNITS + 1).count() > MAX_SOURCE_UNITS {
        Err(SessionError::SourceLimit)
    } else { Ok(()) }
}
fn check_cases(cases: &[PolicyCase]) -> Result<(), SessionError> {
    validate_cases(cases).map_err(SessionError::InvalidMetadata)?;
    let mut total = 0usize;
    for case in cases {
        for value in case.inputs.values().chain(std::iter::once(&case.expected)) {
            if let Value::Text(text) = value {
                total = total.checked_add(text.len()).filter(|n| *n <= MAX_SESSION_TEXT_UNITS)
                    .ok_or(SessionError::CaseTextLimit)?;
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
struct SnapshotData {
    revision: u64,
    uri: Arc<str>,
    document: Arc<PolicyDocument>,
    compiled: Result<Arc<CompiledExpression>, Diagnostic>,
    cancellation: CancellationToken,
}

/// Identity uses an opaque allocation, never URI/text/revision-number equality.
#[derive(Clone, Debug)]
pub struct Snapshot(Arc<SnapshotData>);
impl Snapshot {
    pub fn revision(&self) -> u64 { self.0.revision }
    pub fn uri(&self) -> &str { &self.0.uri }
    pub fn document(&self) -> &PolicyDocument { &self.0.document }
    pub fn source(&self) -> &str { &self.document().source }
    pub fn inputs(&self) -> &[InputSpec] { &self.document().inputs }
    pub fn cases(&self) -> &[PolicyCase] { &self.document().cases }
    pub fn diagnostic(&self) -> Option<&Diagnostic> { self.0.compiled.as_ref().err() }
    pub fn value_type(&self) -> Option<Type> { self.0.compiled.as_ref().ok().map(|p| p.value_type()) }
    pub fn same_revision(&self, other: &Self) -> bool { Arc::ptr_eq(&self.0, &other.0) }

    fn create(revision: u64, uri: Arc<str>, document: PolicyDocument) -> Result<Self, SessionError> {
        check_source(&document.source)?;
        check_schema(&document.inputs).map_err(SessionError::InvalidMetadata)?;
        check_cases(&document.cases)?;
        let compiled = compile_with_inputs(&document.source, &document.inputs).map(Arc::new);
        Ok(Self(Arc::new(SnapshotData {
            revision, uri, document: Arc::new(document), compiled,
            cancellation: CancellationToken::new(),
        })))
    }
    fn committed_copy(&self) -> Self {
        Self(Arc::new(SnapshotData {
            revision: self.revision(), uri: self.0.uri.clone(), document: self.0.document.clone(),
            compiled: self.0.compiled.clone(), cancellation: CancellationToken::new(),
        }))
    }
}

/// One typed policy document. Mutating methods require its single owning thread.
/// This is not a filesystem transaction, project graph, or authenticated audit log.
#[derive(Debug)]
pub struct PolicySession { current: Snapshot, closed: bool }
impl PolicySession {
    /// Incomplete expression syntax remains inspectable; malformed metadata is rejected.
    pub fn new(uri: &str, document: PolicyDocument) -> Result<Self, SessionError> {
        if uri.trim().is_empty() || uri.len() > MAX_URI_BYTES { return Err(SessionError::InvalidUri); }
        Ok(Self { current: Snapshot::create(1, Arc::from(uri), document)?, closed: false })
    }
    pub fn snapshot(&self) -> Snapshot { self.current.clone() }
    pub fn is_closed(&self) -> bool { self.closed }
    pub fn is_current(&self, snapshot: &Snapshot) -> bool {
        !self.closed && self.current.same_revision(snapshot)
    }
    /// Also match a RunTicket if multiple runs of this revision are in flight.
    /// Currentness alone is not success, latest-request ordering, or deployment.
    pub fn receipt_is_current(&self, receipt: &RunReceipt) -> bool { self.is_current(receipt.snapshot()) }
    pub fn close(&mut self) { self.closed = true; self.current.0.cancellation.cancel(); }
    fn check_base(&self, base: &Snapshot) -> Result<(), SessionError> {
        if self.closed { Err(SessionError::Closed) }
        else if !self.current.same_revision(base) { Err(SessionError::StaleBase) }
        else { Ok(()) }
    }
    fn next_revision(&self) -> Result<u64, SessionError> {
        self.current.revision().checked_add(1).ok_or(SessionError::RevisionExhausted)
    }
    fn install(&mut self, next: Snapshot) -> Snapshot {
        self.current.0.cancellation.cancel();
        self.current = next;
        self.snapshot()
    }
    /// A direct source edit preserves both the input schema and all expectations.
    pub fn edit_source(&mut self, base: &Snapshot, source: &str) -> Result<Snapshot, SessionError> {
        self.check_base(base)?;
        check_source(source)?;
        let mut document = self.current.document().clone();
        document.source = source.to_owned();
        let next = Snapshot::create(self.next_revision()?, self.current.0.uri.clone(), document)?;
        Ok(self.install(next))
    }
    /// Rename source-facing inputs and update code together, without rewriting cases.
    pub fn edit_policy(&mut self, base: &Snapshot, source: &str, inputs: &[InputSpec]) -> Result<Snapshot, SessionError> {
        self.check_base(base)?;
        check_source(source)?;
        check_schema(inputs).map_err(SessionError::InvalidMetadata)?;
        let document = PolicyDocument { source: source.to_owned(), inputs: inputs.to_vec(), cases: self.current.cases().to_vec() };
        let next = Snapshot::create(self.next_revision()?, self.current.0.uri.clone(), document)?;
        Ok(self.install(next))
    }
    /// Editing examples/expected values is separate from editing the implementation.
    pub fn revise_cases(&mut self, base: &Snapshot, cases: &[PolicyCase]) -> Result<Snapshot, SessionError> {
        self.check_base(base)?;
        check_cases(cases)?;
        let document = PolicyDocument { source: self.current.source().to_owned(), inputs: self.current.inputs().to_vec(), cases: cases.to_vec() };
        let next = Snapshot::create(self.next_revision()?, self.current.0.uri.clone(), document)?;
        Ok(self.install(next))
    }
    /// Move the returned immutable request to a host worker; no thread is spawned here.
    pub fn prepare_case(&self, id: &str, limits: Limits, options: ObservationOptions) -> Result<RunRequest, SessionError> {
        self.check_base(&self.current)?;
        let index = self.current.cases().iter().position(|c| c.id == id).ok_or(SessionError::UnknownCase)?;
        Ok(RunRequest { snapshot: self.snapshot(), index, limits, options, ticket: RunTicket(Arc::new(())) })
    }
    pub fn propose(&self, base: &Snapshot, document: PolicyDocument, reason: &str, author: Author) -> Result<ChangeProposal, SessionError> {
        self.check_base(base)?;
        if reason.trim().is_empty() || reason.len() > MAX_REASON_BYTES { return Err(SessionError::InvalidReason); }
        let candidate = Snapshot::create(self.next_revision()?, self.current.0.uri.clone(), document)?;
        Ok(ChangeProposal { base: base.clone(), candidate, reason: reason.to_owned(), author })
    }
    /// Apply only the private candidate carried by this completed, immutable review.
    /// Acknowledgment permits value changes, not failed execution or missing evidence.
    pub fn apply(&mut self, review: &ChangeReview, historical: HistoricalPolicy) -> Result<Snapshot, SessionError> {
        self.check_base(review.proposal.base())?;
        if review.proposal.candidate.diagnostic().is_some() { return Err(SessionError::InvalidCandidate); }
        let report = review.result.as_ref().map_err(|_| SessionError::ReviewUnavailable)?;
        if PolicyReview::status(&report.baseline_results) != SuiteStatus::Passed {
            return Err(SessionError::BaselineNotVerified);
        }
        if PolicyReview::status(&report.candidate_results) != SuiteStatus::Passed {
            return Err(SessionError::CandidateNotVerified);
        }
        if report.historical_results.iter().any(|r| r.outcome.result.is_err()) {
            return Err(SessionError::HistoricalExecutionFailed);
        }
        if historical == HistoricalPolicy::Preserve && PolicyReview::status(&report.historical_results) != SuiteStatus::Passed {
            return Err(SessionError::HistoricalExpectationFailed);
        }
        // New identity: a staged preview can never be relabeled as a current run.
        Ok(self.install(review.proposal.candidate.committed_copy()))
    }
}
impl Drop for PolicySession {
    fn drop(&mut self) { self.current.0.cancellation.cancel(); }
}

/// Distinguishes requests for the same case/revision with different execution settings.
#[derive(Clone, Debug)]
pub struct RunTicket(Arc<()>);
impl RunTicket {
    pub fn matches(&self, receipt: &RunReceipt) -> bool { Arc::ptr_eq(&self.0, &receipt.ticket.0) }
}
#[derive(Debug)]
pub struct RunRequest {
    snapshot: Snapshot, index: usize, limits: Limits, options: ObservationOptions, ticket: RunTicket,
}
impl RunRequest {
    pub fn snapshot(&self) -> &Snapshot { &self.snapshot }
    pub fn ticket(&self) -> RunTicket { self.ticket.clone() }
    pub fn execute(self) -> RunReceipt { self.execute_observed(|_, _| ControlFlow::Continue(())) }
    pub fn execute_observed<F: FnMut(&Snapshot, &TraceStep) -> ControlFlow<()>>(self, mut observer: F) -> RunReceipt {
        let (phase, observed) = match &self.snapshot.0.compiled {
            Ok(program) => (Phase::Execution, evaluate_bound_observed(
                program, &self.snapshot.cases()[self.index].inputs, self.limits, self.options,
                &self.snapshot.0.cancellation, |event| observer(&self.snapshot, event),
            )),
            Err(error) => (Phase::Compilation, ObservedOutcome {
                outcome: Outcome { result: Err(error.clone()), trace: Vec::new(), steps: 0 },
                options: self.options, emitted_events: 0, observed_text_units: 0,
            }),
        };
        RunReceipt { snapshot: self.snapshot, index: self.index, limits: self.limits, ticket: self.ticket, phase, observed }
    }
}

#[derive(Clone, Debug)]
pub struct RunReceipt {
    snapshot: Snapshot, index: usize, limits: Limits, ticket: RunTicket, phase: Phase, observed: ObservedOutcome,
}
impl RunReceipt {
    pub fn snapshot(&self) -> &Snapshot { &self.snapshot }
    pub fn case(&self) -> &PolicyCase { &self.snapshot.cases()[self.index] }
    pub fn limits(&self) -> Limits { self.limits }
    pub fn phase(&self) -> Phase { self.phase }
    pub fn observation(&self) -> &ObservedOutcome { &self.observed }
    pub fn outcome(&self) -> &Outcome { &self.observed.outcome }
    pub fn check(&self) -> CheckState {
        if self.phase == Phase::Compilation { return CheckState::CompilationError; }
        match &self.outcome().result {
            Err(_) => CheckState::ExecutionError,
            Ok(value) if value == &self.case().expected => CheckState::Passed,
            Ok(_) => CheckState::Mismatch,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChangeProposal { base: Snapshot, candidate: Snapshot, reason: String, author: Author }
impl ChangeProposal {
    pub fn base(&self) -> &Snapshot { &self.base }
    pub fn candidate(&self) -> &Snapshot { &self.candidate }
    pub fn reason(&self) -> &str { &self.reason }
    pub fn author(&self) -> Author { self.author }
    pub fn preview(&self, limits: ReviewLimits) -> ChangeReview {
        let result = match (&self.base.0.compiled, &self.candidate.0.compiled) {
            (Ok(base), Ok(candidate)) => review_policies_cancellable(
                base, candidate, self.base.cases(), self.candidate.cases(), limits, &self.base.0.cancellation,
            ),
            (Err(error), _) | (_, Err(error)) => Err(error.clone()),
        };
        ChangeReview { proposal: self.clone(), result }
    }
}

/// No public constructor or mutable report: external copies cannot change approval.
#[derive(Clone, Debug)]
pub struct ChangeReview { proposal: ChangeProposal, result: Result<PolicyReview, Diagnostic> }
impl ChangeReview {
    pub fn proposal(&self) -> &ChangeProposal { &self.proposal }
    pub fn report(&self) -> Result<&PolicyReview, &Diagnostic> { self.result.as_ref() }
}

#[cfg(test)]
mod revision_tests {
    use super::*;
    #[test]
    fn exhausted_revision_does_not_wrap_or_cancel_current() {
        let document = PolicyDocument { source: "1".into(), inputs: vec![], cases: vec![] };
        let mut session = PolicySession {
            current: Snapshot::create(u64::MAX, Arc::from("memory:test"), document).unwrap(), closed: false,
        };
        let base = session.snapshot();
        assert_eq!(session.edit_source(&base, "2").unwrap_err(), SessionError::RevisionExhausted);
        assert!(session.is_current(&base));
        assert!(!base.0.cancellation.is_cancelled());
    }
}
