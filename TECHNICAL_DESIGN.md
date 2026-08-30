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
| `SurvivalState` | Metabolic energy, hydration, vitality, nutrition, fractional vitality-recovery carry, terminal consumed matter/fluid totals, pending direct-consumption custody |

Cross-owner operations coordinate owner APIs; they do not mutate another owner's private storage directly.

## Physical quantities

| Type | Unit | Storage |
| --- | --- | --- |
| `Mass` / `AggregateMass` | milligram | `u64` / `u128` |
| `Temperature` | absolute millikelvin | `u32` |
| `Energy` | nanojoule | `u128` |
| `PreciseEnergy` | derived nanojoules + femtojoule remainder | `u128` + normalized `u32` |
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
owners rather than cached totals. Finite energy stores transact authoritative whole-nanojoule `Energy`.
Read-only derived thermal accounting uses `PreciseEnergy` when material composition or fractional-milligram
fluid mass can imply sub-nanojoule energy; narrowing to `Energy` is allowed only when the exact femtojoule
remainder is zero.

## Materials, inventory, and geology

### Materials and lots

Materials are immutable definitions. Forms define phase, particle-state policy, and physical cohesion.
Only consolidated non-particulate solids may directly become rigid infrastructure components or structural
embodiment; loose forms require an explicit shaping or consolidation process first. Infrastructure embodiment
does not carry stockpile preservation history, so a commodity authored as food cannot appear in equipment,
energy-store, or storage-enclosure assembly/upgrade inputs until embodied perishability aging is modeled.
`CommodityKey` combines one material and one form, but runtime ownership is limited to exact pairs explicitly
authored in the material registry; independently valid material and form IDs do not imply a valid commodity. Composition
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

Built-in ordinary storage deliberately exposes a preparation tradeoff at equal 20 kg usable capacity. The
lidded timber chest embodies 2.4 kg of joined boards and provides a 2x preservation multiplier; its full raw
route consumes three 1 kg logs, emits 0.6 kg of chips, and occupies 230 player-attention ticks. The double-wall
chest embodies 4.0 kg, provides a 3x preservation multiplier, consumes five logs, emits 1.0 kg of chips, and
occupies 370 ticks. Both retain the same 333.15 K containment ceiling, so heavier joinery improves future food
preservation without acting as extra capacity or high-temperature containment.

Enclosure dismantling is the inverse custody transition for that exact embodied matter, not generic demolition.
The target must be unmounted, have no reserved inbound work, and remain valid under the ambient storage profile.
Retained lots checkpoint exposure under the enclosure's current preservation multiplier before the profile
reverts, so dismantling cannot reset or improve food age. The enclosure traces then enter a distinct recovery
stockpile with their exact temperature, composition, particle state, and provenance. Recovery capacity, lot-ID
space, inventory revisions, and any destination structural-load increase are prevalidated before mutation. This
transition does not itself model player dismantling labor, tools, or duration.

Detached timber bodies may then be reused intact or entered into explicit manual salvage. Standard-body salvage
occupies 70 player-attention ticks and reforms 2.4 kg of body into 1.6 kg boards plus 0.8 kg chips; double-wall
salvage occupies 100 ticks and reforms 4.0 kg into 3.2 kg boards plus the same 0.8 kg chip residue. The one-board-
equivalent loss makes reconfiguration costly without deleting matter. Recovered boards can immediately feed the
ordinary enclosure joinery chain, so a dismantled double-wall chest can become a standard chest while leaving
0.8 kg reusable boards and 0.8 kg represented chips.

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
applies wear, releases player work, and creates an explicit durable claim boundary. Completed output remains
mining-owned with its destination capacity reserved until claim succeeds, so unrelated simulation time and work
can continue while a blocked claim is repaired without losing, duplicating, or silently storing matter.

## Production and processing

### Production jobs

`ProcessDefinition` owns immutable process identity, material requirements, and typed capability requirements.
Resolver-owned equipment capability IDs also appear as `AtLeast` process requirements so generic provider
discovery and resolver admission use the same capability dimensions. Operation-specific duration, yield,
energy, wear, and dynamic batch limits belong in resolver output.

`ProcessResolution` binds one concrete operation to exact selected inputs, duration, output streams, and finite
resource consequences. Production reserves output capacity at start and owns consumed matter and modeled
in-process energy until completion. Inherited material storage exposure remains wall-clock based while work is
in process, including any suspension interval, so a blocked operation cannot preserve perishable matter merely
by remaining incomplete. Completion is revision-bound and conserves represented matter and modeled energy
across all streams.

Manual shaping conserves material identity and mass, preserves temperature, cannot change phase, and only emits
forms whose particle-size state is untracked. Particulate output requires an owner that defines particle-size
state. `chip` and `scrap` outputs remain represented matter; no owner may reinterpret them as fuel or fresh
components without an explicit recovery process. Clean built-in copper scrap has two such routes: a slower
manual cold-work process reforms an exact reinforcement mass without phase change, while pure-copper melting
accepts authored copper ingot, reinforcement, native-metal, and scrap forms and resolves all of them through the
same conserved fusion physics. Pure stone scrap has a separate cold reknapping route: one 1 kg batch produces
0.8 kg consolidated stone tooling plus 0.2 kg stone chips in 60 attention ticks, compared with 40 ticks when
starting from a fresh 1 kg lump. Reknapping preserves the selected scrap temperature and cannot combine lots at
different temperatures because no thermal-mixing owner exists. This does not make wood scrap or stone/wood chips
fresh components; those remain represented terminal matter in the current ordinary loop. Nor does it make ore,
crushed ore, or concentrate meltable; those feeds still require a separate reduction/smelting owner that is not
implemented.

Loss of required equipment or output support may suspend a job. Suspension preserves work-in-process,
reservations, and exact remaining active time. Production schedules also persist completed wall-clock suspension
time so trusted load can replay `completes_at = started_at + active_duration + completed_suspension_time` instead
of trusting an arbitrary due tick; the currently open suspension is excluded until resume. Suspended manual
production releases `PlayerWorkState`; resumption must reacquire labor and pass the remaining survival-budget
admission.

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
  energy sources/sinks, equipment limits, phase boundaries, and latent heat. Ppm-weighted sensible heat is first
  resolved at femtojoule precision; a runtime transfer that is not exactly representable in whole nanojoules is
  rejected rather than rounded down, while read-only material thermal accounting retains the remainder. Each
  pure phase-change definition
  binds one exact authored material rather than inferring material identity from the first selected lot. Melting
  owns a canonical nonempty set of accepted solid feed forms for that material and one liquid output form, so
  physically equivalent recovery feeds can share one fusion resolver without recipe aliases. Casting binds one
  liquid-to-solid form pair for its exact material and also owns the completed-solid temperature. Persisted jobs
  replay the same material, accepted forms, and physical resolution used at admission.

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

Built-in primitive copper reinforcement uses that additive path for extraction, manual power, crushing, and
separation equipment. Reinforced processors increase authored flow and maximum batch capability without
replacing the machine instance. Their condition curves degrade both productive flow and safe batch capacity;
component service replaces only the stone working component and leaves copper reinforcement embodied. Worn
disassembly returns the copper trace as copper scrap, which can re-enter the manual reinforcement-recovery route.
Stone working-component service emits the exact replaced mass as stone scrap. Once at least 1 kg of compatible
pure scrap has accumulated, manual reknapping can produce another exact 0.8 kg pick/separator component. The
remaining scrap and produced chips stay represented, so repeated service reduces but does not eliminate fresh
stone demand.

### Player work and survival

`PlayerWorkState` allows at most one active player-attention operation across manual production, prospecting,
mining, manual power, eating, and drinking. Work admission binds the required metabolic-energy and hydration
budget. Suspended manual production releases attention; resumption must reacquire it and revalidate the exact
remaining budget.

Direct manual power requires portable unmounted equipment and a compatible finite energy destination. Duration
is limited by provider capability, destination input power, sustainable metabolic output, and requested work.
Energy creation, physiological cost, and equipment wear share one validated operation.

`SurvivalState` owns metabolic energy, hydration, vitality, recent nutrition, terminal consumed matter/fluid
totals, and exact pending direct-consumption custody. Eating and drinking transfer selected physical quantities
into survival ownership at admission, then release their physiological energy, hydration, and nutrition over
the authored exclusive-attention interval. Uptake is allocated from cumulative integer fractions so intermediate
ticks cannot receive future benefit and the final tick recovers every whole-unit remainder. Each installment is
capped against the capacity remaining after that tick's basal/exertion expenditure. If starting reserves cannot
fully pay that expenditure, the installment first pays the exact energy/hydration shortfall; only its residual may
refill stored reserves, and starvation/dehydration damage is applied only when the installment cannot cover that
shortfall. Nutrition intake is available for same-tick recovery before decay. Admission therefore does not require
pre-action reserve headroom, because physiological expenditure during consumption can create capacity. A drink is
rejected only when its selected volume resolves to no whole-unit hydration benefit. If the player dies while an
intake is pending, no further physiological benefit is released; pending survival custody and its eating/drinking
attention record are canceled together on the next authoritative tick.
Current-schema saves persist pending intake identity and timing, and trusted load validates that custody against
the matching player-work interval and cumulative consumed accounting.

Diet quality is limited by the weakest Grain/Fruit/Protein reserve. Fractional vitality recovery is persisted;
read-only assessment exposes a rounded presentation rate.

### Energy and fluids

Energy stores own carrier, capacity, directional power limits, stored energy, revision, optional embodied
traces, and optional passive dissipation. Runtime consumers/producers act through validated owner operations.
Passive dissipation is an environmental/loss sink, not controllable output power. Its authored rate must
integrate to exact whole nanojoules per tick. Tick execution derives loss from the pre-tick store snapshot and
applies it after same-tick ingress. Generic store-to-store transfer is absent because no transfer path or
carrier-conversion owner exists.

Material-backed energy stores may define one additive upgrade from another store definition. Registry assembly
requires the target carrier to match the base, capacity and transfer limits not to regress, passive loss not to
increase, and the target assembly to equal the base assembly plus the authored additions exactly. Runtime
upgrade requires an empty store with no production or direct-manual-power occupancy, consumes the addition
traces from inventory, preserves store identity and original creation time, and advances the energy revision.
Commit rechecks both energy state and occupancy because player-work reservation can change without changing the
energy revision. Trusted load accepts post-construction embodiment only up to the cumulative authored additions
along the current definition's upgrade ancestry, while still requiring the exact target assembly, valid material
state, and nonfuture provenance.
Disassembly remains the inverse exact-custody route for empty, idle stores.

The built-in copper-banded stone flywheel adds one 20 g copper reinforcement to the ordinary 900 g stone plus
200 g wood accumulator. It keeps the base 150 W input limit, 500 W output limit, and 0.05 W passive loss but
raises stored-work capacity from 500 J to 750 J. That reserve is directly usable by primitive processing: the
built-in crusher requires 1 J per gram, so a fully charged upgraded flywheel can cover a 750 g crushing charge
that cannot fit in the base accumulator.

Fluid stores own identity, volume, temperature, capacity, revision, and optional structural support. A material
has at most one fluid identity in the current homogeneous-fluid model. Runtime supports exact withdrawal and
support changes; generic transfer, pumping, and mixing are absent. One fluid-owner mass projection converts
represented volume and authored density to exact micrograms; structural loading and thermal accounting consume
that same physical projection rather than re-deriving density arithmetic independently. Stored-fluid sensible
energy is projected exactly from represented mass, temperature, and specific heat; when the backing material has
authored fusion properties, the ledger also includes liquid latent heat without rounding sub-nanojoule remainders. Passive fluid heat transport
remains absent. The thermal fate of food or fluid after either crosses the terminal survival-consumption boundary
remains outside the explicit-energy ledger; biological transformation, body heat, and waste streams are not yet
modeled.

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
