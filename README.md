# Deep Hearth

Deep Hearth is a deterministic Rust simulation core for a first-person survival, settlement, and
industrialization game. This repository owns headless gameplay state and simulation. Rendering, input,
networking, platform integration, and save-file storage are adapter concerns.

Current ordinary play reaches:

`local clues -> prospect -> stone tools -> evidence-gated hand mining -> scarce-copper choice -> primitive machinery -> ore processing -> second reinforcement`

Industrial workshop, ore-preparation, and foundry behavior is executable through controlled harness setup but
is not ordinarily acquirable. [`STATUS.md`](STATUS.md) is the authority for reachability.

## Start here

1. Read workspace [`../AGENTS.md`](../AGENTS.md), then project [`AGENTS.md`](AGENTS.md).
2. Check [`STATUS.md`](STATUS.md) before assuming a capability exists or is reachable.
3. Use the task map below to find the owner, canonical boundary, and contract.
4. Read the owning production source and its adjacent tests.
5. Read only the authority page that owns the contract you are changing.
6. Use [`TESTING.md`](TESTING.md) for the smallest complete verification lane.

## Authorities

| Question | Authority |
| --- | --- |
| Project execution rules | [`AGENTS.md`](AGENTS.md) |
| Ownership, mutation, determinism, persistence, API shape | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| Implemented subsystem and physical contracts | [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) |
| Product direction and intended player experience | [`GAME_DESIGN.md`](GAME_DESIGN.md) |
| Current reachable, capability-only, and absent scope | [`STATUS.md`](STATUS.md) |
| Tests, gameplay harnesses, and local verification | [`TESTING.md`](TESTING.md) |

## Repository map

| Area | Purpose |
| --- | --- |
| `src/content/`, `src/registry/` | authored definitions, registry construction, validation |
| `src/core/`, `src/simulation/`, `src/persistence/` | root state, time/RNG, tick orchestration, trusted load |
| `src/*` domain modules | authoritative subsystem state and canonical operations |
| `tests/gameplay_harness/` | player-facing behavior evaluation through production APIs |
| `tests/` | focused and consolidated gameplay targets |
| `ci.py`, `tools/`, `.cargo/config.toml` | local verification and developer tooling |
| `assets/` | renderer-neutral authored assets and asset-specific contracts |

## Task map

| Concern | Owner and canonical boundary | Contract | Focused proof |
| --- | --- | --- | --- |
| Time, RNG, root state, tick order | `src/core/`, `src/simulation/`; `AppState`, `advance_tick` | [Global runtime facts](TECHNICAL_DESIGN.md#global-runtime-facts), [Runtime owners](TECHNICAL_DESIGN.md#runtime-owners) | adjacent unit test |
| Save admission and graph validation | `src/persistence/`, `src/core/state/`; `LoadedSaveEnvelope::into_state`, `validate_loaded_state` | [Trusted load](TECHNICAL_DESIGN.md#trusted-load) | persistence/owner unit test |
| Materials, inventory, matter | `src/material/`, `src/inventory/`, `src/matter/`; owner ingress/egress/reform validators | [Materials, inventory, and geology](TECHNICAL_DESIGN.md#materials-inventory-and-geology) | adjacent unit test |
| Geology, knowledge, mining | `src/geology/`, `src/mining/`; prospecting, target resolution, mining validation | [Materials, inventory, and geology](TECHNICAL_DESIGN.md#materials-inventory-and-geology) | adjacent test; progression lane for player behavior |
| Production and processing | `src/production/`, `src/crafting/`, `src/ore_processing/`, `src/thermal/`; resolver -> validate -> commit | [Production and processing](TECHNICAL_DESIGN.md#production-and-processing) | adjacent test or focused gameplay lane |
| Equipment, labor, maintenance, survival | `src/equipment/`, `src/labor/`, `src/maintenance/`, `src/survival/`; provider resolution and validators | [Equipment, labor, survival, energy, and fluids](TECHNICAL_DESIGN.md#equipment-labor-survival-energy-and-fluids) | adjacent test or survival/workshop lane |
| Energy, electrical, mechanical, fluids | `src/energy/`, `src/electrical/`, `src/mechanical/`, `src/fluid/`; typed calculations and owner validators | [Equipment, labor, survival, energy, and fluids](TECHNICAL_DESIGN.md#equipment-labor-survival-energy-and-fluids) | adjacent unit test |
| Structures and spatial support | `src/structural/`, `src/spatial/`; structural validators and analysis | [Structures](TECHNICAL_DESIGN.md#structures) | adjacent test or workshop lane |
| Textures and shaders | `src/texture/`, `src/shader/`, `src/content/textures.rs`, `src/content/shaders.rs`, `assets/shaders/` | [Spatial and presentation boundaries](TECHNICAL_DESIGN.md#spatial-and-presentation-boundaries), [`assets/shaders/README.md`](assets/shaders/README.md) | shader lane |
| Gameplay evaluation | `tests/gameplay_harness/`; production APIs after controlled setup | [`TESTING.md`](TESTING.md) | matching gameplay lane |
| Verification tooling | `ci.py`, `.cargo/config.toml`, `tools/` | [`TESTING.md`](TESTING.md) | `python ci.py quick` |

## Change requirements

- Start with the authoritative state owner; coordinators call owner APIs rather than creating another source of truth.
- Persist every fact that affects supported continuation; rebuild only deterministic derived data.
- Give fallible cross-owner mutation one canonical validation/commit path and preserve state on rejection.
- Validate new authored identities and update schema/version contracts when persisted identity or payload semantics change.
- Add executable coverage and update `STATUS.md` when a capability becomes implemented or reachable.
- Update the authority page that owns any changed public, physical, gameplay, persistence, or verification contract.
