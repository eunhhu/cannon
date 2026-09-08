# Assistant integration contract

The engine is provider-independent. No LLM API key, provider account, prompt history, or remote call is required to run it.

## Read surfaces

`compile(source, uri)` returns diagnostics, exact snapshot, bound definitions, and dependencies. `inspectProject(compilation)` exposes the actual model and honest status. `inspectDefinition(compilation, stableId)` narrows the context to one definition and its immediate relations. An AI wrapper can choose what source to provide after explicit user authorization; nothing here automatically uploads a repository.

## Proposal surface

`createProposal(base, candidateSource, rationale, author)` records a proposal without applying it. The author is a declared category, not authenticated identity. Rationale is unverified text. `previewChange(current, proposal)` enforces the base version, compiles the candidate, computes the structural changes, and runs candidate and historical scenarios. `applyChange(current, proposal, reviewedPreviewId)` returns the exact reviewed candidate; the host is responsible for source persistence and document concurrency.

The CLI's `apply` is one such persistence adapter. Its filesystem limitations are documented in `semantics.md`. An editor should use document-version-aware workspace edits rather than assuming that a filesystem write coordinates with an open unsaved buffer.

## Review facts must not be synthesized

A provider may explain a computed policy change, suggest a new schema, propose architecture, or search for counterexamples. Its interpretation should point to change IDs and trace spans. It may not claim “no other impact” from a lack of observed test failures. It may not replace structural diagnostics or executed facts with persuasive prose.

Expected-value edits, scenario deletion, invariant changes, and interface changes must remain visible. An assistant must not auto-approve its own result because tests now pass. It can propose new acceptance criteria, but that is a policy decision, not merely an implementation fix.

## Current integration status

The library and CLI contracts exist. A provider adapter, MCP server, authentication, cost control, prompt construction, and model-specific structured-output schema are NOT implemented. Before adding one, keep the engine independent, obtain explicit source-sharing authorization, limit the supplied context, and log the exact base snapshot and provider-generated proposal without making conversation history a required part of project state.
