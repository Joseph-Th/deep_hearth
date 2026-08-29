# Technical Design

This page owns implemented subsystem and physical contracts. Use [`README.md`](README.md) for routing,
[`ARCHITECTURE.md`](ARCHITECTURE.md) for cross-cutting engineering rules, and [`STATUS.md`](STATUS.md) for
runtime scope. Source and adjacent tests own concrete edge cases and typed errors.

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
- `advance_tick` decides all fallible phase work against one pre-tick snapshot. Its application stage
  prechecks shared-owner revisions before mutation; after the completion transaction succeeds, remaining
  phase applies are infallible and assertion-backed before the clock advances.
- `CURRENT_SAVE_SCHEMA_VERSION` is the only accepted runtime payload shape; `RegistrySchemaVersion` identifies
  authored identity and physical-definition compatibility.
- Untrusted save data enters through `LoadedSaveEnvelope::into_state`; the [Trusted load](#trusted-load)
  section owns the promotion and validation contract. Raw `AppState` has no public `Deserialize` path.
- Save encoding and storage are adapter concerns.

## Runtime owners

`AppState` is the root of generated state. Each subsystem owns its records, generated IDs, revisions, and
synchronized indexes.

| Owner | Authoritative state |
| --- | --- |
| `InventoryState` | Stockpiles, material lots, reservations, routing, preservation, material-backed storage enclosures, stockpile support |
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

Cross-owner operations coordinate owner APIs; they do not mutate another owner's private storage directly.

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
| `Volume` / `AggregateVolume` | microliter | `u64` / `u128` |
| `MassSpecificEnergy` | nanojoule/milligram | `u64` |
| `MassFlow` | milligram/second | `u64` |

Potentially overflowing arithmetic is checked. Conservation-sensitive systems account from authoritative
owners rather than cached totals.

## Materials, inventory, and geology

### Materials and lots

Materials are immutable definitions. Forms define phase, particle-state policy, and physical cohesion.
Only consolidated non-particulate solids may directly become rigid infrastructure components or structural
embodiment; loose forms require an explicit shaping or consolidation process first. `CommodityKey` combines
one material and one form, but runtime ownership is limited to exact pairs explicitly authored in the
material registry; independently valid material and form IDs do not imply a valid commodity. Composition
remains a separate exact property. Authoring a liquid commodity requires fusion properties for its material,
so the registry cannot contain a liquid identity for which no physically valid runtime temperature exists.

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

Stockpiles own capacity, containment, preservation, optional enclosure identity, inbound reservations, and
derived routing/mass indexes. Inventory owns custody, not general movement authorization. Runtime movement
requires a canonical owner that binds exact ingress, egress, reform, relocation, or reserved-output effects.
General stockpile transport is not implemented. The gameplay harness may authorize controlled conserved
transfers only as setup or controlled-event infrastructure.

Same-material reform may change form without changing material phase. It preserves temperature, composition,
and particle state; phase transitions remain owned by thermal processing.

Storage enclosures are immutable definitions with a capacity limit, storage profile, and exact consolidated
assembly profile. Construction transfers selected traces from inventory into persistent stockpile-owned
enclosure matter. Before commit, every existing lot must satisfy the completed enclosure's phase, temperature,
material-phase, and particle-state containment rules. Existing lots checkpoint accumulated exposure before the
new preservation multiplier takes effect, so improved storage affects future spoilage only. Trusted load
validates enclosure definition, construction time, storage profile, and embodied traces.

Supported stockpiles contribute `StructuralLoadKind::StoredMatter` for stored contents plus enclosure matter.
Stored-mass mutations and their structural-load consequences commit atomically.

### Geology and knowledge

Geological deposits are a separate finite matter owner. They contain spatial bounds, material profile,
excavation hardness, provenance, remaining mass, and lifecycle. Player-facing code cannot enumerate
hidden deposit truth.

Geological knowledge is a separate persisted owner. Observations contain authorized spatial evidence and
bounded abundance estimates, not deposit identity. Recording requires an opaque `ProspectingResolution`;
assessment combines only acquired evidence and preserves contradiction or spatial incomparability.

### Prospecting and mining

Prospecting is exclusive `PlayerWorkState` labor over an authored method and bounded region. Completion may
read hidden geology only to produce a bounded `GeologicalObservationRecord`; actor-visible output never exposes
deposit identity or exact hidden state. Acquired observations combine through `GeologicalKnowledgeState`.
In-progress prospecting persists as player work.

Mining target resolution converts sufficiently precise, compatible acquired evidence into opaque extraction
authorization. Resolution rejects absent, contradictory, spatially incomparable, insufficiently localized, or
ambiguous evidence. Querying a smaller region cannot create precision that was not acquired. Hidden geology is
never a public tie-breaker.

Mining start validates authorization, tool, labor, capability, wear, destination, and reservation constraints.
Geology retains ownership of the selected batch during labor. Completion removes the batch from geology,
applies wear, releases player work, and creates an explicit claim boundary. Time cannot advance while completed
mining output remains unclaimed; claim failure can therefore be repaired without losing or duplicating matter.

## Production and processing

### Production jobs

`ProcessDefinition` owns immutable process identity, material requirements, and typed capability requirements.
Resolver-owned equipment capability IDs also appear as `AtLeast` process requirements so generic provider
discovery and resolver admission use the same capability dimensions. Operation-specific duration, yield,
energy, wear, and dynamic batch limits belong in resolver output.

`ProcessResolution` binds one concrete operation to exact selected inputs, duration, output streams, and finite
resource consequences. Production reserves output capacity at start and owns consumed matter and modeled
in-process energy until completion. Completion is revision-bound and conserves represented matter and modeled
energy across all streams.

Manual shaping conserves material identity and mass, preserves temperature, cannot change phase, and only emits
forms whose particle-size state is untracked. Particulate output requires an owner that defines particle-size
state. `chip` and `scrap` outputs remain represented matter; no current owner may reinterpret them as fuel or
fresh components without an explicit recovery process.

Loss of required equipment or output support may suspend a job. Suspension preserves work-in-process,
reservations, and exact remaining active time. Suspended manual production releases `PlayerWorkState`; resumption
must reacquire labor and pass the remaining survival-budget admission.

### Physical resolvers

Powered ore-processing definitions share `PoweredOreProcessProfile` for throughput capability, batch limit,
energy carrier, mass-specific work, and active-tick wear. Runtime admission and trusted-load replay derive those
consequences from the shared profile; each resolver owns only its material transformation.

Implemented resolver contracts:

- **Comminution:** validates feed and output particle state, batch limits, condition-adjusted throughput,
  finite work energy, duration, and wear. Direct-labor and powered routes share the same material projection.
- **Dry screening:** partitions fully resolved particle classes around an authored aperture without inventing
  unresolved fractions.
- **Constituent separation:** applies authored target and non-target recovery to liberated particulate feed.
  Unrecovered constituents remain in physical residue. Sorting and concentration preserve exact composition,
  use deterministic remainder allocation, and emit forms that prevent unsupported repeat-processing loops.
- **Thermal processing:** sensible heating, pure-material melting, and casting use exact selected matter, finite
  energy sources/sinks, equipment limits, phase boundaries, and latent heat. Melting/casting bind authored forms;
  casting also owns the completed-solid temperature. Persisted jobs replay the same physical resolution used at
  admission.

## Equipment, labor, survival, energy, and fluids

### Equipment and maintenance

Capabilities use typed values and explicit `AtLeast`/`AtMost` requirements. Runtime providers expose
condition-adjusted capability through the same evaluation boundary as nominal definitions; failed equipment
provides no productive capability.

Equipment owns identity, condition, embodied traces, occupancy, and optional structural support. Fixed
machinery requires active support before new work starts. Mounted equipment contributes its own structural-load
channel.

Assembly consumes exact traces. Additive upgrades preserve identity, condition, and prior embodiment while
adding authored matter. Disassembly and worn recovery are allowed only through authored routes. Maintenance is
physical: aggregate replacement consumes an exact commodity and emits conserved spent matter; traced component
service replaces one complete authored component while preserving unrelated traces and upgrades. Phase or
particle transformations remain owned by their physical process.

### Player work and survival

`PlayerWorkState` allows at most one active player-attention operation across manual production, prospecting,
mining, manual power, eating, and drinking. Work admission binds the required metabolic-energy and hydration
budget. Suspended manual production releases attention; resumption must reacquire it and revalidate the exact
remaining budget.

Direct manual power requires portable unmounted equipment and a compatible finite energy destination. Duration
is limited by provider capability, destination input power, sustainable metabolic output, and requested work.
Energy creation, physiological cost, and equipment wear share one validated operation.

`SurvivalState` owns metabolic energy, hydration, vitality, recent nutrition, and terminal consumed matter/fluid
totals. Eating and drinking consume exact physical quantities. The authored direct-consumption envelope limits
quantity and derives exclusive attention duration. Drinking also rejects effective hydration above remaining
capacity. Consumption that improves no reserve is rejected before resource withdrawal.

Diet quality is limited by the weakest Grain/Fruit/Protein reserve. Fractional vitality recovery is persisted;
read-only assessment exposes a rounded presentation rate.

### Energy and fluids

Energy stores own carrier, capacity, directional power limits, stored energy, revision, optional embodied
traces, and optional passive dissipation. Runtime consumers/producers act through validated owner operations.
Passive dissipation is an environmental/loss sink, not controllable output power. Its authored rate must
integrate to exact whole nanojoules per tick. Tick execution derives loss from the pre-tick store snapshot and
applies it after same-tick ingress. Generic store-to-store transfer is absent because no transfer path or
carrier-conversion owner exists.

Fluid stores own identity, volume, temperature, capacity, revision, and optional structural support. A material
has at most one fluid identity in the current homogeneous-fluid model. Runtime supports exact withdrawal and
support changes; generic transfer, pumping, and mixing are absent. Supported-fluid load derives from authored
density and updates with canonical withdrawal. Fluid thermal transport and consumed-fluid thermal fate are
outside the explicit-energy ledger.

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

`LoadedSaveEnvelope::into_state(registries)` is the public decoded-save promotion boundary and
`validate_loaded_state(registries, state)` is its exhaustive graph validator. Raw `AppState` does not
implement public deserialization, so adapters cannot bypass schema/reconstruction checks by decoding the
runtime root directly. Validation recomputes rather than trusting cached claims. Admission:

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
