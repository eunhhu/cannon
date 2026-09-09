use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::sync::mpsc;
use std::time::Duration;
use cannon_core::{CancellationToken, InputBindings, InputSpec, InputType, Value, MAX_INPUT_TEXT_UNITS};
use cannon_core::project::*;

fn definition(source: &str, has_text: bool) -> ProjectSpec {
    let inputs = if has_text { vec![InputSpec::new("text", "text", InputType::String)] } else { vec![] };
    let links = if has_text { BTreeMap::from([("text".into(), InputSource::External("text".into()))]) } else { BTreeMap::new() };
    ProjectSpec { modules: vec![ModuleSpec { id: "M".into(), name: "Module".into(), imports: BTreeSet::new() }],
        inputs: inputs.iter().map(|spec| OwnedInput { module: "M".into(), spec: spec.clone() }).collect(),
        policies: vec![PolicySpec { id: "p".into(), name: "Policy".into(), module: "M".into(), exported: false,
            uri: "memory:policy".into(), source: source.into(), inputs, links }] }
}
#[test]
fn rejected_payload_is_not_reported_as_valid_empty_input() {
    let p = compile_project(&definition("1", false)).unwrap();
    let empty = execute_project(&p, &InputBindings::new(), ProjectLimits::default());
    assert_eq!(empty.accepted_inputs(), Some(&InputBindings::new()));
    let invalid = execute_project(&p, &InputBindings::from([("extra".into(), Value::Int(1))]), ProjectLimits::default());
    assert!(invalid.result().is_err()); assert_eq!(invalid.accepted_inputs(), None);
    assert!(invalid.inputs().is_empty());
}
#[test]
fn oversized_rejected_payload_is_not_cloned_into_receipt() {
    let p = compile_project(&definition("text", true)).unwrap();
    let values = InputBindings::from([("text".into(), Value::Text(vec![65; MAX_INPUT_TEXT_UNITS + 1]))]);
    let r = execute_project(&p, &values, ProjectLimits::default());
    assert!(r.accepted_inputs().is_none()); assert!(r.nodes().is_empty());
    assert_eq!(r.result().as_ref().unwrap_err().diagnostic.as_ref().unwrap().code, "INPUT_LIMIT");
}
#[test]
fn accepted_input_receipts_do_not_follow_caller_mutations() {
    let p = compile_project(&definition("text", true)).unwrap();
    let mut values = InputBindings::from([("text".into(), Value::Text(vec![65]))]);
    let r = execute_project(&p, &values, ProjectLimits::default()); values.clear();
    assert_eq!(r.accepted_inputs().unwrap()["text"], Value::Text(vec![65]));
}
#[test]
fn invalid_options_do_not_claim_the_input_was_validated() {
    let p = compile_project(&definition("1", false)).unwrap();
    let r = execute_project(&p, &InputBindings::new(), ProjectLimits { max_events: 0, ..ProjectLimits::default() });
    assert_eq!(r.accepted_inputs(), None); assert_eq!(r.result().as_ref().unwrap_err().code, "INVALID_LIMIT");
}
#[test]
fn unbounded_provider_metadata_is_rejected_before_looking_it_up() {
    let mut s = definition("text", true);
    s.policies[0].links.insert("text".into(), InputSource::Policy("x".repeat(129)));
    assert_eq!(compile_project(&s).unwrap_err().code, "PROJECT_SCHEMA");
}
#[test]
fn shared_provider_executes_once_in_a_diamond() {
    let mut s = definition("2", false);
    for id in ["left", "right"] {
        s.policies.push(PolicySpec { id: id.into(), name: id.into(), module: "M".into(), exported: false,
            uri: format!("memory:{id}"), source: "value + 1".into(),
            inputs: vec![InputSpec::new("in", "value", InputType::Int)],
            links: BTreeMap::from([("in".into(), InputSource::Policy("p".into()))]) });
    }
    let p = compile_project(&s).unwrap(); let r = execute_project(&p, &InputBindings::new(), ProjectLimits::default());
    assert_eq!(r.nodes().iter().filter(|n| n.id() == "p").count(), 1);
    assert_eq!(r.nodes().len(), 3);
    assert_eq!(r.result().as_ref().unwrap()["left"], Value::Int(3));
    assert_eq!(r.result().as_ref().unwrap()["right"], Value::Int(3));
}
#[test]
fn failure_of_a_declared_independent_policy_is_not_silently_ignored() {
    let mut s = definition("1", false);
    let mut other = s.policies[0].clone(); other.id = "z".into(); other.source = "9007199254740991 + 1".into();
    s.policies.push(other);
    let p = compile_project(&s).unwrap(); let r = execute_project(&p, &InputBindings::new(), ProjectLimits::default());
    assert!(r.result().is_err()); assert_eq!(r.nodes().len(), 2);
    assert_eq!(r.nodes()[0].observed().outcome.result, Ok(Value::Int(1)));
}
#[test]
fn a_host_thread_can_cancel_at_a_known_event_boundary() {
    let p = compile_project(&definition("1 + 2", false)).unwrap();
    let token = CancellationToken::new(); let worker_token = token.clone();
    let (ready_tx, ready_rx) = mpsc::sync_channel(0);
    let (resume_tx, resume_rx) = mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        let mut first = true;
        execute_project_observed(&p, &InputBindings::new(), ProjectLimits::default(), &worker_token, |_, _| {
            if first {
                first = false; ready_tx.send(()).unwrap();
                resume_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            }
            ControlFlow::Continue(())
        })
    });
    ready_rx.recv_timeout(Duration::from_secs(10)).unwrap(); token.cancel(); resume_tx.send(()).unwrap();
    let r = worker.join().unwrap();
    assert_eq!(r.result().as_ref().unwrap_err().diagnostic.as_ref().unwrap().code, "CANCELLED");
    assert_eq!(r.emitted_events(), 1);
}
#[test]
fn external_input_display_names_are_local_to_their_owner() {
    let mut s = definition("text", true);
    s.modules.push(ModuleSpec { id: "Other".into(), name: "Other".into(), imports: BTreeSet::new() });
    s.inputs.push(OwnedInput { module: "Other".into(), spec: InputSpec::new("other.text", "text", InputType::String) });
    let p = compile_project(&s).unwrap();
    let values = InputBindings::from([("text".into(), Value::Text(vec![65])), ("other.text".into(), Value::Text(vec![66]))]);
    let r = execute_project(&p, &values, ProjectLimits::default());
    assert_eq!(r.result().as_ref().unwrap()["p"], Value::Text(vec![65]));
}
