# Deep Hearth

Deep Hearth is a deterministic Rust simulation core for a first-person survival, settlement, and industrialization
game. It owns headless state, authored definitions, and simulation rules; client/platform IO belongs to adapters.

[`STATUS.md`](STATUS.md) is the sole authority for ordinary reachability, capability-only evaluation, and
absent runtime scope. Do not infer reachability from source presence or controlled gameplay-harness setup.

## Orientation

1. Read workspace [`../AGENTS.md`](../AGENTS.md), then project [`AGENTS.md`](AGENTS.md).
2. Read [`STATUS.md`](STATUS.md) for current reachable, capability-only, and absent scope.
3. Use the task map below to find the owner, canonical boundary, contract, and focused proof.
4. Read that owner and its adjacent tests; open deeper authority pages only as needed.
5. Use [`TESTING.md`](TESTING.md) for verification and gameplay-evaluation commands.

## Abstraction ladder

Use one descending reasoning stack; each layer answers a different question without duplicating mutable facts.

| Layer | Question | Authority |
| --- | --- | --- |
| Intent | What experience and long-term system behavior are we trying to create? | [`GAME_DESIGN.md`](GAME_DESIGN.md) |
| Direction, when planning | Which missing edge or vertical slice should be built next for maximum connective leverage? | [`DIRECTION.md`](DIRECTION.md) |
| Reality | What is implemented, ordinarily reachable, capability-only, or absent now? | [`STATUS.md`](STATUS.md) |
| System contract | What physical/state semantics does the implemented system obey? | [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) |
| Engineering law | How are ownership, mutation, determinism, persistence, and APIs structured? | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| Concrete owner | Which records and canonical operations actually control the fact? | routed `src/` owner plus adjacent tests |
| Evidence | What is the cheapest complete proof that the intended contract holds? | [`TESTING.md`](TESTING.md) |

Descend only until uncertainty resolves; after editing, prove owner/crossed contracts and update only changed authority.

## Control coordinate

A feature name is not a system address. Use:

`authority / owner / stage / flow / proof`

Authority locates truth; owner the generated fact; stage the `observe -> resolve -> validate -> commit ->
continue -> outcome/audit` position; flow the matter/fluid/energy/labor/information/support/capacity/identity/time
edge; proof the owner/boundary/continuation/system/exploration level. Missing legitimate observation, blocker,
mutation, continuation, or outcome surfaces are control-surface debt, not permission for parallel rules.

## Source role map

Directory presence does not imply ownership. Classify new facts/resolvers/projections/commands here first.

| Role | Modules | Agent interpretation |
| --- | --- | --- |
| Foundational vocabulary | `src/core/`, `src/capability/`, `src/material/`, `src/maintenance/`, `src/spatial/` | Defines reusable time, quantity, identity, physical-state, capability, condition, and coordinate semantics. Do not make these depend on higher-level workflows. |
| Authored definition aggregation | `src/registry/`, `src/content/` | Builds and cross-validates immutable definitions. Runtime progress does not belong here. |
| Durable runtime owners | `src/energy/`, `src/equipment/`, `src/fluid/`, `src/geology/`, `src/inventory/`, `src/labor/`, `src/mining/`, `src/production/`, `src/structural/`, `src/survival/` | Owns one or more fields under `AppState::SystemState`; consequential durable mutation terminates in these owners. |
| Transformation/resolution overlays | `src/crafting/`, `src/ore_processing/`, `src/thermal/` | Derives physical operations and delegates durable custody/scheduling to runtime owners, primarily production, inventory, energy, equipment, labor, and survival. |
| Cross-owner accounting projection | `src/matter/` | Reconciles authoritative owners read-only. It is evidence, not another matter store. |
| Persistence and orchestration | `src/persistence/`, `src/simulation/` | Promotes trusted state and sequences owner decisions/applies. It coordinates semantics but does not absorb their ownership. |
| Renderer-neutral presentation definitions | `src/texture/`, `src/shader/` | Owns deterministic presentation definitions/assembly, not simulation state or graphics resources. |

`tools/check_authority_docs.py` keeps this map synchronized with public modules and the runtime-owner atlas.

## Authorities

| Question | Authority |
| --- | --- |
| Project execution rules | [`AGENTS.md`](AGENTS.md) |
| Ownership, mutation, determinism, persistence, API shape | [`ARCHITECTURE.md`](ARCHITECTURE.md) |
| Implemented subsystem and physical contracts | [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) |
| Product direction and intended player experience | [`GAME_DESIGN.md`](GAME_DESIGN.md) |
| Future system-integration priority and accretion sequence | [`DIRECTION.md`](DIRECTION.md) |
| Current reachable, capability-only, and absent scope | [`STATUS.md`](STATUS.md) |
| Test organization, proof selection, and local verification | [`TESTING.md`](TESTING.md) |
| Automated-player boundaries and gameplay evidence semantics | [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) |

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
| Gameplay evaluation | `tests/gameplay_harness/`; production APIs after controlled setup | [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) | matching gameplay lane from [`TESTING.md`](TESTING.md) |
| Verification tooling | `ci.py`, `.cargo/config.toml`, `tools/` | [`TESTING.md`](TESTING.md) | `python ci.py quick` |

For transfers, reservations, support, delayed custody, or owner interaction, start at the
[cross-owner edge atlas](TECHNICAL_DESIGN.md#cross-owner-edge-atlas).

## Change-impact map

Use this map when a change crosses an authority boundary.

| Change | Required companion work |
| --- | --- |
| Persisted runtime state | Keep the fact in its runtime owner; update strict serialization, deterministic index rebuilds, trusted-load validation, schema ownership when semantics change, and persistence coverage. |
| Authored identity or physical definition | Validate references during registry construction; update registry schema ownership when persisted identity/replay semantics change; update affected resolvers and registry-derived tests. |
| Canonical command or cross-owner mutation | Preserve one production path, typed rejection, stale-state protection, and atomicity; reuse it from tests and harnesses. |
| Tick or scheduled behavior | Persist future-affecting schedule state, keep `advance_tick` ordering explicit, validate continuation on load, and test deterministic completion or resumption. |
| Gameplay capability or reachability | Add executable production-path coverage and update [`STATUS.md`](STATUS.md). Update [`GAME_DESIGN.md`](GAME_DESIGN.md) only when intended experience changes. |
| Future integration priority | Update [`DIRECTION.md`](DIRECTION.md); do not change current-scope claims until implementation changes, and do not change product intent unless the intended experience changes. |
| Gameplay-harness policy or evidence | Preserve actor/diagnostic separation, reproducible seeds, bounded search, replay inputs, evidence labels, and production-derived legality; update [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md). Update [`TESTING.md`](TESTING.md) only when commands, test organization, or verification selection changes. |
| Texture, shader, or adapter contract | Keep authored definitions renderer-neutral; update [`assets/shaders/README.md`](assets/shaders/README.md) when its binding contract changes; run the relevant lane. |
| Verification or developer tooling | Keep selectors fail-closed and commands local/reproducible; update [`TESTING.md`](TESTING.md). |

When several rows apply, treat them as one vertical slice. Update only the authority that owns each changed truth.
