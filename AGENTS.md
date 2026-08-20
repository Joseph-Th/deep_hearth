# Deep Hearth Agent Guide

This is the repository execution card. The detailed authorities are:

- [`ARCHITECTURE.md`](ARCHITECTURE.md): engineering architecture, ownership, APIs, naming, and code structure;
- [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md): project-specific physical and subsystem contracts;
- [`STATUS.md`](STATUS.md): implemented and unavailable capability;
- [`GAME_DESIGN.md`](GAME_DESIGN.md): gameplay intent and progression;
- [`TESTING.md`](TESTING.md): tests, gameplay harnesses, and local CI.

## Start

1. Read [`../AGENTS.md`](../AGENTS.md) and preserve unrelated working-tree state.
2. Run `python ../tools/tasks.py list deep_hearth` and avoid overlapping claimed work.
3. Use [`README.md`](README.md) for repository routing and [`STATUS.md`](STATUS.md) before assuming a
   system exists.
4. Read the owning source module and adjacent tests before editing.
5. Read only the authority document that owns the contract you are changing.
6. Select the narrowest lane from [`TESTING.md`](TESTING.md). Do not run a proving build before editing unless reproducing a failure or establishing a baseline the task actually needs; once behavior is ready, prefer the focused executable proof directly over a compile-only pass that would build the same surface.

If implementation, tests, and documentation disagree, reconcile the authoritative owner. Do not choose
a convenient description or preserve stale prose.

## Mandatory project rules

- Immutable definitions live in registries; generated mutable state lives in `AppState` and subsystem
  state owners; systems own validation and consequential mutation.
- Every consequential operation has one canonical production path. Tests, adapters, importers, and
  tooling do not receive alternate mutation paths.
- Fallible multi-owner work validates before mutation and uses consumed authorization tokens when
  atomicity or staleness require them. Read-heavy decisions use explicit decide/apply boundaries.
- Preserve typed identity, synchronized indexes, deterministic state-owned RNG, stable ordering,
  checked physical quantities, and explicit top-level simulation order.
- Matter, fluid, and modeled energy do not appear, disappear, or move without an implemented physical
  owner and path.
- Future-affecting state is serializable; load admission validates references and complete invariants.
- Core systems perform no implicit IO. External effects stay behind explicit adapter boundaries.
- Delete replaced production paths and stale documentation. Do not add compatibility scaffolding,
  public test shims, fake callers, or broad warning suppressions without an active contract.
- Repository verification is local. Do not create or depend on GitHub Actions or hosted CI.

## Finish

Use focused tests while editing when they shorten feedback or isolate a failure. For completion, choose exactly the smallest gate from [`TESTING.md`](TESTING.md) that owns the changed contract, plus only specialized lanes whose distinct surface changed. A focused proof is not a mandatory predecessor to a completion gate that will compile and exercise the same behavior; do not buy the same confidence twice merely because both commands are documented. Review the task-scoped diff and update the single authority document that owns any changed contract. Documentation describes the current system and forward requirements; Git history owns the story of how it got there.
