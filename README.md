# Deep Hearth

Deep Hearth is a deterministic Rust simulation core for a first-person survival, settlement, and
industrialization game. This repository owns headless gameplay state and simulation. Rendering, input,
networking, platform integration, and save-file storage belong to adapters.

## Current scope

Ordinary play currently supports:

`local clues -> prospect -> stone tools -> evidence-gated hand mining -> scarce-copper choice -> primitive machinery -> process ore -> second upgrade`

Installed industrial workshop, ore-preparation, and foundry systems are exercised by gameplay harnesses but
are not ordinarily acquirable. [`STATUS.md`](STATUS.md) is the authority for current reachability.
[`GAME_DESIGN.md`](GAME_DESIGN.md) is the forward product target.

## Cold-start route

1. Read [`../AGENTS.md`](../AGENTS.md) and [`AGENTS.md`](AGENTS.md).
2. Check the requested capability in [`STATUS.md`](STATUS.md).
3. Use the task map below to identify the owning source, canonical boundary, and contract document.
4. Read that source and its adjacent tests.
5. Use [`TESTING.md`](TESTING.md) to choose the smallest complete proof.

## Authority map

| Question | Authority |
| --- | --- |
| Project execution procedure | [`AGENTS.md`](AGENTS.md) |
| Ownership, mutation, determinism, persistence, API rules | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| Implemented subsystem contracts and physical semantics | [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) |
| Intended player experience and progression | [`GAME_DESIGN.md`](GAME_DESIGN.md) |
| Current ordinary-play, capability-only, and absent scope | [`STATUS.md`](STATUS.md) |
| Test organization, gameplay harnesses, verification commands | [`TESTING.md`](TESTING.md) |

## Task map

| Change | Owning source | Canonical boundary | Contract | Focused proof |
| --- | --- | --- | --- | --- |
| Time, RNG, root state, tick order | `src/core/`, `src/simulation/` | `AppState`, `advance_tick` | [Global runtime facts](TECHNICAL_DESIGN.md#global-runtime-facts), [Runtime owners](TECHNICAL_DESIGN.md#runtime-owners) | adjacent unit test |
| Save admission and runtime graph validation | `src/persistence/`, `src/core/state/` | `LoadedSaveEnvelope::into_state`, `validate_loaded_state` | [Trusted load](TECHNICAL_DESIGN.md#trusted-load) | persistence or owner unit test |
| Materials, composition, inventory, matter accounting | `src/material/`, `src/inventory/`, `src/matter/` | owner-specific ingress/egress/reform validators | [Materials, inventory, and geology](TECHNICAL_DESIGN.md#materials-inventory-and-geology) | adjacent unit test |
| Geology, knowledge, prospecting, mining | `src/geology/`, `src/mining/` | prospecting validation, mining-target resolution, mining validation | [Materials, inventory, and geology](TECHNICAL_DESIGN.md#materials-inventory-and-geology) | `python ci.py gate --gameplay progression` when player behavior changes |
| Production, crafting, ore processing, thermal work | `src/production/`, `src/crafting/`, `src/ore_processing/`, `src/thermal/` | resolver -> validated production start -> commit | [Production and processing](TECHNICAL_DESIGN.md#production-and-processing) | adjacent unit test or focused gameplay lane |
| Equipment, labor, maintenance, survival | `src/equipment/`, `src/labor/`, `src/maintenance/`, `src/survival/` | provider resolution and subsystem validators | [Equipment, labor, survival, energy, and fluids](TECHNICAL_DESIGN.md#equipment-labor-survival-energy-and-fluids) | adjacent unit test or survival/workshop lane |
| Energy, electrical, mechanical, fluids | `src/energy/`, `src/electrical/`, `src/mechanical/`, `src/fluid/` | typed calculations and owner-specific validated transfer/use | [Equipment, labor, survival, energy, and fluids](TECHNICAL_DESIGN.md#equipment-labor-survival-energy-and-fluids) | adjacent unit test |
| Structural support and loads | `src/structural/`, `src/spatial/` | structural validators and analysis | [Structures](TECHNICAL_DESIGN.md#structures) | adjacent unit test or workshop lane |
| Textures and shaders | `src/texture/`, `src/shader/`, `src/content/textures.rs`, `src/content/shaders.rs`, `assets/shaders/` | registry bake/assembly APIs | [Spatial and presentation boundaries](TECHNICAL_DESIGN.md#spatial-and-presentation-boundaries); [`assets/shaders/README.md`](assets/shaders/README.md) | `python ci.py gate --shaders` |
| Gameplay evaluation | `tests/gameplay_harness/` | production APIs only after controlled setup | [`TESTING.md`](TESTING.md) | matching gameplay lane |
| Verification tooling | `ci.py`, `.cargo/config.toml`, `tools/` | documented local lanes | [`TESTING.md`](TESTING.md) | `python ci.py quick` |

## Change-impact map

| Change class | Required companion work |
| --- | --- |
| Persisted or future-affecting state | owner invariants, serialization, trusted-load validation, round-trip coverage |
| New or changed cross-owner mutation | one canonical validation/commit path, stale-state handling, unchanged-state rejection coverage |
| New authored definition or identity | registry validation, reference validation, schema/version update when persisted identity or payload shape changes |
| New implemented capability | production path, persistence semantics where needed, executable coverage, `STATUS.md`, relevant Technical Design section |
| Public or adapter-facing contract | explicit ownership/failure semantics, focused tests, owning documentation |
| Gameplay balance or agency behavior | canonical actor boundary, focused gameplay harness evidence, report output only for observational balance data |

Start with the authoritative state owner. Cross-owner code coordinates owner APIs; it does not create a
second source of truth or mutation path.
