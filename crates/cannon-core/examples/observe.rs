//! Run with: cargo run -p cannon-core --example observe
//! This is a native API example, not an editor or an agent.
use std::ops::ControlFlow;
use cannon_core::{compile_expression, evaluate_observed, report, CancellationToken, Limits, ObservationOptions};

fn main() -> Result<(), String> {
    let source = "if 8 + 2 <= 10 then 0 else 3000";
    let program = compile_expression(source).map_err(|error| error.message)?;
    println!("Exact source: {}", report::quote(source));
    let options = ObservationOptions { retain_trace: false, max_trace_text_units: 4096 };
    let completed = evaluate_observed(&program, Limits::default(), options, &CancellationToken::new(), |event| {
        let value = event.value.as_ref().map(report::value_json).unwrap_or_else(|| "not evaluated".to_owned());
        println!("{}:{} {} {} = {}", event.span.line, event.span.column, event.kind, event.label, value);
        ControlFlow::Continue(())
    });
    let value = completed.outcome.result.map_err(|error| error.message)?;
    println!("Completed: {}; delivered {} events; retained {} events", report::value_json(&value), completed.emitted_events, completed.outcome.trace.len());

    let cancelled = evaluate_observed(&program, Limits::default(), options, &CancellationToken::new(), |_| ControlFlow::Break(()));
    let error = cancelled.outcome.result.expect_err("observer must cancel");
    assert_eq!(error.code, "CANCELLED");
    println!("Cancelled preview: {}; no completed result", error.code);
    Ok(())
}
