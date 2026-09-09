# Native typed inputs and scalar-policy review

This extends the **published expression engine**. It does not import the separate blocked full-file workbench, register a record/module parser, replace the TypeScript full-language command, or restore previously excluded editor files.

## What is now executable

The Rust library can compile one policy expression with an explicit input schema, bind values by stable ID, execute it repeatedly, and review two policies against independent examples. There is no source-text substitution and no Node subprocess in the native library or CLI.

```sh
cargo run --locked --offline --bin cannon-policy -- eval 'reserved + quantity <= onHand' --input stock.total:onHand:NonNegativeInt=10 --input stock.held:reserved:NonNegativeInt=8 --input request.quantity:quantity:PositiveInt=2 --json
cargo run --locked --offline --bin cannon-policy -- demo --json
```

The first command returns `true` and observed input/arithmetic/comparison steps. The second demonstrates a candidate that changes both `<=` to `<` and the expected boundary result: candidate examples pass, historical replay fails, and the review gate is false. The demo exits successfully only if that deliberately constructed regression is exposed; demo success is not a passing policy review.

`cannon-policy` is an additional binary so `cannon-native eval` and the existing npm `cannon` retain their command and output contracts. No files are written, no credentials are used, and nothing is deployed by these commands. Native builds need the pinned Rust toolchain installed; `--offline` does not install it. Windows binaries have the `.exe` suffix.

## Input contract

Use `compile_with_inputs(source, &[InputSpec])`, then `evaluate_bound` or `evaluate_bound_observed`. `InputSpec` contains an opaque stable ID, an expression-facing name, and one of `Int`, `PositiveInt`, `NonNegativeInt`, `Bool`, `String`. Numeric refinements share the static `Int` expression type but are validated at the argument boundary, not proven by a theorem solver.

Names are ASCII identifiers, at most 128 bytes, excluding `if`, `then`, `else`, `true`, `false`, `and`, `or`, `not`. IDs are nonempty UTF-8 strings of at most 128 bytes without whitespace/control characters; no normalization or inferred mapping is performed. The CLI additionally excludes `:` in an ID because its assignment format is `ID:NAME:TYPE=LITERAL`. The API has no such separator restriction.

All declared inputs must be supplied, even if a branch skips them or the expression does not use them. Extra IDs, missing values, incompatible scalar types, out-of-domain integers and invalid refinements fail **before expression evaluation**, with zero expression steps and no observed events. Metadata/argument errors use a documented zero-width boundary span at offset 0, not a fabricated declaration location. Invalid execution limits take precedence over binding errors; binding errors take precedence over a pre-existing cancellation signal. For valid inputs cancellation behavior is unchanged.

Argument validation is not included in expression step counts. It is separately bounded by at most 256 inputs and 262,144 cumulative UTF-16 units in text inputs. This is not a total-process memory or wall-clock bound. Scalar input values preserve escaped lone surrogates, checked safe integers, and the existing `[-9007199254740991, 9007199254740991]` domain.

Input schemas are cloned into opaque compiled arenas. Values are borrowed during a run and cloned only when read. Renaming or reordering input declarations does not change the ID used to find a value. Replacing an ID requires explicit data adaptation; names are not migration hints. `input_references()` reports statically bound read locations, including unselected branches; a trace entry is emitted only when the read actually executes.

CLI values accept Cannon scalar literals (including a negative integer or JSON-escaped string), not arbitrary expressions. Parentheses around a literal are accepted. These are not a claim to accept all JSON or arbitrary shell data. Do not pass secrets in command-line arguments: shells/process listings may retain them, and reports/traces expose supplied values. Hosts handling private data must design retention/redaction before publishing reports.

## Reviewing a change

`review_policies` accepts two compiled expressions, an independently owned old example suite, a candidate suite, and limits. Each `PolicyCase` has its own stable ID, name, ID-keyed inputs and expected scalar value. Neither evaluator computes its own expected answers.

The report separates input additions/removals/renames/type changes; bound-syntax changes; case names/inputs/expected values; baseline results; candidate-suite results; and **candidate execution with the exact old inputs and expectations**. Deleted candidate examples still replay from the baseline suite. A changed or deleted input identity is an explicit failed replay, never a guessed mapping.

`same_logic` compares the typed arena's expression structure and input IDs while ignoring positions, whitespace, aliases such as `&&`/`and`, and display names. It is not an all-input equivalence proof, and input-schema changes must still be inspected even when it returns true.

A suite with zero examples has `NoCases`, not `Passed`. `PolicyReview::passed()` requires nonempty passing baseline, candidate, and historical suites. A baseline failure remains visible but is not mislabeled as a new regression. A deliberate change may fail historical expectations; the report does not forbid it or rewrite acceptance criteria. Applying or approving that policy is outside this API.

Review is limited to 128 examples per suite, checked metadata sizes, per-run execution budgets and cumulative observed event/text budgets across all three suites. Cumulative exhaustion returns `REVIEW_LIMIT` with no complete report. These finite-run safeguards are not secure hosting or a total memory proof. Output objects remain ordinary host values, not signed or tamper-proof attestations.

## Output and provenance boundary

`cannon.native.bound-expression/1` wraps exact source, declared inputs, provided values, statically bound input references and observed execution. `cannon.native.policy-review/1` retains both exact policy sources/schemas, original/candidate examples, limits, categorized changes and each run's trace. Historical replay traces point at **candidate policy source**, because these cases contain scalar inputs rather than historical call-site expressions.

These formats are **not** the full engine's hashed snapshot/evidence/proposal formats. Package version is reported, but authenticated build identity, freshness verification, persistent approvals and stale-file application are not implemented here. Consumers must keep reports with their exact sources and must not use an old source-only renderer for bound execution, treat an empty retained trace as a complete history, or promote a report into a correctness certificate.

## Verification

```sh
cargo test --workspace --locked --offline
cargo test --workspace --release --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
npm ci --ignore-scripts
npm test
cargo build --workspace --locked --offline
node scripts/check-native-conformance.mjs
node scripts/check-bound-conformance.mjs
```

The original 39 Rust tests and 60 expression fixtures remain in place. The typed-policy addition contains 36 Rust tests (including 3,060 valid inventory argument combinations) and 30 independently specified scalar fixtures. The new runner checks expected results/errors in both implementations before comparing complete executed bodies, read locations and step counts. Only the explicitly known TypeScript rule-call wrapper and `ref`/`input` terminology are normalized. Input-validation diagnostics/step accounting are deliberately a separate native host API contract; complete arbitrary diagnostic equivalence is not claimed.

The workflow runs debug/release Rust tests, strict Clippy, the unchanged TypeScript suite, both conformance runners and all demos on Linux, Windows and macOS. Passing results must come from the actual workflow for the exact commit. Local development in this conversation checked the 30 reference cases against the unchanged TypeScript core; no local Rust compiler was available. The historical `VALIDATION.json` does not certify this change.

Full `.intent` declarations/records/state transitions in Rust, multi-file architecture enforcement, IDE UI, persistent review identity, data migrations and real AI-provider integration are still separate work. This API supplies a usable native execution/review boundary for scalar policies, not completion of that larger system.
