//! Bounded, explicitly wired scalar-policy projects over the existing evaluator.
//! This is a host API, not a replacement parser or an external-effect sandbox.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::ControlFlow;
use std::sync::Arc;

use crate::inputs::{check_schema, resolve, valid_id};
use crate::{compile_with_inputs, evaluate_bound_observed, CancellationToken, CompiledExpression,
    Diagnostic, InputBindings, InputSpec, Limits, ObservationOptions, ObservedOutcome,
    TraceStep, Value, MAX_DEPTH, MAX_INPUTS, MAX_SOURCE_UNITS};

pub const MAX_MODULES: usize = 64;
pub const MAX_POLICIES: usize = 128;
pub const MAX_PROJECT_SOURCE_UNITS: usize = 1_048_576;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleSpec {
    pub id: String,
    pub name: String,
    /// Direct permitted dependencies, not transitive access grants.
    pub imports: BTreeSet<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedInput { pub module: String, pub spec: InputSpec }
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputSource { External(String), Policy(String) }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicySpec {
    pub id: String,
    pub name: String,
    pub module: String,
    pub exported: bool,
    pub uri: String,
    pub source: String,
    pub inputs: Vec<InputSpec>,
    /// Every local port ID is explicitly connected exactly once.
    pub links: BTreeMap<String, InputSource>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSpec {
    pub modules: Vec<ModuleSpec>,
    pub inputs: Vec<OwnedInput>,
    pub policies: Vec<PolicySpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectError {
    pub code: &'static str,
    /// A declaration ID, never a fabricated source offset.
    pub subject: Option<String>,
    pub message: String,
    /// Present only when the existing compiler/evaluator supplied this diagnostic.
    pub diagnostic: Option<Diagnostic>,
}
impl ProjectError {
    pub(crate) fn new(code: &'static str, subject: Option<&str>, message: impl Into<String>) -> Self {
        Self { code, subject: subject.map(str::to_owned), message: message.into(), diagnostic: None }
    }
    fn engine(code: &'static str, subject: Option<&str>, diagnostic: Diagnostic) -> Self {
        Self { code, subject: subject.map(str::to_owned), message: diagnostic.message.clone(), diagnostic: Some(diagnostic) }
    }
}
impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}: {}", self.code,
            self.subject.as_ref().map(|s| format!(" [{s}]")).unwrap_or_default(), self.message)
    }
}
impl std::error::Error for ProjectError {}

/// Read-only compiled graph. Cloning an Arc preserves exact project identity.
#[derive(Debug)]
pub struct CompiledProject {
    spec: ProjectSpec,
    compiled: BTreeMap<String, CompiledExpression>,
    order: Vec<String>,
    dependencies: BTreeMap<String, BTreeSet<String>>,
    external_validator: CompiledExpression,
}
impl CompiledProject {
    pub fn spec(&self) -> &ProjectSpec { &self.spec }
    pub fn order(&self) -> &[String] { &self.order }
    pub fn dependencies(&self) -> &BTreeMap<String, BTreeSet<String>> { &self.dependencies }
    pub fn policy(&self, id: &str) -> Option<&PolicySpec> { self.spec.policies.iter().find(|p| p.id == id) }
    pub fn expression(&self, id: &str) -> Option<&CompiledExpression> { self.compiled.get(id) }
    /// Conservative transitive consumers, including known seeds themselves.
    pub fn impacted(&self, seeds: &BTreeSet<String>) -> BTreeSet<String> {
        let mut result: BTreeSet<_> = seeds.iter().filter(|id| self.compiled.contains_key(*id)).cloned().collect();
        loop {
            let added: Vec<_> = self.dependencies.iter()
                .filter(|(id, deps)| !result.contains(*id) && deps.iter().any(|d| result.contains(d)))
                .map(|(id, _)| id.clone()).collect();
            if added.is_empty() { break; }
            result.extend(added);
        }
        result
    }
}

fn label_ok(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.len() <= max && !text.chars().any(char::is_control)
}
fn topological(graph: &BTreeMap<String, BTreeSet<String>>, code: &'static str) -> Result<Vec<String>, ProjectError> {
    let mut done = BTreeSet::new();
    let mut order = Vec::with_capacity(graph.len());
    while done.len() < graph.len() {
        let next = graph.iter().find(|(id, deps)| !done.contains(*id) && deps.iter().all(|d| done.contains(d)));
        let Some((id, _)) = next else {
            return Err(ProjectError::new(code, None, "Dependency cycle; no execution plan was created."));
        };
        done.insert(id.clone());
        order.push(id.clone());
    }
    Ok(order)
}

/// Validate the complete graph before any policy runs. The caller's spec is copied.
/// Identity namespaces are separate for modules, external inputs, policies and local ports.
pub fn compile_project(input: &ProjectSpec) -> Result<Arc<CompiledProject>, ProjectError> {
    let schema = |id: Option<&str>, message: &str| ProjectError::new("PROJECT_SCHEMA", id, message);
    if input.modules.is_empty() || input.modules.len() > MAX_MODULES || input.policies.is_empty()
        || input.policies.len() > MAX_POLICIES || input.inputs.len() > MAX_INPUTS {
        return Err(schema(None, "Require 1..=64 modules, 1..=128 policies and at most 256 external inputs."));
    }
    let mut modules = BTreeMap::new();
    for module in &input.modules {
        if !valid_id(&module.id) || !label_ok(&module.name, 128) || module.imports.len() > MAX_MODULES
            || modules.insert(module.id.clone(), module).is_some() {
            return Err(schema(Some(&module.id), "Invalid or duplicate module identity/name/import count."));
        }
    }
    let mut module_graph = BTreeMap::new();
    for module in &input.modules {
        if module.imports.iter().any(|id| !modules.contains_key(id)) {
            return Err(schema(Some(&module.id), "Import refers to an unknown module."));
        }
        module_graph.insert(module.id.clone(), module.imports.clone());
    }
    topological(&module_graph, "MODULE_CYCLE")?;
    let mut external = BTreeMap::new();
    for owned in &input.inputs {
        check_schema(std::slice::from_ref(&owned.spec))
            .map_err(|d| ProjectError::engine("PROJECT_INPUT_SCHEMA", Some(&owned.spec.id), d))?;
        if !modules.contains_key(&owned.module) || external.insert(owned.spec.id.clone(), owned).is_some() {
            return Err(schema(Some(&owned.spec.id), "Unknown input owner or duplicate external input ID."));
        }
    }
    let mut policy_map = BTreeMap::new();
    let mut units = 0usize;
    for policy in &input.policies {
        if !valid_id(&policy.id) || !label_ok(&policy.name, 128) || !label_ok(&policy.uri, 4096)
            || !modules.contains_key(&policy.module) || policy.links.len() > MAX_INPUTS
            || policy_map.insert(policy.id.clone(), policy).is_some() {
            return Err(schema(Some(&policy.id), "Invalid/duplicate policy metadata, owner or link count."));
        }
        let size = policy.source.encode_utf16().take(MAX_SOURCE_UNITS + 1).count();
        if size > MAX_SOURCE_UNITS {
            return Err(ProjectError::new("PROJECT_SOURCE_LIMIT", Some(&policy.id), "Policy source exceeds its limit."));
        }
        units = units.checked_add(size).filter(|n| *n <= MAX_PROJECT_SOURCE_UNITS)
            .ok_or_else(|| ProjectError::new("PROJECT_SOURCE_LIMIT", None, "Combined project source exceeds its limit."))?;
    }
    let mut compiled = BTreeMap::new();
    for policy in policy_map.values() {
        let expression = compile_with_inputs(&policy.source, &policy.inputs)
            .map_err(|d| ProjectError::engine("POLICY_COMPILE", Some(&policy.id), d))?;
        compiled.insert(policy.id.clone(), expression);
    }
    let mut dependencies = BTreeMap::new();
    for policy in policy_map.values() {
        if policy.links.len() != policy.inputs.len()
            || policy.links.keys().any(|id| !policy.inputs.iter().any(|s| &s.id == id)) {
            return Err(ProjectError::new("LINK_SCHEMA", Some(&policy.id), "Connect every declared local port, with no extra port IDs."));
        }
        let mut deps = BTreeSet::new();
        for port in &policy.inputs {
            let connection = policy.links.get(&port.id)
                .ok_or_else(|| ProjectError::new("LINK_SCHEMA", Some(&policy.id), "Missing local port connection."))?;
            let actual_type = match connection {
                InputSource::External(id) => {
                    let owned = external.get(id).ok_or_else(|| ProjectError::new("UNKNOWN_EXTERNAL", Some(&policy.id), format!("Unknown external input {id}.")))?;
                    if owned.module != policy.module {
                        return Err(ProjectError::new("INPUT_OWNERSHIP", Some(&policy.id), format!("{id} is owned by {}; publish a policy result instead.", owned.module)));
                    }
                    owned.spec.input_type.base_type()
                }
                InputSource::Policy(id) => {
                    let provider = policy_map.get(id).ok_or_else(|| ProjectError::new("UNKNOWN_POLICY", Some(&policy.id), format!("Unknown policy {id}.")))?;
                    if provider.module != policy.module {
                        if !provider.exported {
                            return Err(ProjectError::new("PRIVATE_POLICY", Some(&policy.id), format!("Policy {id} is not exported.")));
                        }
                        if !modules[&policy.module].imports.contains(&provider.module) {
                            return Err(ProjectError::new("UNDECLARED_IMPORT", Some(&policy.id), format!("Module {} must explicitly import {}.", policy.module, provider.module)));
                        }
                    }
                    deps.insert(id.clone());
                    compiled[id].value_type()
                }
            };
            if actual_type != port.input_type.base_type() {
                return Err(ProjectError::new("LINK_TYPE", Some(&policy.id), format!("Input {} has an incompatible provider type.", port.id)));
            }
        }
        dependencies.insert(policy.id.clone(), deps);
    }
    let order = topological(&dependencies, "POLICY_CYCLE")?;
    // Reuse existing argument validation. Synthetic names are internal only;
    // actual stable IDs and declared refinements are preserved in this validator.
    let validation_inputs: Vec<_> = external.values().enumerate().map(|(index, owned)|
        InputSpec::new(&owned.spec.id, format!("argument_{index}"), owned.spec.input_type)).collect();
    let external_validator = compile_with_inputs("true", &validation_inputs)
        .map_err(|d| ProjectError::engine("PROJECT_INPUT_SCHEMA", None, d))?;
    let mut spec = input.clone();
    spec.modules.sort_by(|a, b| a.id.cmp(&b.id));
    spec.inputs.sort_by(|a, b| a.spec.id.cmp(&b.spec.id));
    spec.policies.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Arc::new(CompiledProject { spec, compiled, order, dependencies, external_validator }))
}

#[derive(Clone, Copy, Debug)]
pub struct ProjectLimits {
    pub per_policy: Limits,
    pub max_steps: usize,
    pub max_events: usize,
    /// Cumulative observed text payload, not a total-process memory limit.
    pub max_text_units: usize,
    pub retain_trace: bool,
}
impl Default for ProjectLimits {
    fn default() -> Self {
        Self { per_policy: Limits::default(), max_steps: 100_000, max_events: 100_000,
            max_text_units: 1_048_576, retain_trace: true }
    }
}
impl ProjectLimits {
    pub(crate) fn validate(self) -> Result<(), ProjectError> {
        let l = self.per_policy;
        if l.max_steps == 0 || l.max_steps > 1_000_000 || l.max_depth == 0 || l.max_depth > MAX_DEPTH
            || l.max_trace == 0 || l.max_trace > 1_000_000 || self.max_steps == 0 || self.max_steps > 1_000_000
            || self.max_events == 0 || self.max_events > 1_000_000 || self.max_text_units > 16_777_216 {
            return Err(ProjectError::new("INVALID_LIMIT", None, "Invalid policy or aggregate project budget."));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct PolicyRun { id: String, bindings: InputBindings, observed: ObservedOutcome }
impl PolicyRun {
    pub fn id(&self) -> &str { &self.id }
    pub fn bindings(&self) -> &InputBindings { &self.bindings }
    pub fn observed(&self) -> &ObservedOutcome { &self.observed }
}
#[derive(Debug)]
pub struct ProjectRun {
    project: Arc<CompiledProject>,
    inputs: InputBindings,
    limits: ProjectLimits,
    nodes: Vec<PolicyRun>,
    result: Result<InputBindings, ProjectError>,
    steps: usize,
    events: usize,
    text_units: usize,
}
impl ProjectRun {
    pub fn project(&self) -> &Arc<CompiledProject> { &self.project }
    pub fn inputs(&self) -> &InputBindings { &self.inputs }
    pub fn limits(&self) -> ProjectLimits { self.limits }
    pub fn nodes(&self) -> &[PolicyRun] { &self.nodes }
    pub fn result(&self) -> &Result<InputBindings, ProjectError> { &self.result }
    pub fn steps(&self) -> usize { self.steps }
    pub fn emitted_events(&self) -> usize { self.events }
    pub fn observed_text_units(&self) -> usize { self.text_units }
    pub fn is_for(&self, project: &Arc<CompiledProject>) -> bool { Arc::ptr_eq(&self.project, project) }
}

fn execute_inner<F: FnMut(&str, &TraceStep) -> ControlFlow<()>>(
    run: &mut ProjectRun, token: &CancellationToken, mut observer: F,
) -> Result<InputBindings, ProjectError> {
    run.limits.validate()?;
    let project = run.project.clone();
    resolve(&project.external_validator, &run.inputs)
        .map_err(|d| ProjectError::engine("PROJECT_INPUT", None, d))?;
    let mut values = InputBindings::new();
    for id in &project.order {
        if token.is_cancelled() {
            return Err(ProjectError::new("CANCELLED", Some(id), "Project execution was cancelled; no completed project result."));
        }
        if run.steps >= run.limits.max_steps || run.events >= run.limits.max_events {
            return Err(ProjectError::new("PROJECT_BUDGET", Some(id), "Aggregate step/event budget exhausted before this policy."));
        }
        let policy = project.policy(id).expect("validated policy ID");
        let mut bindings = InputBindings::new();
        for (port, connection) in &policy.links {
            let value = match connection {
                InputSource::External(key) => &run.inputs[key],
                InputSource::Policy(key) => &values[key],
            };
            bindings.insert(port.clone(), value.clone());
        }
        let per_policy = run.limits.per_policy;
        let limits = Limits {
            max_steps: per_policy.max_steps.min(run.limits.max_steps - run.steps),
            max_depth: per_policy.max_depth,
            max_trace: per_policy.max_trace.min(run.limits.max_events - run.events),
        };
        let options = ObservationOptions { retain_trace: run.limits.retain_trace,
            max_trace_text_units: run.limits.max_text_units - run.text_units };
        let observed = evaluate_bound_observed(&project.compiled[id], &bindings, limits, options, token,
            |event| observer(id, event));
        run.steps += observed.outcome.steps;
        run.events += observed.emitted_events;
        run.text_units += observed.observed_text_units;
        let value = observed.outcome.result.clone();
        run.nodes.push(PolicyRun { id: id.clone(), bindings, observed });
        match value {
            Ok(value) => { values.insert(id.clone(), value); }
            Err(diagnostic) => return Err(ProjectError::engine("POLICY_EXECUTION", Some(id), diagnostic)),
        }
    }
    if token.is_cancelled() {
        return Err(ProjectError::new("CANCELLED", None, "Project execution cancelled before completion."));
    }
    Ok(values)
}

/// Eager dataflow: all declared policies execute once in deterministic topological
/// order. Expression branches retain their normal lazy/short-circuit semantics.
/// The observer is trusted synchronous host code, not Cannon-supplied code.
pub fn execute_project_observed<F: FnMut(&str, &TraceStep) -> ControlFlow<()>>(
    project: &Arc<CompiledProject>, inputs: &InputBindings, limits: ProjectLimits,
    cancellation: &CancellationToken, observer: F,
) -> ProjectRun {
    // Do not clone unbounded external payloads into a long-lived receipt.
    let preflight = limits.validate().and_then(|_| resolve(&project.external_validator, inputs)
        .map(|_| ()).map_err(|d| ProjectError::engine("PROJECT_INPUT", None, d)));
    let mut run = ProjectRun { project: project.clone(), inputs: InputBindings::new(), limits,
        nodes: Vec::new(), result: Ok(InputBindings::new()), steps: 0, events: 0, text_units: 0 };
    if let Err(error) = preflight { run.result = Err(error); return run; }
    run.inputs = inputs.clone();
    run.result = execute_inner(&mut run, cancellation, observer);
    run
}
pub fn execute_project(project: &Arc<CompiledProject>, inputs: &InputBindings, limits: ProjectLimits) -> ProjectRun {
    execute_project_observed(project, inputs, limits, &CancellationToken::new(), |_, _| ControlFlow::Continue(()))
}
