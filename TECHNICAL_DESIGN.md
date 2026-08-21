# Technical Design

This document owns project-specific contracts for implemented systems. [`ARCHITECTURE.md`](ARCHITECTURE.md)
owns general engineering rules, [`STATUS.md`](STATUS.md) owns capability presence, and
[`GAME_DESIGN.md`](GAME_DESIGN.md) owns player-facing intent.

## Core boundary

Deep Hearth is a deterministic headless simulation. Gameplay state and rules do not depend on a
renderer, window, input device, network connection, or storage backend.

```text
content -> registries -> simulation systems -> core state
                         ^
                         |
                      adapters
```

- `core` owns domain-neutral runtime primitives and `AppState`.
- `registry` owns immutable definition aggregates and lookup contracts.
- `content` authors built-in immutable definitions.
- `simulation` owns the visible top-level tick pipeline.
- `persistence` owns save envelopes and load validation, not filesystem or encoding policy.
- Gameplay subsystems own their records, indexes, validation tokens, plans, and canonical system paths.
- Adapters depend on the simulation core; the simulation core does not depend on adapters.

## Determinism

Authoritative simulation behavior is deterministic for the same immutable registries, persisted
state, explicit inputs, and external snapshots.

- Runtime randomness is owned by `RandomState` in `AppState` and fully persisted.
- PRNG and stream-derivation algorithms have explicit versioned identities.
- Typed `RngStreamId` values create independent streams derived from the world seed; advancing one
  subsystem stream cannot shift another subsystem's sequence.
- Ordering that can affect results uses deterministic collections or explicit sorting. Hash
  iteration order must never decide simulation outcomes.
- Authoritative physical quantities use integer representations and checked arithmetic.
- Parallel work may be introduced only when reduction and commit order remain deterministic.

## Simulation time

`SimulationTick` is absolute authoritative time. `TickSpan` is a distinct relative duration type, so
an absolute tick cannot be passed accidentally where a duration is required. Simulation ticks are
world-time units, not wall-clock update-frequency units. The built-in calendar maps 24,000 ticks to
86,400 physical world seconds, so one authoritative tick represents exactly 3.6 physical seconds.
Rate-authored physics such as watts, mass/second, and volume/second integrate against that physical
world duration. Per-tick gameplay costs such as metabolism, hydration, exertion, aging, and wear are
authored directly in authoritative world ticks.

`CalendarDefinition` is immutable registry content. It projects authoritative ticks into year,
month, day, day-relative tick, and one of four seasons without introducing mutable calendar state.
The built-in calendar uses 24,000 ticks per 86,400-second day, eight days per month, and twelve months
per year.

`PeriodicSchedule` provides deterministic clock-derived phase scheduling for static slow systems
such as ecology, soil, weather, migration, and settlement economics without introducing callbacks or
hidden mutable countdown state. Dynamic scheduled work such as production remains explicit persisted
records with dedicated indexes.

## State owners

`AppState` is the root of generated mutable state that must survive restart boundaries. Each subsystem
owns its authoritative records, generated IDs, revisions, and synchronized indexes. Callers receive
read-only state views; canonical system functions retain mutation access.

Primary runtime owners are:

- `InventoryState`: stockpiles, material lots, reservations, containment/preservation state, routing
  indexes, and stockpile support assignments;
- `EnergyState`: finite energy stores and embodied store traces;
- `FluidState`: finite homogeneous fluid stores and support assignments;
- `EquipmentState`: equipment instances, condition, embodied traces, and support assignments;
- `StructureState`: members, support topology, embodied matter, source-separated loads, and damage;
- `GeologyState`: finite geological deposits and depletion;
- `GeologicalKnowledgeState`: acquired observations only, never hidden deposit identity;
- `ProductionState`: active production jobs, schedules, routing, and exclusive resource occupancy;
- `MiningState`: mining work-in-process and scheduling;
- `PlayerWorkState`: at most one active player labor operation;
- `SurvivalState`: metabolic energy, hydration, vitality, nutrition, and cumulative terminal
  survival-consumption matter/fluid ownership.

Runtime-only derived indexes are rebuilt deterministically from authoritative records when loading.
Validated transaction tokens bind the owner revisions and snapshots they checked so intervening
mutation makes stale commits fail.

## Mutation model

Consequential mutations happen in canonical system functions. Fallible operations validate before
mutation. Multi-owner operations use consumed validated tokens when atomicity or staleness require it.
Read-heavy decisions with narrow writes use explicit decide/apply boundaries.

Top-level tick order remains visible in one function. Subsystems do not hide gameplay mutation in
callbacks, record methods, adapter hooks, or engine lifecycle events. Resolvers calculate physical
outcomes; validators authorize state transitions; commit tokens apply already-validated mutations.

## Persistence

The core defines a semantic current-schema save envelope. Byte encoding, filesystem layout, atomic
writes, compression, and cloud storage belong to adapters.

Two versions are distinct:

- `CURRENT_SAVE_SCHEMA_VERSION` owns the supported runtime payload shape;
- `RegistrySchemaVersion` owns authored identity and physics compatibility.

A change to authored definitions that alters persisted physical consequences requires a registry-schema
advance even when IDs remain unchanged.

A load must:

1. reject unsupported save or registry schema versions;
2. rebuild runtime-only derived indexes from authoritative persisted records;
3. validate each subsystem's local invariants;
4. resolve every authored and runtime reference;
5. validate cross-owner reservations, occupancy, lifecycle, support, provenance, and conservation;
6. replay operation-specific physical outcomes where persisted jobs depend on them;
7. return `AppState` only after exhaustive validation succeeds.

Only the current save schema is accepted.

## Spatial and presentation boundaries

Persistent spatial references use checked chunk-independent 64-bit voxel coordinates and half-open
bounds. Domain records must not depend on a chunk layout, ECS, scene graph, renderer object, or
streaming policy.

Renderer-neutral texture and shader definitions are immutable registry content, not `AppState`.
Texture content uses indexed 32x32 tiles, authored palette ramps, explicit block-face/object-slot
bindings, deterministic baking, and discrete indexed mip levels. The adapter boundary receives dense
stable descriptors and owns graphics-resource creation.

WGSL libraries and programs have typed identities, validated acyclic dependencies, deterministic
assembly, explicit entry points/pipeline requirements, and bounded work budgets. Renderer-specific
frame scheduling, GPU synchronization, resource allocation, and platform pipeline creation remain
adapter responsibilities. `assets/shaders/README.md` owns the concrete shader binding contract.

## Performance and validation

Hot paths use owner-maintained indexes or cursors rather than global scans. Derived indexes update with
their authoritative records, long-running work reserves output capacity before start, and compatible
material fragments coalesce deterministically. Per-tick validation stays cheap; exhaustive graph,
index, and physics validation belongs at load and explicit audit boundaries.

Authoritative mutations preserve local and cross-owner invariants, deterministic replay is stable from
the same persisted state and RNG streams, conservation-sensitive paths account for every represented
owner, and release builds retain integer overflow checks. [`TESTING.md`](TESTING.md) owns validation
commands and lane selection.

## Authoritative physical quantities

Conservation and engineering calculations use explicit integer units:

| Type | Unit | Storage |
| --- | --- | --- |
| `Mass` | milligram | `u64` |
| `AggregateMass` | milligram | `u128` |
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
| `Volume` | microliter | `u64` |
| `AggregateVolume` | microliter | `u128` |
| `MassSpecificEnergy` | nanojoule/milligram | `u64` |
| `MassFlow` | milligram/second | `u64` |
| `VolumetricFlow` | microliter/second | `u64` |

Potentially overflowing arithmetic is checked. Implemented authoritative physical calculations do not
use floating point.

## Materials and physical forms

Materials are immutable typed definitions containing physical properties. Forms define phase and
particle-state policy. `CommodityKey` combines one material and one form for coarse identity.

`MaterialComposition` is canonical normalized mass fraction in integer parts per million, sorted by
material ID and totaling exactly 1,000,000 ppm. Mixed matter therefore preserves composition without
requiring a synthetic material definition for every mixture.

Particulate forms use validated `ParticleSizeDistribution` values containing non-overlapping size
classes with canonical relative weights. A single class represents an unresolved envelope. Screening
may split only fully resolved classes on one side of an aperture; it rejects a cut through an
unresolved class or a partition that cannot be represented exactly at whole-milligram resolution.

Thermal state is phase-aware. Sensible heat is composition-weighted; pure-material fusion uses an
explicit fusion temperature and latent heat. Solid matter cannot exceed its fusion boundary and liquid
matter cannot fall below it. Mixed-liquid phase behavior is unsupported until an explicit alloy or
solution phase model exists.

## Matter ownership, inventory, and geology

`MaterialLotRecord` is the authoritative stored-matter record. A lot owns identity, stockpile,
`MaterialLotProfile`, mass, provenance, and storage exposure. Physical profile differences that affect
phase, composition, temperature, particle size, or freshness prevent unsafe coalescing.

Stockpiles own capacity, inbound reservations, containment rules, preservation behavior, and derived
mass/commodity caches. Runtime lot routing is indexed by stockpile and commodity and rebuilt from
persisted authoritative lots. Reservations represent future space, not present matter or weight.

Inventory is custody, not a movement authorizer. `validate_material_transfer` consumes an opaque
`MaterialTransferResolution` produced by a physical/logistics owner. Validation performs deterministic
selection, destination admission, revision checks, and any structural-load plan. Commit moves the exact
selected profiles through the canonical relocation path. The resolution and validated token are
single-use.

Cross-owner resolvers that already selected exact matter use the same internal relocation primitive
with their bound `ConsumptionSelection`; they do not reselect equivalent matter. Partial movement uses
stable lot-ID splitting and deterministic compatible coalescing.

Supported stockpiles derive `StructuralLoadKind::StoredMatter` from aggregate authoritative stored mass.
All inventory operations that change supported mass update inventory and structural state in one
validated transaction.

Geological deposits are a separate finite matter owner with spatial bounds, material profile,
deposit-scale excavation hardness, provenance, remaining mass, and lifecycle. Excavation hardness is
a geological-body property rather than an inference from the deposit's coarse commodity identity or
assay composition. Natural geological matter is solid and does not carry processed particulate state.
Player-facing code does not receive authoritative deposit enumeration.

Mining is the gameplay extraction owner. It moves exact geological matter into mining work-in-process
only after tool, labor, capability, wear, destination, and reservation validation. Completion releases
work occupancy; claim moves the already-owned output into inventory.

`calculate_matter_accounting` and modeled-energy accounting recompute totals from authoritative owners,
including geological, inventory, embodied, terminal survival-consumption, and in-flight state.
Ownership transitions do not create or delete represented matter or modeled energy.

## Prospecting knowledge

Geological truth and acquired player knowledge are separate owners. `GeologicalObservationRecord`
stores only authorized evidence: spatial footprint, provenance, observation time, and bounded
material-abundance estimates.

`ProspectingResolution` is opaque, non-cloneable, and consumed by validation. A physical survey owner
must determine findings before knowledge can be recorded. Knowledge persistence never queries hidden
deposits.

Assessment reads acquired evidence only. Bounds are combined only where observations share a common
locality; disjoint evidence is marked spatially incomparable and contradictory overlapping evidence
remains visible. Precision ranking and regional maps are deterministic.

## Timed production

`ProcessDefinition` stores immutable identity, material requirements, and typed capability requirements.
Operation-specific physics live in resolver outputs, not static recipe duration/yield fields.

`ProcessResolution` describes one concrete operation: duration, exact output streams, and any finite
energy/equipment consequences. Physical resolvers create it before canonical start validation. It is a
planning object and may be reused for route validation while its bound selections/revisions keep stale
execution from mutating state.

Production obeys these contracts:

- selected input traces are exact and are the same traces the resolver inspected;
- resolved output material mass equals consumed input mass unless every difference is represented by
  explicit material streams;
- destination capacity is reserved at start;
- the job owns consumed matter and modeled energy while in flight;
- start and completion update supported-stockpile loads according to authoritative matter ownership;
- same-tick multi-output completion plans structural consequences as one deterministic batch;
- completion validates bound revisions before output, wear, energy, or job removal becomes authoritative;
- persisted operation-specific traces are sufficient to recompute physical outcomes on load.

Comminution resolves authored feed/output particle distributions, batch limits, condition-sensitive
throughput, finite work energy, power-limited elapsed duration, and processing-duty wear. Screening is
multi-stream classification: it routes fully resolved particle classes around an authored aperture and
never invents a split through unresolved material.

Thermal resolvers cover phase-aware sensible heating, pure-material melting, and pure-material casting.
They use real selected matter, explicit fusion properties, finite energy sources/sinks, equipment
limits, and conserved released heat.

A job whose required equipment support becomes unavailable may suspend with exact remaining active
time and conserved work-in-process. Recovery can restore the support relationship and resume the same
committed operation without re-resolving its physical inputs.

## Capabilities, equipment condition, energy, and fluids

Capabilities are typed physical requirements with explicit value kinds and `AtLeast`/`AtMost`
semantics. Static definitions provide nominal values; runtime providers may expose condition-adjusted
values through the same evaluator.

Equipment uses normalized `Condition`. Authored numeric capability curves may interpolate from a
failed endpoint to the nominal pristine value using checked integer arithmetic. Presence-only
capabilities require explicit discrete policy and are not represented by numeric interpolation.
Productive operations with authored active-tick wear must fit entirely inside the provider's remaining
condition lifetime. The final useful tick may reduce condition to `FAILED`; no later productive tick is
authorized because failed condition-sensitive equipment contributes zero usable capability. Runtime
resolution and persisted-job replay enforce the same discrete-tick boundary.

Maintenance is a conserved cross-owner operation. `EquipmentMaintenanceProfile` specifies exact
replacement matter, a distinct same-material spent form, and a restored condition. Resolver output is
an opaque single-use `EquipmentRepairResolution`; validation binds equipment/inventory state,
occupancy, replacement selection, and structural consequences before commit. Repair changes physical
form and condition without deleting replacement matter or manufacturing reusable parts.

Equipment definitions that accumulate condition wear provide a maintenance route.
Runtime-assembled equipment may additionally author a worn-recovery form. Pristine disassembly returns
exact embodied traces, while worn decommissioning reforms each trace into that same-material recovery
form. Registry validation forbids the recovery form from also being a direct assembly input, so wear
cannot be cleared by disassemble/reassemble cycling.

Engineering scalar modules provide exact integer foundations for:

- power integrated over the physical world duration represented by ticks into energy with carried
  remainder;
- volumetric flow integrated over the physical world duration represented by ticks into volume with
  carried remainder;
- electrical power and resistive voltage drop;
- torque, angular speed, power, efficiency, and rational transmission ratios;
- independent torque/speed/power operating limits.

Finite energy stores own carrier, capacity, directional power envelopes, stored energy, identity, and
revision. Production reserves participating sources/sinks exclusively. Material-backed stores also own
exact construction traces.

`EnergyTransferResolution` is an opaque single-use authorization for already-resolved same-carrier
movement. Energy storage validates endpoints, occupancy, carrier, capacity, quantity, and revisions but
does not choose a path, convert carriers, model network losses, or generate energy.

Finite fluid stores own fluid identity, volume, temperature, capacity, revision, and optional structural
support. A fluid definition references its material; support-load mass is derived from that material's
authored density. Runtime allocation creates empty capacity only.

`FluidTransferResolution` is an opaque physical authorization. Storage validates exact conserved
movement and all affected fluid-owned structural loads. It does not authorize pathless movement or
invent mixing/thermal-equilibration behavior for unlike contents.

Survival eating and drinking use exact inventory/fluid egress into a terminal consumption boundary
that remains included in global matter and fluid accounting. The boundary records cumulative consumed
matter and fluid rather than pretending it is live body mass or body water. Accepted meals and drinks
consume the exact requested physical portion while individual physiological gains clamp at authored
reserve capacities. Validation rejects a meal when none of metabolic energy, hydration, or nutrition
would increase, and rejects a drink when hydration would not increase. This prevents pure no-benefit
resource waste without silently resizing an otherwise useful requested portion.

## Structural matter

Structural planning and material ownership are separate. A planned member contains geometry and
references but cannot activate until construction matter is committed.

`StructuralConstructionResolution` is an opaque single-use authorization containing an exact inventory
selection. Validation binds inventory and structural revisions, requires an unmaterialized planned
member, and accepts only consolidated solid pure matter matching the authored structural material.
Construction transfers exact physical/provenance traces into structural ownership.

`StructuralLoadKind::SelfWeight` is derived from embodied mass under registry gravity and is writable
only by the structural owner. Supported source-stockpile load changes are part of the same validated
construction transaction.

Materialized members cannot be generically deleted. `StructuralDeconstructionResolution` is an opaque
single-use authorization that couples member removal with conserved inventory ingress. Undamaged
members preserve exact traces. Cracked or failed members reform every trace into the structural
profile's authored damaged-recovery form, and that form is rejected as direct construction feedstock
for the same profile. This preserves matter and provenance without turning structural damage into a
free reset. Any future fractional or lossy demolition must model recovered, waste, and debris streams
explicitly.

## Stockpile structural support

A stockpile may reference one structural support; `InventoryState` owns the synchronized reverse index.
Mount/unmount is a revision-bound inventory/structure transaction and mounting requires an active
member.

Inventory exclusively owns `StructuralLoadKind::StoredMatter`. All stockpile mass on a support is
aggregated before gravity conversion. Every canonical stored-mass mutation updates that derived load in
the same transaction, including transfers, production, mining claims, and structural material
movement.

Added weight can strain, crack, or fail support. Failed supports reject newly initiated inbound matter
but can be unloaded. A support cannot be removed while referenced. Stockpiles occupied by production
cannot be relocated until that ownership permits it.

Multi-stockpile and same-tick output changes use one final-load plan so structural consequences do not
depend on mutation order.

## Equipment, assembly, player labor, and mining

Equipment may reference one structural support; `EquipmentState` owns the reverse index and
`StructuralLoadKind::Equipment`. Mount, unmount, and relocation are revision-bound cross-owner
transactions. Aggregate mounted mass determines the equipment load. A failed support cannot authorize
new equipment use, and active production normally prevents relocation.

`validate_relocate_equipment` plans source unloading and target loading together and exposes the final
structural analysis before commit. Relocation is permitted for equipment whose production job is
suspended specifically because support is unavailable, enabling recovery without abandoning committed
work-in-process.

`MaterialAssemblyProfile` defines exact conserved inputs for equipment and material-backed energy-store
construction. Assembly transfers exact traces into the new owner. Rigid infrastructure inputs must be
registered consolidated solids unless a future physical transformation owner provides another route.

`EquipmentUpgradeProfile` is additive. The target assembly must equal the base assembly plus exact
additions. Upgrade preserves equipment identity, creation time, condition, and existing traces; it is
not repair.

Pristine, idle, unmounted assembled equipment can be disassembled to exact traces. Authored worn
equipment recovery decommissions embodied traces into same-material scrap rather than restoring shaped
components. Material-backed energy stores can be disassembled only while empty and idle. Stored energy
blocks exact reversal where the current model cannot represent the recovered state.

`PlayerWorkState` is the exclusive labor owner for manual crafting, mining, and direct player power.
The survival system projects authored exertion from active work and combines it with basal physiology.
Work admission binds the survival revision and requires enough metabolic energy and hydration for the
scheduled interval.

Manual crafting emits ordinary `ProcessResolution` values and uses canonical production ownership.
Repeated batches scale authored matter and time together.

Direct player power binds a real Power capability from portable, unmounted equipment and a finite
destination energy store. Admission and persistence reject mounted direct-power tools; equipment support
mutations already reject active manual-power occupancy, so the tool remains unmounted for the entire
work interval without carrying a redundant structural dependency in `ManualPowerWork`.
Duration respects equipment/store transfer limits and the method's maximum sustainable metabolic
output. Active physiological exertion is then scaled to the actual mechanical work required at the
authored metabolic efficiency, so slower equipment or destination bottlenecks do not charge full
effort for unused human output capacity. Energy and wear
become authoritative together at completion. A validated manual-power start exposes the same
authoritative `PlayerWorkResourceBudget` used for admission, so callers can present or compare the
projected metabolic-energy and hydration cost without duplicating survival formulas. Successful
completion consumes exactly that projected budget.

Hand mining binds a finite geological deposit, method, tool, player labor, destination reservation,
condition-sensitive flow, batch limit, material-hardness limit, and wear. Exact matter moves into
`MiningState` at start, becomes claimable after active work completes, and reaches inventory only through
the claim transaction. Persisted working jobs must replay the same physical schedule and wear result.

## Cross-subsystem runtime invariants

`validate_loaded_state(registries, state)` is the exhaustive admission boundary for persisted runtime
state. It validates each local owner and reconstructs cross-owner agreement rather than trusting cached
or derived claims.

Cross-owner validation includes, as applicable:

- authored and runtime references plus monotonic identity cursors;
- synchronized forward/reverse indexes and derived caches;
- material provenance, phase, composition, particle-state, containment, and reservations;
- geological and prospecting record validity;
- production/mining/player-work lifecycle, scheduling, occupancy, and in-flight ownership;
- exact matter and modeled-energy conservation across active work;
- structural topology, material embodiment, self-weight, damage, and source-owned load channels;
- equipment, stockpile, and fluid support assignments with independently recomputed loads;
- energy source/sink reservations and operation-specific physical replay;
- deterministic continuation from persisted RNG and schedule state.

New systems must define an immutable authored contract where appropriate, one runtime owner for each
consequential fact, a canonical validated mutation path, persistence semantics, typed errors, and
invariant coverage. `STATUS.md` owns whether a broader capability is implemented; this document does
not duplicate the deferred-feature inventory.

