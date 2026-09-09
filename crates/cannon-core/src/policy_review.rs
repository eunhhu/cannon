//! Review scalar policies using fixed, ID-keyed examples. No file writes or AI calls.
use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use crate::{compile_with_inputs, evaluate_bound_observed, CancellationToken, CompiledExpression,
    Diagnostic, InputBindings, InputSpec, Limits, ObservationOptions, Outcome, Value, MAX_INT,
    MAX_INPUTS, MAX_INPUT_TEXT_UNITS};
use crate::inputs::{boundary_span, valid_id};

pub const MAX_REVIEW_CASES: usize = 128;

#[derive(Clone, Debug)]
pub struct PolicyCase { pub id: String, pub name: String, pub inputs: InputBindings, pub expected: Value }
#[derive(Clone, Debug)]
pub struct CaseResult { pub case: PolicyCase, pub outcome: Outcome, pub passed: bool }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuiteStatus { Passed, Failed, NoCases }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change { pub id: String, pub kind: &'static str }
#[derive(Clone, Copy, Debug)]
pub struct ReviewLimits { pub execution: Limits, pub max_events: usize, pub max_text_units: usize }
impl Default for ReviewLimits {
    fn default() -> Self { Self { execution: Limits::default(), max_events: 100_000, max_text_units: 1_048_576 } }
}
#[derive(Clone, Debug)]
pub struct PolicySnapshot { pub source: String, pub inputs: Vec<InputSpec> }
impl PolicySnapshot {
    fn of(program: &CompiledExpression) -> Self { Self { source: program.source().to_owned(), inputs: program.inputs().to_vec() } }
    pub fn compile(&self) -> Result<CompiledExpression, Diagnostic> { compile_with_inputs(&self.source, &self.inputs) }
}
#[derive(Clone, Debug)]
pub struct PolicyReview {
    pub baseline: PolicySnapshot, pub candidate: PolicySnapshot,
    pub logic_changed: bool, pub input_changes: Vec<Change>, pub case_changes: Vec<Change>,
    pub baseline_results: Vec<CaseResult>, pub candidate_results: Vec<CaseResult>, pub historical_results: Vec<CaseResult>,
    pub limits: ReviewLimits,
}
impl PolicyReview {
    pub fn status(results: &[CaseResult]) -> SuiteStatus {
        if results.is_empty() { SuiteStatus::NoCases }
        else if results.iter().all(|r| r.passed) { SuiteStatus::Passed } else { SuiteStatus::Failed }
    }
    /// Empty suites and failed baselines cannot turn a review into a passing gate.
    pub fn passed(&self) -> bool {
        [&self.baseline_results, &self.candidate_results, &self.historical_results]
            .iter().all(|s| Self::status(s) == SuiteStatus::Passed)
    }
    pub fn regressions(&self) -> Vec<&str> {
        self.baseline_results.iter().zip(&self.historical_results)
            .filter(|(old, new)| old.passed && !new.passed).map(|(_, new)| new.case.id.as_str()).collect()
    }
}
fn validate_cases(cases: &[PolicyCase]) -> Result<(), Diagnostic> {
    let error = |m| Diagnostic::new("CASE_SCHEMA", m, boundary_span());
    if cases.len() > MAX_REVIEW_CASES { return Err(error("At most 128 cases are supported.")); }
    let mut ids = BTreeSet::new();
    for case in cases {
        if !valid_id(&case.id) || !ids.insert(&case.id) || case.name.len() > 1024 || case.inputs.len() > MAX_INPUTS {
            return Err(error("Invalid or duplicate case identity, name, or input count."));
        }
        let mut text = 0usize;
        // Reject oversized metadata before cloning it into a review. Invalid input
        // types/refinements still become explicit execution failures, not repairs.
        for value in case.inputs.values().chain(std::iter::once(&case.expected)) {
            if let Value::Text(units) = value {
                text = text.checked_add(units.len()).filter(|n| *n <= MAX_INPUT_TEXT_UNITS)
                    .ok_or_else(|| error("Case text budget exceeded."))?;
            }
        }
        if case.inputs.keys().any(|id| !valid_id(id)) { return Err(error("Invalid case input ID.")); }
        if matches!(&case.expected, Value::Int(n) if !(-MAX_INT..=MAX_INT).contains(n)) {
            return Err(error("Expected values must belong to the language integer domain."));
        }
    }
    Ok(())
}
struct Budget { limits: ReviewLimits, events: usize, text: usize }
impl Budget {
    fn run(&mut self, program: &CompiledExpression, cases: &[PolicyCase]) -> Result<Vec<CaseResult>, Diagnostic> {
        let mut results = Vec::new();
        for case in cases {
            let mut exhausted = false;
            let limits = self.limits;
            let observed = evaluate_bound_observed(program, &case.inputs, limits.execution,
                ObservationOptions { retain_trace: true, max_trace_text_units: limits.max_text_units },
                &CancellationToken::new(), |event| {
                    let units = match &event.value { Some(Value::Text(s)) => s.len(), _ => 0 };
                    match (self.events.checked_add(1), self.text.checked_add(units)) {
                        (Some(count), Some(text)) if count <= limits.max_events && text <= limits.max_text_units => {
                            self.events = count; self.text = text; ControlFlow::Continue(())
                        }
                        _ => { exhausted = true; ControlFlow::Break(()) }
                    }
                });
            if exhausted || matches!(&observed.outcome.result, Err(e) if e.code == "TRACE_VALUE_LIMIT") { return Err(Diagnostic::new("REVIEW_LIMIT", "Cumulative review trace budget exhausted; no complete review.", boundary_span())); }
            let outcome = observed.outcome;
            let passed = matches!(&outcome.result, Ok(value) if value == &case.expected);
            results.push(CaseResult { case: case.clone(), outcome, passed });
        }
        Ok(results)
    }
}

pub fn review_policies(baseline: &CompiledExpression, candidate: &CompiledExpression,
    old_cases: &[PolicyCase], new_cases: &[PolicyCase], limits: ReviewLimits) -> Result<PolicyReview, Diagnostic> {
    validate_cases(old_cases)?; validate_cases(new_cases)?;
    let e = limits.execution;
    if e.max_steps == 0 || e.max_steps > 1_000_000 || e.max_depth == 0 || e.max_depth > crate::MAX_DEPTH
        || e.max_trace == 0 || e.max_trace > 1_000_000 || limits.max_events == 0 || limits.max_events > 1_000_000
        || limits.max_text_units > 16_777_216 {
        return Err(Diagnostic::new("INVALID_LIMIT", "Invalid review or execution limits.", boundary_span()));
    }
    let before: BTreeMap<_, _> = baseline.inputs().iter().map(|s| (s.id.as_str(), s)).collect();
    let after: BTreeMap<_, _> = candidate.inputs().iter().map(|s| (s.id.as_str(), s)).collect();
    let mut input_changes = Vec::new();
    for id in before.keys().chain(after.keys()).copied().collect::<BTreeSet<_>>() {
        let mut add = |kind| input_changes.push(Change { id: id.to_owned(), kind });
        match (before.get(id), after.get(id)) {
            (Some(a), Some(b)) => { if a.name != b.name { add("renamed"); } if a.input_type != b.input_type { add("type-changed"); } }
            (None, Some(_)) => add("added"), (Some(_), None) => add("removed"), _ => {}
        }
    }
    let before: BTreeMap<_, _> = old_cases.iter().map(|c| (c.id.as_str(), c)).collect();
    let after: BTreeMap<_, _> = new_cases.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut case_changes = Vec::new();
    for id in before.keys().chain(after.keys()).copied().collect::<BTreeSet<_>>() {
        let mut add = |kind| case_changes.push(Change { id: id.to_owned(), kind });
        match (before.get(id), after.get(id)) {
            (Some(a), Some(b)) => {
                if a.name != b.name { add("renamed"); }
                if a.inputs != b.inputs { add("input-changed"); }
                if a.expected != b.expected { add("expectation-changed"); }
            }
            (None, Some(_)) => add("added"), (Some(_), None) => add("removed"), _ => {}
        }
    }
    let mut budget = Budget { limits, events: 0, text: 0 };
    let baseline_results = budget.run(baseline, old_cases)?;
    let candidate_results = budget.run(candidate, new_cases)?;
    // Never derive historical inputs or expectations from the candidate's suite.
    let historical_results = budget.run(candidate, old_cases)?;
    Ok(PolicyReview { baseline: PolicySnapshot::of(baseline), candidate: PolicySnapshot::of(candidate),
        logic_changed: !baseline.same_logic(candidate), input_changes, case_changes,
        baseline_results, candidate_results, historical_results, limits })
}
