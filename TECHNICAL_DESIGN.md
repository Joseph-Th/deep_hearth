# Technical Design

This document owns implemented, project-specific runtime contracts. [`ARCHITECTURE.md`](ARCHITECTURE.md)
owns general engineering rules. [`STATUS.md`](STATUS.md) owns capability presence. Source and adjacent
tests own concrete edge cases and typed errors.

Read only the section for the subsystem being changed.

## Contract map

| Change concerns | Read |
| --- | --- |
| Time, save versions, runtime state owners | Global runtime facts; Runtime owners |
| Units, checked arithmetic, conservation | Physical quantities |
| Materials, inventory, geology, knowledge, mining | Materials, inventory, and geology |
| Jobs, crafting, ore processing, thermal work | Production and processing |
| Equipment, labor, survival, energy, fluids | Equipment, labor, survival, energy, and fluids |
| Structural support, loads, failure | Structures |
| Coordinates, textures, shaders, renderer boundary | Spatial and presentation boundaries |
| Trusted-load graph validation | Trusted load |

## Global runtime facts

- `SimulationTick` is absolute world time; `TickSpan` is relative duration.
- The built-in calendar maps 24,000 ticks to 86,400 seconds; one tick is 3.6 seconds.
- Rate-authored physics integrate against physical tick duration. Per-tick gameplay costs use world ticks.
- `RandomState` is persisted and owns typed RNG streams derived from the world seed.
- Implemented authoritative physical calculations use checked integer arithmetic, not floating point.
- Dynamic scheduled work persists as explicit records. `PeriodicSchedule` is for static clock-derived
  phase scheduling.
- `CURRENT_SAVE_SCHEMA_VERSION` is the only accepted runtime payload shape.
- `RegistrySchemaVersion` identifies authored identity and physical-definition compatibility.
- Save encoding and storage are adapter concerns.

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

### Materials and lots

Materials are immutable definitions. Forms define phase, particle-state policy, and physical cohesion.
Only consolidated non-particulate solids may directly become rigid infrastructure components or structural
embodiment; loose forms require an explicit shaping or consolidation process first. `CommodityKey` combines
one material and one form, but runtime ownership is limited to exact pairs explicitly authored in the
material registry; independently valid material and form IDs do not imply a valid commodity. Composition
remains a separate exact property.

`MaterialComposition` is sorted normalized mass fraction totaling exactly 1,000,000 ppm. Mixed matter
preserves composition without inventing synthetic material identities. Particulate state uses validated,
non-overlapping particle-size classes. Thermal state is phase-aware; pure-material fusion uses explicit
fusion temperature and latent heat.

`MaterialLotRecord` is the stored-matter authority. A lot owns stockpile, material profile, mass,
temperature/composition/particle state, provenance, and exposure. Physically relevant profile differences
prevent unsafe coalescing. Lot IDs identify persistent distinct lots, not transaction attempts: compatible
ingress, completion output, reform output, and relocation fragments bind to the identity that will survive
coalescing, and the monotonic lot cursor advances only when a distinct lot will actually persist.

### Inventory

Stockpiles own capacity, containment, preservation, inbound reservations, and derived routing/mass
indexes. Inventory is custody, not movement authorization. Runtime movement requires a canonical owner that
binds exact ingress, egress, reform, or reserved-output consequences. Generic stockpile transport has no
runtime authorizer. The `test-gameplay` harness may inject one controlled conserved delivery as setup/event
infrastructure; this does not grant general transport authority. Same-material reform is valid only when the
commodity form changes without changing material phase; input already entirely in the target commodity is
rejected. Reform preserves temperature, composition, and particle state, so phase transitions remain owned
by explicit thermal processing rather than by inventory relabeling.

Supported stockpiles contribute `StructuralLoadKind::StoredMatter`. Every canonical stored-mass mutation
updates inventory ownership and the resulting structural load atomically.

### Geology and knowledge

Geological deposits are a separate finite matter owner. They contain spatial bounds, material profile,
excavation hardness, provenance, remaining mass, and lifecycle. Player-facing code cannot enumerate
hidden deposit truth.

Geological knowledge is a separate persisted owner. Observations contain authorized spatial evidence and
bounded abundance estimates, not deposit identity. Recording requires an opaque `ProspectingResolution`;
assessment combines only acquired evidence and preserves contradiction or spatial incomparability.

### Prospecting and mining

Local prospecting is exclusive `PlayerWorkState` labor over one voxel. Field inspection produces coarse
surface-abundance evidence. Detailed field survey costs more time and survival reserve for narrower evidence.
Start validation checks method, known material, spatial limit, duration, and survival budget. Completion
uses hidden geology internally to derive authored uncertainty bounds, records one observation, and exposes no
deposit identity or count. Overlapping observations combine through `GeologicalKnowledgeState`; empty ground
also produces bounded evidence. In-progress prospecting persists and validates as player work.

Mining target resolution converts compatible acquired evidence into an opaque deposit-bound authorization.
Resolution fails when evidence is absent, contradictory, spatially incomparable, excludes the material,
remains too uncertain, or still matches multiple live deposits. Hidden geology is never used as a public
tie-break. A resolution binds geology and knowledge revisions, and mining rechecks them before admission.
Public mining state does not expose deposit identity, exact hidden remaining mass, pre-claim composition, or
exact target hardness.

Mining transfers exact geological matter into `MiningState` after target, tool, labor, capability, wear,
destination, and reservation validation. Completion releases work occupancy; claim transfers the owned output
to inventory.

## Production and processing

### Production jobs

`ProcessDefinition` owns immutable identity, material requirements, and typed capability requirements.
Operation-specific physics belong in resolver outputs, not static duration/yield fields.

`ProcessResolution` describes one concrete operation: exact selected inputs, duration, output streams, and
finite energy/equipment consequences. Production reserves output capacity at start and owns consumed
matter plus modeled energy while work is in flight. Completion is revision-bound and must preserve exact
represented matter and modeled energy across all streams.

Manual shaping conserves material identity and mass, preserves input temperature, cannot change phase,
and only authors output forms whose particle-size state is untracked. A particulate output requires an
operation with an explicit output particle-size distribution rather than an underspecified hand recipe.

Required equipment support or reserved-output support may suspend a production job when that support
becomes unavailable. Suspension preserves work-in-process, reservations, and exact remaining active time.
Suspended manual crafting releases `PlayerWorkState`. Resumption reacquires player labor through the normal
attention and survival-budget admission boundary. Without available labor, the job remains suspended and
consumes no exertion or active process time.

### Physical resolvers

Implemented resolvers are:

- **Comminution:** authored feed/output particle state, condition-sensitive throughput, batch limits,
  finite work energy, power-limited duration, and active-tick wear;
- **Dry screening:** partitions fully resolved particle classes around an authored aperture without
  inventing fractional or unresolved splits;
- **Constituent separation:** handles physically liberated particulate feed. Binary definitions may restrict feed
  to one authored target plus one authored residue material; concentration definitions accept arbitrary
  non-target gangue without composition-specific recipes. Output mass is derived from exact selected
  composition rather than a fixed yield. Concentration authors distinct target and lower non-target
  recoveries, so product grade emerges from feed assay and separator selectivity rather than perfect
  gangue rejection. Fractional component remainders are deterministically distributed across blended
  particulate lots so represented constituent content remains exact. Separation cannot consolidate matter:
  target outputs remain loose, while concentrate and residue retain input particulate state. Persisted jobs
  replay composition, streams, energy, duration, and wear;
- **Thermal processing:** sensible heating, pure-material melting, and pure-material casting use real
  selected matter, finite energy sources/sinks, equipment limits, phase boundaries, and latent heat. Melting
  and casting definitions bind authored input and output forms; admission and persisted replay cannot
  substitute a different form solely because its material and phase are compatible. Casting outputs are
  non-particulate solids because the current casting resolver does not invent a particle-size distribution.

## Equipment, labor, survival, energy, and fluids

### Equipment and maintenance

Capabilities use explicit typed values and `AtLeast`/`AtMost` requirements. Equipment providers expose
runtime condition-adjusted capabilities through the same evaluation boundary as nominal definitions.
Failed equipment exposes no productive capability.

Equipment owns persistent identity, condition, embodied traces, occupancy, and optional structural
installation. Fixed machinery requires an active support before it can authorize new work; portable tools
remain usable without one. Mounted equipment contributes equipment-owned structural load.

Assembly consumes exact material traces. Additive upgrades preserve identity, condition, and existing
traces while adding authored matter. Pristine idle unmounted equipment may disassemble to exact traces;
worn recovery, where authored, reforms traces into a same-material recovery form that cannot immediately
reset wear through reassembly. Exact assembled equipment does not currently author aggregate maintenance:
component replacement would need to swap the corresponding embodied traces rather than convert incoming
replacement stock directly into spent material.

Maintenance consumes an exact replacement commodity, produces a distinct conserved spent form with the
same material phase and particle-state policy, and restores the authored condition target. It is a physical
material reform for the currently untraced maintainable machinery, not condition-only mutation; phase or
particle transformations require their owning processes.

### Player work and survival

`PlayerWorkState` is exclusive across manual crafting, field prospecting, hand mining, and direct manual
power. Work admission binds projected metabolic-energy and hydration cost. Suspended manual production
does not reserve player attention; resumption must pass the same admission again for its exact remaining
active time. Successful active work consumes the corresponding physiological budget.

Direct player power uses a real portable unmounted power provider and finite compatible destination
store. Duration respects provider/store transfer limits and sustainable metabolic output; physiological
cost scales to actual mechanical work at authored efficiency. Energy creation and equipment wear commit
together.

Survival tracks metabolic energy, hydration, vitality, and category-specific recent nutrition. Eating and
drinking consume exact physical portions into terminal conservation owners; physiological gains clamp
independently to authored reserve capacities. Vitality recovery is limited by the weakest
Grain/Fruit/Protein reserve. Persisted fixed-point carry preserves fractional recovery between ticks; the
read-only assessment rounds the exact rate for presentation. Consumption that improves no reserve is
rejected before finite resources are consumed.

### Energy and fluids

Energy stores own carrier, capacity, directional power envelopes, stored energy, identity, revision, and
optional embodied traces. Runtime owners consume or supply energy through their own validated reservations;
direct manual power is an explicit generator. Generic store-to-store transfer is not authorized because no
physical path, carrier conversion, or transfer consequence is modeled.

Fluid stores own identity, volume, temperature, capacity, revision, and optional support. One underlying
material has at most one fluid identity while composition, contamination, concentration, and phase-mixture
state are absent; distinct IDs cannot stand in for unmodeled fluid properties. Runtime operations support
exact finite withdrawal and support changes. There is no generic inter-store transfer, pumping, or mixing
path, so cross-store movement and mixing require an explicit owning system. Supported fluid load derives
from authored material density; canonical withdrawal updates that load. Fluid temperature prevents
thermally incompatible contents from being treated as interchangeable, but finite-fluid thermal transport
and the thermal fate of consumed fluid are not yet modeled in the explicit-energy conservation ledger.

## Structures

Structural members own geometry, topology, embodied material, self-weight, external source-separated
loads, lifecycle, and damage. Analysis models axial tension/compression and deterministic
stable/strained/cracked/failed transitions with support-loss cascades. Support edges require touching or
overlapping voxel bounds; sub-voxel joint geometry is outside the model.

The gameplay-audit fixture may materialize a planned member from exact conserved inventory traces after
validating geometry-derived mass, consolidated form, composition, source capacity, and self-weight. This is
setup infrastructure, not a player construction action. Embodied matter has no generic deletion path; any
demolition/recovery operation must explicitly model authorization, labor/tools/time, and conserved salvage.

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

## Trusted load

`validate_loaded_state(registries, state)` is the exhaustive trusted-load boundary. It recomputes rather
than trusting cached claims. Admission:

1. validates save and registry versions;
2. rebuilds derived indexes;
3. validates each local owner and all authored/runtime references;
4. validates cross-owner occupancy, reservations, support, provenance, and ownership;
5. replays operation-specific physical outcomes where persisted work depends on them;
6. returns `AppState` only after the complete graph is valid.

Cross-owner validation covers, as applicable:

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
