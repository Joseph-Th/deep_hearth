# Deep Hearth Agent Guide

**BCA policy:** ratchet

`bca.toml` and [`TESTING.md`](TESTING.md) own the cognitive-complexity ratchet; other BCA metrics remain advisory.

This is the project execution card. Root [`../AGENTS.md`](../AGENTS.md) owns workspace coordination and
concurrency procedure. [`README.md`](README.md) owns project routing. [`STATUS.md`](STATUS.md) owns current
capability.

## Cold start

1. Read root [`../AGENTS.md`](../AGENTS.md) and preserve unrelated working-tree state.
2. Check current scope in [`STATUS.md`](STATUS.md).
3. Use the task map in [`README.md`](README.md) to identify the owning subsystem, canonical operation, and
   contract document.
4. Read that production source and its adjacent tests.
5. Read only the authority page that owns the contract being changed.

## Engineering rules

- Registries own immutable definitions. `AppState` and subsystem states own generated runtime state.
- Consequential behavior uses one canonical production path. Tests and tools reuse it.
- Fallible multi-owner mutation uses validated tokens. Read-heavy decisions with narrow writes use
  decide/apply boundaries.
- Preserve typed identity, synchronized indexes, deterministic RNG and ordering, checked physical
  arithmetic, explicit tick order, and exact represented matter/fluid/energy ownership.
- Persist future-affecting state. Trusted load rebuilds derived indexes and validates the complete runtime
  graph before state becomes usable.
- Core systems perform no implicit IO. Adapters own external effects.
- Remove obsolete code and stale documentation. Do not add compatibility scaffolding, test-only public
  APIs, fake callers, or broad warning suppressions without an active contract.
- Verification is local. Do not add or depend on hosted CI.

## Verification

Use [`TESTING.md`](TESTING.md) to select the smallest complete proof. For documentation-only changes, run
`python tools/check_authority_docs.py`.

## Completion

Review only the task-scoped diff, update the authority page that owns any changed contract, and run the
smallest completion gate that covers the changed surface.
