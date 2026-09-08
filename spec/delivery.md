# Cannon repository delivery boundary

The user created `eunhhu/cannon` and authorized importing the prototype. The repository was empty and public; visibility was not changed.

## Published scope

This deliverable is the runnable CLI and semantic engine, with 90 core/CLI tests, inventory examples, design contracts, and a CI workflow. It is not the complete editor distribution.

## Blocked scope

GitHub connector writes containing `vscode/extension.cjs` were blocked with an indeterminate security decision, including an individual-file request. The editor sources were not sent through an alternate path, encoding, or credential. No claim of a complete editor upload is made.

To avoid broken package entrypoints and intentionally failing missing-file tests, this source package excludes the editor entrypoint, editor assets/configuration, and its five tests. No core or CLI acceptance test was weakened. The full local Cannon package is preserved separately and had 95 passing local tests; those are not the test count of this repository.

## Verification distinction

Local registry DNS was unavailable. The local compiler is TypeScript 5.8.3 with preinstalled @types/node 22.19.7 and undici-types 6.21.0. The lockfile specifies @types/node 22.15.33. A local passing result does not claim a clean locked install. The independent GitHub Actions run checks the lockfile on Node 22 and 24.

Do not infer a successful remote CI run, running Extension Host, configured AI provider, persistent database, or deployment from source presence.
