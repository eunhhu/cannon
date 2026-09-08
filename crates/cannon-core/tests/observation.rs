use std::ops::ControlFlow;
use cannon_core::{compile_expression, evaluate, evaluate_observed, CancellationToken, Limits, ObservationOptions, Value};

#[test]
fn observation_preserves_results_steps_and_all_trace_fields() {
    for source in ["1 + 2 * 3", "-4 + 5", "if true then 7 else 8", "false and (9007199254740991 + 1 == 0)", "true or false", "\"가😀\" == \"가😀\"", "9007199254740991 + 1"] {
        let p = compile_expression(source).unwrap();
        let expected = evaluate(&p, Limits::default());
        let mut observed = Vec::new();
        let actual = evaluate_observed(&p, Limits::default(), ObservationOptions::default(), &CancellationToken::new(), |event| { observed.push(event.clone()); ControlFlow::Continue(()) });
        assert_eq!(actual.outcome.result, expected.result, "{source}");
        assert_eq!(actual.outcome.steps, expected.steps, "{source}");
        assert_eq!(actual.outcome.trace, expected.trace, "{source}");
        assert_eq!(observed, expected.trace, "{source}");
        assert_eq!(actual.emitted_events, observed.len());
    }
}

#[test]
fn streaming_does_not_retain_values_or_change_execution() {
    let p = compile_expression("if 8 + 2 <= 10 then 0 else 3000").unwrap();
    let expected = evaluate(&p, Limits::default());
    let mut events = Vec::new();
    let options = ObservationOptions { retain_trace: false, ..ObservationOptions::default() };
    let actual = evaluate_observed(&p, Limits::default(), options, &CancellationToken::new(), |event| { events.push(event.clone()); ControlFlow::Continue(()) });
    assert_eq!(actual.outcome.result, expected.result);
    assert_eq!(actual.outcome.steps, expected.steps);
    assert_eq!(events, expected.trace);
    assert!(actual.outcome.trace.is_empty());
    assert!(!actual.options.retain_trace);
    assert_eq!(actual.emitted_events, events.len());
}

#[test]
fn cancelled_before_start_produces_no_steps_or_events() {
    let p = compile_expression("1 + 2").unwrap();
    let token = CancellationToken::new(); token.cancel();
    let actual = evaluate_observed(&p, Limits::default(), ObservationOptions::default(), &token, |_| panic!("no event expected"));
    assert_eq!(actual.outcome.result.unwrap_err().code, "CANCELLED");
    assert_eq!(actual.outcome.steps, 0);
    assert_eq!(actual.emitted_events, 0);
    assert!(actual.outcome.trace.is_empty());
}

#[test]
fn observer_cancellation_keeps_only_already_observed_events() {
    let p = compile_expression("1 + (9007199254740991 + 1)").unwrap();
    let actual = evaluate_observed(&p, Limits::default(), ObservationOptions::default(), &CancellationToken::new(), |_| ControlFlow::Break(()));
    assert_eq!(actual.outcome.result.unwrap_err().code, "CANCELLED");
    assert_eq!(actual.emitted_events, 1);
    assert_eq!(actual.outcome.trace.len(), 1);
    assert_eq!(actual.outcome.trace[0].value, Some(Value::Int(1)));
    assert_eq!(actual.outcome.steps, 2);
}

#[test]
fn observer_can_signal_the_shared_token() {
    let p = compile_expression("1 + 2").unwrap();
    let token = CancellationToken::new(); let signal = token.clone();
    let actual = evaluate_observed(&p, Limits::default(), ObservationOptions::default(), &token, |_| { signal.cancel(); ControlFlow::Continue(()) });
    assert_eq!(actual.outcome.result.unwrap_err().code, "CANCELLED");
    assert_eq!(actual.emitted_events, 1);
    assert!(token.is_cancelled());
}

#[test]
fn another_thread_can_cancel_at_a_deterministic_observation_boundary() {
    let p = compile_expression("1 + 2").unwrap();
    let token = CancellationToken::new(); let signal = token.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(0);
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || { ready_rx.recv().unwrap(); signal.cancel(); done_tx.send(()).unwrap(); });
    let actual = evaluate_observed(&p, Limits::default(), ObservationOptions::default(), &token, |_| {
        ready_tx.send(()).unwrap(); done_rx.recv().unwrap(); ControlFlow::Continue(())
    });
    worker.join().unwrap();
    assert_eq!(actual.outcome.result.unwrap_err().code, "CANCELLED");
    assert_eq!(actual.emitted_events, 1);
}

#[test]
fn cancellation_on_the_last_event_is_not_a_success() {
    let p = compile_expression("7").unwrap();
    let actual = evaluate_observed(&p, Limits::default(), ObservationOptions::default(), &CancellationToken::new(), |_| ControlFlow::Break(()));
    assert_eq!(actual.outcome.result.unwrap_err().code, "CANCELLED");
    assert_eq!(actual.outcome.trace[0].value, Some(Value::Int(7)));
}

#[test]
fn completed_results_are_not_retroactively_changed_by_cancellation() {
    let p = compile_expression("7").unwrap(); let token = CancellationToken::new();
    let actual = evaluate_observed(&p, Limits::default(), ObservationOptions::default(), &token, |_| ControlFlow::Continue(()));
    token.cancel();
    assert_eq!(actual.outcome.result.unwrap(), Value::Int(7));
    let rerun = evaluate_observed(&p, Limits::default(), ObservationOptions::default(), &token, |_| ControlFlow::Continue(()));
    assert_eq!(rerun.outcome.result.unwrap_err().code, "CANCELLED");
    assert_eq!(evaluate(&p, Limits::default()).result.unwrap(), Value::Int(7));
}

#[test]
fn cloned_tokens_share_one_way_state_but_new_tokens_are_independent() {
    let token = CancellationToken::new(); let clone = token.clone();
    assert!(!clone.is_cancelled()); token.cancel(); token.cancel();
    assert!(clone.is_cancelled()); assert!(!CancellationToken::new().is_cancelled());
}

#[test]
fn streaming_cannot_bypass_the_trace_count_limit() {
    let p = compile_expression("1 + 2").unwrap();
    let limits = Limits { max_trace: 1, ..Limits::default() };
    let options = ObservationOptions { retain_trace: false, ..ObservationOptions::default() };
    let actual = evaluate_observed(&p, limits, options, &CancellationToken::new(), |_| ControlFlow::Continue(()));
    assert_eq!(actual.outcome.result.unwrap_err().code, "TRACE_LIMIT");
    assert_eq!(actual.emitted_events, 1);
    assert!(actual.outcome.trace.is_empty());
}

#[test]
fn text_budget_is_measured_in_utf16_units_not_utf8_bytes() {
    for (source, limit, expected) in [("\"가\"", 1, true), ("\"😀\"", 1, false), ("\"😀\"", 2, true), (r#""\ud800""#, 1, true)] {
        let p = compile_expression(source).unwrap();
        let options = ObservationOptions { max_trace_text_units: limit, ..ObservationOptions::default() };
        let actual = evaluate_observed(&p, Limits::default(), options, &CancellationToken::new(), |_| ControlFlow::Continue(()));
        assert_eq!(actual.outcome.result.is_ok(), expected, "{source}");
        if !expected { assert_eq!(actual.outcome.result.unwrap_err().code, "TRACE_VALUE_LIMIT"); assert_eq!(actual.emitted_events, 0); }
    }
}

#[test]
fn repeated_text_observations_consume_budget_even_without_retention() {
    let p = compile_expression("\"ab\" == \"ab\"").unwrap();
    let options = ObservationOptions { retain_trace: false, max_trace_text_units: 3 };
    let actual = evaluate_observed(&p, Limits::default(), options, &CancellationToken::new(), |_| ControlFlow::Continue(()));
    assert_eq!(actual.outcome.result.unwrap_err().code, "TRACE_VALUE_LIMIT");
    assert_eq!(actual.observed_text_units, 2);
    assert_eq!(actual.emitted_events, 1);
    assert!(actual.outcome.trace.is_empty());
}

#[test]
fn exact_text_budget_boundary_is_accepted() {
    let p = compile_expression("\"ab\" == \"ab\"").unwrap();
    let options = ObservationOptions { retain_trace: false, max_trace_text_units: 4 };
    let actual = evaluate_observed(&p, Limits::default(), options, &CancellationToken::new(), |_| ControlFlow::Continue(()));
    assert_eq!(actual.outcome.result.unwrap(), Value::Bool(true));
    assert_eq!(actual.observed_text_units, 4);
    assert_eq!(actual.emitted_events, 3);
}

#[test]
fn zero_text_budget_does_not_disable_numeric_execution() {
    let p = compile_expression("1 + 2").unwrap();
    let options = ObservationOptions { max_trace_text_units: 0, ..ObservationOptions::default() };
    let actual = evaluate_observed(&p, Limits::default(), options, &CancellationToken::new(), |_| ControlFlow::Continue(()));
    assert_eq!(actual.outcome.result.unwrap(), Value::Int(3));
    assert_eq!(actual.observed_text_units, 0);
}

#[test]
fn short_circuit_stream_has_no_fabricated_right_operand_result() {
    let p = compile_expression("false and (9007199254740991 + 1 == 0)").unwrap();
    let mut events = Vec::new();
    let actual = evaluate_observed(&p, Limits::default(), ObservationOptions::default(), &CancellationToken::new(), |event| { events.push(event.clone()); ControlFlow::Continue(()) });
    assert_eq!(actual.outcome.result.unwrap(), Value::Bool(false));
    let skipped = events.iter().find(|e| e.kind == "short-circuit").unwrap();
    assert_eq!(skipped.value, None);
    assert!(!events.iter().any(|e| e.label == "+"));
}

#[test]
fn invalid_limits_are_rejected_without_calling_the_observer() {
    let p = compile_expression("1").unwrap();
    let actual = evaluate_observed(&p, Limits { max_steps: 0, ..Limits::default() }, ObservationOptions::default(), &CancellationToken::new(), |_| panic!("no event expected"));
    assert_eq!(actual.outcome.result.unwrap_err().code, "INVALID_LIMIT");
    assert_eq!(actual.emitted_events, 0);
}

#[test]
fn numerical_overflow_keeps_only_real_preceding_events() {
    let p = compile_expression("9007199254740991 + 1").unwrap();
    let actual = evaluate_observed(&p, Limits::default(), ObservationOptions::default(), &CancellationToken::new(), |_| ControlFlow::Continue(()));
    assert_eq!(actual.outcome.result.unwrap_err().code, "INTEGER_RANGE");
    assert_eq!(actual.emitted_events, 2);
    assert!(actual.outcome.trace.iter().all(|e| e.kind == "literal"));
}

#[test]
fn cancellation_does_not_mutate_reusable_compilations() {
    let p = compile_expression("1 + 2").unwrap();
    for _ in 0..10 {
        let cancelled = evaluate_observed(&p, Limits::default(), ObservationOptions::default(), &CancellationToken::new(), |_| ControlFlow::Break(()));
        assert_eq!(cancelled.outcome.result.unwrap_err().code, "CANCELLED");
        assert_eq!(evaluate(&p, Limits::default()).result.unwrap(), Value::Int(3));
    }
}
