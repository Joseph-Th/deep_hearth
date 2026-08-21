# Deep Hearth Agent Guide

Use this file as the repository execution card. [`README.md`](README.md) routes the codebase;
[`STATUS.md`](STATUS.md) defines the current capability boundary; [`ARCHITECTURE.md`](ARCHITECTURE.md),
[`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md), [`GAME_DESIGN.md`](GAME_DESIGN.md), and
[`TESTING.md`](TESTING.md) own their respective contracts.

## Start

1. Read [`../AGENTS.md`](../AGENTS.md) and preserve unrelated working-tree state.
2. Run `python ../tools/tasks.py list deep_hearth`; do not overlap active claimed work.
3. Use [`README.md`](README.md) to find the owning subsystem and [`STATUS.md`](STATUS.md) to confirm the
   capability exists.
4. Read the owner implementation and adjacent tests before editing.
5. Read only the authority documents relevant to the changed contract.
6. Use the narrowest validation lane in [`TESTING.md`](TESTING.md) that proves the change.

During implementation, use the build-free `quick` lane for frequent feedback. Batch related edits and
run a build-producing `standard`, focused executable, or `full` lane only at a coherent checkpoint; do
not compile the project after every file mutation.

When code, tests, and documentation disagree, reconcile them to the actual authoritative contract.

## Project rules

- Registries own immutable definitions. `AppState` and subsystem state types own generated runtime state.
- Each consequential operation has one canonical production path. Tests and tooling do not gain alternate
  mutation paths.
- Fallible multi-owner work validates before mutation. Use consumed validated tokens for atomicity and
  staleness; use decide/apply boundaries for read-heavy decisions with narrow writes.
- Preserve typed identity, synchronized indexes, state-owned deterministic RNG, stable ordering, checked
  physical arithmetic, and explicit top-level simulation order.
- Matter, fluid, and modeled energy move or transform only through an implemented physical owner and path.
- Future-affecting state is serializable. Load admission validates references and complete invariants.
- Core systems perform no implicit IO; adapters own external effects.
- Remove obsolete production paths and stale documentation. Do not add compatibility scaffolding, public
  test shims, fake callers, or broad warning suppressions without an active contract.
- Verification is local. Do not create or depend on GitHub Actions or hosted CI.

## Finish

Review the task-scoped diff, update the authority document that owns any changed contract, and run the
smallest completion gate from [`TESTING.md`](TESTING.md) that covers the changed surface.
