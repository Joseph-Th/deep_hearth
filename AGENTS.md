# Deep Hearth Agent Guide

This file is the execution card for repository work. [ARCHITECTURE.md](ARCHITECTURE.md) owns engineering architecture and coding law, [TESTING.md](TESTING.md) owns tests and validation lanes, [STATUS.md](STATUS.md) owns implemented scope, [TECHNICAL_DESIGN.md](TECHNICAL_DESIGN.md) owns project-specific technical design, and [GAME_DESIGN.md](GAME_DESIGN.md) owns product intent.

## Start here

1. Read [`../AGENTS.md`](../AGENTS.md) and preserve unrelated working-tree state.
2. Read [STATUS.md](STATUS.md) before assuming a capability exists.
3. Identify the owning subsystem and canonical operation from the relevant source/module docs and [ARCHITECTURE.md](ARCHITECTURE.md).
4. Read the owner implementation and adjacent tests before editing.
5. Read [TECHNICAL_DESIGN.md](TECHNICAL_DESIGN.md) only when the change crosses its project-specific physical/technical design contracts.
6. Use [TESTING.md](TESTING.md) for the narrowest exact test and the required specialized/completion lanes.
7. Update the one document that owns any changed contract.

This project applies the Universal, Stateful Application, Deterministic System, and Automated Behavior Evaluation portfolio profiles. If current authorities, tests, and implementation conflict, reconcile the owner instead of choosing a convenient description.

## Project guardrails

- Registries own immutable definitions; `AppState` and subsystem state owners own generated mutable state; records own local runtime values; systems own validation, decisions, and consequential mutation.
- Every consequential operation uses one canonical production path. Tests, importers, migrations, adapters, and administrative tools do not gain mutation shortcuts.
- Multi-resource work validates before mutation and commits through a consumed validated token when staleness/atomicity require it. Read-heavy decisions use `decide_*` then `apply_*` in one pipeline.
- Preserve typed identity, synchronized owner indexes, deterministic state-owned RNG, stable ordering/tie-breaking, checked physical quantities, and explicit top-level execution order.
- Future-affecting generated state is serializable. Load/import validates references and complete invariants before trusted use.
- Core systems perform no implicit IO. Recoverable external work crosses explicit adapter/durable-command boundaries.
- Project-owned enums and closed record mappings are explicit; consequential fields remain private; new fallible operations use dedicated typed errors.
- Delete replaced production paths. Do not add fake production callers, public test shims, broad warning suppressions, or compatibility scaffolding without an active contract.
- Repository verification is local. Do not create or depend on GitHub Actions workflows or hosted runners.

## Naming and module route

Follow the naming/module conventions in [ARCHITECTURE.md](ARCHITECTURE.md). Every production source file has a concise `//!` ownership/purpose statement; multi-file subsystem names make execution, integration, loader, UI, and adapter roles discoverable.

## Completion

Run the narrowest qualified test while iterating, then the applicable lanes from [TESTING.md](TESTING.md). Before commit, confirm canonical ownership/mutation, deterministic ordering/RNG, persistence and invariant integration, explicit external effects, current documentation, removal of replaced paths, and a task-scoped diff.
