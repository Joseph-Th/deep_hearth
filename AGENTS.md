# Deep Hearth Agent Guide

Use this file for execution procedure. Use [`README.md`](README.md) for routing and
[`STATUS.md`](STATUS.md) for current capability. Do not read every authority document by default.

## Start

1. Read [`../AGENTS.md`](../AGENTS.md) and preserve unrelated working-tree state.
2. Run `python ../tools/tasks.py list deep_hearth`; do not overlap active claimed work.
3. Confirm the requested capability and boundary in [`STATUS.md`](STATUS.md).
4. Find the state owner in [`README.md`](README.md), then read its production source and adjacent tests.
5. Read only the authority document that owns the changed contract.

## Project rules

- Registries own immutable definitions. `AppState` and subsystem states own generated runtime state.
- Every consequential operation has one canonical production path. Tests and tools use that path.
- Use validated tokens for fallible multi-owner mutation and decide/apply boundaries for read-heavy
  decisions with narrow writes.
- Preserve typed identity, synchronized indexes, deterministic RNG and ordering, checked physical
  arithmetic, explicit simulation order, and exact represented matter/fluid/energy ownership.
- Persist future-affecting state. Trusted load rebuilds derived indexes and validates the complete runtime
  graph before returning state.
- Core systems perform no implicit IO. Adapters own external effects.
- Remove obsolete paths and stale documentation. Do not add compatibility scaffolding, public test shims,
  fake callers, or broad warning suppressions without an active contract.
- Verification is local. Do not add or depend on GitHub Actions or hosted CI.

## Verification

Use [`TESTING.md`](TESTING.md) to select the smallest proof for the changed contract. `python ci.py quick`
is the build-free edit loop. Avoid redundant compile-only checks when the selected executable lane already
compiles the same surface.

For documentation-only changes, run `python tools/check_authority_docs.py`.

## Finish

Review the task-scoped diff, update the authority document that owns any changed contract, and run the
smallest completion gate that covers the changed surface.
