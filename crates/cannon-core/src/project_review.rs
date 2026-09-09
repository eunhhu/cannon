//! Structural architecture facts and preserved-case replay for compiled projects.
//! No AI interpretation, deployment claims, file writes, or approval credentials.
use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::sync::Arc;

use crate::inputs::valid_id;
use crate::project::{execute_project_observed, CompiledProject, InputSource,
    ProjectError, ProjectLimits, ProjectRun, MAX_POLICIES};
use crate::{CancellationToken, InputBindings, Value, MAX_INPUTS, MAX_INPUT_TEXT_UNITS, MAX_INT};

pub const MAX_PROJECT_CASES: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectChange { pub scope: &'static str, pub id: String, pub kind: &'static str }
#[derive(Clone, Debug)]
pub struct ProjectDifference {
    pub changes: Vec<ProjectChange>,
    /// Conservative consumers in either version; not a proof of changed behavior.
    pub impacted_policies: BTreeSet<String>,
}
fn add(changes: &mut Vec<ProjectChange>, scope: &'static str, id: &str, kind: &'static str) {
    changes.push(ProjectChange { scope, id: id.to_owned(), kind });
}
fn keys<'a, T, U>(before: &'a BTreeMap<&str, T>, after: &'a BTreeMap<&str, U>) -> BTreeSet<&'a str> {
    before.keys().chain(after.keys()).copied().collect()
}

/// Compute changes from declarations and compiled expressions, not generated prose.
pub fn compare_projects(before: &CompiledProject, after: &CompiledProject) -> ProjectDifference {
    let mut changes = Vec::new();
    let a: BTreeMap<_, _> = before.spec().modules.iter().map(|m| (m.id.as_str(), m)).collect();
    let b: BTreeMap<_, _> = after.spec().modules.iter().map(|m| (m.id.as_str(), m)).collect();
    for id in keys(&a, &b) {
        match (a.get(id), b.get(id)) {
            (Some(x), Some(y)) => {
                if x.name != y.name { add(&mut changes, "module", id, "renamed"); }
                if x.imports != y.imports { add(&mut changes, "module", id, "imports-changed"); }
            }
            (None, Some(_)) => add(&mut changes, "module", id, "added"),
            (Some(_), None) => add(&mut changes, "module", id, "removed"),
            _ => {}
        }
    }
    let a: BTreeMap<_, _> = before.spec().inputs.iter().map(|m| (m.spec.id.as_str(), m)).collect();
    let b: BTreeMap<_, _> = after.spec().inputs.iter().map(|m| (m.spec.id.as_str(), m)).collect();
    for id in keys(&a, &b) {
        match (a.get(id), b.get(id)) {
            (Some(x), Some(y)) => {
                if x.module != y.module { add(&mut changes, "input", id, "owner-changed"); }
                if x.spec.name != y.spec.name { add(&mut changes, "input", id, "renamed"); }
                if x.spec.input_type != y.spec.input_type { add(&mut changes, "input", id, "type-changed"); }
            }
            (None, Some(_)) => add(&mut changes, "input", id, "added"),
            (Some(_), None) => add(&mut changes, "input", id, "removed"),
            _ => {}
        }
    }
    let a: BTreeMap<_, _> = before.spec().policies.iter().map(|m| (m.id.as_str(), m)).collect();
    let b: BTreeMap<_, _> = after.spec().policies.iter().map(|m| (m.id.as_str(), m)).collect();
    for id in keys(&a, &b) {
        match (a.get(id), b.get(id)) {
            (Some(x), Some(y)) => {
                if x.name != y.name { add(&mut changes, "policy", id, "renamed"); }
                if x.module != y.module { add(&mut changes, "policy", id, "owner-changed"); }
                if x.exported != y.exported { add(&mut changes, "policy", id, "visibility-changed"); }
                if x.inputs != y.inputs { add(&mut changes, "policy", id, "input-schema-changed"); }
                if x.links != y.links { add(&mut changes, "policy", id, "links-changed"); }
                if x.uri != y.uri { add(&mut changes, "policy", id, "source-uri-changed"); }
                if x.source != y.source { add(&mut changes, "policy", id, "source-text-changed"); }
                if !before.expression(id).expect("compiled ID").same_logic(after.expression(id).expect("compiled ID")) {
                    add(&mut changes, "policy", id, "logic-changed");
                }
            }
            (None, Some(_)) => add(&mut changes, "policy", id, "added"),
            (Some(_), None) => add(&mut changes, "policy", id, "removed"),
            _ => {}
        }
    }
    let mut impacted_policies = BTreeSet::new();
    for project in [before, after] {
        let mut seeds = BTreeSet::new();
        for change in &changes {
            for policy in &project.spec().policies {
                let impacted = match change.scope {
                    "policy" => policy.id == change.id,
                    "module" => policy.module == change.id,
                    "input" => policy.links.values().any(|link| matches!(link, InputSource::External(id) if id == &change.id)),
                    _ => false,
                };
                if impacted { seeds.insert(policy.id.clone()); }
            }
        }
        impacted_policies.extend(project.impacted(&seeds));
    }
    ProjectDifference { changes, impacted_policies }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectCase {
    pub id: String,
    pub name: String,
    pub inputs: InputBindings,
    /// Independent assertions keyed by policy ID, not display name or position.
    pub expected: InputBindings,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseStatus { Passed, Mismatch, ExecutionError, NoExpectations }
#[derive(Debug)]
pub struct ProjectCaseResult { case: ProjectCase, run: ProjectRun, status: CaseStatus, failed_assertions: Vec<String> }
impl ProjectCaseResult {
    pub fn case(&self) -> &ProjectCase { &self.case }
    pub fn run(&self) -> &ProjectRun { &self.run }
    pub fn status(&self) -> CaseStatus { self.status }
    pub fn failed_assertions(&self) -> &[String] { &self.failed_assertions }
}
#[derive(Clone, Copy, Debug)]
pub struct ProjectReviewLimits {
    pub execution: ProjectLimits,
    pub max_steps: usize,
    pub max_events: usize,
    pub max_text_units: usize,
}
impl Default for ProjectReviewLimits {
    fn default() -> Self {
        Self { execution: ProjectLimits::default(), max_steps: 1_000_000,
            max_events: 1_000_000, max_text_units: 4_194_304 }
    }
}
#[derive(Debug)]
pub struct ProjectReview {
    baseline: Arc<CompiledProject>, candidate: Arc<CompiledProject>,
    difference: ProjectDifference, case_changes: Vec<ProjectChange>,
    baseline_results: Vec<ProjectCaseResult>, candidate_results: Vec<ProjectCaseResult>, historical_results: Vec<ProjectCaseResult>,
    limits: ProjectReviewLimits,
}
impl ProjectReview {
    pub fn baseline(&self) -> &Arc<CompiledProject> { &self.baseline }
    pub fn candidate(&self) -> &Arc<CompiledProject> { &self.candidate }
    pub fn difference(&self) -> &ProjectDifference { &self.difference }
    pub fn case_changes(&self) -> &[ProjectChange] { &self.case_changes }
    pub fn baseline_results(&self) -> &[ProjectCaseResult] { &self.baseline_results }
    pub fn candidate_results(&self) -> &[ProjectCaseResult] { &self.candidate_results }
    pub fn historical_results(&self) -> &[ProjectCaseResult] { &self.historical_results }
    pub fn limits(&self) -> ProjectReviewLimits { self.limits }
    pub fn passed(&self) -> bool {
        [&self.baseline_results, &self.candidate_results, &self.historical_results].iter()
            .all(|suite| !suite.is_empty() && suite.iter().all(|r| r.status == CaseStatus::Passed))
    }
    pub fn regressions(&self) -> Vec<&str> {
        self.baseline_results.iter().zip(&self.historical_results)
            .filter(|(a, b)| a.status == CaseStatus::Passed && b.status != CaseStatus::Passed)
            .map(|(a, _)| a.case.id.as_str()).collect()
    }
}

fn validate_cases(cases: &[ProjectCase]) -> Result<usize, ProjectError> {
    let fail = || ProjectError::new("PROJECT_CASE_SCHEMA", None, "Invalid/duplicate case metadata, expected value or case payload budget.");
    if cases.len() > MAX_PROJECT_CASES { return Err(fail()); }
    let mut ids = BTreeSet::new();
    let mut total_text = 0usize;
    for case in cases {
        if !valid_id(&case.id) || !ids.insert(&case.id) || case.name.trim().is_empty() || case.name.len() > 1024
            || case.inputs.len() > MAX_INPUTS || case.expected.len() > MAX_POLICIES
            || case.inputs.keys().chain(case.expected.keys()).any(|id| !valid_id(id)) {
            return Err(fail());
        }
        let mut case_text = 0usize;
        for value in case.inputs.values().chain(case.expected.values()) {
            if let Value::Text(text) = value {
                case_text = case_text.checked_add(text.len()).filter(|n| *n <= MAX_INPUT_TEXT_UNITS).ok_or_else(fail)?;
            }
        }
        if case.expected.values().any(|v| matches!(v, Value::Int(n) if !(-MAX_INT..=MAX_INT).contains(n))) {
            return Err(fail());
        }
        total_text = total_text.checked_add(case_text).filter(|n| *n <= 1_048_576).ok_or_else(fail)?;
    }
    Ok(total_text)
}
fn compare_cases(before: &[ProjectCase], after: &[ProjectCase]) -> Vec<ProjectChange> {
    let a: BTreeMap<_, _> = before.iter().map(|c| (c.id.as_str(), c)).collect();
    let b: BTreeMap<_, _> = after.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut changes = Vec::new();
    for id in keys(&a, &b) {
        match (a.get(id), b.get(id)) {
            (Some(x), Some(y)) => {
                if x.name != y.name { add(&mut changes, "case", id, "renamed"); }
                if x.inputs != y.inputs { add(&mut changes, "case", id, "inputs-changed"); }
                if x.expected != y.expected { add(&mut changes, "case", id, "expectations-changed"); }
            }
            (None, Some(_)) => add(&mut changes, "case", id, "added"),
            (Some(_), None) => add(&mut changes, "case", id, "removed"),
            _ => {}
        }
    }
    changes
}
struct Budget { limits: ProjectReviewLimits, steps: usize, events: usize, text: usize }
impl Budget {
    fn run(&mut self, project: &Arc<CompiledProject>, cases: &[ProjectCase], token: &CancellationToken) -> Result<Vec<ProjectCaseResult>, ProjectError> {
        let incomplete = || ProjectError::new("REVIEW_INCOMPLETE", None, "Review cancelled or exhausted an execution/aggregate budget; no complete review.");
        let mut results = Vec::new();
        for case in cases {
            if token.is_cancelled() || self.steps >= self.limits.max_steps || self.events >= self.limits.max_events { return Err(incomplete()); }
            let mut limits = self.limits.execution;
            limits.max_steps = limits.max_steps.min(self.limits.max_steps - self.steps);
            limits.max_events = limits.max_events.min(self.limits.max_events - self.events);
            limits.max_text_units = limits.max_text_units.min(self.limits.max_text_units - self.text);
            let run = execute_project_observed(project, &case.inputs, limits, token, |_, _| ControlFlow::Continue(()));
            self.steps += run.steps();
            self.events += run.emitted_events();
            self.text += run.observed_text_units();
            if let Err(error) = run.result() {
                if matches!(error.code, "CANCELLED" | "PROJECT_BUDGET")
                    || error.diagnostic.as_ref().is_some_and(|d| matches!(d.code,
                        "CANCELLED" | "STEP_LIMIT" | "DEPTH_LIMIT" | "TRACE_LIMIT" | "TRACE_VALUE_LIMIT")) {
                    return Err(incomplete());
                }
            }
            let mut failed_assertions = Vec::new();
            let status = match run.result() {
                Err(_) => CaseStatus::ExecutionError,
                Ok(_) if case.expected.is_empty() => CaseStatus::NoExpectations,
                Ok(values) => {
                    failed_assertions.extend(case.expected.iter()
                        .filter(|(id, expected)| values.get(*id) != Some(*expected)).map(|(id, _)| id.clone()));
                    if failed_assertions.is_empty() { CaseStatus::Passed } else { CaseStatus::Mismatch }
                }
            };
            results.push(ProjectCaseResult { case: case.clone(), run, status, failed_assertions });
        }
        Ok(results)
    }
}

/// Replays original inputs and answers against the candidate graph even when its
/// own cases are changed/deleted. Returned reviews are read-only scoped evidence.
pub fn review_projects(baseline: &Arc<CompiledProject>, candidate: &Arc<CompiledProject>,
    old_cases: &[ProjectCase], new_cases: &[ProjectCase], limits: ProjectReviewLimits,
    cancellation: &CancellationToken,
) -> Result<ProjectReview, ProjectError> {
    let old_text = validate_cases(old_cases)?;
    let new_text = validate_cases(new_cases)?;
    if old_text + new_text > 1_048_576 {
        return Err(ProjectError::new("PROJECT_CASE_SCHEMA", None, "Combined case payload exceeds the review limit."));
    }
    limits.execution.validate()?;
    if limits.max_steps == 0 || limits.max_steps > 10_000_000 || limits.max_events == 0 || limits.max_events > 1_000_000
        || limits.max_text_units > 16_777_216 {
        return Err(ProjectError::new("INVALID_LIMIT", None, "Invalid aggregate review limits."));
    }
    if cancellation.is_cancelled() {
        return Err(ProjectError::new("REVIEW_INCOMPLETE", None, "Review cancelled before starting."));
    }
    let mut budget = Budget { limits, steps: 0, events: 0, text: 0 };
    let baseline_results = budget.run(baseline, old_cases, cancellation)?;
    let candidate_results = budget.run(candidate, new_cases, cancellation)?;
    let historical_results = budget.run(candidate, old_cases, cancellation)?;
    Ok(ProjectReview { baseline: baseline.clone(), candidate: candidate.clone(),
        difference: compare_projects(baseline, candidate), case_changes: compare_cases(old_cases, new_cases),
        baseline_results, candidate_results, historical_results, limits })
}
