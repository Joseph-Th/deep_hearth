# Technical Design

This document owns implemented, project-specific technical contracts. [`ARCHITECTURE.md`](ARCHITECTURE.md)
owns general engineering law, [`STATUS.md`](STATUS.md) owns capability presence, and
[`GAME_DESIGN.md`](GAME_DESIGN.md) owns player-facing intent. Source and adjacent tests own concrete edge
cases and error details.

## Runtime model

Deep Hearth is a deterministic headless simulation. Gameplay state does not depend on rendering, input,
networking, platform services, or a storage backend.

| Layer | Responsibility |
| --- | --- |
| `core` | Domain-neutral quantities, identity, time, RNG, and root `AppState` primitives |
| `registry` | Immutable validated definition aggregates and lookup contracts |
| `content` | Built-in authored definitions |
| gameplay subsystems | Authoritative records, indexes, validation/planning, and canonical mutations |
| `simulation` | Explicit top-level tick order |
| `persistence` | Semantic save envelope and trusted-load admission |
| adapters | Filesystem, encoding, rendering, input, networking, platform integration |

Registries describe what may exist. `AppState` records what does exist. Runtime-only derived indexes may
be omitted from persistence only when they rebuild deterministically from authoritative records.

## Determinism and time

Authoritative results are a function of immutable registries, persisted state, ordered explicit inputs,
state-owned randomness, and explicitly modeled external snapshots.

- `RandomState` is persisted. Typed RNG streams derive independently from the world seed.
- Result-affecting ordering uses deterministic collections or explicit sorting with complete tie-breakers.
- Authoritative physical quantities use checked integer arithmetic; implemented physical calculations do
  not use floating point.
- Parallelism may change throughput, never authoritative reduction or commit order.
- `SimulationTick` is absolute world time; `TickSpan` is relative duration.
- The built-in calendar maps 24,000 ticks to 86,400 physical seconds, so one tick is 3.6 seconds.
- Rate-authored physics integrate against physical tick duration. Per-tick gameplay costs are authored in
  world ticks.
- Dynamic scheduled work is persisted as explicit records. `PeriodicSchedule` is reserved for static,
  clock-derived phase scheduling.

## Runtime owners

`AppState` is the root of generated state. Each subsystem owns its records, generated IDs, revisions, and
synchronized indexes.

| Owner | Authoritative state |
| --- | --- |
| `InventoryState` | Stockpiles, material lots, reservations, routing, preservation, stockpile support |
| `EnergyState` | Finite energy stores and embodied construction traces |
| `FluidState` | Finite homogeneous fluid stores and support assignments |
| `EquipmentState` | Equipment instances, condition, embodied traces, support assignments |
| `StructureState` | Members, topology, embodied matter, source-separated loads, damage |
| `GeologyState` | Finite hidden geological deposits and depletion |
| `GeologicalKnowledgeState` | Acquired observations only |
| `ProductionState` | Active jobs, schedules, routing, exclusive resource occupancy |
| `MiningState` | Mining work-in-process and schedules |
| `PlayerWorkState` | At most one active player labor operation |
| `SurvivalState` | Metabolic energy, hydration, vitality, nutrition, fractional vitality-recovery carry, terminal consumed matter/fluid totals |

Cross-owner operations coordinate these owners; no owner reaches into another owner's private storage.

## Mutation and persistence

Consequential mutations use the canonical patterns defined in `ARCHITECTURE.md`:

- `validate_* -> Validated* -> commit` for fallible revision-bound state changes;
- `decide_* -> Plan/Outcome/Delta -> apply_*` for read-heavy decisions with narrow writes;
- direct owner mutation only when one owner can preserve every invariant on every return path.

Resolvers calculate physical outcomes. Validators authorize state transitions. Consumed tokens bind the
revisions, selections, and snapshots they checked; stale commits fail before partial mutation.

Persistence distinguishes two versions:

- `CURRENT_SAVE_SCHEMA_VERSION`: supported runtime payload shape;
- `RegistrySchemaVersion`: authored identity and physical-definition compatibility.

Only the current save schema is accepted. Trusted load admission:

1. validates save and registry versions;
2. rebuilds derived indexes;
3. validates every local owner;
4. resolves authored and runtime references;
5. validates cross-owner occupancy, reservations, support, provenance, and ownership;
6. replays operation-specific physical outcomes where persisted work depends on them;
7. returns `AppState` only after the complete graph is valid.

## Physical quantities

| Type | Unit | Storage |
| --- | --- | --- |
| `Mass` / `AggregateMass` | milligram | `u64` / `u128` |
| `Temperature` | absolute millikelvin | `u32` |
| `Energy` | nanojoule | `u128` |
| `Pressure` | pascal | `u64` |
| `Area` | square millimeter | `u64` |
| `Length` | micrometer | `u64` |
| `Acceleration` | micrometer/second² | `u64` |
| `Force` | millinewton | `u128` |
| `Power` | picowatt | `u128` |
| `Torque` | micronewton-meter | `u64` |
| `AngularSpeed` | microradian/second | `u64` |
| `ElectricPotential` | microvolt | `u64` |
| `ElectricCurrent` | microampere | `u64` |
| `ElectricalResistance` | microohm | `u64` |
| `Volume` / `AggregateVolume` | microliter | `u64` / `u128` |
| `MassSpecificEnergy` | nanojoule/milligram | `u64` |
| `MassFlow` | milligram/second | `u64` |
| `VolumetricFlow` | microliter/second | `u64` |

Potentially overflowing arithmetic is checked. Conservation-sensitive systems account from authoritative
owners rather than cached totals.

## Materials, inventory, and geology

Materials are immutable definitions. Forms define phase and particle-state policy. `CommodityKey`
combines one material and one form; composition remains a separate exact property.

`MaterialComposition` is sorted normalized mass fraction totaling exactly 1,000,000 ppm. Mixed matter
preserves composition without inventing synthetic material identities. Particulate state uses validated,
non-overlapping particle-size classes. Thermal state is phase-aware; pure-material fusion uses explicit
fusion temperature and latent heat.

`MaterialLotRecord` is the stored-matter authority. A lot owns stockpile, material profile, mass,
temperature/composition/particle state, provenance, and exposure. Physically relevant profile differences
prevent unsafe coalescing. Lot IDs identify persistent distinct lots, not transaction attempts: compatible
ingress, completion output, reform output, and relocation fragments bind to the identity that will survive
coalescing, and the monotonic lot cursor advances only when a distinct lot will actually persist.

Stockpiles own capacity, containment, preservation, inbound reservations, and derived routing/mass
indexes. Inventory is custody, not movement authorization: material relocation requires an opaque
physical/logistics resolution or an already-bound exact selection from another canonical resolver.
Same-material reform must perform a real commodity-form change; a selection already entirely in the
target commodity is rejected instead of manufacturing a meaningless inventory mutation.

Supported stockpiles contribute `StructuralLoadKind::StoredMatter`. Every canonical stored-mass mutation
updates inventory ownership and the resulting structural load atomically.

Geological deposits are a separate finite matter owner. They contain spatial bounds, material profile,
excavation hardness, provenance, remaining mass, and lifecycle. Player-facing code cannot enumerate
hidden deposit truth.

Geological knowledge is a separate persisted owner. Observations contain authorized spatial evidence and
bounded abundance estimates, not deposit identity. Recording requires an opaque `ProspectingResolution`;
assessment combines only acquired evidence and preserves contradiction or spatial incomparability.

Mining moves exact geological matter into `MiningState` after tool, labor, capability, wear, destination,
and reservation validation. Completion releases work occupancy; claim transfers the already-owned output
to inventory.

## Production and processing

`ProcessDefinition` owns immutable identity, material requirements, and typed capability requirements.
Operation-specific physics belong in resolver outputs, not static duration/yield fields.

`ProcessResolution` describes one concrete operation: exact selected inputs, duration, output streams, and
finite energy/equipment consequences. Production reserves output capacity at start and owns consumed
matter plus modeled energy while work is in flight. Completion is revision-bound and must preserve exact
represented matter and modeled energy across all streams.

Supported equipment may cause a production job to suspend when support becomes unavailable. Suspension
keeps work-in-process and exact remaining active time; valid recovery resumes the same committed job.

Implemented physical resolvers include:

- comminution with authored feed/output particle state, condition-sensitive throughput, batch limits,
  finite work energy, power-limited duration, and active-tick wear;
- dry screening that partitions fully resolved particle classes around an authored aperture without
  inventing fractional or unresolved splits;
- sensible heating, pure-material melting, and pure-material casting with real selected matter, finite
  energy sources/sinks, equipment limits, phase boundaries, and latent heat.

## Equipment, labor, survival, energy, and fluids

Capabilities use explicit typed values and `AtLeast`/`AtMost` requirements. Equipment providers expose
runtime condition-adjusted capabilities through the same evaluation boundary as nominal definitions.
Failed equipment exposes no productive capability.

Equipment owns persistent identity, condition, embodied traces, occupancy, and optional structural
installation. Fixed machinery requires an active support before it can authorize new work; portable tools
remain usable without one. Mounted equipment contributes equipment-owned structural load.

Assembly consumes exact material traces. Additive upgrades preserve identity, condition, and existing
traces while adding authored matter. Pristine idle unmounted equipment may disassemble to exact traces;
worn recovery, where authored, reforms traces into a same-material recovery form that cannot immediately
reset wear through reassembly.

Maintenance consumes an exact replacement commodity, produces a distinct conserved spent form, and
restores the authored condition target. It is a physical material reform, not condition-only mutation.

`PlayerWorkState` is exclusive across manual crafting, hand mining, and direct manual power. Work
admission binds projected metabolic-energy and hydration cost. Successful completion consumes that same
budget.

Direct player power uses a real portable unmounted power provider and finite compatible destination
store. Duration respects provider/store transfer limits and sustainable metabolic output; physiological
cost scales to actual mechanical work at authored efficiency. Energy creation and equipment wear commit
together.

Survival tracks metabolic energy, hydration, vitality, and category-specific recent nutrition. Eating and
drinking consume exact physical portions into terminal conservation owners; physiological gains clamp
independently to authored reserve capacities. Vitality recovery scales with the weakest Grain/Fruit/Protein
reserve, so calories concentrated in one category cannot mathematically stand in for a balanced recent
diet. Fractional recovery is accumulated in persisted fixed-point state so whole-ppm vitality storage does
not create artificial healing-rate cliffs; the read-only assessment rounds that exact rate for presentation.
No-benefit consumption is rejected rather than silently wasting finite resources.

Energy stores own carrier, capacity, directional power envelopes, stored energy, identity, revision, and
optional embodied traces. Transfer requires an opaque same-carrier authorization; storage does not choose
paths, convert carriers, or generate energy.

Fluid stores own identity, volume, temperature, capacity, revision, and optional support. Transfer is
exact and opaque-authorized. The current model does not implicitly mix unlike fluids or temperatures.
Supported fluid load derives from authored material density.

## Structures

Structural members own geometry, topology, embodied material, self-weight, external source-separated
loads, lifecycle, and damage. Current analysis models axial tension/compression and deterministic
stable/strained/cracked/failed transitions with support-loss cascades.

Construction transfers exact inventory traces into structural ownership. Deconstruction returns exact
traces for undamaged members and reforms damaged members into the profile's authored non-load-bearing
recovery form. Materialized members are never generically deleted.

Stockpile, equipment, and fluid owners each maintain their own structural load channel. Multi-owner load
changes are planned against final aggregate load so results do not depend on mutation order.

## Spatial and presentation boundaries

Persistent spatial references use checked chunk-independent 64-bit voxel coordinates and half-open
bounds. Runtime records do not depend on a chunk layout, ECS, scene graph, renderer object, or streaming
policy.

Texture and shader definitions are immutable registry content. Texture baking is deterministic and
renderer-neutral. WGSL libraries/programs have typed identities, deterministic assembly, validated
dependencies, explicit entry points/pipeline requirements, and bounded work. Graphics-resource creation
and frame scheduling belong to adapters. [`assets/shaders/README.md`](assets/shaders/README.md) owns the
concrete shader binding contract.

## Cross-owner invariants

`validate_loaded_state(registries, state)` is the exhaustive trusted-load boundary. It recomputes rather
than trusting cached claims. Cross-owner validation covers, as applicable:

- authored/runtime references and monotonic identity cursors;
- forward/reverse indexes and derived caches;
- material profile, provenance, containment, and reservations;
- production, mining, and player-work lifecycle/occupancy;
- exact represented matter, fluid, and modeled-energy ownership;
- structural topology, embodiment, damage, and source-owned load channels;
- support assignments and independently recomputed loads;
- persisted RNG, schedules, and operation-specific physical replay.

New systems define an immutable authored contract where appropriate, one owner for each consequential
fact, one canonical mutation path, persistence semantics, typed failures, and invariant coverage before
`STATUS.md` lists the capability as implemented.
