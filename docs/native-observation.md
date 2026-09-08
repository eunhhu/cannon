# Cooperative native observation and cancellation

This change extends the already-published **expression** engine. It does not deliver the separate full-file workbench candidate or change the default TypeScript command. The full-file parser/checker upload was blocked by an indeterminate connector security decision; that content is not retransmitted in this change.

## One evaluator, two ways to consume it

`evaluate(program, limits)` remains source-compatible and uses the same implementation as the new `evaluate_observed(program, limits, options, token, observer)` API. The original default trace, arithmetic, diagnostics, step accounting, and source positions remain the compatibility contract. Existing independent cross-engine cases must still pass.

An observer receives each actual trace event synchronously. It returns `ControlFlow::Continue(())` or `ControlFlow::Break(())`. A break produces `CANCELLED`, never a completed value. Already delivered events remain observations, not proof the run completed. Skipped branches still have no fabricated values.

`CancellationToken` is cloneable and one-way. A host thread can cancel a preview whose source became stale. The evaluator checks before visiting expressions, before emitting events, and after the observer returns. It does not spawn threads, forcibly interrupt a callback, promise a deadline, or invalidate previously completed results. A cancellation racing after the last checkpoint may follow successful completion; hosts must also compare source/document versions before presenting results.

## Streaming and budgets

`ObservationOptions::retain_trace = false` avoids retaining a second copy of delivered trace entries. It does not mean no computation occurred. `ObservedOutcome` reports both `emitted_events` and retention options explicitly. Do not pass a stripped outcome to the legacy JSON renderer and describe its empty trace as a complete execution history.

The existing `max_trace` counts **emitted**, not retained, events. Streaming cannot bypass it. `max_trace_text_units` additionally bounds the cumulative UTF-16 text-value payload delivered in events, counting repeated observations. The default is unbounded for backwards compatibility; zero allows only events without nonempty text values. Crossing this budget produces `TRACE_VALUE_LIMIT` before that event is delivered. This is not a cap on total process memory, source allocation, JSON size, or downstream queues.

The observer is trusted host code. Its time, allocation, I/O, and panic behavior are not sandboxed. Clients should use bounded queues and handle backpressure explicitly. The core never evaluates callbacks supplied by Cannon source.

## Run

```sh
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo run --locked --offline -p cannon-core --example observe
```

The example streams a shipping-cost expression, then demonstrates cancelling a separate preview. Node is not involved. Full record/rule/transition parsing, LSP, persistence, GUI and AI provider integration remain outside this delivery.

## Checks

New tests cover observer/default parity, empty retention with delivered events, cancellation before start/at the last event/across a synchronized host thread, repeatable compiled expressions, cancellation of skipped-branch work, trace-count limits during streaming, UTF-16 payload accounting, exact budget boundaries, and preservation of arithmetic failures. Passing these tests is bounded evidence, not full-language completion or a concurrency proof.
