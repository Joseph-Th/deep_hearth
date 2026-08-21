# Deep Hearth

Deep Hearth is a deterministic, systems-driven survival and industrialization simulation written in
Rust. The repository is centered on a headless simulation core. Rendering, input, networking,
platform integration, and save-file storage are adapter concerns, not gameplay-state owners.

## Cold start

1. Read [`../AGENTS.md`](../AGENTS.md) and [`AGENTS.md`](AGENTS.md).
2. Run `python ../tools/tasks.py list deep_hearth`.
3. Check [Status](STATUS.md) before assuming a capability exists.
4. Use the subsystem map below to find the state owner, implementation, and design authority.
5. Read the owner implementation and adjacent tests, then use [Testing](TESTING.md) for validation.

`GAME_DESIGN.md` describes the target game. `STATUS.md` describes the current runtime.

## Documentation authority

| Question | Authority |
| --- | --- |
| How should an agent work in this repository? | [`AGENTS.md`](AGENTS.md) |
| What engineering architecture and code conventions are mandatory? | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| What gameplay and progression should the game provide? | [`GAME_DESIGN.md`](GAME_DESIGN.md) |
| What technical contracts govern implemented systems? | [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) |
| What capabilities exist or are absent? | [`STATUS.md`](STATUS.md) |
| How are tests, harnesses, and local CI organized? | [`TESTING.md`](TESTING.md) |

## Repository map

| Area | Primary source | Design authority |
| --- | --- | --- |
| Core state, identity, time, deterministic RNG | `src/core/`, `src/simulation/` | `ARCHITECTURE.md`, `TECHNICAL_DESIGN.md` |
| Immutable authored definitions and registries | `src/content/`, `src/registry/`, `src/capability/` | `TECHNICAL_DESIGN.md` |
| Matter, materials, inventory | `src/matter/`, `src/material/`, `src/inventory/` | `TECHNICAL_DESIGN.md` |
| Geology, prospecting knowledge, mining | `src/geology/`, `src/mining/` | `TECHNICAL_DESIGN.md`, `GAME_DESIGN.md` |
| Production, crafting, ore processing, thermal work | `src/production/`, `src/crafting/`, `src/ore_processing/`, `src/thermal/` | `TECHNICAL_DESIGN.md` |
| Equipment, maintenance, labor, survival | `src/equipment/`, `src/maintenance/`, `src/labor/`, `src/survival/` | `TECHNICAL_DESIGN.md`, `GAME_DESIGN.md` |
| Energy, electrical, mechanical, fluid | `src/energy/`, `src/electrical/`, `src/mechanical/`, `src/fluid/` | `TECHNICAL_DESIGN.md` |
| Structures and spatial primitives | `src/structural/`, `src/spatial/` | `TECHNICAL_DESIGN.md` |
| Persistence admission and continuation | `src/persistence/` plus each state owner | `ARCHITECTURE.md`, `TECHNICAL_DESIGN.md` |
| Renderer-neutral textures and shaders | `src/texture/`, `src/shader/`, `assets/shaders/` | `TECHNICAL_DESIGN.md`, `assets/shaders/README.md` |
| Gameplay evaluation | `tests/gameplay_harness/` | `TESTING.md` |
| Local verification | `.cargo/config.toml`, `ci.py`, `tools/` | `TESTING.md` |

Start from the subsystem that owns the state being changed. Cross-owner work coordinates canonical
owner operations; it does not introduce convenience mutation paths between owners.

## Verification

Use [Testing](TESTING.md) to select the smallest lane that proves the changed contract. Use
`python ci.py quick` during ordinary editing and `python ci.py gate` at a coherent production checkpoint.
Documentation-only changes use `python tools/check_authority_docs.py`; Rust API documentation uses
`python ci.py gate --rustdoc`. Broad runtime audits require an explicit scope and are not an automatic
follow-up to ordinary compilation or focused tests. All verification is local.
