# Deep Hearth

Deep Hearth is a deterministic, systems-driven survival and industrialization simulation written in
Rust. The repository is centered on a headless simulation core. Rendering, input, networking,
platform integration, and save-file storage are adapter concerns, not gameplay-state owners.

## Cold start

For repository work, use this order:

1. Read [`../AGENTS.md`](../AGENTS.md) and [`AGENTS.md`](AGENTS.md).
2. Run `python ../tools/tasks.py list deep_hearth` to detect overlapping work.
3. Use [Status](STATUS.md) to confirm whether the relevant capability exists.
4. Use the subsystem map below to find the owning source and design document.
5. Read the owner implementation and adjacent tests before editing.
6. Use [Testing](TESTING.md) for the narrowest useful validation lane.
7. Update only the document that owns any contract changed by the code.

Do not infer capability from design intent. `GAME_DESIGN.md` describes the intended game;
`STATUS.md` describes what exists.

## Documentation authority

| Question | Authority |
| --- | --- |
| How should an agent work in this repository? | [`AGENTS.md`](AGENTS.md) |
| What engineering architecture and code conventions are mandatory? | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| What gameplay and progression should the game provide? | [`GAME_DESIGN.md`](GAME_DESIGN.md) |
| What technical contracts govern implemented systems? | [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) |
| What capabilities exist or are absent? | [`STATUS.md`](STATUS.md) |
| How are tests, harnesses, and local CI organized? | [`TESTING.md`](TESTING.md) |
| What coordinated work is available or already claimed? | [`TASKS.md`](TASKS.md) |

Documentation is present-tense and forward-facing. Git history owns implementation history. Do not
preserve migration stories, replaced designs, or completed-work narratives in authority documents.

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

Cross-owner mutations still have one canonical owner for each consequential fact. Start from the
subsystem that owns the state being changed; do not add convenience mutation paths between owners.

## Fast local workflow

Use compile-only feedback while an edit is mechanical, then run the narrowest executable test that
proves changed behavior:

```text
cargo check-fast
cargo check-tests          # ordinary test edits
cargo check-gameplay       # gameplay-harness edits
cargo test-fast <qualified-test-name> -- --exact
```

At a checkpoint:

```text
python ci.py gate
python ci.py gate --gameplay   # gameplay/content behavior
python ci.py gate --soak       # long-horizon state/conservation changes
python ci.py gate --shaders    # WGSL/shader contracts
python ci.py gate --docs       # documentation contracts
```

Use `python ci.py full` for the common core-plus-gameplay checkpoint and `python ci.py hardening` for
explicit broad local hardening. All verification is local; GitHub Actions and hosted CI are outside the
repository contract.

See [Testing](TESTING.md) for lane details and [Status](STATUS.md) for the current capability boundary.
