//! Human-owned module wiring, three source files, and preserved multi-output cases.
//! Running this example performs no file writes, provider calls or deployment.
use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use cannon_core::{CancellationToken, InputBindings, InputSpec, InputType, Value};
use cannon_core::project::*;
use cannon_core::project_review::*;

fn policy(id: &str, owner: &str, source: &str, ports: &[(&str, &str, InputType, InputSource)]) -> PolicySpec {
    PolicySpec { id: id.into(), name: id.into(), module: owner.into(), exported: id == "canReserve",
        uri: format!("example:{id}"), source: source.into(),
        inputs: ports.iter().map(|(id, name, ty, _)| InputSpec::new(*id, *name, *ty)).collect(),
        links: ports.iter().map(|(id, _, _, link)| ((*id).into(), link.clone())).collect() }
}
fn definition() -> ProjectSpec {
    use InputSource::{External as E, Policy as P};
    ProjectSpec {
        modules: vec![
            ModuleSpec { id: "Inventory".into(), name: "Inventory".into(), imports: BTreeSet::new() },
            ModuleSpec { id: "Checkout".into(), name: "Checkout".into(), imports: BTreeSet::from(["Inventory".into()]) },
        ],
        inputs: [("stock.total", "onHand", InputType::NonNegativeInt),
            ("stock.held", "reserved", InputType::NonNegativeInt),
            ("request.quantity", "quantity", InputType::PositiveInt)].into_iter()
            .map(|(id, name, ty)| OwnedInput { module: "Inventory".into(), spec: InputSpec::new(id, name, ty) }).collect(),
        policies: vec![
            policy("available", "Inventory", include_str!("../../../examples/projects/inventory/available.expr"), &[
                ("p.total", "onHand", InputType::NonNegativeInt, E("stock.total".into())),
                ("p.held", "reserved", InputType::NonNegativeInt, E("stock.held".into())),
            ]),
            policy("canReserve", "Inventory", include_str!("../../../examples/projects/inventory/can-reserve.expr"), &[
                ("p.left", "remaining", InputType::NonNegativeInt, P("available".into())),
                ("p.quantity", "quantity", InputType::PositiveInt, E("request.quantity".into())),
            ]),
            policy("shipping", "Checkout", include_str!("../../../examples/projects/inventory/shipping.expr"), &[
                ("p.allowed", "allowed", InputType::Bool, P("canReserve".into())),
            ]),
        ],
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = definition();
    let baseline = compile_project(&spec)?;
    println!("Checked execution order: {:?}", baseline.order());
    let inputs = InputBindings::from([("stock.total".into(), Value::Int(10)),
        ("stock.held".into(), Value::Int(8)), ("request.quantity".into(), Value::Int(2))]);
    let run = execute_project_observed(&baseline, &inputs, ProjectLimits::default(), &CancellationToken::new(), |id, step| {
        println!("{id}:{}:{} {} {:?}", step.span.line, step.span.column, step.label, step.value);
        ControlFlow::Continue(())
    });
    assert_eq!(run.result().as_ref().map(|v| &v["shipping"]), Ok(&Value::Int(0)));

    let mut candidate_spec = spec.clone();
    candidate_spec.policies[1].source = "quantity < remaining".into();
    let candidate = compile_project(&candidate_spec)?;
    let old_case = ProjectCase { id: "case.exact".into(), name: "Reserve all remaining stock".into(), inputs,
        expected: BTreeMap::from([("canReserve".into(), Value::Bool(true)), ("shipping".into(), Value::Int(0))]) };
    let mut new_case = old_case.clone();
    new_case.expected.insert("canReserve".into(), Value::Bool(false));
    new_case.expected.insert("shipping".into(), Value::Int(3000));
    let review = review_projects(&baseline, &candidate, &[old_case], &[new_case], ProjectReviewLimits::default(), &CancellationToken::new())?;
    println!("Computed changes: {:?}", review.difference().changes);
    println!("Affected policy candidates: {:?}", review.difference().impacted_policies);
    println!("Candidate's own case: {:?}", review.candidate_results()[0].status());
    println!("Preserved original case: {:?}", review.historical_results()[0].status());
    println!("Historical failed outputs: {:?}", review.historical_results()[0].failed_assertions());
    assert_eq!(review.candidate_results()[0].status(), CaseStatus::Passed);
    assert_eq!(review.historical_results()[0].status(), CaseStatus::Mismatch);
    assert!(!review.passed());

    let mut invalid = spec;
    invalid.policies[1].exported = false;
    let error = compile_project(&invalid).unwrap_err();
    assert_eq!(error.code, "PRIVATE_POLICY");
    println!("Rejected architecture change before execution: {error}");
    println!("Demo completed: the regression was deliberately exposed, not approved.");
    Ok(())
}
