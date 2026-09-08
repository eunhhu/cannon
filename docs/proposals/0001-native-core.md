# Proposal 0001: native semantic core, thin editor integration

Status: **Proposed; not an approved migration and not an implemented Rust backend.**

This proposal answers the implementation-language question without changing Cannon's architecture silently. The current executable reference is TypeScript. The owner has asked why it was chosen instead of Rust or C++; asking that question is not approval to rewrite the engine.

## What was optimized in the first prototype

The first deliverable optimized the development of one inspectable vertical slice: executable policy, traced evaluation, structural change comparison, preserved historical expectations, and reviewed proposals. TypeScript let the same model and APIs be used by the interpreter, CLI, tests, and an experimental editor adapter with a single initial toolchain.

This was a prototyping decision, not a benchmark result or a requirement that all future Cannon programs execute on JavaScript. The language used to implement Cannon, the execution strategy for Cannon programs, and any future compilation target are separate decisions. The current prototype is an interpreter; it is not a native compiler.

## Proposed direction

Prefer **Rust for the long-lived shared semantic engine and CLI**, with **JavaScript/TypeScript only for the thin VS Code adapter and presentation layer**. Keep the existing TypeScript engine as a temporary reference during migration, not as a second permanent authority.

Proposed responsibility boundary:

```text
Cannon source
    -> native core: parse, bind, check, evaluate, trace, compare, review
        -> CLI
        -> editor protocol adapter -> VS Code client
        -> structured proposal API -> optional AI provider adapter
```

The editor and AI integrations must not implement their own policy evaluator. LSP can carry standard editor features; domain-specific inspection, evidence, comparison, and proposal messages need their own versioned contract. LSP alone does not define those operations.

Use an explicit process protocol first rather than making a native Node addon, C ABI, or WebAssembly runtime a prerequisite. Those are possible later adapters, not promised deliverables. A separate process is a failure/resource boundary, not automatically a hostile-code sandbox.

## Why Rust is the leading candidate

Rust's compile-time ownership rules are useful for managing the lifetime of parsed projects, analysis state, and execution records. The goal is explicit ownership and controlled memory/resource use in a long-running engine without depending on a garbage collector. This does not automatically make an interpreter fast, make snapshots immutable, or prove business rules correct.

Rust introduces real work: graph representations and caches need careful ownership design; serialization and position mapping must be explicit; native binaries need platform-specific build and distribution testing. Multi-file incremental analysis and cancellation require design and measurement regardless of language.

There is **no Cannon benchmark showing that the existing TypeScript engine is too slow, or quantifying a Rust speedup**. The proposal is about the intended architecture and control over the runtime; performance claims remain open until measured.

## Where C++ fits

C++ is not excluded. Prefer it if a concrete integration requires extensive direct C++ APIs, or if the maintainers' expertise and existing native code materially reduce development and maintenance cost. Choosing a future compiler backend does not by itself settle the language of the semantic core.

For a new core without such a dependency, this proposal favors Rust's ownership discipline. That is a project preference, not a claim that C++ cannot safely or efficiently implement this design. No C++ proof of concept or Rust/C++ benchmark has been performed.

## Contracts that must survive a port

The current contract is in `spec/semantics.md`; implementation convenience must not redefine it.

- Preserve declaration and field IDs, nominal record identity, dependency relationships, and historical ID binding across names changing.
- Preserve the current safe-integer domain, checked arithmetic, and structured overflow errors. Rust `i64` is wider than the current language integer domain; do not silently widen it. A wider integer model requires a separate language-version decision.
- Preserve evaluation order, short-circuiting, selected branches, boundary checks, guard rejection, and the distinction between execution errors and unequal scenario values.
- Preserve source origins. Existing spans use UTF-16 offsets; Rust string byte indices are not substitutes. Test Unicode and explicit position conversion, including non-BMP characters and line endings.
- Preserve baseline replay independently of changed or deleted candidate expectations. A port must not make the new implementation calculate its own expected answers.
- Specify serialization, object-key ordering, error representation, and trace structure before comparing output across implementations.
- Preserve conservative evidence invalidation. An engine or runtime change produces new evidence provenance; equal results do not imply equal snapshot IDs or allow old evidence to become fresh.
- Preserve reviewed proposal identity and stale-base rejection. Filesystem application remains outside the pure core, with its current limitations made explicit.

An interpreter written in Rust does not make Cannon programs automatically acquire Rust ownership, memory safety, or concurrency semantics. Those remain properties of Cannon's own design and its host integrations.

## Migration sequence if approved

1. Extract language-neutral conformance fixtures with independently specified expected results, errors, identity changes, historical replay outcomes, and position mappings. Use the existing implementation to expose discrepancies, not to author every expected answer.
2. Define a versioned external contract for values, diagnostics, source origins, trace events, comparison facts, and proposals. Keep build/runtime provenance distinct from semantic comparison.
3. Implement a Rust library and CLI for the bounded pure subset. Start with values and evaluation, then binding/checking, traces, comparison, and proposal review. No database or LLM SDK belongs in that core.
4. Run both implementations against the conformance suite. Differential agreement is useful but is not a proof when both implementations can share a mistake. Review disagreements against the written semantics.
5. Add a thin editor client to the agreed protocol. Existing editor source excluded from an earlier delivery is not re-uploaded or routed around its recorded transfer restriction by this proposal.
6. Measure cold start, repeated checks, execution with and without traces, comparison, peak memory, and cancellation on agreed workloads. State hardware and versions. Set regression budgets from measured requirements, not invented speedup targets.
7. Choose one default engine only after conformance and delivery checks. Retire the reference implementation according to an explicit decision; do not maintain two independent sources of language truth indefinitely.

## Approval boundary

The owner still needs to decide whether to adopt Rust for the core, what compatibility guarantees the first native release must keep, and which deployment targets matter first. This proposal does not approve an integer change, new syntax, new business policy, or a new execution/effect model.

A migration implementation should be a separate reviewable PR. That PR must state semantic changes independently from mechanical translation, include conformance evidence, and identify remaining unknowns.

## Current evidence and non-claims

This PR adds a proposal and a review template only. It does not add Rust code, restore the VS Code package, modify the TypeScript engine, run a native backend, or establish native performance. The existing repository delivery boundary remains described in `spec/delivery.md`.

The new files are outside the current engine build-fingerprint inputs. Existing validation records are not rewritten as evidence of a port. Remote CI and local tests must be reported from their actual runs, not copied from earlier delivery counts.

## References

- Rust ownership: https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html
- VS Code language client/server separation and language-independent servers: https://code.visualstudio.com/api/language-extensions/language-server-extension-guide
- Current Cannon contracts: `spec/architecture.md`, `spec/semantics.md`, `spec/assistant-contract.md`, and `AGENTS.md`.
