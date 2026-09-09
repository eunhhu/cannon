# Work on Cannon without hiding its meaning

Read `STATUS.md`, `spec/architecture.md`, and `spec/semantics.md` before changing the engine.

The human owns architecture, data meaning, policy, and acceptance criteria. An assistant may propose changes to any of those, but must label the proposal rather than smuggling it into an implementation patch. Do not rename or regenerate stable IDs just to make code easier to rewrite.

`src/core/` is the shared meaning engine. It must not depend on VS Code, an LLM SDK, a database client, or an agent conversation. `src/language/` translates text to this model; editor and assistant integrations consume the same public API.

Never make scenario expectations match new output merely to obtain passing tests. Record expectation changes separately and preserve baseline replay. A type check, executed test, formal proof, deployment observation, and AI hypothesis are different kinds of evidence. Do not claim one from another.

Run `npm test` and `node dist/src/cli.js demo` after changes. Test errors and stale-state paths, not only happy paths. Update the semantics and status documents when the actual contract changes. Source changes require a rebuild; do not hand-edit `dist/` or generated build fingerprints.

No install/build/test command may publish a repository, send code to an AI provider, call a production database, or change a deployment. Publishing must remain an explicit action. Do not commit `.env`, credentials, private runtime data, caches, or `.intent/` reports.

Do not portray the interpreter as a hostile-code sandbox or evidence hashes as signed attestations. Do not claim full cross-process compare-and-swap from an atomic filesystem rename. Mark unimplemented and unverified boundaries explicitly.

When handing off, state what changed, the evidence actually obtained, and the remaining unknowns. The project must remain understandable without the conversation that produced it.

## Production roadmap and agent handoff

Start with `ROADMAP.md`, `docs/strategy/README.md`, `docs/strategy/tasks.json`, and `docs/strategy/DELIVERY.md`. The last document records strategy supplements that were not delivered; their placeholder pages are not approved designs. Future plans do not supersede implemented semantics or prove completion.

The native shared engine is `crates/cannon-core`; its existing evaluator, typed inputs, sessions and project graph must not acquire editor, provider or production I/O dependencies. Do not create another evaluator in an adapter. Read the applicable `docs/native-*` contract before changing those modules.

Use `docs/strategy/agent-execution.md` and its task/handoff templates. Claim a task with an exact base SHA and narrowed write scope. Only start when dependencies have evidence and relevant owner decisions are resolved. Keep all genuinely unstarted tasks planned; missing tools mean blocked, not done. The initial frontier is CN-001, CN-005 and CN-006, subject to current repository state.

Use isolated worktrees and one integrator. Shared IR, syntax, schemas, manifests and workflows have a single writer at a time. Preserve independent original expectations and distinguish new behavior, acceptance changes and mechanical edits. Do not treat proposal rationale or declared authorship as authenticated approval.

For native changes, run debug/release Cargo tests, strict Clippy, both existing cross-engine suites, and affected examples in addition to the reference checks. Record exact commands, environment, commit and results. A mock integration, source inspection or older CI run is not a new successful execution.

When direct main updates are authorized and repository rules allow them, integrate only a verified descendant, re-read remote main before and after, and never force-push or discard concurrent changes. PR requirements are governed by the actual repository rules, not by this document. Git integration does not authorize production releases, new costs or data transfer.
