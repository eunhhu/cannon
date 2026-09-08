# Recorded implementation decisions

## ADR-001 — Executable source is the canonical model

Accepted for the prototype. Data and policy are source, not an AI-readable specification that must be translated into another hidden implementation. Views, traces, and change reports derive from one shared frozen compilation. Design rationale remains versioned prose connected to the implementation; it does not override runtime rules.

## ADR-002 — Separate identity, source version, and location

Accepted. Stable IDs survive renaming; snapshots identify exact source and engine; spans identify where a specific source originated. Historical scenarios use bound stable references when replayed. Field IDs are strongly recommended and omission is visible as a warning.

## ADR-003 — Hand-written parser for the first deliverable

The earlier design recommended Langium. The delivery environment could not reach the npm registry and did not have Langium cached. The original artifact recorded a local cached build. In this Cannon import, TypeScript 5.8.3 is available but registry DNS is unavailable. Local validation uses the documented preinstalled Node type definitions; clean lockfile installation must be verified independently in CI.

To avoid shipping an untested dependency-heavy scaffold, this executable slice uses a small recursive-descent/precedence parser behind a narrow adapter. This is a prototype substitution, not a rejection of Langium. No Langium/LSP integration is claimed. The replacement criterion is that a Langium frontend lowers to the same semantic model and passes the same execution/review conformance tests. Language semantics must not leak into editor code during that migration.

## ADR-004 — Interpreter first, no emitted business logic

Accepted. The rule being inspected is the rule being executed. Any future code-generation backend must be tested against this reference behavior rather than silently changing overflow, effects, state validation, or trace meaning.

## ADR-005 — Historical expected outcomes do not move with the candidate

Accepted. Candidate tests and historical replay are independent views. Old inputs run against candidate definitions using stable IDs. Expected outcomes remain evaluated in the old snapshot. A changed or deleted scenario cannot erase that old evidence.

## ADR-006 — Conservative freshness before clever incremental reuse

Accepted. Source path/content, engine fingerprint, runtime, and execution options all matter. The prototype does not reuse prior successful checks across edits. Fine-grained invalidation can be added only when its dependency rules are tested. Checksums provide consistency, not authenticated attestation.

## ADR-007 — Explicit external actions only

Accepted. Build, test, inspection, and demo do not create repositories, call AI providers, or modify deployed systems. Source application is an explicit command. Repository publishing is outside the runtime and is not performed by any install/build/test hook.

## ADR-008 — Test independent cases before adding more language

Accepted. The test suite includes parser/checker failures, runtime guards, stable identity renames, expectation edits, stale proposals, filesystem workflows, and a bounded exhaustive inventory set. The 3,060 input combinations are evidence for that range, not a universal proof.

## Follow-on decisions not yet made

Do not pick persistence isolation, data migration defaults, cross-module ownership, default retry behavior, effects, or an AI provider silently while implementing another feature. Bring each in as a reviewable design change with its own acceptance cases.

## Reference documentation

These are implementation references, not evidence that this entire design already exists:

- Node test runner: https://nodejs.org/api/test.html
- VS Code extension API: https://code.visualstudio.com/api
- VS Code webviews: https://code.visualstudio.com/api/extension-guides/webview
- Langium grammar frontend: https://langium.org/docs/reference/grammar-language/
- GitHub CLI repository creation: https://cli.github.com/manual/gh_repo_create
