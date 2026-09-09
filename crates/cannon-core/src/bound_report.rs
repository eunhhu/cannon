//! Reports include input schemas and values; these are not approval certificates.
use crate::{CompiledExpression, InputBindings, InputSpec, Limits, Outcome};
use crate::policy_review::{CaseResult, Change, PolicyReview, PolicySnapshot, SuiteStatus};
use crate::report::{quote, render as execution_json, value_json};

fn inputs_json(inputs: &[InputSpec]) -> String {
    format!("[{}]", inputs.iter().map(|s| format!("{{\"id\":{},\"name\":{},\"type\":{}}}",
        quote(&s.id), quote(&s.name), quote(s.input_type.name()))).collect::<Vec<_>>().join(","))
}
fn bindings_json(bindings: &InputBindings) -> String {
    format!("{{{}}}", bindings.iter().map(|(id, value)| format!("{}:{}", quote(id), value_json(value))).collect::<Vec<_>>().join(","))
}
/// Source and schema are explicitly included even if compilation failed. Successful
/// callers pass the compiled arena to include static input-reference locations.
pub fn render(source: &str, inputs: &[InputSpec], bindings: &InputBindings,
    compiled: Option<&CompiledExpression>, limits: Limits, phase: &str, outcome: &Outcome) -> String {
    let refs = compiled.map(|p| p.input_references()).unwrap_or_default().iter().map(|r| {
        format!("{{\"id\":{},\"name\":{},\"span\":{{\"start\":{},\"end\":{},\"line\":{},\"column\":{}}}}}",
            quote(&r.id), quote(&r.name), r.span.start, r.span.end, r.span.line, r.span.column)
    }).collect::<Vec<_>>().join(",");
    format!("{{\"schema\":\"cannon.native.bound-expression/1\",\"inputs\":{},\"bindings\":{},\"inputReferences\":[{}],\"execution\":{}}}",
        inputs_json(inputs), bindings_json(bindings), refs, execution_json(source, "argument.policy", limits, phase, outcome))
}
fn snapshot_json(snapshot: &PolicySnapshot) -> String {
    format!("{{\"source\":{},\"inputs\":{}}}", quote(&snapshot.source), inputs_json(&snapshot.inputs))
}
fn changes_json(changes: &[Change]) -> String {
    format!("[{}]", changes.iter().map(|c| format!("{{\"id\":{},\"kind\":{}}}", quote(&c.id), quote(c.kind))).collect::<Vec<_>>().join(","))
}
fn results_json(snapshot: &PolicySnapshot, cases: &[CaseResult], limits: Limits, uri: &str) -> String {
    format!("[{}]", cases.iter().map(|c| format!(
        "{{\"id\":{},\"name\":{},\"inputs\":{},\"expected\":{},\"passed\":{},\"execution\":{}}}",
        quote(&c.case.id), quote(&c.case.name), bindings_json(&c.case.inputs), value_json(&c.case.expected), c.passed,
        execution_json(&snapshot.source, uri, limits, "execution", &c.outcome))).collect::<Vec<_>>().join(","))
}
fn status(cases: &[CaseResult]) -> &'static str {
    match PolicyReview::status(cases) { SuiteStatus::Passed => "passed", SuiteStatus::Failed => "failed", SuiteStatus::NoCases => "no-cases" }
}
pub fn render_review(review: &PolicyReview) -> String {
    let l = review.limits;
    format!(concat!("{{\"schema\":\"cannon.native.policy-review/1\",\"engineVersion\":{},",
        "\"baseline\":{},\"candidate\":{},\"logicChanged\":{},\"inputChanges\":{},\"caseChanges\":{},",
        "\"baselineStatus\":{},\"candidateStatus\":{},\"historicalStatus\":{},\"passed\":{},\"regressions\":[{}],",
        "\"limits\":{{\"maxSteps\":{},\"maxDepth\":{},\"maxTrace\":{},\"maxEvents\":{},\"maxTextUnits\":{}}},",
        "\"baselineResults\":{},\"candidateResults\":{},\"historicalResults\":{}}}"),
        quote(env!("CARGO_PKG_VERSION")), snapshot_json(&review.baseline), snapshot_json(&review.candidate), review.logic_changed,
        changes_json(&review.input_changes), changes_json(&review.case_changes),
        quote(status(&review.baseline_results)), quote(status(&review.candidate_results)), quote(status(&review.historical_results)), review.passed(),
        review.regressions().iter().map(|id| quote(id)).collect::<Vec<_>>().join(","),
        l.execution.max_steps, l.execution.max_depth, l.execution.max_trace, l.max_events, l.max_text_units,
        results_json(&review.baseline, &review.baseline_results, l.execution, "baseline.policy"),
        results_json(&review.candidate, &review.candidate_results, l.execution, "candidate.policy"),
        results_json(&review.candidate, &review.historical_results, l.execution, "candidate.policy"))
}
