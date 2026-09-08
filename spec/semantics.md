# Intent 0.1 execution and review semantics

This document states implemented behavior, including important limitations. Tests should challenge it rather than merely mirror implementation output.

## Syntax and values

A source contains one `module Name` followed by `record`, `rule`, `transition`, or `scenario` declarations. Every declaration has `@id("stable-id")`. Fields may have their own annotation; omitted IDs are derived from record ID and field name with a warning. Global IDs must be unique. Identifier syntax is ASCII; strings are double-quoted JSON strings and may contain Unicode. `//` comments and optional declaration semicolons are supported.

Types are `Int`, `PositiveInt`, `NonNegativeInt`, `Bool`, `String`, and nominal record types. Integers use the JavaScript safe integer interval; out-of-range literals and arithmetic overflow are errors. There is no floating-point arithmetic, division, implicit coercion, nullable value, array, union, generic, loop, arbitrary JavaScript, network function, or storage statement in v0.1.

Operators, from low to high precedence, are `or`/`||`, `and`/`&&`, `==`/`!=`, comparisons, `+`/`-`, `*`, unary `-`/`not`/`!`, and field access. Binary operators associate left. `and` and `or` short-circuit. `if ... then ... else ...` evaluates only the selected branch. The non-selected expression is still type checked.

Records are nominal. Canonical runtime representation is `{ $record: stableRecordId, fields: { stableFieldId: value } }`. Display names are a view. Equality is structural over canonical JSON values and stable IDs; JavaScript object prototypes are not part of language equality. Constructors require every declared field exactly once and reject unknown fields.

## Type checking and validation

Basic type incompatibilities, missing references, unknown fields, arity errors, duplicate identities and unsupported recursion are static errors. The checker permits numeric refinement conversions and validates them at execution boundaries. This is not refinement-type theorem proving.

Record invariants see that record's fields directly. They must be Boolean and cannot call rules or construct records. Record construction, external callable arguments, and callable return values validate field refinements and invariants. A failed check returns a structured runtime error, never a repaired value.

Recursive call graphs and recursive record types are rejected. The prototype does not attempt termination proofs or support cyclic runtime data.

## Rules and transitions

Rules are pure expressions. Parameters bind left-to-right evaluated argument values. A transition is also pure: ordered `require condition else "REASON"` guards precede a `next` expression. The first false guard returns `GUARD_REJECTED` with its reason. A transition does not write a database, retry a conflict, or publish an event.

Transitions can be invoked by the host API or a scenario's actual expression, not from a rule or another transition. Record/type boundaries are validated before arguments enter and after a result leaves the callable.

The memory store demonstrates versioned compare-and-set in one synchronous JavaScript instance. It is not an implementation of cross-process, distributed, or database transaction isolation. Schema migration and persisted data compatibility remain unknown.

## Scenarios

Each scenario contains one `expect actual == expected` assertion. Parenthesize complex Boolean/conditional actual expressions as needed. Expected expressions cannot call rules or transitions: the implementation under test must not compute its own expected answer. They may contain literals, record values, and pure operators.

Scenarios have stable IDs independent of display names. Verification with zero scenarios is not success. An execution error is distinct from an unequal value. The language does not yet have an `expect error` assertion; runtime error cases are tested through the host test suite.

## Execution evidence and resource limits

Trace entries are actual interpreter observations: values, operators, selected branches, skipped Boolean operands, invariants, guards, and returns. Entries carry source ID, URI, UTF-16 start/end offsets, and one-based line/column. Source ranges end exclusively. No LLM generates trace facts.

The source limit is 262,144 UTF-16 code units, tokens are limited to 20,000, definitions to 512, and expression nesting to 128. Default execution limits are 10,000 steps, depth 128, and 10,000 trace entries. Limits fail explicitly; a clipped run is not reported as success. These are development safeguards, not a claim of secure hosting for hostile input or wall-clock isolation.

Evidence records exact source/build/runtime/options and a checksum. Engine fingerprint inputs include source engine/CLI files, semantics documents, build script, package manifests/lock, and compiler configuration. Any change conservatively invalidates prior evidence. A different path or Node version also makes old evidence stale. Evidence hashes are not signatures and cannot establish author identity or resist intentional forgery.

## Change comparison

Definitions and fields are matched by stable IDs. Formatting and source location changes do not count as structural changes. Model, interface, policy, identity, scenario-input, and expectation changes remain separate categories. Type name changes may still produce conservative interface changes; this version does not claim a canonical proof of type-alias equivalence.

Dependency traversal produces impact candidates, not a complete behavioral proof. There are no dynamic external calls in the DSL, and the checker does not analyze arbitrary host adapters.

For baseline replay, the old actual expression uses stable references against new definitions. The expected outcome is evaluated in the old snapshot and preserved. New scenario expectations cannot redefine that historical result. Deleted tests are still replayed from the old snapshot. Missing definitions or incompatible record shapes are explicit errors. Historical trace spans are never relabeled as candidate source.

## Proposals and application

A proposal includes author category, unverified rationale, candidate text, base snapshot ID, and a checksum. Preview is deterministic for exact inputs. Applying requires the exact preview ID and unchanged base snapshot. A malformed or statically invalid candidate cannot be applied.

Approval is not a correctness claim. A reviewed policy change can deliberately fail a historical expectation; the tool must display that fact rather than silently updating the expectation. No provider is trusted more than a human or another provider.

The CLI applies one file with a cooperative exclusive lock, a temporary file, a last source check, and atomic rename. This prevents this tool's concurrent writers from silently overwriting each other. It is NOT an OS-level compare-and-swap against unrelated editors: a non-cooperating writer can race after the last check. Multi-file atomic application, crash recovery, symlink-ancestor hardening, and editor document-version transactions are future work. Stale lock files are not automatically deleted; inspect before removing them.

## Not claimed

This is neither a general hostile-code sandbox, formal verification system, production database, full architectural ownership checker, complete IDE, authenticated audit system, nor autonomous agent. It implements a tested subset of the intended development model and preserves the remaining boundaries explicitly.
