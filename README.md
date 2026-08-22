# Deep Hearth

Deep Hearth is a deterministic Rust simulation of survival, settlement, and industrialization. The
repository contains a headless gameplay core; rendering, input, networking, platform integration, and
save-file storage are adapter concerns.

## Cold start

1. Read [`../AGENTS.md`](../AGENTS.md) and [`AGENTS.md`](AGENTS.md).
2. Run `python ../tools/tasks.py list deep_hearth`.
3. Read [`STATUS.md`](STATUS.md) to confirm the requested capability exists.
4. Find the owning subsystem below and read its production source plus adjacent `*_tests.rs` files.
5. Read only the authority document that owns the contract being changed.
6. Use [`TESTING.md`](TESTING.md) to run the smallest proof for that change.

## Authority

| Need | Document |
| --- | --- |
| Repository workflow and non-negotiable agent rules | [`AGENTS.md`](AGENTS.md) |
| Engineering architecture, ownership, determinism, API conventions | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| Implemented technical contracts | [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) |
| Intended player experience and progression | [`GAME_DESIGN.md`](GAME_DESIGN.md) |
| Current implemented and absent capability | [`STATUS.md`](STATUS.md) |
| Tests, gameplay harnesses, and local verification | [`TESTING.md`](TESTING.md) |

`STATUS.md` answers what exists now. `GAME_DESIGN.md` describes the target game. Do not infer runtime
capability from design intent.

## Code map

| Domain | Source |
| --- | --- |
| Root state, time, identity, RNG, tick orchestration | `src/core/`, `src/simulation/` |
| Registries and authored definitions | `src/registry/`, `src/content/`, `src/capability/` |
| Matter and storage | `src/matter/`, `src/material/`, `src/inventory/` |
| Geology and extraction | `src/geology/`, `src/mining/` |
| Production and processing | `src/production/`, `src/crafting/`, `src/ore_processing/`, `src/thermal/` |
| Equipment, labor, maintenance, survival | `src/equipment/`, `src/labor/`, `src/maintenance/`, `src/survival/` |
| Energy, electrical, mechanical, fluids | `src/energy/`, `src/electrical/`, `src/mechanical/`, `src/fluid/` |
| Structures and spatial primitives | `src/structural/`, `src/spatial/` |
| Persistence admission | `src/persistence/` and each state owner |
| Renderer-neutral assets | `src/texture/`, `src/shader/`, `src/content/textures.rs`, `src/content/shaders.rs`, `assets/shaders/` |
| Gameplay evaluation | `tests/gameplay_harness/` |
| Local verification tooling | `ci.py`, `.cargo/config.toml`, `tools/`, `src/bin/validate_shaders.rs` |

Start with the owner of the authoritative state. Cross-owner behavior coordinates owner operations; it
does not create a second mutation path.
