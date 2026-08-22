# Deep Hearth Agent Guide

Use this file as the execution card. [`README.md`](README.md) maps the repository;
[`STATUS.md`](STATUS.md) defines the current capability boundary; the remaining authority documents own
architecture, technical contracts, game intent, and verification.

## Start

1. Read [`../AGENTS.md`](../AGENTS.md) and preserve unrelated working-tree state.
2. Run `python ../tools/tasks.py list deep_hearth`; do not overlap active claimed work.
3. Confirm the requested capability in [`STATUS.md`](STATUS.md).
4. Find the owning subsystem in [`README.md`](README.md), then read its production source and adjacent
   tests.
5. Read only the authority document that owns the contract being changed.

## Rules

- Registries own immutable definitions. `AppState` and subsystem states own generated runtime state.
- Each consequential operation has one canonical production path. Tests and tools do not gain alternate
  mutation paths.
- Validate fallible multi-owner work before mutation. Use consumed validated tokens for atomicity and
  stale-state rejection; use decide/apply boundaries for read-heavy decisions with narrow writes.
- Preserve typed identity, synchronized indexes, deterministic RNG and ordering, checked physical
  arithmetic, explicit simulation order, and exact represented matter/fluid/energy ownership.
- Persist future-affecting state. Load admission rebuilds derived indexes and validates complete runtime
  invariants before returning trusted state.
- Core systems perform no implicit IO. Adapters own external effects.
- Remove obsolete paths and stale documentation. Do not add compatibility scaffolding, public test shims,
  fake callers, or broad warning suppressions without an active contract.
- Verification is local. Do not add or depend on GitHub Actions or hosted CI.

## Verification

Use [`TESTING.md`](TESTING.md) to select the smallest proof for the changed contract. `python ci.py quick`
is the build-free edit loop. Batch related edits before any build-producing checkpoint and do not run a
compile-only lane when the selected executable lane already compiles the same surface.

For documentation-only changes, run `python tools/check_authority_docs.py`.

## Finish

Review the task-scoped diff, update the authority document that owns any changed contract, and run the
smallest completion gate that covers the changed surface. Do not add broader verification for reassurance.
