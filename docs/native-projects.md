# Native module boundaries and composable policy projects

This is an additive Rust host API over the published typed scalar-policy engine.
It does not import the separately blocked full-file workbench/editor, introduce a
second expression evaluator, or change the existing commands and language syntax.

The new unit of work is a checked project graph: modules own external data and
policies, policies explicitly connect typed ports to data or other policy results,
and execution and historical review consume the same immutable compiled graph.

## Run the complete example

```sh
cargo run --locked --offline -p cannon-core --example project
```

An installed pinned Rust toolchain is required. Node is not involved in this
example. The three editable expressions live in `examples/projects/inventory/`;
they are embedded at build time by the Rust example. Ownership, imports and port
wiring are declared in `crates/cannon-core/examples/project.rs`, not inferred from
file paths. This is not a runtime project-file loader or a new `.intent` parser.

The example executes available stock -> reservation decision -> shipping price.
Checkout imports Inventory and consumes its exported reservation result. It cannot
read Inventory's raw stock inputs or private policies through the project graph.
A candidate then changes both the reservation comparison and its expected answers.
Its own case passes, but the original case fails for both the reservation decision
and shipping price. Finally an attempted private-policy link is rejected before
execution. Demo success means these checks worked, not that the candidate review
passed or that the new policy was approved/applied.

## Authoritative declarations

`ProjectSpec` has `ModuleSpec`, `OwnedInput` and `PolicySpec` declarations.
A policy owns its exact expression text, URI, input schema, explicit port links,
module ID and export flag. `InputSource::External(id)` connects an owned raw input;
`InputSource::Policy(id)` connects a previously computed scalar result.

IDs remain distinct from labels. Module, external-input and policy IDs each have
a separate namespace; port IDs are local to their policy. External input labels
may repeat across owners because values bind by ID. Changing a label never
implicitly changes or remaps an ID. Module/policy display labels may repeat;
interfaces must present IDs/owners when a label would be ambiguous.

`compile_project` validates and copies declarations, compiles every expression
with the existing `compile_with_inputs`, and returns `Arc<CompiledProject>`.
Fields are private; callers can inspect the spec, checked expressions, dependency
map and execution order but cannot mutate compiled nodes. Mutating the original
host spec does not mutate the compiled project. Recompiling equal text makes a
new identity. These handles are not durable snapshot hashes or signed evidence.

## Enforced architecture checks

- Raw external inputs can be linked only by policies in their owning module.
- Same-module policies can use private results. Cross-module links require both
  an explicit direct import and an exported provider policy. Imports are not
  transitive access grants.
- Module imports and policy dependencies must each be acyclic, including unused
  declared imports. Unknown owners/providers/imports and duplicate IDs fail.
- Every declared local input is connected once. Missing/extra ports fail even if
  an expression branch would not read that port.
- Provider output/base-input types must match port types. Numeric refinements
  remain runtime boundary checks; an `Int` result is not statically proven positive.

These checks constrain this pure graph, not arbitrary Rust host code. Export flags
are not confidentiality controls: inspection and execution receipts intentionally
expose internal policies and values to the trusted host. There is no OS memory
isolation, database permission system or external-effect checker here.

## Execution and source origins

`execute_project` and `execute_project_observed` share one path. All declared
policies run exactly once in deterministic topological order, breaking ties by
policy ID. This is **eager dataflow**, not lazy function invocation: even a policy
with no consumers runs, and all its declared inputs are resolved. Within an
expression, ordinary short-circuit and selected-branch semantics are unchanged.
Shared providers in a diamond are not recomputed for each consumer.

All external values are validated with the existing input validator before any
policy executes. Invalid limits take precedence over external-input errors, which
take precedence over cancellation. Only validated, bounded payloads are cloned
into a receipt. `accepted_inputs()` returns `None` on preflight failure and
`Some(empty_map)` for an accepted zero-input invocation; `inputs()` exposes only
the retained accepted map and is empty when the payload was omitted. Invalid
input errors do not claim an empty invocation actually executed. Review case
records separately retain the bounded submitted example for diagnosis.

Every `PolicyRun` exposes its actual local bindings and `ObservedOutcome`.
Use its ID to find the exact source/URI in `ProjectRun::project()`; UTF-16 spans
remain relative to that source. The observer receives policy ID plus real engine
events, not an AI-generated explanation. Observation and cancellation use the
same existing evaluator. A failure stops dependent and remaining policies; prior
receipts stay visible, but `ProjectRun::result()` is an error, not a partial success.
The successful result map contains all policy results, including private policies.

`ProjectRun::is_for` compares the immutable project handle, not equal text. It does
not implement newest-request scheduling, source freshness after a file edit,
authenticated approval, or a persisted build identity. Hosts must coordinate
receipt consumption with their own current project/version and request settings.

## Budgets and cancellation

Compilation permits 1..=64 modules, 1..=128 policies and at most 256 external inputs.
The existing 262144 UTF-16 units per expression and 256 local ports per policy
remain; combined expression source is capped at 1048576 UTF-16 units. Existing
ID/name constraints apply to input schemas, and module/policy IDs are nonempty,
at most 128 UTF-8 bytes without whitespace/control characters. Display labels
are nonblank and at most 128 bytes; source URIs are nonblank and at most 4096 bytes.

Project defaults are the existing per-policy limits plus 100000 aggregate steps,
100000 emitted events and 1048576 observed text units. Remaining budgets are
carried into the next policy, never reset per policy. Trace retention can be off
without disabling emitted-event or text accounting. Text budgets count repeated
UTF-16 payload observations, not UTF-8 bytes or retained entries. Step counts
include the evaluator's attempted over-budget visit when a step limit fails.

Observers are synchronous trusted host callbacks. Cancellation is cooperative,
checked around policy execution and by the existing evaluator. It cannot forcibly
interrupt a callback. Partial observations are not completed results. These limits
are not total process-memory, total receipt-retention or wall-clock guarantees;
hosts must bound retained projects, receipts, input copies and queues themselves.

## Structural comparison and historical review

`compare_projects` reports module imports/names; raw-input owner/name/type;
policy owner/visibility/ports/links/URI/text and compiled logic separately.
Whitespace/comments can change source text without changing compiled logic.
Declaration-list order is canonicalized; local schema-order differences remain
conservative interface changes. Impacts are transitive consumers in either graph,
including changed metadata; they are candidates to inspect, not proofs of behavior.

A `ProjectCase` supplies stable ID, label, external inputs and independent expected
outputs keyed by policy ID. Expected outputs are values, never calls into the
implementation. A nonexistent output ID is an explicit failed assertion.

`review_projects` runs baseline cases on baseline code, candidate cases on candidate
code, then **baseline inputs and baseline answers on candidate code**. It does not
replace historical data from the candidate suite. Deleted cases still replay;
removed expected-output policies fail; changed input constraints remain execution
errors. Both immutable graphs and the actual case/receipt for every run remain
inspectable. Case input and expectation changes are separate facts.

The aggregate review gate requires all three suites to be nonempty and every case
to have at least one passing assertion. Missing expectations are `NoExpectations`,
not success. Existing failures remain visible but are not called new regressions.
`regressions()` reports only cases that passed before and no longer pass in replay.
No return value proves all-input equivalence or human authorization.

Each suite allows up to 64 unique cases. Case input and expectation payloads are
bounded, and total old/new text payload is capped before copying. The review has
additional cumulative step/event/text budgets shared across **all three suites**.
Cancellation or exhausted execution budgets yield `REVIEW_INCOMPLETE`, not a
complete or passing report. Ordinary typed-input/arithmetic failures remain case
execution failures with their actual diagnostics. Review limits are recorded.

## Validation and remaining scope

The project tests exercise architecture violations, cycles, exact port wiring,
refinements, source locations, shared providers, aggregate budgets, threaded
cancellation, immutable receipts, changed/deleted expectations and input data,
and 3060 inventory combinations. Existing scalar, session, TypeScript and both
cross-engine suites are preserved. The new project example runs in native CI on
Linux, Windows and macOS; debug/release and strict Clippy stay enabled.

Use exact-commit CI results as evidence. The initial candidate's 53 project tests
passed on Linux but strict Clippy rejected its large error representation and an
unused import; those issues were corrected without suppressing lint or weakening
tests. Further boundary tests explicitly cover omitted invalid input payloads.
Local Rust compilation was unavailable in the development container.

This delivers host-declared modules and scalar-policy composition/review. It does
not deliver full `.intent` declaration parsing, record/state-transition support in
Rust, project file loading, incremental compilation, project-wide session apply,
persistence, LSP/GUI, a real AI provider, effect capabilities or production deployment.
Single-policy sessions and existing commands continue to work unchanged. Earlier
blocked workbench/editor content is not retransmitted by this implementation.
