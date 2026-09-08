# Architecture

## Purpose

A developer edits the domain model and policy directly. AI can propose or critique that meaning, but neither implementation nor conversational memory replaces the project's inspectable state.

The first vertical slice is inventory reservation. This is not an inventory-specific hard-coded evaluator: the same expression engine supports other bounded pure models and policies. The language is nevertheless intentionally small and not yet a general-purpose programming system.

## Data flow and ownership

`source text → parser → binding/type checks → frozen Compilation → execution / comparison / inspection → CLI or another adapter`

`Compilation` carries the exact source snapshot, an engine build fingerprint, stable-ID-bound expressions, diagnostics, and dependencies. It is the shared semantic model, not a second editable database. The parser does not execute anything. The type checker does not silently invent missing fields, policies, or migration defaults.

`src/core/model.ts` defines that contract. `src/language/parser.ts` only parses text. `src/core/checking.ts` binds names to IDs, checks types and restrictions, and derives dependencies. `src/core/compile.ts` composes parsing and checking and freezes the result.

`src/core/execution.ts` interprets expressions, validates boundary values, and records real evaluation steps. `src/core/changes.ts` compares structure and replays historical scenarios. `src/core/evidence.ts` associates executed results with exact source/build/runtime/options. `src/core/proposals.ts` makes human/assistant changes go through the same preview and exact-review check. `src/core/inspection.ts` produces a view of the current model without inventing deployment state.

`src/filesystem.ts` handles source I/O and cooperative single-file application outside the semantic engine. `src/adapters/memory-store.ts` is a deliberately separate illustrative compare-and-set store. It does not persist data or automatically validate an arbitrary record's business constraints.

`src/cli.ts` is the shipped consumer. Editor and AI integrations should consume the same public API in `src/index.ts`. The editor source is not included in this remote delivery; see `delivery.md`.

## Identity and provenance

A declaration ID identifies a persistent domain concept. A snapshot digest identifies exact source plus engine build. A source ID identifies exact URI and text. A span points into one source ID; it is not a promise that the same line in the latest file means the same thing.

Fields may have explicit stable IDs. A name-derived fallback is allowed only with a warning. Calls, field access, and record construction are bound to IDs during checking. This allows historical scenario input expressions to be evaluated against renamed candidate definitions.

During baseline replay, some expressions originate in the historical source and called definitions originate in the candidate source. Each trace step retains its own origin. The change report embeds both snapshots so neither origin becomes an unexplained line number.

## Trust boundaries

A valid type is not a proven policy. A passing scenario is not universal correctness. A digest is not an authenticated signature. A proposed rationale is not a computed fact. A source file in the workspace is not a deployment.

The core never imports an AI SDK or performs I/O effects. Host TypeScript adapters remain ordinary trusted application code; their claims are not verified by the DSL checker. A future capability/effect system must explicitly distinguish declarations from enforced restrictions.

## Why not a separate design database?

It would create synchronization work between a design model, implementation, and generated explanations. Here diagrams, inspection, diagnostics, traces, and review reports are derived views of executable source. Decision rationale stays in ordinary versioned documents, not as a competing source of runtime policy.

## Scalability limits

The current checker and dependency analysis favor clarity over large-project indexing. One file is capped in size and nesting, and execution is budgeted. Multi-file incremental compilation, selective evidence invalidation, and IDE worker isolation are subsequent work. Do not infer large-codebase performance from this prototype.
