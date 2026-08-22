# Deep Hearth

Deep Hearth is a deterministic Rust simulation of survival, settlement, and industrialization. The
repository contains the headless gameplay core. Rendering, input, networking, platform integration, and
save-file storage belong to adapters.

## Start here

For a new task:

1. Read [`../AGENTS.md`](../AGENTS.md) and [`AGENTS.md`](AGENTS.md).
2. Run `python ../tools/tasks.py list deep_hearth` and avoid overlapping active work.
3. Read [`STATUS.md`](STATUS.md) to determine the current runtime boundary.
4. Use the code map below to find the owning subsystem. Read its production source and adjacent tests.
5. Read the authority document for the contract being changed.
6. Use [`TESTING.md`](TESTING.md) to select the smallest complete verification lane.

Do not infer implemented capability from design intent. `STATUS.md` is the capability inventory;
`GAME_DESIGN.md` is the forward design target.

## Documentation map

| Question | Authority |
| --- | --- |
| How should an agent work in this repository? | [`AGENTS.md`](AGENTS.md) |
| How is state owned, mutated, persisted, and kept deterministic? | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| What project-specific technical contracts are implemented? | [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) |
| What player experience and progression should the game create? | [`GAME_DESIGN.md`](GAME_DESIGN.md) |
| What capability exists or is absent? | [`STATUS.md`](STATUS.md) |
| Which tests and gameplay harnesses prove a change? | [`TESTING.md`](TESTING.md) |

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
create a second mutation path.
