# Versioned native sessions and exact-review application

This integrates the previously uncompiled session candidate with the published
**typed scalar-policy engine**, based on commit `3ac446e`. It adds no second
parser/evaluator, no full-file workbench, and no provider SDK. Existing native and
TypeScript commands, expectations, integer and Unicode semantics are unchanged.
The separately blocked workbench/editor content is not part of this change.

## Run the complete example

```sh
cargo run --locked --offline -p cannon-core --example review
```

The native example executes a baseline, exposes an expectation-changing policy
regression, rejects it under `HistoricalPolicy::Preserve`, explicitly acknowledges
that policy change, applies the exact immutable review, and re-executes the new
revision. It then demonstrates stale queued work and a candidate that changes its
input data to conceal a stricter comparison: the original input still fails.
No Node process, file mutation, network call or deployment occurs in the example.
The pinned Rust toolchain must already be installed for offline Cargo commands.

## One source of execution meaning

`ExpressionSession::new` is the zero-input convenience API.
`new_with_inputs` freezes source, input schema, example bindings, and an independent
optional expected value. `edit`, `revise_inputs`, and `revise_expectation` are
separate explicit changes; none silently repairs the other fields.

A `Snapshot` retains exact URI/text/schema/data/expectation and either the existing
`compile_with_inputs` result or its diagnostic. Compilation failures remain
editable, not replaced by last-good code. Missing, extra, ill-typed or out-of-range
bindings remain errors from `evaluate_bound_observed`; there is no text substitution.

Snapshot identity is an opaque, private `Arc` handle. Display revisions increase
without wrapping but are not credentials. Different sessions, identical-text edits,
and changing A to B back to A cannot resurrect an old handle. Handles are valid
only in this process, not cross-build evidence, signatures or persistent review IDs.

`prepare_run` produces an immutable request movable to a worker. Observers receive
actual source snapshots and interpreter events. Editing, closing or dropping the
session signals cooperative cancellation. Completed old results are retained as
history, not retroactively changed to cancellation. The owner must call
`receipt_is_current` immediately before consuming a result and coordinate that
check with UI state. Freshness does not mean correctness or newest-request order:
hosts must also match the requested settings/request and serialize UI updates.

## Historical inputs are not candidate inputs

`Snapshot::draft` makes a mutable `SessionDraft`; `propose_draft` validates and copies
it into an immutable proposal. Later mutations of the caller's draft cannot alter
the review. `propose` retains the current schema/data and changes only the supplied
source/expectation. Caller-declared author and rationale are not authenticated facts.

A preview runs three independently inspectable invocations using the same engine:

1. Baseline code with baseline data, checked against the independent baseline answer.
2. Candidate code with candidate data, checked against the candidate's own answer.
3. Candidate code with **baseline data and baseline answer**, joined by stable input ID.

`ChangeReview::historical` exposes that third receipt, including the data actually
used. Its `snapshot()` is the candidate source/schema; its `bindings()` and
`expected()` are the historical invocation values. Do not use the candidate
snapshot's stored example in place of the receipt's actual invocation inputs.
Changing/deleting a candidate expectation or changing its data cannot change the
historical check. New required IDs, removed IDs or stronger refinements can produce
explicit replay errors; no positional/name remapping or migration defaults are invented.

`CheckState` separates absent expectations, matches, mismatches, compilation errors,
and execution errors. `introduces_regression` is true only when the original example
passed and historical replay no longer passes. A broken original example is not a
new regression; its failure stays visible. A missing expectation is never a passing
verification. This session owns one example, not a complete test suite or proof of
all-input equivalence; use the existing `review_policies` API for multi-case reports.

Change facts separate exact text, schema, input-value, expected-value and inferred
result-type changes. Schema sequence differences are conservative changes; comments
are text changes, not proof of behavioral differences. No AI generates these facts.

## Applying the exact review

`apply` takes the immutable `ChangeReview` rather than a replaceable source payload.
It rechecks the session/base handle and rejects bad candidate compilations, incomplete
executions, and candidate expectation mismatches. `Preserve` also requires the old
expectation when present. `AcknowledgeChange` deliberately allows an old expectation
to change/fail; it cannot override candidate failure or stale/foreign/used reviews.
Neither setting proves a human reviewed it. Human and assistant proposals use the
same checks. Direct edits remain possible and do not pretend to be approved reviews.

Application changes **in-memory state only**. It creates a fresh snapshot identity
and cancellation lifetime while sharing the checked compilation. Staged receipts
are never promoted to current evidence: rerun the applied revision. Existing queued
work is cancelled, and all other reviews of the old base become stale. There is no
filesystem write, Git commit, deployment, authenticated approval or durable audit log.

## Limits and error scope

The existing source/token/depth/input limits are preserved. URI length is bounded
at 4096 bytes and proposal rationale at 8192 bytes, both nonempty after trimming.
Expected integers must be in the language's safe-integer domain; expected strings
have at most 262144 UTF-16 units. At most 256 schema entries and bindings, 128-byte
input IDs/names, and 262144 cumulative input text units are retained per snapshot.
Oversized payloads fail before session mutation. Bounded invalid schemas are retained
as compilation diagnostics; wrong bindings are execution diagnostics, not repaired values.

Each of the three preview invocations has its own execution/observation limits.
`preview` additionally caps observed text at 1048576 UTF-16 units **per invocation**.
`preview_with_options` explicitly selects alternative observation limits/retention.
These are not aggregate project memory or wall-clock limits. Retention-disabled
receipts still record emitted counts and failures; clipped/cancelled runs cannot be
applied as successful candidates. Callers can retain old snapshots indefinitely and
must budget their own queues/history. Compiler cancellation, deadlines, hostile-code
isolation, newest-request scheduling and persistent evidence remain outside this API.

## Verification

This integration adds 41 session tests and 16 typed-input session tests. The example
is also run by native CI on Linux, Windows and macOS. Existing tests and both
cross-engine fixture suites are preserved. Read the exact commit's Actions results;
file presence or previous validation records do not certify this integration.

```sh
cargo test --workspace --locked --offline
cargo test --workspace --release --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo run --locked --offline -p cannon-core --example review
npm ci --ignore-scripts
npm test
node scripts/check-native-conformance.mjs
node scripts/check-bound-conformance.mjs
```

The development container lacked Rust and could not download its toolchain.
Compilation, ownership checking and execution are established by remote CI, not
inferred from source inspection. Full `.intent` parsing, records/transitions, GUI,
LSP, real AI-client integration and production persistence are not completed here.
