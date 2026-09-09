use std::collections::BTreeSet;
use std::ops::ControlFlow;
use std::sync::Arc;

use cannon_core::{CancellationToken, InputBindings, InputSpec, InputType, Value, MAX_INT};
use cannon_core::project::*;
use cannon_core::project_review::*;

fn module(id: &str, imports: &[&str]) -> ModuleSpec {
    ModuleSpec { id: id.into(), name: id.into(), imports: imports.iter().map(|s| (*s).into()).collect() }
}
fn owned(id: &str, name: &str, ty: InputType) -> OwnedInput {
    OwnedInput { module: "Inventory".into(), spec: InputSpec::new(id, name, ty) }
}
fn policy(id: &str, owner: &str, source: &str, ports: &[(&str, &str, InputType, InputSource)]) -> PolicySpec {
    PolicySpec { id: id.into(), name: id.into(), module: owner.into(), exported: false,
        uri: format!("memory:{id}"), source: source.into(),
        inputs: ports.iter().map(|(id, name, ty, _)| InputSpec::new(*id, *name, *ty)).collect(),
        links: ports.iter().map(|(id, _, _, link)| ((*id).into(), link.clone())).collect() }
}
fn fixture() -> ProjectSpec {
    use InputSource::{External as E, Policy as P};
    let available = policy("available", "Inventory", "onHand - reserved", &[
        ("port.total", "onHand", InputType::NonNegativeInt, E("stock.total".into())),
        ("port.held", "reserved", InputType::NonNegativeInt, E("stock.held".into())),
    ]);
    let mut reserve = policy("canReserve", "Inventory", "quantity <= remaining", &[
        ("port.quantity", "quantity", InputType::PositiveInt, E("request.quantity".into())),
        ("port.remaining", "remaining", InputType::NonNegativeInt, P("available".into())),
    ]);
    reserve.exported = true;
    let shipping = policy("shipping", "Checkout", "if allowed then 0 else 3000", &[
        ("port.allowed", "allowed", InputType::Bool, P("canReserve".into())),
    ]);
    ProjectSpec { modules: vec![module("Checkout", &["Inventory"]), module("Inventory", &[])],
        inputs: vec![owned("stock.total", "onHand", InputType::NonNegativeInt),
            owned("stock.held", "reserved", InputType::NonNegativeInt),
            owned("request.quantity", "quantity", InputType::PositiveInt)],
        policies: vec![shipping, reserve, available] }
}
fn edit_policy<'a>(spec: &'a mut ProjectSpec, id: &str) -> &'a mut PolicySpec {
    spec.policies.iter_mut().find(|p| p.id == id).unwrap()
}
fn data(total: i64, held: i64, quantity: i64) -> InputBindings {
    InputBindings::from([("stock.total".into(), Value::Int(total)),
        ("stock.held".into(), Value::Int(held)), ("request.quantity".into(), Value::Int(quantity))])
}
fn case(id: &str, quantity: i64, allowed: bool) -> ProjectCase {
    ProjectCase { id: id.into(), name: id.into(), inputs: data(10, 8, quantity),
        expected: InputBindings::from([("canReserve".into(), Value::Bool(allowed)),
            ("shipping".into(), Value::Int(if allowed { 0 } else { 3000 }))]) }
}
fn baseline() -> Arc<CompiledProject> { compile_project(&fixture()).unwrap() }
fn changed() -> Arc<CompiledProject> {
    let mut s = fixture(); edit_policy(&mut s, "canReserve").source = "quantity < remaining".into();
    compile_project(&s).unwrap()
}
fn review(a: &Arc<CompiledProject>, b: &Arc<CompiledProject>, old: &[ProjectCase], new: &[ProjectCase]) -> ProjectReview {
    review_projects(a, b, old, new, ProjectReviewLimits::default(), &CancellationToken::new()).unwrap()
}
fn strings() -> (ProjectSpec, InputBindings) {
    let p = policy("echo", "Inventory", "text", &[("p.text", "text", InputType::String, InputSource::External("text".into()))]);
    (ProjectSpec { modules: vec![module("Inventory", &[])],
        inputs: vec![owned("text", "text", InputType::String)], policies: vec![p] },
        InputBindings::from([("text".into(), Value::Text(vec![0xd83e, 0xdd80]))]))
}

#[test]
fn executes_reusable_policies_in_dependency_order() {
    let p = baseline();
    assert_eq!(p.order(), &["available", "canReserve", "shipping"]);
    let r = execute_project(&p, &data(10, 8, 2), ProjectLimits::default());
    let values = r.result().as_ref().unwrap();
    assert_eq!(values["available"], Value::Int(2));
    assert_eq!(values["canReserve"], Value::Bool(true));
    assert_eq!(values["shipping"], Value::Int(0));
    assert_eq!(r.nodes()[1].bindings()["port.remaining"], Value::Int(2));
}
#[test]
fn exhaustive_three_policy_inventory_for_3060_inputs() {
    let p = baseline(); let mut count = 0;
    for total in 0..=16 { for held in 0..=total { for quantity in 1..=20 {
        let run = execute_project(&p, &data(total, held, quantity), ProjectLimits::default());
        let actual = run.result().as_ref().unwrap();
        assert_eq!(actual["available"], Value::Int(total - held));
        assert_eq!(actual["canReserve"], Value::Bool(quantity <= total - held));
        assert_eq!(actual["shipping"], Value::Int(if quantity <= total - held { 0 } else { 3000 }));
        count += 1;
    } } }
    assert_eq!(count, 3060);
}
#[test]
fn declaration_order_does_not_change_plan_or_logic_facts() {
    let a = fixture(); let mut b = a.clone();
    b.modules.reverse(); b.inputs.reverse(); b.policies.reverse();
    let x = compile_project(&a).unwrap(); let y = compile_project(&b).unwrap();
    assert_eq!(x.order(), y.order());
    assert!(compare_projects(&x, &y).changes.is_empty());
}
#[test]
fn compiled_project_does_not_follow_mutated_host_specs() {
    let mut s = fixture(); let p = compile_project(&s).unwrap();
    edit_policy(&mut s, "canReserve").source = "false".into();
    assert_eq!(p.policy("canReserve").unwrap().source, "quantity <= remaining");
}
#[test]
fn receipts_belong_to_the_exact_project_not_equal_text() {
    let a = baseline(); let b = baseline();
    let r = execute_project(&a, &data(10, 8, 2), ProjectLimits::default());
    assert!(r.is_for(&a)); assert!(!r.is_for(&b)); assert!(r.is_for(&a.clone()));
}
#[test]
fn modules_cannot_read_another_modules_raw_inputs() {
    let mut s = fixture();
    let shipping = edit_policy(&mut s, "shipping");
    shipping.source = "raw".into();
    shipping.inputs = vec![InputSpec::new("p.raw", "raw", InputType::Int)];
    shipping.links = [("p.raw".into(), InputSource::External("stock.total".into()))].into();
    assert_eq!(compile_project(&s).unwrap_err().code, "INPUT_OWNERSHIP");
}
#[test]
fn imported_modules_do_not_expose_private_policies() {
    let mut s = fixture(); edit_policy(&mut s, "canReserve").exported = false;
    assert_eq!(compile_project(&s).unwrap_err().code, "PRIVATE_POLICY");
}
#[test]
fn public_policy_still_requires_explicit_import() {
    let mut s = fixture(); s.modules[0].imports.clear();
    assert_eq!(compile_project(&s).unwrap_err().code, "UNDECLARED_IMPORT");
}
#[test]
fn imports_do_not_grant_transitive_access() {
    let mut s = fixture();
    s.modules.push(module("Bridge", &["Inventory"]));
    s.modules[0].imports = BTreeSet::from(["Bridge".into()]);
    assert_eq!(compile_project(&s).unwrap_err().code, "UNDECLARED_IMPORT");
}
#[test]
fn module_cycles_are_rejected_even_without_execution_edges() {
    let mut s = fixture(); s.modules[1].imports.insert("Checkout".into());
    assert_eq!(compile_project(&s).unwrap_err().code, "MODULE_CYCLE");
}
#[test]
fn module_self_import_is_a_cycle() {
    let mut s = fixture(); s.modules[1].imports.insert("Inventory".into());
    assert_eq!(compile_project(&s).unwrap_err().code, "MODULE_CYCLE");
}
#[test]
fn policy_cycles_are_rejected_without_recursive_execution() {
    let mut s = fixture();
    let p = edit_policy(&mut s, "available");
    p.links.insert("port.total".into(), InputSource::Policy("available".into()));
    assert_eq!(compile_project(&s).unwrap_err().code, "POLICY_CYCLE");
}
#[test]
fn unknown_imports_and_owners_are_not_implicit_modules() {
    let mut s = fixture(); s.modules[0].imports.insert("Unknown".into());
    assert_eq!(compile_project(&s).unwrap_err().code, "PROJECT_SCHEMA");
    let mut s = fixture(); s.inputs[0].module = "Unknown".into();
    assert_eq!(compile_project(&s).unwrap_err().code, "PROJECT_SCHEMA");
}
#[test]
fn duplicate_identity_namespaces_are_rejected() {
    let mut s = fixture(); s.modules.push(s.modules[0].clone());
    assert_eq!(compile_project(&s).unwrap_err().code, "PROJECT_SCHEMA");
    let mut s = fixture(); s.inputs.push(s.inputs[0].clone());
    assert_eq!(compile_project(&s).unwrap_err().code, "PROJECT_SCHEMA");
    let mut s = fixture(); s.policies.push(s.policies[0].clone());
    assert_eq!(compile_project(&s).unwrap_err().code, "PROJECT_SCHEMA");
}
#[test]
fn every_port_requires_exactly_one_explicit_connection() {
    let mut s = fixture(); edit_policy(&mut s, "canReserve").links.remove("port.quantity");
    assert_eq!(compile_project(&s).unwrap_err().code, "LINK_SCHEMA");
    let mut s = fixture(); edit_policy(&mut s, "canReserve").links.insert("extra".into(), InputSource::External("stock.total".into()));
    assert_eq!(compile_project(&s).unwrap_err().code, "LINK_SCHEMA");
}
#[test]
fn unknown_providers_are_compile_errors() {
    let mut s = fixture(); edit_policy(&mut s, "canReserve").links.insert("port.remaining".into(), InputSource::Policy("missing".into()));
    assert_eq!(compile_project(&s).unwrap_err().code, "UNKNOWN_POLICY");
    let mut s = fixture(); edit_policy(&mut s, "canReserve").links.insert("port.quantity".into(), InputSource::External("missing".into()));
    assert_eq!(compile_project(&s).unwrap_err().code, "UNKNOWN_EXTERNAL");
}
#[test]
fn provider_types_are_checked_before_any_policy_executes() {
    let mut s = fixture(); edit_policy(&mut s, "shipping").inputs[0].input_type = InputType::Int;
    edit_policy(&mut s, "shipping").source = "allowed + 1".into();
    assert_eq!(compile_project(&s).unwrap_err().code, "LINK_TYPE");
}
#[test]
fn refinement_of_derived_values_is_checked_at_the_real_boundary() {
    let p = baseline(); let r = execute_project(&p, &data(2, 3, 1), ProjectLimits::default());
    let e = r.result().as_ref().unwrap_err();
    assert_eq!(e.subject.as_deref(), Some("canReserve"));
    assert_eq!(e.diagnostic.as_ref().unwrap().code, "REFINEMENT_VIOLATION");
    assert_eq!(r.nodes().len(), 2);
    assert_eq!(r.nodes()[0].observed().outcome.result, Ok(Value::Int(-1)));
    assert_eq!(r.nodes()[1].observed().outcome.steps, 0);
}
#[test]
fn invalid_external_values_fail_before_any_policy_or_callback() {
    let p = baseline();
    for values in [data(10, 8, 0), data(-1, 0, 1), data(MAX_INT + 1, 0, 1), InputBindings::new()] {
        let mut seen = 0;
        let r = execute_project_observed(&p, &values, ProjectLimits::default(), &CancellationToken::new(), |_, _| { seen += 1; ControlFlow::Continue(()) });
        assert_eq!(r.result().as_ref().unwrap_err().code, "PROJECT_INPUT");
        assert_eq!(seen, 0); assert_eq!(r.steps(), 0); assert!(r.nodes().is_empty());
    }
}
#[test]
fn extra_external_input_ids_are_rejected() {
    let p = baseline(); let mut values = data(10, 8, 2); values.insert("extra".into(), Value::Bool(true));
    let r = execute_project(&p, &values, ProjectLimits::default());
    assert_eq!(r.result().as_ref().unwrap_err().diagnostic.as_ref().unwrap().code, "UNKNOWN_INPUT");
}
#[test]
fn all_external_inputs_are_validated_even_if_no_expression_reads_them() {
    let mut s = fixture(); s.inputs.push(owned("unused", "unused", InputType::PositiveInt));
    let p = compile_project(&s).unwrap();
    let r = execute_project(&p, &data(10, 8, 2), ProjectLimits::default());
    assert_eq!(r.result().as_ref().unwrap_err().diagnostic.as_ref().unwrap().code, "MISSING_INPUT");
}
#[test]
fn errors_include_original_policy_diagnostics() {
    let mut s = fixture(); edit_policy(&mut s, "available").source = "if true then 1 else false".into();
    let e = compile_project(&s).unwrap_err();
    assert_eq!(e.code, "POLICY_COMPILE"); assert_eq!(e.subject.as_deref(), Some("available"));
    assert_eq!(e.diagnostic.unwrap().code, "TYPE_MISMATCH");
}
#[test]
fn arithmetic_failure_does_not_run_dependents_or_return_partial_success() {
    let mut s = fixture(); edit_policy(&mut s, "available").source = format!("{MAX_INT} + 1");
    let p = compile_project(&s).unwrap(); let r = execute_project(&p, &data(10, 8, 2), ProjectLimits::default());
    assert_eq!(r.nodes().len(), 1);
    assert_eq!(r.result().as_ref().unwrap_err().diagnostic.as_ref().unwrap().code, "INTEGER_RANGE");
}
#[test]
fn project_size_and_metadata_limits_are_explicit() {
    let mut s = fixture(); s.policies.clear(); assert_eq!(compile_project(&s).unwrap_err().code, "PROJECT_SCHEMA");
    let mut s = fixture(); s.modules[0].id = "bad id".into(); assert_eq!(compile_project(&s).unwrap_err().code, "PROJECT_SCHEMA");
    let mut s = fixture(); s.policies[0].uri = "x".repeat(4097); assert_eq!(compile_project(&s).unwrap_err().code, "PROJECT_SCHEMA");
    let mut s = fixture(); s.policies[0].source = " ".repeat(cannon_core::MAX_SOURCE_UNITS + 1);
    assert_eq!(compile_project(&s).unwrap_err().code, "PROJECT_SOURCE_LIMIT");
}
#[test]
fn aggregate_source_limit_is_not_only_a_per_file_limit() {
    let mut s = fixture(); s.policies.clear();
    for n in 0..5 { s.policies.push(policy(&format!("large{n}"), "Inventory", &format!("1 //{}", "x".repeat(250_000)), &[])); }
    assert_eq!(compile_project(&s).unwrap_err().code, "PROJECT_SOURCE_LIMIT");
}
#[test]
fn source_origins_stay_attached_to_each_policy() {
    let mut s = fixture(); edit_policy(&mut s, "available").source = "// 🦀\r\nonHand - reserved".into();
    let p = compile_project(&s).unwrap(); let r = execute_project(&p, &data(10, 8, 2), ProjectLimits::default());
    for node in r.nodes() {
        let origin = r.project().policy(node.id()).unwrap();
        assert_eq!(origin.uri, format!("memory:{}", node.id()));
        for step in &node.observed().outcome.trace {
            assert!(step.span.end <= origin.source.encode_utf16().count());
            if node.id() == "available" { assert_eq!(step.span.line, 2); }
        }
    }
}
#[test]
fn streaming_and_retained_execution_have_identical_observations() {
    let p = baseline(); let a = execute_project(&p, &data(10, 8, 2), ProjectLimits::default());
    let mut events = Vec::new();
    let b = execute_project_observed(&p, &data(10, 8, 2), ProjectLimits { retain_trace: false, ..ProjectLimits::default() },
        &CancellationToken::new(), |id, event| { events.push((id.to_owned(), event.clone())); ControlFlow::Continue(()) });
    let expected: Vec<_> = a.nodes().iter().flat_map(|n| n.observed().outcome.trace.iter().map(move |e| (n.id().to_owned(), e.clone()))).collect();
    assert_eq!(events, expected); assert_eq!(a.result(), b.result()); assert_eq!(a.steps(), b.steps());
    assert_eq!(b.emitted_events(), events.len());
    assert!(b.nodes().iter().all(|n| n.observed().outcome.trace.is_empty()));
}
#[test]
fn aggregate_step_budget_does_not_reset_per_policy() {
    let p = baseline();
    let r = execute_project(&p, &data(10, 8, 2), ProjectLimits { max_steps: 3, ..ProjectLimits::default() });
    assert!(r.result().is_err()); assert_eq!(r.nodes().len(), 1); assert_eq!(r.steps(), 3);
}
#[test]
fn aggregate_event_budget_survives_streaming_without_retention() {
    let p = baseline();
    let r = execute_project(&p, &data(10, 8, 2), ProjectLimits { max_events: 4, retain_trace: false, ..ProjectLimits::default() });
    assert!(r.result().is_err()); assert_eq!(r.emitted_events(), 4);
}
#[test]
fn exact_aggregate_budget_can_complete() {
    let p = baseline(); let a = execute_project(&p, &data(10, 8, 2), ProjectLimits::default());
    let b = execute_project(&p, &data(10, 8, 2), ProjectLimits { max_steps: a.steps(), max_events: a.emitted_events(), max_text_units: 0, ..ProjectLimits::default() });
    assert_eq!(a.result(), b.result());
}
#[test]
fn invalid_limits_precede_input_errors_and_cancellation() {
    let p = baseline(); let token = CancellationToken::new(); token.cancel();
    let r = execute_project_observed(&p, &InputBindings::new(), ProjectLimits { max_steps: 0, ..ProjectLimits::default() }, &token, |_, _| panic!("must not run"));
    assert_eq!(r.result().as_ref().unwrap_err().code, "INVALID_LIMIT");
}
#[test]
fn text_payload_budget_is_shared_across_policies() {
    let (mut s, values) = strings();
    s.policies.push(policy("second", "Inventory", "text", &[("p.text", "text", InputType::String, InputSource::Policy("echo".into()))]));
    let p = compile_project(&s).unwrap();
    let r = execute_project(&p, &values, ProjectLimits { max_text_units: 3, retain_trace: false, ..ProjectLimits::default() });
    assert_eq!(r.observed_text_units(), 2); assert_eq!(r.emitted_events(), 1);
    assert_eq!(r.result().as_ref().unwrap_err().diagnostic.as_ref().unwrap().code, "TRACE_VALUE_LIMIT");
}
#[test]
fn lone_surrogate_values_are_preserved_across_links() {
    let (mut s, mut values) = strings();
    values.insert("text".into(), Value::Text(vec![0xd800]));
    s.policies.push(policy("second", "Inventory", "text", &[("p.text", "text", InputType::String, InputSource::Policy("echo".into()))]));
    let p = compile_project(&s).unwrap(); let r = execute_project(&p, &values, ProjectLimits::default());
    assert_eq!(r.result().as_ref().unwrap()["second"], Value::Text(vec![0xd800]));
}
#[test]
fn cancellation_before_start_and_at_last_event_never_completes() {
    let p = baseline(); let token = CancellationToken::new(); token.cancel();
    let r = execute_project_observed(&p, &data(10, 8, 2), ProjectLimits::default(), &token, |_, _| panic!("must not run"));
    assert_eq!(r.result().as_ref().unwrap_err().code, "CANCELLED"); assert!(r.nodes().is_empty());
    let r = execute_project_observed(&p, &data(10, 8, 2), ProjectLimits::default(), &CancellationToken::new(),
        |id, event| if id == "shipping" && event.kind == "if" { ControlFlow::Break(()) } else { ControlFlow::Continue(()) });
    assert_eq!(r.result().as_ref().unwrap_err().diagnostic.as_ref().unwrap().code, "CANCELLED");
    assert_eq!(r.nodes().len(), 3);
}
#[test]
fn observer_failure_does_not_corrupt_reusable_project() {
    let p = baseline();
    let cancelled = execute_project_observed(&p, &data(10, 8, 2), ProjectLimits::default(), &CancellationToken::new(), |_, _| ControlFlow::Break(()));
    assert!(cancelled.result().is_err());
    assert!(execute_project(&p, &data(10, 8, 2), ProjectLimits::default()).result().is_ok());
}
#[test]
fn candidate_expectation_changes_do_not_redefine_history() {
    let r = review(&baseline(), &changed(), &[case("exact", 2, true)], &[case("exact", 2, false)]);
    assert_eq!(r.candidate_results()[0].status(), CaseStatus::Passed);
    assert_eq!(r.historical_results()[0].status(), CaseStatus::Mismatch);
    assert_eq!(r.regressions(), ["exact"]); assert!(!r.passed());
    assert!(r.case_changes().iter().any(|c| c.kind == "expectations-changed"));
    assert_eq!(r.historical_results()[0].failed_assertions(), &["canReserve", "shipping"]);
}
#[test]
fn candidate_input_changes_do_not_redefine_history() {
    let r = review(&baseline(), &changed(), &[case("exact", 2, true)], &[case("exact", 1, true)]);
    assert_eq!(r.candidate_results()[0].status(), CaseStatus::Passed);
    assert_eq!(r.historical_results()[0].case().inputs, data(10, 8, 2));
    assert_eq!(r.historical_results()[0].run().inputs(), &data(10, 8, 2));
    assert!(r.case_changes().iter().any(|c| c.kind == "inputs-changed")); assert!(!r.passed());
}
#[test]
fn deleted_candidate_tests_still_run_the_historical_cases() {
    let r = review(&baseline(), &changed(), &[case("exact", 2, true)], &[]);
    assert!(r.candidate_results().is_empty()); assert_eq!(r.historical_results().len(), 1);
    assert_eq!(r.regressions(), ["exact"]); assert!(!r.passed());
}
#[test]
fn empty_suites_or_empty_assertions_are_not_verified_success() {
    let p = baseline(); assert!(!review(&p, &p, &[], &[]).passed());
    let mut c = case("none", 2, true); c.expected.clear();
    let r = review(&p, &p, std::slice::from_ref(&c), std::slice::from_ref(&c));
    assert_eq!(r.baseline_results()[0].status(), CaseStatus::NoExpectations); assert!(!r.passed());
}
#[test]
fn a_failed_original_example_is_not_called_a_new_regression() {
    let p = baseline(); let c = case("broken-original", 2, false);
    let r = review(&p, &p, std::slice::from_ref(&c), std::slice::from_ref(&c));
    assert_eq!(r.baseline_results()[0].status(), CaseStatus::Mismatch);
    assert!(r.regressions().is_empty()); assert!(!r.passed());
}
#[test]
fn stable_ids_preserve_history_across_display_and_port_renames() {
    let mut s = fixture();
    let p = edit_policy(&mut s, "canReserve"); p.name = "eligible".into();
    p.source = "quantity <= freeStock".into();
    p.inputs.iter_mut().find(|i| i.id == "port.remaining").unwrap().name = "freeStock".into();
    let b = compile_project(&s).unwrap(); let c = case("exact", 2, true);
    let r = review(&baseline(), &b, std::slice::from_ref(&c), std::slice::from_ref(&c));
    assert!(r.passed());
    assert!(!r.difference().changes.iter().any(|c| c.kind == "logic-changed"));
}
#[test]
fn changing_a_policy_id_is_not_automatically_remapped_in_expectations() {
    let mut s = fixture(); edit_policy(&mut s, "shipping").id = "new-shipping".into();
    let p = compile_project(&s).unwrap(); let c = case("exact", 2, true);
    let r = review(&baseline(), &p, std::slice::from_ref(&c), std::slice::from_ref(&c));
    assert_eq!(r.historical_results()[0].failed_assertions(), &["shipping"]); assert!(!r.passed());
}
#[test]
fn comments_are_text_changes_not_logic_changes() {
    let mut s = fixture(); edit_policy(&mut s, "canReserve").source.push_str(" // reason");
    let d = compare_projects(&baseline(), &compile_project(&s).unwrap());
    assert!(d.changes.iter().any(|c| c.kind == "source-text-changed"));
    assert!(!d.changes.iter().any(|c| c.kind == "logic-changed"));
}
#[test]
fn upstream_changes_include_transitive_consumers() {
    let p = baseline();
    assert_eq!(p.impacted(&BTreeSet::from(["available".into()])), BTreeSet::from(["available".into(), "canReserve".into(), "shipping".into()]));
    let d = compare_projects(&p, &changed());
    assert_eq!(d.impacted_policies, BTreeSet::from(["canReserve".into(), "shipping".into()]));
}
#[test]
fn link_changes_are_reported_without_fabricating_expression_changes() {
    let mut s = fixture(); edit_policy(&mut s, "canReserve").links.insert("port.remaining".into(), InputSource::External("stock.total".into()));
    let d = compare_projects(&baseline(), &compile_project(&s).unwrap());
    assert!(d.changes.iter().any(|c| c.kind == "links-changed"));
    assert!(!d.changes.iter().any(|c| c.kind == "logic-changed"));
}
#[test]
fn strengthening_external_constraints_becomes_a_historical_execution_error() {
    let mut s = fixture(); s.inputs.iter_mut().find(|i| i.spec.id == "stock.held").unwrap().spec.input_type = InputType::PositiveInt;
    let mut old = case("zero-held", 2, true); old.inputs = data(10, 0, 2);
    let candidate = case("zero-held", 2, true);
    let r = review(&baseline(), &compile_project(&s).unwrap(), &[old], &[candidate]);
    assert_eq!(r.candidate_results()[0].status(), CaseStatus::Passed);
    assert_eq!(r.historical_results()[0].status(), CaseStatus::ExecutionError);
    assert_eq!(r.regressions(), ["zero-held"]);
}
#[test]
fn output_assertions_may_not_silently_reference_missing_policies() {
    let p = baseline(); let mut c = case("bad-output", 2, true); c.expected.insert("missing".into(), Value::Int(0));
    let r = review(&p, &p, std::slice::from_ref(&c), std::slice::from_ref(&c));
    assert_eq!(r.baseline_results()[0].failed_assertions(), &["missing"]); assert!(!r.passed());
}
#[test]
fn review_retains_both_source_graphs_and_exact_invocation_data() {
    let a = baseline(); let b = changed();
    let r = review(&a, &b, &[case("exact", 2, true)], &[case("exact", 1, true)]);
    assert!(Arc::ptr_eq(r.baseline(), &a)); assert!(Arc::ptr_eq(r.candidate(), &b));
    assert!(r.historical_results()[0].run().is_for(&b));
    assert_eq!(r.historical_results()[0].case().expected["canReserve"], Value::Bool(true));
    assert_eq!(r.historical_results()[0].case().inputs["request.quantity"], Value::Int(2));
}
#[test]
fn duplicate_case_ids_and_invalid_expectations_are_rejected() {
    let p = baseline(); let c = case("same", 2, true);
    assert_eq!(review_projects(&p, &p, &[c.clone(), c], &[], ProjectReviewLimits::default(), &CancellationToken::new()).unwrap_err().code, "PROJECT_CASE_SCHEMA");
    let mut c = case("bad", 2, true); c.expected.insert("shipping".into(), Value::Int(MAX_INT + 1));
    assert_eq!(review_projects(&p, &p, &[c], &[], ProjectReviewLimits::default(), &CancellationToken::new()).unwrap_err().code, "PROJECT_CASE_SCHEMA");
}
#[test]
fn review_budget_is_shared_by_baseline_candidate_and_history() {
    let p = baseline(); let c = case("exact", 2, true);
    let events = execute_project(&p, &c.inputs, ProjectLimits::default()).emitted_events();
    let limits = ProjectReviewLimits { max_events: events * 2, ..ProjectReviewLimits::default() };
    assert_eq!(review_projects(&p, &p, std::slice::from_ref(&c), std::slice::from_ref(&c), limits, &CancellationToken::new()).unwrap_err().code, "REVIEW_INCOMPLETE");
}
#[test]
fn cancelled_reviews_are_not_partial_success() {
    let p = baseline(); let token = CancellationToken::new(); token.cancel();
    assert_eq!(review_projects(&p, &p, &[], &[], ProjectReviewLimits::default(), &token).unwrap_err().code, "REVIEW_INCOMPLETE");
}
#[test]
fn numerical_projects_do_not_require_a_text_payload_budget() {
    let p = baseline(); let c = case("exact", 2, true);
    let limits = ProjectReviewLimits { max_text_units: 0, ..ProjectReviewLimits::default() };
    assert!(review_projects(&p, &p, std::slice::from_ref(&c), std::slice::from_ref(&c), limits, &CancellationToken::new()).unwrap().passed());
}
#[test]
fn expected_values_are_copied_not_shared_with_mutable_case_data() {
    let p = baseline(); let mut cases = vec![case("exact", 2, true)];
    let r = review(&p, &p, &cases, &cases); cases[0].expected.clear(); cases[0].inputs.clear();
    assert!(r.passed()); assert_eq!(r.baseline_results()[0].case().expected.len(), 2);
}
