# Deep Hearth Agent Guide

This file owns project execution rules. Workspace [`../AGENTS.md`](../AGENTS.md) owns cross-project
coordination. [`README.md`](README.md) is the project routing hub.

## Start

1. Preserve unrelated working-tree changes.
2. Follow the start route in [`README.md`](README.md).
3. Read the owning source and adjacent tests before editing.
4. Read only the authority document that owns the contract being changed.

## Rules

- Registries own immutable definitions; `AppState` and subsystem states own generated runtime state.
- Consequential behavior has one canonical production path. Tests, harnesses, and tools reuse it.
- Use validated tokens for fallible multi-owner mutation and decide/apply for read-heavy decisions with narrow writes.
- Preserve typed identity, synchronized indexes, deterministic RNG/order, checked physical arithmetic, explicit tick order, and exact represented matter/fluid/energy ownership.
- Persist every fact that affects supported continuation. Trusted load rebuilds derived indexes and validates the complete runtime graph before use.
- Core systems perform no implicit IO; adapters own external effects.
- Remove obsolete code and stale documentation. Do not add compatibility scaffolding, test-only public APIs, fake callers, or broad warning suppressions without an active contract.
- Verification is local. Do not add or depend on hosted CI.

## Verify and finish

Use [`TESTING.md`](TESTING.md) for the smallest complete proof. The BCA cognitive-complexity policy is a
ratchet owned by `bca.toml` and `TESTING.md`; other BCA metrics are advisory.

For documentation-only changes, run `python tools/check_authority_docs.py`. Before handoff, review the
task-scoped diff and update every authority page whose contract changed.
