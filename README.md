# Deep Hearth

Deep Hearth is a deterministic Rust simulation core for a first-person survival, settlement, and
industrialization game. This repository owns headless gameplay state and simulation. Rendering, input,
networking, platform integration, and save-file storage belong to adapters.

## Orientation

The current ordinary-play slice is:

`local clues -> prospect -> build stone tools -> mine -> choose a scarce-copper investment -> build primitive machinery -> process ore -> gain the second upgrade`

Industrial workshop, ore-preparation, and foundry scenarios exercise already-installed systems only.
They do not imply ordinary-play acquisition. [`STATUS.md`](STATUS.md) is authoritative for the current
capability boundary; [`GAME_DESIGN.md`](GAME_DESIGN.md) is the forward design target.

## Start here

For a new task:

1. Read [`../AGENTS.md`](../AGENTS.md) and [`AGENTS.md`](AGENTS.md), then run
   `python ../tools/tasks.py list deep_hearth`.
2. Read [`STATUS.md`](STATUS.md) for the capability being changed.
3. Use the code map below to find the state owner and read its production source plus adjacent tests.
4. Read the single authority document that owns the changed contract.
5. Use [`TESTING.md`](TESTING.md) to select the smallest complete proof.

## Authority map

| Question | Authority |
| --- | --- |
| How should an agent execute work here? | [`AGENTS.md`](AGENTS.md) |
| What engineering rules apply to ownership, mutation, determinism, and persistence? | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| What project-specific runtime contracts are implemented? | [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) |
| What player experience and long-term progression are intended? | [`GAME_DESIGN.md`](GAME_DESIGN.md) |
| What exists now, what is capability-only, and what is absent? | [`STATUS.md`](STATUS.md) |
| Which local tests and gameplay harnesses prove a contract? | [`TESTING.md`](TESTING.md) |

## Code map

| Domain | Source |
| --- | --- |
| Root state, time, identity, RNG, tick orchestration | `src/core/`, `src/simulation/` |
| Registries, authored definitions, typed capabilities | `src/registry/`, `src/content/`, `src/capability/` |
| Matter, materials, inventory | `src/matter/`, `src/material/`, `src/inventory/` |
| Geology, acquired knowledge, mining | `src/geology/`, `src/mining/` |
| Production, crafting, ore processing, thermal work | `src/production/`, `src/crafting/`, `src/ore_processing/`, `src/thermal/` |
| Equipment, labor, maintenance, survival | `src/equipment/`, `src/labor/`, `src/maintenance/`, `src/survival/` |
| Energy, electrical, mechanical, fluids | `src/energy/`, `src/electrical/`, `src/mechanical/`, `src/fluid/` |
| Structures and spatial primitives | `src/structural/`, `src/spatial/` |
| Persistence admission | `src/persistence/` plus each state owner |
| Renderer-neutral textures and shaders | `src/texture/`, `src/shader/`, `src/content/textures.rs`, `src/content/shaders.rs`, `assets/shaders/` |
| Gameplay evaluation | `tests/gameplay_harness/` |
| Local verification tooling | `ci.py`, `.cargo/config.toml`, `tools/`, `src/bin/validate_shaders.rs` |

Start with the owner of the authoritative state. Cross-owner behavior coordinates owner APIs; it does not
create a second source of truth or mutation path.
