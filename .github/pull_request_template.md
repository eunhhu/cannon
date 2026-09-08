## Intent and scope

What human decision or requested behavior does this change implement? Mark proposals as proposals, not accepted decisions. State what is deliberately unchanged.

## Meaning changes

Describe architecture/ownership, data model or migration, business rules, and effects separately from mechanical implementation changes. Write `none` where appropriate. Do not hide new policy inside an implementation patch.

## Expectations and historical behavior

Were scenario inputs, expected values, acceptance criteria, or tests changed/deleted? Explain why. Report baseline replay independently of candidate tests. Passing candidate tests does not imply preserved historical behavior.

## Verification actually performed

Give exact commands, outcome, commit/source scope, tool versions, and dependency conditions. Distinguish static checks, executed tests, formal proofs, remote CI, deployment observations, and AI hypotheses. A documentation-only PR may say tests were not run; do not reuse an earlier test count as a new result.

## Unknowns and trust boundaries

List unverified behavior, host adapters, migration risks, editor/OS coverage, and deployment state. Clearly identify evidence that is stale or missing.

## Human review and follow-up

What decision is requested from the reviewer? Identify any architectural proposal that needs separate approval. State rollback or follow-up work where relevant. Do not auto-merge an architectural proposal merely because checks pass.
