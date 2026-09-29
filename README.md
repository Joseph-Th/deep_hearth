# Deep Hearth

**Role:** Project entry point and task router.

Deep Hearth is a deterministic Rust simulation core for a first-person survival, settlement, and industrialization
game. It owns headless state, authored definitions, and simulation rules; platform and renderer IO belong to
adapters.

## Orientation

1. Read [`AGENTS.md`](AGENTS.md) once for execution rules.
2. Use the [task map](#task-map) to find the owner, canonical boundary, and contract.
3. Read that owner and its adjacent tests. Consult [`STATUS.md`](STATUS.md) when scope or reachability matters.
4. Verify with the smallest complete lane in [`TESTING.md`](TESTING.md).

Do not read every authority page up front. Follow links only when the task crosses that authority. For
consequential work, identify the owner, operation stage, crossed flow, and proof before widening the search.

## Authority map

| Question | Authority |
| --- | --- |
| What must agents follow? | [`AGENTS.md`](AGENTS.md) |
| What exists, is capability-only, or is absent? | [`STATUS.md`](STATUS.md) |
| What physical/state semantics do implemented systems obey? | [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) |
| How are ownership, mutation, determinism, persistence, and APIs structured? | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| What player experience and progression are intended? | [`GAME_DESIGN.md`](GAME_DESIGN.md) |
| What should be integrated next? | [`DIRECTION.md`](DIRECTION.md) |
| How may automated gameplay actors reason and what can their evidence establish? | [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) |
| How are tests selected and work completed? | [`TESTING.md`](TESTING.md) |

Code and adjacent tests own concrete operation behavior and typed edge cases. Design and direction pages do not
establish current reachability.

## Source role map

| Role | Modules |
| --- | --- |
| Foundational vocabulary and root state | `src/core/`, `src/capability/`, `src/material/`, `src/maintenance/`, `src/spatial/` |
| Immutable definition aggregation | `src/registry/`, `src/content/` |
| Durable runtime owners | `src/energy/`, `src/equipment/`, `src/fluid/`, `src/geology/`, `src/inventory/`, `src/labor/`, `src/logistics/`, `src/mining/`, `src/production/`, `src/structural/`, `src/survival/` |
| Transformation/resolution overlays | `src/crafting/`, `src/ore_processing/`, `src/thermal/` |
| Cross-owner accounting | `src/matter/` |
| Persistence and orchestration | `src/persistence/`, `src/simulation/` |
| Renderer-neutral presentation definitions | `src/texture/`, `src/shader/` |

`src/core/state.rs` aggregates runtime owners as `AppState`; it does not absorb their ownership.
`tools/check_authority_docs.py` keeps this map synchronized with public crate modules.

## Task map

| Concern | Owner and canonical boundary | Contract |
| --- | --- | --- |
| Time, root state, tick order | `src/core/`, `src/simulation/`; `AppState`, `advance_tick` | [Global runtime facts](TECHNICAL_DESIGN.md#global-runtime-facts) |
| Save admission and graph validation | `src/persistence/`, `src/core/state/`; `LoadedSaveEnvelope::into_state`, `validate_loaded_state` | [Trusted load](TECHNICAL_DESIGN.md#trusted-load) |
| Materials, inventory, matter | `src/material/`, `src/inventory/`, `src/matter/`; owner ingress/egress/reform validators | [Materials, inventory, and geology](TECHNICAL_DESIGN.md#materials-inventory-and-geology) |
| Geology, knowledge, mining | `src/geology/`, `src/mining/`; prospecting, target resolution, mining validation | [Materials, inventory, and geology](TECHNICAL_DESIGN.md#materials-inventory-and-geology) |
| Production and processing | `src/production/`, `src/crafting/`, `src/ore_processing/`, `src/thermal/`; resolver -> validate -> commit | [Production and processing](TECHNICAL_DESIGN.md#production-and-processing) |
| Equipment, labor, maintenance, survival | `src/equipment/`, `src/labor/`, `src/maintenance/`, `src/survival/`; provider resolution and validators | [Equipment, labor, survival, energy, and fluids](TECHNICAL_DESIGN.md#equipment-labor-survival-energy-and-fluids) |
| Energy and fluids | `src/energy/`, `src/fluid/`; finite stores and owner validators | [Equipment, labor, survival, energy, and fluids](TECHNICAL_DESIGN.md#equipment-labor-survival-energy-and-fluids) |
| Structures and spatial support | `src/structural/`, `src/spatial/`; structural validators and analysis | [Structures](TECHNICAL_DESIGN.md#structures) |
| Textures and shaders | `src/texture/`, `src/shader/`, `src/content/textures.rs`, `src/content/shaders.rs`, `assets/shaders/` | [Spatial and presentation boundaries](TECHNICAL_DESIGN.md#spatial-and-presentation-boundaries), [shader contract](assets/shaders/README.md) |
| Gameplay evaluation | `tests/gameplay_harness/`; production APIs after controlled setup | [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) |
| Verification tooling | `ci.py`, `.cargo/config.toml`, `tools/` | [`TESTING.md`](TESTING.md) |

For transfers, reservations, support, delayed custody, or owner interaction, start at the
[cross-owner edge atlas](TECHNICAL_DESIGN.md#cross-owner-edge-atlas).

## Change-impact map

| Change | Companion work |
| --- | --- |
| Persisted runtime state | Update owner serialization, index rebuild/validation, trusted-load checks, and continuation tests. |
| Authored identity or physical definition | Validate references at registry construction and update affected resolvers/tests. |
| Canonical command or cross-owner mutation | Preserve typed rejection, stale-state protection, atomicity, and one production path. |
| Tick or scheduled behavior | Persist future-affecting state, keep tick ordering explicit, and prove deterministic continuation. |
| Gameplay capability/reachability | Add production-path evidence and update [`STATUS.md`](STATUS.md); change [`GAME_DESIGN.md`](GAME_DESIGN.md) only if intent changed. |
| Future priority | Update [`DIRECTION.md`](DIRECTION.md), not current-scope claims. |
| Gameplay actor/evidence policy | Update [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) and preserve actor/diagnostic separation. |
| Verification tooling | Keep selectors fail-closed and local; update [`TESTING.md`](TESTING.md). |
