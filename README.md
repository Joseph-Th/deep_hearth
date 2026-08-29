# Deep Hearth

Deep Hearth is a deterministic Rust simulation core for a first-person survival, settlement, and
industrialization game. This repository owns headless gameplay state, authored definitions, and simulation
rules. It does not contain a playable client or general engine shell; rendering, input, networking, platform
integration, and save-file storage belong to adapters.

[`STATUS.md`](STATUS.md) is the sole authority for ordinary reachability, capability-only evaluation, and
absent runtime scope. Do not infer reachability from source presence or controlled gameplay-harness setup.

## Orientation

1. Read workspace [`../AGENTS.md`](../AGENTS.md), then project [`AGENTS.md`](AGENTS.md).
2. Read [`STATUS.md`](STATUS.md) for current reachable, capability-only, and absent scope.
3. Use the task map below to find the owner, canonical boundary, contract, and focused proof.
4. Read that owner and its adjacent tests; open deeper authority pages only as needed.
5. Use [`TESTING.md`](TESTING.md) for verification and gameplay-evaluation commands.

## Authorities

| Question | Authority |
| --- | --- |
| Project execution rules | [`AGENTS.md`](AGENTS.md) |
| Ownership, mutation, determinism, persistence, API shape | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| Implemented subsystem and physical contracts | [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) |
| Product direction and intended player experience | [`GAME_DESIGN.md`](GAME_DESIGN.md) |
| Current reachable, capability-only, and absent scope | [`STATUS.md`](STATUS.md) |
| Tests, gameplay harnesses, and local verification | [`TESTING.md`](TESTING.md) |

## Task map

| Concern | Owner and canonical boundary | Contract | Focused proof |
| --- | --- | --- | --- |
| Time, RNG, root state, tick order | `src/core/`, `src/simulation/`; `AppState`, `advance_tick` | [Global runtime facts](TECHNICAL_DESIGN.md#global-runtime-facts), [Runtime owners](TECHNICAL_DESIGN.md#runtime-owners) | adjacent unit test |
| Save admission and graph validation | `src/persistence/`, `src/core/state/`; `LoadedSaveEnvelope::into_state`, `validate_loaded_state` | [Trusted load](TECHNICAL_DESIGN.md#trusted-load) | persistence/owner unit test |
| Materials, inventory, matter | `src/material/`, `src/inventory/`, `src/matter/`; owner ingress/egress/reform validators | [Materials, inventory, and geology](TECHNICAL_DESIGN.md#materials-inventory-and-geology) | adjacent unit test |
| Geology, knowledge, mining | `src/geology/`, `src/mining/`; prospecting, target resolution, mining validation | [Materials, inventory, and geology](TECHNICAL_DESIGN.md#materials-inventory-and-geology) | adjacent test; progression lane for player behavior |
| Production and processing | `src/production/`, `src/crafting/`, `src/ore_processing/`, `src/thermal/`; resolver -> validate -> commit | [Production and processing](TECHNICAL_DESIGN.md#production-and-processing) | adjacent test or focused gameplay lane |
| Equipment, labor, maintenance, survival | `src/equipment/`, `src/labor/`, `src/maintenance/`, `src/survival/`; provider resolution and validators | [Equipment, labor, survival, energy, and fluids](TECHNICAL_DESIGN.md#equipment-labor-survival-energy-and-fluids) | adjacent test or survival/workshop lane |
| Energy and fluids | `src/energy/`, `src/fluid/`; finite stores, exact integration/withdrawal, and owner validators | [Equipment, labor, survival, energy, and fluids](TECHNICAL_DESIGN.md#equipment-labor-survival-energy-and-fluids) | adjacent unit test or focused gameplay lane |
| Structures and spatial support | `src/structural/`, `src/spatial/`; structural validators and analysis | [Structures](TECHNICAL_DESIGN.md#structures) | adjacent test or workshop lane |
| Textures and shaders | `src/texture/`, `src/shader/`, `src/content/textures.rs`, `src/content/shaders.rs`, `assets/shaders/` | [Spatial and presentation boundaries](TECHNICAL_DESIGN.md#spatial-and-presentation-boundaries), [`assets/shaders/README.md`](assets/shaders/README.md) | shader lane |
| Gameplay evaluation | `tests/gameplay_harness/`; production APIs after controlled setup | [`TESTING.md`](TESTING.md) | matching gameplay lane |
| Verification tooling | `ci.py`, `.cargo/config.toml`, `tools/` | [`TESTING.md`](TESTING.md) | `python ci.py quick` |

## Change-impact map

Use this map when a change crosses an authority boundary.

| Change | Required companion work |
| --- | --- |
| Persisted runtime state | Keep the fact in its runtime owner; update strict serialization, deterministic index rebuilds, trusted-load validation, schema ownership when semantics change, and persistence coverage. |
| Authored identity or physical definition | Validate references during registry construction; update registry schema ownership when persisted identity/replay semantics change; update affected resolvers and registry-derived tests. |
| Canonical command or cross-owner mutation | Preserve one production path, typed rejection, stale-state protection, and atomicity; reuse it from tests and harnesses. |
| Tick or scheduled behavior | Persist future-affecting schedule state, keep `advance_tick` ordering explicit, validate continuation on load, and test deterministic completion or resumption. |
| Gameplay capability or reachability | Add executable production-path coverage and update [`STATUS.md`](STATUS.md). Update [`GAME_DESIGN.md`](GAME_DESIGN.md) only when intended experience changes. |
| Gameplay-harness policy or evidence | Preserve actor/diagnostic separation, reproducible seeds, bounded search, replay inputs, evidence labels, and production-derived legality; update [`TESTING.md`](TESTING.md). |
| Texture, shader, or adapter contract | Keep authored definitions renderer-neutral; update [`assets/shaders/README.md`](assets/shaders/README.md) when its binding contract changes; run the relevant lane. |
| Verification or developer tooling | Keep selectors fail-closed and commands local/reproducible; update [`TESTING.md`](TESTING.md). |

Update only the authority page that owns the changed contract. Do not copy the same mutable fact into unrelated
documents.
