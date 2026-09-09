# Native typed-policy sessions and exact-review application

This implements the previously unintegrated session idea on top of the **current typed-input engine**, not the older constant-expression API. It reuses `compile_with_inputs`, `evaluate_bound_observed` and the existing policy-review algorithm. It does not deliver the separate blocked full-file workbench or editor package, switch the default engine, or add a second evaluator.

## Run the complete example

```sh
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo run --locked --offline -p cannon-core --example session_review
```

The example executes a typed inventory boundary, proposes a policy plus expected-value change, shows that historical replay still fails, rejects default application, explicitly acknowledges the changed value, re-runs the newly installed revision, rejects stale review reuse, and cancels a queued old run. All changes are in memory. There is no Node subprocess, network request, provider invocation or file write in this API. A pinned Rust toolchain must already be installed for offline commands.

## Document and identity

`PolicyDocument` contains source text, `InputSpec` declarations and independent `PolicyCase` examples. `PolicySession::new` takes ownership and exposes only immutable `Snapshot` handles. A snapshot retains exact URI, source, schema, cases, compilation or diagnostic, and revision-local cancellation. Type/binding failures in example inputs remain execution errors; the session does not repair or infer values.

Revisions are opaque **process-local identities**. A matching number, URI, source, or case list does not identify the same revision in another session. Even editing to identical text increments the revision; changing A to B and back to A cannot resurrect old results. Revision overflow is an explicit error, not wraparound. Closing or dropping the owner cancels current work. Existing completed receipts remain readable historical results.

These handles are not hashes, signed approvals, persistent audit records or a wire protocol. Reopening a file starts a new session. No serialization of private review objects is provided. A remote editor/AI transport will need its own versioned, authenticated application boundary instead of assuming these in-process handles survive transport.

## Editing and execution

`edit_source` preserves both schema and cases. `edit_policy` changes source/schema together while preserving cases, enabling stable-ID input renames. `revise_cases` changes examples/expectations explicitly, independently of code. Every accepted edit invalidates the old revision and signals cancellation. A rejected metadata/size edit does neither.

Incomplete or ill-typed expression text remains a snapshot with a compilation diagnostic so editing can continue. Invalid schema/case metadata and excessive resource inputs are rejected before installation. The existing source and metadata limits apply. Session cases additionally have a cumulative limit of 1,048,576 UTF-16 units across textual inputs and expected values, separate from the source budget and per-case limit. Host-owned copies, retained old revisions, callback allocations and process memory are not globally bounded by this limit.

`prepare_case` resolves a stable case ID and returns an immutable `RunRequest`. The host may move this to its worker thread. The session never spawns a worker. `execute_observed` emits actual events from the same typed evaluator with the exact origin snapshot. Existing input/limit validation precedence, arithmetic, short-circuiting, UTF-16 spans and observation budgets are unchanged. Cancellation is cooperative, not forced termination, and compilation diagnostics may be returned without entering execution.

`RunReceipt` binds the snapshot, case input/expected value, limits, observation settings and outcome. It distinguishes assertion mismatch, compilation error and execution error. Streaming explicitly reports emitted versus retained events. A current revision does **not** mean a successful result, latest request or deployment. When several runs of the same revision are pending, retain the desired `RunTicket` and check both `ticket.matches(receipt)` and `session.receipt_is_current(receipt)` before presentation. The session does not choose newest-request ordering for the host.

## Proposals and reviews

`propose` takes an exact base snapshot, an owned candidate document, rationale and declared author. Rationale/authorship are unverified metadata, not computed facts. It does not alter the current document. Both humans and assistants use the same APIs.

`preview` reuses the existing structured policy comparison and its independent historical replay. Source logic, input schema, case input and expected-value changes remain separate. Candidate changes/deletions cannot rewrite baseline inputs or expected values. The additive `review_policies_cancellable` entrypoint checks a shared token before work, between cases, during evaluation, and after completion. Cancellation or cumulative review-budget exhaustion returns an error, not a complete review. The original `review_policies` function is preserved as a fresh, uncancelled wrapper. Existing review validation precedence and successful outputs stay unchanged.

`ChangeReview` has private fields and exposes only shared references. Mutating a detached clone of its public report cannot alter the candidate or validation used by `apply`. A document changed after proposal creation causes stale application rejection, even if the text now happens to be equal again.

## Application contract

Application changes **only in-memory session state**, never files or deployments. It requires the exact current base, a compilable candidate, a complete review, and nonempty passing baseline and candidate suites. Default `HistoricalPolicy::Preserve` also requires passing historical replay.

`AcknowledgeChangedValues` is a separate explicit choice permitting historical value mismatches. It does not bypass baseline failures, removed-all candidate cases, compilation failures, invalid input shapes, runtime errors or exhausted review budgets. In particular, an incompatible schema that makes old inputs unexecutable requires a separate migration design; it cannot be labeled a simple expected-value change.

A successful apply installs exactly the reviewed candidate with a **new snapshot identity and cancellation lifetime**. It does not promote candidate-preview observations into current execution receipts. Re-execute the installed revision to obtain a current result. The original review remains unchanged, including any deliberately acknowledged regressions. Applying the same review twice or applying another old-base proposal fails. Copying a review does not create a new approval.

Direct editing remains available for exploration and for repairing a broken baseline; it does not pretend to have passed the stricter proposal gate. This is deliberately not a capability restriction on AI source authorship and not a universal policy-correctness proof.

## Verification and remaining scope

The integration adds 44 session tests and one revision-overflow unit test. They exercise typed inputs, ID-preserving renames, concurrent edit/cancel checkpoints, same-revision request identity, stale results/reviews, rejected edits, case deletion, independent historical replay, explicit acknowledgment boundaries, report-copy isolation, incomplete source and resource limits. CI runs debug/release plus strict Clippy on Linux, Windows and macOS, the existing 60 expression fixtures and 30 bound-input fixtures, existing Node 22/24 checks, and the example above.

Check the exact commit's Actions result for actual passes. Local source inspection or writing tests is not evidence that Rust compiled. This session lacked a local Rust toolchain, so native execution verification is performed on the repository's CI. Old `VALIDATION.json` records are not rewritten as current native evidence.

This is one typed scalar policy document, not a full `.intent` module graph, LSP server, GUI, persistence adapter, effect checker, authenticated AI integration, formal proof system or finished QA release. Previously blocked full-file/editor sources are not repackaged or transmitted by this change.
