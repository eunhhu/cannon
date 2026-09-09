//! Version-scoped editing and review over the existing native typed expression engine.
//!
//! Handles are process-local capabilities, not serialized IDs or signed evidence.
//! No filesystem, network, full-file parser, or provider-specific policy lives here.
use std::fmt;
use std::ops::ControlFlow;
use std::sync::Arc;

use crate::{
    compile_with_inputs, evaluate_bound_observed, CancellationToken, CompiledExpression,
    Diagnostic, Limits, ObservationOptions, ObservedOutcome, Outcome, TraceStep,
    Type, Value, InputBindings, InputSpec, MAX_INPUTS, MAX_INPUT_TEXT_UNITS, MAX_INT, MAX_SOURCE_UNITS,
};

const MAX_URI_BYTES: usize = 4096;
const MAX_REASON_BYTES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionError {
    Closed,
    StaleBase,
    SourceTooLarge,
    InvalidUri,
    InvalidReason,
    InvalidExpectedValue,
    InvalidInputPayload,
    RevisionExhausted,
    InvalidCandidate,
    CandidateExecutionFailed,
    CandidateExpectationFailed,
    HistoricalExpectationFailed,
}
impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Closed => "SESSION_CLOSED: this document session is closed",
            Self::StaleBase => "STALE_BASE: the exact source revision is no longer current",
            Self::SourceTooLarge => "SOURCE_LIMIT: source exceeds the UTF-16 input budget",
            Self::InvalidUri => "INVALID_URI: use a nonempty URI of at most 4096 bytes",
            Self::InvalidReason => "INVALID_REASON: supply a nonempty rationale of at most 8192 bytes",
            Self::InvalidExpectedValue => "INVALID_EXPECTED_VALUE: expected value exceeds the language domain or text budget",
            Self::InvalidInputPayload => "INVALID_INPUT_PAYLOAD: input metadata or values exceed the session budget",
            Self::RevisionExhausted => "REVISION_EXHAUSTED: revision numbers must never wrap",
            Self::InvalidCandidate => "INVALID_CANDIDATE: candidate source does not compile",
            Self::CandidateExecutionFailed => "CANDIDATE_EXECUTION_FAILED: preview did not complete successfully",
            Self::CandidateExpectationFailed => "CANDIDATE_EXPECTATION_FAILED: candidate output contradicts its own expected value",
            Self::HistoricalExpectationFailed => "HISTORICAL_EXPECTATION_FAILED: explicitly acknowledge the old expectation before changing it",
        };
        f.write_str(message)
    }
}
impl std::error::Error for SessionError {}

/// Caller-declared authorship. This field does not authenticate a person or model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Author { Human, Assistant }

/// A changed historical expectation requires a separate, explicit apply policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoricalPolicy { Preserve, AcknowledgeChange }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState { NotSpecified, Passed, Mismatch, CompilationError, ExecutionError }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase { Compilation, Execution }

/// Mutable input to a proposal; validation and copying happen before it becomes a snapshot.
#[derive(Clone, Debug)]
pub struct SessionDraft {
    pub source: String,
    pub inputs: Vec<InputSpec>,
    pub bindings: InputBindings,
    pub expected: Option<Value>,
}

#[derive(Debug)]
struct CaseData { bindings: InputBindings, expected: Option<Value> }

#[derive(Debug)]
struct SnapshotData {
    revision: u64,
    uri: Arc<str>,
    source: Arc<str>,
    inputs: Arc<[InputSpec]>,
    case: Arc<CaseData>,
    compiled: Result<Arc<CompiledExpression>, Diagnostic>,
    cancellation: CancellationToken,
}

/// Immutable text, expectation, and compilation for one exact revision.
/// Numeric revisions are display metadata; identity is the opaque handle itself.
#[derive(Clone, Debug)]
pub struct Snapshot(Arc<SnapshotData>);
impl Snapshot {
    pub fn revision(&self) -> u64 { self.0.revision }
    pub fn uri(&self) -> &str { &self.0.uri }
    pub fn source(&self) -> &str { &self.0.source }
    pub fn expected(&self) -> Option<&Value> { self.0.case.expected.as_ref() }
    pub fn inputs(&self) -> &[InputSpec] { &self.0.inputs }
    pub fn bindings(&self) -> &InputBindings { &self.0.case.bindings }
    pub fn draft(&self) -> SessionDraft {
        SessionDraft { source: self.source().to_owned(), inputs: self.inputs().to_vec(),
            bindings: self.bindings().clone(), expected: self.expected().cloned() }
    }
    pub fn diagnostic(&self) -> Option<&Diagnostic> { self.0.compiled.as_ref().err() }
    pub fn value_type(&self) -> Option<Type> {
        self.0.compiled.as_ref().ok().map(|p| p.value_type())
    }
    pub fn same_revision(&self, other: &Self) -> bool { Arc::ptr_eq(&self.0, &other.0) }

    fn create(revision: u64, uri: Arc<str>, source: &str, expected: Option<Value>,
        inputs: &[InputSpec], bindings: &InputBindings) -> Result<Self, SessionError> {
        if uri.trim().is_empty() || uri.len() > MAX_URI_BYTES { return Err(SessionError::InvalidUri); }
        if source.encode_utf16().take(MAX_SOURCE_UNITS + 1).count() > MAX_SOURCE_UNITS {
            return Err(SessionError::SourceTooLarge);
        }
        validate_expected(expected.as_ref())?;
        validate_input_payload(inputs, bindings)?;
        Ok(Self(Arc::new(SnapshotData {
            revision, uri, source: Arc::from(source), inputs: Arc::from(inputs),
            case: Arc::new(CaseData { bindings: bindings.clone(), expected }),
            compiled: compile_with_inputs(source, inputs).map(Arc::new),
            cancellation: CancellationToken::new(),
        })))
    }

    // Reuse a checked compilation, but never promote a staged receipt to current
    // evidence. Committing creates a fresh identity and cancellation lifetime.
    fn commit_copy(&self) -> Self {
        Self(Arc::new(SnapshotData {
            revision: self.revision(), uri: self.0.uri.clone(), source: self.0.source.clone(),
            inputs: self.0.inputs.clone(), case: self.0.case.clone(), compiled: self.0.compiled.clone(),
            cancellation: CancellationToken::new(),
        }))
    }
}

fn validate_expected(expected: Option<&Value>) -> Result<(), SessionError> {
    let valid = match expected {
        Some(Value::Int(n)) => (-MAX_INT..=MAX_INT).contains(n),
        Some(Value::Text(text)) => text.len() <= MAX_SOURCE_UNITS,
        _ => true,
    };
    if valid { Ok(()) } else { Err(SessionError::InvalidExpectedValue) }
}

// Bound payloads before cloning them into long-lived session snapshots. Schema
// correctness stays with compile_with_inputs; binding correctness stays with the
// evaluator. Invalid but bounded editing states retain their actual diagnostics.
fn validate_input_payload(inputs: &[InputSpec], bindings: &InputBindings) -> Result<(), SessionError> {
    if inputs.len() > MAX_INPUTS || bindings.len() > MAX_INPUTS
        || inputs.iter().any(|s| s.id.len() > 128 || s.name.len() > 128)
        || bindings.keys().any(|id| id.len() > 128) {
        return Err(SessionError::InvalidInputPayload);
    }
    let mut units = 0usize;
    for value in bindings.values() {
        if let Value::Text(text) = value {
            units = units.checked_add(text.len()).filter(|n| *n <= MAX_INPUT_TEXT_UNITS)
                .ok_or(SessionError::InvalidInputPayload)?;
        }
    }
    Ok(())
}

/// This session owns one scalar policy and one example. It is not a project/module graph.
#[derive(Debug)]
pub struct ExpressionSession { current: Snapshot, closed: bool }
impl ExpressionSession {
    /// Invalid expressions are retained with a diagnostic so editing can continue.
    pub fn new(uri: &str, source: &str, expected: Option<Value>) -> Result<Self, SessionError> {
        Self::new_with_inputs(uri, source, &[], &InputBindings::new(), expected)
    }
    /// Typed scalar inputs are frozen with the source and independent expectation.
    pub fn new_with_inputs(uri: &str, source: &str, inputs: &[InputSpec], bindings: &InputBindings,
        expected: Option<Value>) -> Result<Self, SessionError> {
        Ok(Self { current: Snapshot::create(1, Arc::from(uri), source, expected, inputs, bindings)?, closed: false })
    }
    pub fn snapshot(&self) -> Snapshot { self.current.clone() }
    pub fn is_closed(&self) -> bool { self.closed }
    pub fn is_current(&self, snapshot: &Snapshot) -> bool {
        !self.closed && self.current.same_revision(snapshot)
    }
    /// Checks the editable revision only, not success or newest-request ordering.
    /// A host must also match the requested run/settings before displaying a result.
    pub fn receipt_is_current(&self, receipt: &RunReceipt) -> bool {
        self.is_current(receipt.snapshot()) && Arc::ptr_eq(&receipt.case, &self.current.0.case)
    }
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

    /// A direct text edit never silently changes the expected answer.
    /// Even an identical-text edit gets a fresh revision (no ABA resurrection).
    pub fn edit(&mut self, base: &Snapshot, source: &str) -> Result<Snapshot, SessionError> {
        self.check_base(base)?;
        let next = Snapshot::create(self.next_revision()?, self.current.0.uri.clone(), source,
            self.current.expected().cloned(), self.current.inputs(), self.current.bindings())?;
        Ok(self.install(next))
    }

    /// Schema/data edits do not silently alter source or expected answers.
    pub fn revise_inputs(&mut self, base: &Snapshot, inputs: &[InputSpec], bindings: &InputBindings) -> Result<Snapshot, SessionError> {
        self.check_base(base)?;
        let next = Snapshot::create(self.next_revision()?, self.current.0.uri.clone(), self.current.source(),
            self.current.expected().cloned(), inputs, bindings)?;
        Ok(self.install(next))
    }

    /// Expected answers are an explicit edit, independent of source text edits.
    pub fn revise_expectation(&mut self, base: &Snapshot, expected: Option<Value>) -> Result<Snapshot, SessionError> {
        self.check_base(base)?;
        validate_expected(expected.as_ref())?;
        let next = Snapshot(Arc::new(SnapshotData {
            revision: self.next_revision()?, uri: self.current.0.uri.clone(), source: self.current.0.source.clone(),
            inputs: self.current.0.inputs.clone(),
            case: Arc::new(CaseData { bindings: self.current.bindings().clone(), expected }),
            compiled: self.current.0.compiled.clone(), cancellation: CancellationToken::new(),
        }));
        Ok(self.install(next))
    }

    /// Prepare in the owning thread; execute the immutable request on a worker.
    pub fn prepare_run(&self, limits: Limits, options: ObservationOptions) -> Result<RunRequest, SessionError> {
        self.check_base(&self.current)?;
        Ok(RunRequest { snapshot: self.snapshot(), limits, options })
    }

    /// No mutation, publication, or provider call occurs here. Candidate diagnostics
    /// remain inspectable; invalid candidates cannot later be applied.
    pub fn propose(&self, base: &Snapshot, source: &str, expected: Option<Value>, reason: &str, author: Author) -> Result<ChangeProposal, SessionError> {
        self.check_base(base)?;
        let draft = SessionDraft { source: source.to_owned(), inputs: base.inputs().to_vec(),
            bindings: base.bindings().clone(), expected };
        self.propose_draft(base, &draft, reason, author)
    }

    /// Source, schema, data and expected answers remain separate review facts.
    pub fn propose_draft(&self, base: &Snapshot, draft: &SessionDraft, reason: &str, author: Author) -> Result<ChangeProposal, SessionError> {
        self.check_base(base)?;
        if reason.trim().is_empty() || reason.len() > MAX_REASON_BYTES { return Err(SessionError::InvalidReason); }
        let candidate = Snapshot::create(self.next_revision()?, self.current.0.uri.clone(), &draft.source,
            draft.expected.clone(), &draft.inputs, &draft.bindings)?;
        Ok(ChangeProposal { base: base.clone(), candidate, reason: reason.to_owned(), author })
    }

    /// Apply exactly this immutable review, not a separately replaceable candidate.
    /// This changes in-memory state only; it is neither a file write nor deployment.
    pub fn apply(&mut self, review: &ChangeReview, historical: HistoricalPolicy) -> Result<Snapshot, SessionError> {
        self.check_base(&review.proposal.base)?;
        if review.proposal.candidate.diagnostic().is_some() { return Err(SessionError::InvalidCandidate); }
        if review.candidate.outcome().result.is_err() { return Err(SessionError::CandidateExecutionFailed); }
        if review.candidate.check() == CheckState::Mismatch { return Err(SessionError::CandidateExpectationFailed); }
        if historical == HistoricalPolicy::Preserve && !matches!(review.historical_check(), CheckState::Passed | CheckState::NotSpecified) {
            return Err(SessionError::HistoricalExpectationFailed);
        }
        // The proposal was made against this exact current snapshot, therefore its
        // next revision is current+1. The handle check also rejects another session.
        Ok(self.install(review.proposal.candidate.commit_copy()))
    }
}
impl Drop for ExpressionSession {
    fn drop(&mut self) { self.current.0.cancellation.cancel(); }
}

#[derive(Debug)]
pub struct RunRequest { snapshot: Snapshot, limits: Limits, options: ObservationOptions }
impl RunRequest {
    pub fn snapshot(&self) -> &Snapshot { &self.snapshot }
    pub fn execute(self) -> RunReceipt { self.execute_observed(|_, _| ControlFlow::Continue(())) }
    pub fn execute_observed<F: FnMut(&Snapshot, &TraceStep) -> ControlFlow<()>>(self, mut observer: F) -> RunReceipt {
        let token = self.snapshot.0.cancellation.clone();
        let origin = self.snapshot.clone();
        run_snapshot(self.snapshot, origin.0.case.clone(), self.limits, self.options, &token, |step| observer(&origin, step))
    }
}

/// Read-only receipt bound to the exact source and execution settings.
/// Its currentness is checked against the session at consumption time, not at run time.
#[derive(Clone, Debug)]
pub struct RunReceipt { snapshot: Snapshot, case: Arc<CaseData>, limits: Limits, phase: Phase, observed: ObservedOutcome }
impl RunReceipt {
    pub fn snapshot(&self) -> &Snapshot { &self.snapshot }
    pub fn phase(&self) -> Phase { self.phase }
    pub fn limits(&self) -> Limits { self.limits }
    pub fn observation(&self) -> &ObservedOutcome { &self.observed }
    pub fn outcome(&self) -> &Outcome { &self.observed.outcome }
    /// These are the actual invocation inputs, which may differ from a candidate's
    /// own example during historical replay. Never relabel a replay as that example.
    pub fn bindings(&self) -> &InputBindings { &self.case.bindings }
    pub fn expected(&self) -> Option<&Value> { self.case.expected.as_ref() }
    pub fn check(&self) -> CheckState { check(self.expected(), self) }
}

fn run_snapshot<F: FnMut(&TraceStep) -> ControlFlow<()>>(
    snapshot: Snapshot, case: Arc<CaseData>, limits: Limits, options: ObservationOptions,
    cancellation: &CancellationToken, observer: F,
) -> RunReceipt {
    let (phase, observed) = match &snapshot.0.compiled {
        Ok(program) => (Phase::Execution, evaluate_bound_observed(program, &case.bindings, limits, options, cancellation, observer)),
        Err(error) => (Phase::Compilation, ObservedOutcome {
            outcome: Outcome { result: Err(error.clone()), trace: Vec::new(), steps: 0 },
            options, emitted_events: 0, observed_text_units: 0,
        }),
    };
    RunReceipt { snapshot, case, limits, phase, observed }
}
fn check(expected: Option<&Value>, receipt: &RunReceipt) -> CheckState {
    if receipt.phase == Phase::Compilation { return CheckState::CompilationError; }
    match (&receipt.outcome().result, expected) {
        (Err(_), _) => CheckState::ExecutionError,
        (Ok(_), None) => CheckState::NotSpecified,
        (Ok(actual), Some(expected)) if actual == expected => CheckState::Passed,
        _ => CheckState::Mismatch,
    }
}

#[derive(Clone, Debug)]
pub struct ChangeProposal { base: Snapshot, candidate: Snapshot, reason: String, author: Author }
impl ChangeProposal {
    pub fn base(&self) -> &Snapshot { &self.base }
    pub fn candidate(&self) -> &Snapshot { &self.candidate }
    pub fn reason(&self) -> &str { &self.reason }
    pub fn author(&self) -> Author { self.author }
    pub fn preview(&self, limits: Limits) -> ChangeReview {
        self.preview_with_options(limits, ObservationOptions {
            max_trace_text_units: MAX_SOURCE_UNITS * 4, ..ObservationOptions::default()
        })
    }
    pub fn preview_with_options(&self, limits: Limits, options: ObservationOptions) -> ChangeReview {
        // All three runs observe the base cancellation lifetime. Historical inputs
        // are replayed by stable ID against candidate code, never candidate data.
        let token = &self.base.0.cancellation;
        let baseline = run_snapshot(self.base.clone(), self.base.0.case.clone(), limits, options, token, |_| ControlFlow::Continue(()));
        let candidate = run_snapshot(self.candidate.clone(), self.candidate.0.case.clone(), limits, options, token, |_| ControlFlow::Continue(()));
        let historical = run_snapshot(self.candidate.clone(), self.base.0.case.clone(), limits, options, token, |_| ControlFlow::Continue(()));
        ChangeReview { proposal: self.clone(), baseline, candidate, historical }
    }
}

/// Facts computed from exact inputs, not an AI interpretation of intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangeFacts {
    pub source_text_changed: bool,
    pub expected_value_changed: bool,
    pub result_type_changed: bool,
    pub input_schema_changed: bool,
    pub input_values_changed: bool,
}
#[derive(Clone, Debug)]
pub struct ChangeReview { proposal: ChangeProposal, baseline: RunReceipt, candidate: RunReceipt, historical: RunReceipt }
impl ChangeReview {
    pub fn proposal(&self) -> &ChangeProposal { &self.proposal }
    pub fn baseline(&self) -> &RunReceipt { &self.baseline }
    pub fn candidate(&self) -> &RunReceipt { &self.candidate }
    pub fn historical(&self) -> &RunReceipt { &self.historical }
    pub fn facts(&self) -> ChangeFacts {
        ChangeFacts {
            source_text_changed: self.proposal.base.source() != self.proposal.candidate.source(),
            expected_value_changed: self.proposal.base.expected() != self.proposal.candidate.expected(),
            result_type_changed: self.proposal.base.value_type() != self.proposal.candidate.value_type(),
            input_schema_changed: self.proposal.base.inputs() != self.proposal.candidate.inputs(),
            input_values_changed: self.proposal.base.bindings() != self.proposal.candidate.bindings(),
        }
    }
    /// Always uses the baseline's independently supplied expected value, including
    /// when the candidate changes or removes its own expectation.
    pub fn historical_check(&self) -> CheckState {
        if self.proposal.base.expected().is_none() { CheckState::NotSpecified }
        else { self.historical.check() }
    }
    pub fn introduces_regression(&self) -> bool {
        self.baseline.check() == CheckState::Passed && self.historical_check() != CheckState::Passed
    }
}
