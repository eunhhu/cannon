# Work on Cannon without hiding its meaning

Read `STATUS.md`, `spec/architecture.md`, and `spec/semantics.md` before changing the engine.

The human owns architecture, data meaning, policy, and acceptance criteria. An assistant may propose changes to any of those, but must label the proposal rather than smuggling it into an implementation patch. Do not rename or regenerate stable IDs just to make code easier to rewrite.

`src/core/` is the shared meaning engine. It must not depend on VS Code, an LLM SDK, a database client, or an agent conversation. `src/language/` translates text to this model; editor and assistant integrations consume the same public API.

Never make scenario expectations match new output merely to obtain passing tests. Record expectation changes separately and preserve baseline replay. A type check, executed test, formal proof, deployment observation, and AI hypothesis are different kinds of evidence. Do not claim one from another.

Run `npm test` and `node dist/src/cli.js demo` after changes. Test errors and stale-state paths, not only happy paths. Update the semantics and status documents when the actual contract changes. Source changes require a rebuild; do not hand-edit `dist/` or generated build fingerprints.

No install/build/test command may publish a repository, send code to an AI provider, call a production database, or change a deployment. Publishing must remain an explicit action. Do not commit `.env`, credentials, private runtime data, caches, or `.intent/` reports.

Do not portray the interpreter as a hostile-code sandbox or evidence hashes as signed attestations. Do not claim full cross-process compare-and-swap from an atomic filesystem rename. Mark unimplemented and unverified boundaries explicitly.

When handing off, state what changed, the evidence actually obtained, and the remaining unknowns. The project must remain understandable without the conversation that produced it.
