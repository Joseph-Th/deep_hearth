# Technical Design

**Role:** Implemented subsystem and physical-contract authority.

Use [`README.md`](README.md) for routing,
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
| Cross-owner custody, reservations, and continuation | Cross-owner edge atlas |
| Definitions, reads, planning, and command surface by subsystem | Subsystem control index |

## System control model

Implemented systems exchange a small set of authoritative flows:

| Flow | Contract |
| --- | --- |
| Matter | Moves through explicit custody; creation/destruction requires an authored physical source/sink. |
| Fluid | Finite stores own volume; withdrawal/consumption is explicit. Generic transport is absent. |
| Energy | Finite stores and work records own stored/reserved energy; losses are explicit sinks. |
| Labor | `PlayerWorkState` owns exclusive attention; survival supplies physiological work budget. |
| Information | Hidden geology becomes actor-usable only through acquired `GeologicalKnowledgeState`. |
| Support/load | Structures own support; endpoint owners contribute source-separated load. |
| Capacity/time | Reservations, occupancy, and persisted schedules protect delayed work from double booking. |

Trace cross-system mechanics flow-by-flow from owner to owner. Each delayed handoff has an explicit
custody or schedule owner.

Truth-class vocabulary and the resolve/validate/commit control grammar are owned by
[`ARCHITECTURE.md`](ARCHITECTURE.md#agent-legible-control-grammar). This page names the concrete owners, flows,
operations, and persistence obligations that implement those roles.

### Planning topology

`CraftingRegistry` owns deterministic reverse indexes for manual producer/consumer relationships.
`Registries::new` derives and validates cross-registry `ProcessTopology`: one execution family per process,
typed equipment and energy roles, nominal provider definitions, and compatible energy stores. Equipment/energy
assembly, upgrade, maintenance, and recovery relationships remain with their domain definitions.

Keep these claims separate:

1. **Authored possibility:** immutable definitions and deterministic reverse indexes.
2. **Current state:** authoritative ownership, custody, condition, support, knowledge, reservations, and schedules.
3. **Current opportunity:** request-scoped production resolution over legitimate observable state.
4. **Authorization:** validation against the current mutable dependencies.

Topology queries are exhaustive and stably ordered for their declared narrow key. A bounded current-state
search must expose its bound and must not turn an incomplete search into a claim of unavailability. Derived
topology rebuilds from validated definitions and is not persisted as world truth.

Powered ore and thermal planning envelopes are disposable feasibility evidence over the same physics used by
their exact resolvers. Exact selected matter, destination legality, current provider state, and mutation
authorization remain with the canonical resolver/validator path.

### Temporal stepping contract

`advance_tick(registries, state)` is the authoritative time mutation. A batching helper may execute that
operation repeatedly up to a declared horizon and may stop on declared observable events, but it must preserve the
same state and ordered `TickOutcome` sequence as repeated calls.

A known due tick is a bound, not permission to skip intervening semantics. Semantic fast-forward requires a
separate authoritative transition with proved equivalence across every skipped phase.

### Cross-owner edge contract

Each implemented cross-owner handoff has these contract elements where applicable:

- source and destination owners plus stable identities;
- canonical admission boundary;
- exact quantity/relationship transferred or reserved;
- capacity, exclusivity, support, information, and lifecycle prerequisites;
- mutable dependencies bound against staleness;
- custody/schedule owner while delayed;
- atomic rejection boundary, typed committed outcome, and trusted-load obligation.

The [cross-owner edge atlas](#cross-owner-edge-atlas) records implemented edge families rather than every
operation over an existing edge.

## Subsystem contract card

The control index below routes each subsystem through immutable definitions, authoritative reads, planning or
resolution, and canonical mutation/continuation. Source and adjacent tests own concrete edge cases and typed
errors.

### Subsystem control index

This is a routing index, not a duplicate behavior specification. Use it to find the correct abstraction level,
then read the owning section/source for exact semantics and errors.

| Surface | Definitions / immutable input | Authoritative read / observation | Plan / resolve | Mutate / continue |
| --- | --- | --- | --- | --- |
| Root simulation and time | core definitions, typed quantities/time | `AppState::tick()`, immutable owner accessors | per-phase `decide_*` is crate-owned orchestration | `advance_tick`; direct clock/owner applies stay crate-private |
| Registries and built-in content | `Registries` plus domain registries; `build_registries` validates cross-references | public immutable `Registries::*()` accessors; derived authored topology may provide goal-directed reverse lookup | callers inspect authored possibilities; topology never claims current legality or ordinary reachability | none; registries and derived definition indexes are immutable after construction |
| Inventory and storage | material/form/storage definitions | `AppState::inventory()`, stockpile/lot records and stable iterators | feature owners construct explicit lot selections; enclosure validators derive storage consequences | no generic public transport command; feature-specific validators own ingress/egress/reform, enclosure, and support transitions |
| Geological knowledge | prospecting methods plus hidden finite geology | `AppState::geological_knowledge()`, `assess_geological_knowledge`, knowledge map | field prospecting authorization; evidence combination remains actor-safe | `validate_start_field_prospecting` -> tick records observations |
| Mining | `MiningRegistry` methods and physical hardness/tool constraints | `AppState::mining()` plus acquired geological knowledge; hidden `GeologyState` is not public | `resolve_mining_target` binds localized evidence and the best acquired physical hardness band when present | `validate_start_mining` uses acquired conservative hardness when present; unsampled localized attempts use hidden hardness only for opaque tool-sufficiency admission -> tick -> `validate_claim_mining_output` |
| Production | `ProductionRegistry`, `ProcessDefinition` | `AppState::production()`, job records, reservations/occupancy | operation-specific resolvers produce `ProcessResolution` / `Resolved*` | ordinary player execution uses `validate_start_player_process*` to prove exact-local endpoint/resource access before capability-level `validate_start_process*`; controlled capability probes may enter `validate_start_process*` directly -> tick completion |
| Equipment | `EquipmentRegistry`, capability/maintenance/upgrade profiles | `AppState::equipment()`, equipment records | `resolve_equipment_provider`, `resolve_equipment_maintenance` | assembly, upgrade, maintenance, disassembly, mount/unmount/relocate validators |
| Player labor | `LaborRegistry`, manual-power/prospecting definitions | `AppState::player_work()` | `project_manual_power` projects immutable future provider/store physics; `assess_manual_power_energy_envelope` and `assess_manual_power_destination_target` bind current provider/store state for exact read-only generation and post-work stored-energy planning; runtime owner commands still own authorization, attention, survival budget, and revisions | manual power/prospecting/manual production commands -> tick; attention lifecycle is crate-owned |
| Survival | `SurvivalRegistry`, physiology, food/drink definitions | `AppState::survival()`, `assess_survival`, `assess_food_freshness` | consumption validators derive bounded direct intake and physiological schedule | `validate_eat` / `validate_drink` -> tick; `initialize_player_survival` is the ordinary initialization boundary |
| Energy | `EnergyRegistry`, store definitions, carrier/power contracts | `AppState::energy()`, store records, explicit energy accounting | process/manual-power resolvers use `validate_energy_supply` / `validate_energy_sink` as part of their plan | assembly/upgrade/disassembly validators; reserved consumption/release and passive loss apply through canonical owners/tick |
| Fluids | `FluidRegistry`, fluid definitions | `AppState::fluid()`, store records, fluid accounting | consumers validate exact egress internally; no generic routing planner exists | support validators and canonical consumers; generic transfer/pumping/mixing absent |
| Structures | `StructuralRegistry`, profiles and geometry | `AppState::structures()`, `analyze_structure`, `StructuralAssessment` | owner-specific support/load validation plans final aggregate load | support/load commits through inventory/equipment/fluid/structural owners; general player construction remains absent |
| Loose surface matter | surface-resource records plus authored gathering methods | `AppState::available_surface_resources()` exposes only currently gatherable resources at the admitted player's exact voxel; complete owner access and arbitrary-voxel queries are core-only | callers select a locally observed resource and destination; gathering admission binds locality, finite source mass, destination capacity/storage, survival budget, and time | `validate_start_surface_gathering` reserves destination capacity -> tick completion creates ordinary inventory matter and depletes the finite source; controlled fixtures own generation until ordinary world generation exists |
| Manual crafting overlay | `CraftingRegistry`, manual craft definitions, optional-or-required equipment profile | inventory, survival, authored craft definitions, optional equipment instance | `project_manual_craft_hand_work` projects authored fallback duration plus the shared physiological budget without authorizing current state; `resolve_manual_craft` binds selected matter and uses fixed authored duration when fallback is permitted, otherwise requires canonical condition-adjusted MassFlow | `validate_start_manual_craft` -> equipment-reserving production job + player work -> tick |
| Powered crafting overlay | `CraftingRegistry`, `PoweredCraftDefinition` referencing one manual material transform, required MassFlow capability, carrier, specific energy, and wear | inventory, equipment, energy, production state; referenced manual transform remains material/yield authority | `resolve_powered_craft` binds exact selected matter, reuses the referenced transform's batch/output construction, resolves condition-adjusted machine throughput, and validates finite carrier-compatible stored work | `validate_start_powered_craft` first enforces player access to both stockpiles, the machine, and the energy store when player logistics is initialized, then creates an ordinary production job with reserved equipment/energy and no player-work claim -> tick |
| Ore-processing overlay | `OreProcessingRegistry`, manual/powered process profiles | inventory, equipment, energy, production state | `assess_powered_ore_mass_envelope` for one-batch current scale bounds; `project_powered_ore_order` for bounded replenished-work orders with carried wear and optional critical-band service; `resolve_comminution_process`, `resolve_screening_process`, `resolve_constituent_separation_process` for exact selected-lot legality; manual counterparts preserve an equipment-free fallback and may bind one authored condition-adjusted MassFlow tool/workstation | ordinary powered resolutions enter player-authorized `validate_start_player_process*`; capability-only evidence may enter generic production admission; manual start validators bind player work and reserve optional equipment through the same production job |
| Thermal overlay | `ThermalRegistry`, heating/melting/casting definitions | inventory, equipment, energy, production state | `assess_melting_lot_mass_envelope` and `assess_casting_lot_mass_envelope` for current homogeneous-lot scale bounds; `resolve_sensible_heating_process`, `resolve_melting_process`, `resolve_casting_process` for exact selected-lot legality | ordinary resolved work enters player-authorized `validate_start_player_process*`; capability-only evidence may enter generic production admission; tick applies outputs, wear, and energy consequences |
| Conservation/accounting | authored material/fluid/energy properties | `calculate_matter_accounting`, `calculate_fluid_volume_accounting`, `calculate_explicit_energy_accounting` | read-only reconciliation only | none; accounting never mutates or authorizes custody |
| Persistence | current save schema + registry schema | `SaveEnvelope` for output, decoded `LoadedSaveEnvelope` before trust | exact-version admission plus deterministic index rebuild/graph validation | `LoadedSaveEnvelope::into_state`; adapters own bytes/storage, not state promotion |
| Presentation definitions | texture/shader registries and authored assets | immutable definition access and deterministic bake/assembly results | deterministic renderer-neutral assembly | graphics resources/frame effects belong to adapters, outside `AppState` |

If a caller needs a surface not shown here, determine whether the missing surface belongs to the canonical owner
or whether the caller is crossing an ownership boundary it does not control.

## Global runtime facts

- `SimulationTick` is absolute world time; `TickSpan` is relative duration.
- The built-in calendar maps 24,000 ticks to 86,400 seconds; one tick is 3.6 seconds.
- Rate-authored physics integrate against physical tick duration. Per-tick gameplay costs use world ticks.
- Runtime has no stochastic owner; authoritative results derive from persisted state and explicit inputs.
- Implemented authoritative physical calculations use checked integer arithmetic, not floating point.
- Dynamic scheduled work persists as explicit records.
- Known due ticks may bound batched caller stepping, but do not authorize skipping intervening canonical tick
  semantics.
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

| Root owner | Root read boundary | Caller visibility | Authoritative state |
| --- | --- | --- | --- |
| `EnergyState` | `AppState::energy()` | public read | Finite energy stores and embodied construction traces |
| `FluidState` | `AppState::fluid()` | public read | Finite homogeneous fluid stores and support assignments |
| `EquipmentState` | `AppState::equipment()` | public read | Equipment instances, condition, embodied traces, support assignments |
| `StructureState` | `AppState::structures()` | public read | Members, topology, embodied matter, source-separated loads, damage |
| `SurfaceResourceState` | `AppState::available_surface_resources()` | public exact-player-local observation only | Finite loose world-space matter, source lifecycle, and depletion; complete owner access, arbitrary-voxel queries, and persistence remain privileged |
| `GeologyState` | `AppState::geology()` | core-only hidden truth | Finite hidden geological deposits and depletion; actor code must not enumerate this owner |
| `GeologicalKnowledgeState` | `AppState::geological_knowledge()` | public actor-safe evidence | Acquired bounded observations only |
| `InventoryState` | `AppState::inventory()` | public read | Stockpiles, material lots, reservations, routing, preservation, material-backed storage enclosures, stockpile support |
| `LogisticsState` | `AppState::logistics()` | public read | Player voxel, finite carried-stockpile identity, stationary stockpile locations, equipment locations independent of support assignment, finite-energy-store locations, finite-fluid-store locations, and world-space custody revision |
| `ProductionState` | `AppState::production()` | public read | Active jobs, schedules, routing, exclusive resource occupancy |
| `MiningState` | `AppState::mining()` | public read without hidden deposit identity | Mining work-in-process, output-claim custody, and schedules |
| `PlayerWorkState` | `AppState::player_work()` | public read | At most one active player-attention operation |
| `SurvivalState` | `AppState::survival()` | public read | Metabolic energy, hydration, vitality, nutrition, fractional vitality-recovery carry, terminal consumed matter/fluid totals, pending direct-consumption custody |

Cross-owner operations coordinate owner APIs; they do not mutate another owner's private storage directly.

This table is intentionally ordered exactly as the fields in `SystemState`; the documentation checker rejects
owner additions, removals, or drift until the atlas is updated. A top-level directory absent from this table is
not a root runtime owner merely because it contains domain logic.

Public `AppState` access to these owners is read-only. Their mutable root accessors are crate-private and are
used only by canonical owner operations and tick application. This keeps inspection cheap for callers without
turning state records into a second command surface.

### Canonical custody chains

Several high-value chains are intentionally explicit because they connect much of the simulation:

```text
geology -> working mining job -> ready mining output -> inventory lot
inventory lots -> production job custody -> routed output lots
inventory traces -> equipment / energy store / storage enclosure / structural embodiment
finite loose surface matter -> timed same-voxel gathering -> inventory lot
ground inventory lot <-> same-voxel player-carried inventory lot
inventory traces -> equipment @ player voxel -> optional structural support
inventory traces -> finite energy store @ player voxel
finite fluid store @ world voxel -> local drinking / structurally supported vessel
embodiment -> authored maintenance, disassembly, dismantling, or salvage -> inventory traces
finite energy store -> reserved/consumed process energy -> modeled work/heat or explicit loss sink
hidden geology -> bounded prospecting observation -> geological knowledge -> mining authorization
structure -> support assignment -> source-separated load -> availability/failure consequence
survival reserves + PlayerWorkState -> timed direct labor -> physical operation consequence
```

These are control paths as well as accounting paths. Planning code inspects the canonical projection at
each edge when the owner exposes one and invokes the canonical transition rather than reaching through to a
later owner.
If a legitimate caller lacks the read surface needed to control an edge without reconstructing private domain
meaning, treat that as control-surface debt under [`DIRECTION.md`](DIRECTION.md), not permission for a parallel
rules implementation.

### Cross-owner edge atlas

Use this atlas when a task is about an interaction rather than a local calculation. The named boundary is the
semantic entry point; inspect its implementation and adjacent tests before reading every endpoint owner.

| Edge | Canonical boundary | Authoritative handoff and continuation |
| --- | --- | --- |
| Hidden geology -> acquired knowledge | `validate_start_field_prospecting` -> tick | Player work owns the timed action; completion records bounded actor-safe observations in `GeologicalKnowledgeState`. Hidden deposit identity never enters the observation. |
| Acquired knowledge -> extraction authorization | `resolve_mining_target` | Produces actor-safe `MiningTargetResolution` from legitimate evidence without transferring custody or exposing hidden deposit identity. |
| Geology + equipment + labor -> mining work | `validate_start_mining` -> tick | Admission binds requested effort, tool, destination, labor, logistics, and output reservation in `MiningJobRecord`; completion transfers extracted matter from geology into mining claim custody. |
| Mining claim custody -> inventory | `validate_claim_mining_output` | Ready-output custody moves to inventory and `MiningClaimReceipt` reports the exact contribution plus its merge-aware surviving lot identity. |
| Ground inventory <-> player-carried inventory | `validate_pickup_from_ground` / `validate_drop_to_ground` | Logistics proves same-voxel access; inventory owns exact selected-lot relocation and resulting lot/storage semantics. |
| Inventory + providers -> production work | resolver-specific `Resolved*` / `ProcessResolution` -> `validate_start_process*` | Start moves exact input into production work-in-process and binds output reservations, provider occupancy, finite-energy consequences, and site coherence in the durable job. |
| Loose surface matter -> inventory | `AppState::available_surface_resources` -> `validate_start_surface_gathering` -> tick | Public discovery is exhaustive only for the admitted player's current voxel; callers cannot scan arbitrary coordinates. Admission requires player/source/destination locality, reserves exact destination capacity, and records timed player work. Completion transfers the admitted mass into ordinary inventory and decrements the finite source; interrupted work releases its reservation without creating matter. Fixture-only source generation is not ordinary world-source reachability. |
| Production work -> inventory / equipment / energy | `advance_tick` completion | Completion routes exact outputs, applies equipment/energy consequences, releases occupancy/reservations, and emits `ProcessCompletion` with inventory landing identities. |
| Fatal survival -> active player work | `advance_tick` fatal-work planning | Work due on the fatal tick completes first. Otherwise each work owner applies its declared suspend/cancel/release semantics, settles physical equipment consequences for elapsed active ticks, and preserves represented custody plus trusted-load validity. |
| Inventory -> equipment embodiment | `validate_assemble_equipment` / `validate_upgrade_equipment` | Exact material traces become equipment embodiment; assembly establishes world custody when logistics exists, while upgrade preserves equipment identity/location and adds the authored trace. |
| Equipment embodiment -> inventory recovery/service | `validate_disassemble_equipment` / `validate_equipment_maintenance` | Disassembly returns authored recovery and removes equipment custody. Maintenance commits its material exchange at admission; player work owns the service interval, interruption retains proportional completed recovery, and scheduled completion reaches the full service target. |
| Inventory -> finite energy-store embodiment | energy-store assemble/upgrade/disassemble validators | Exact traces cross between inventory and `EnergyState`; lifecycle operations preserve or retire identity/location according to the validated transition. |
| Enclosure matter <-> storage profile | storage-enclosure build/dismantle validators -> tick | Inventory remains matter owner; timed dismantling checkpoints exposure, restores ambient storage, and lands exact enclosure matter in the reserved recovery destination. |
| Inventory/equipment/fluid -> structural load | owner-specific support validators | Endpoint owner retains object custody; `StructureState` owns source-separated load and support consequences after final aggregate-load validation. |
| Player physiology + equipment -> stored mechanical work | `validate_start_manual_power` -> tick | Player work owns pending generation; active ticks incur physiology and equipment wear, while completion deposits the exact admitted work in `EnergyState`, releases attention, and emits `ManualPowerOutcome`. |
| Inventory/fluid -> survival consumption | `validate_eat` / `validate_drink` -> tick | Accepted matter/fluid enters pending survival custody; timed installments grant physiological benefit and terminal totals retain represented consumed custody. |
| Authoritative owners -> accounting | matter/energy/fluid accounting functions | Read-only reconciliation derives totals from owners and never authorizes or stores custody. |

Operations that share ownership, custody, and failure semantics reuse the same edge contract.

#### Destination landing identity

Inventory ingress already resolves merge-aware persistent identity. `apply_material_ingress` returns one
surviving `MaterialLotId` per admitted parcel, reusing an existing identity when compatible matter coalesces.
Reserved delayed ingress uses the same rule: `apply_reserved_deposits` returns one inventory-owned receipt per
reserved request, containing one surviving identity per admitted parcel in request order.

Production completion composes those receipts into `ProcessOutputLanding` values keyed by stream/destination;
each `ProcessParcelLanding` pairs the exact `MaterialLotSpec` contribution with the surviving lot identity.
Mining claim likewise returns its exact claimed `MaterialLotSpec` plus one merge-aware surviving identity in
`MiningClaimReceipt`. A landing identity may therefore refer to an existing lot when compatible matter
coalesced. Other delayed custody edges needing the same answer propagate this inventory-owned result
rather than infer a new lot from pre/post stockpile contents or mint a coordinator-owned identity.

For multi-stream/multi-parcel production, preserve correspondence between `(job, stream, parcel contribution)`
and the surviving lot identity even when several contributions resolve to one lot. The contribution's mass and
material profile remain those of the authoritative resolved output; the landing identity identifies the durable
inventory record that now contains that contribution.

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

Materials and forms are immutable definitions. A runtime commodity exists only for an explicitly authored
`CommodityKey`; independently valid material and form IDs do not imply a valid pair. Forms declare phase,
particle-state policy, and cohesion. Rigid infrastructure inputs must be consolidated non-particulate solids.
Materials with an authored edible form are excluded from infrastructure embodiment until embodied perishability
is modeled.

`MaterialComposition` is an exact normalized mass-fraction vector. Mixed matter preserves composition rather
than inventing synthetic materials. Particulate state uses validated non-overlapping size classes; thermal state
is phase-aware.

`MaterialLotRecord` owns stored matter: stockpile, commodity/profile, mass, temperature, composition, particle
state, provenance, and storage exposure. Coalescing is allowed only when physically relevant profiles are
compatible. Lot IDs identify persistent distinct lots; merge-aware ingress returns the surviving identity and
allocates a new ID only when a distinct lot persists.

### Inventory

Stockpiles own finite capacity, containment, preservation profile, optional material-backed enclosure,
inbound reservations, and derived indexes. Inventory owns material custody; feature owners authorize movement via
explicit ingress, egress, reform, relocation, or reserved-output operations.

Same-material reform may change form without changing phase and preserves temperature, composition, particle
state, provenance, and storage history. Phase change belongs to thermal processing.

Storage exposure is checkpointed when the effective preservation multiplier changes. Equivalent physical
histories remain equivalent across relocation, reform, split/merge, and enclosure changes. Edible-material
history survives non-edible intermediate forms. Freshness projections use the same rational history arithmetic
as authoritative aging and are disposable read-side evidence, not construction/admission authority.

Material-backed storage enclosures own exact consolidated assembly traces and an authored storage profile.
Construction moves exact traces into stockpile embodiment after verifying current contents fit the completed
enclosure. Dismantling is timed player work: the enclosure remains active until completion, then exposure is
checkpointed, ambient storage is restored, and exact enclosure traces land in the reserved recovery destination.
General demolition is a different, absent authority.

Supported stockpiles contribute stored matter plus enclosure matter as structural load. Stored-mass and support
consequences commit atomically.

### Logistics

`LogisticsState` owns the player's voxel, the inventory-owned carried stockpile identity, stationary stockpile
locations, equipment locations, energy-store locations, fluid-store locations, and a monotonic revision. It does
not duplicate lot contents, equipment condition, stored energy/fluid, or capacities.

Player-context operations use shared access checks: carried inventory is implicitly local; other targeted
stockpiles/equipment/energy/fluid stores require an explicit location at the player's voxel once player logistics
is initialized. Controlled capability fixtures may remain locationless only when they are not claiming ordinary
player access.

Ground pickup/drop requires player and loose stockpile at the same voxel and delegates exact matter relocation to
inventory. Mount/unmount changes support without teleporting an existing location. Assembly establishes player-
local world custody for newly created equipment/energy stores; disassembly removes that custody.

Prospecting requires the player inside the survey region and any instrument at the player voxel. Mining requires
the player inside the acquired resolved target region plus local tool/output access. Manual power requires local provider
and destination store. Production uses an actor-independent same-site rule: every explicitly located continuing
endpoint in one admitted job must agree on a voxel. Validation tokens bind logistics revision where location can
invalidate admission.

Movement, pathing, haulage cost, automatic proximity search, generic stockpile/equipment/energy/fluid transport,
and mounted-production site/contact geometry are absent.

### Geology and knowledge

`GeologyState` owns hidden finite deposits: spatial bounds, material profile, excavation hardness, provenance,
initial/remaining mass, and lifecycle. Actor-facing code cannot enumerate this owner.

`GeologicalKnowledgeState` owns acquired observations. Observations contain authorized spatial evidence and
bounded abundance plus any method-supported physical metadata; they never expose deposit identity. Assessments
combine only acquired evidence, preserving contradiction and spatial incomparability instead of consulting hidden
truth.

Trusted load validates observations against the authored method and the geological bodies that could have
existed at acquisition time. Abundance and excavation-hardness evidence replay the same canonical quantization
used by live prospecting over those historical bodies; resource-scale evidence retains bounded historical
plausibility because later extraction means exact observation-time remaining mass is not reconstructible.
Evidence cannot gain precision retroactively from later-generated bodies or current state, and resource-scale
evidence remains observation-time evidence rather than a live reserve oracle.

### Prospecting and mining

Prospecting is exclusive player work over an authored method and bounded region. Completion may inspect hidden
geology only to produce method-bounded observations; in-progress work persists through `PlayerWorkState`.

`resolve_mining_target` turns sufficiently localized, compatible acquired evidence into opaque extraction
authorization without exposing deposit identity. Acquired hardness evidence uses its conservative upper bound for
planning. A localized target without hardness evidence may still be attempted; hidden hardness participates only
in opaque physical admission, never as a read-only planning value.

`resolve_mining_order` is a bounded effort projection over authored method/equipment physics, initial condition,
an acquired hardness bound, requested mass, selected batch mass, and a batch limit. It models sequential wear,
remainder batches, and per-batch tick rounding, but does not promise hidden supply, survival, destination
capacity, or authorization.

Mining start binds target, requested effort, equipment, destination reservation, labor, access, wear/capability,
and relevant owner revisions in `MiningJobRecord`. Because deposit temperature is not acquired target knowledge,
admission permits only a destination whose containment accepts every representable temperature; a finite storage
temperature limit cannot be used as a read-only probe of hidden geology. Geology retains matter custody during labor. Completion
moves the extracted slice into mining-owned ready-to-claim custody; destination capacity and required future
headroom remain reserved. `validate_claim_mining_output` performs the single transfer into inventory and returns
the exact contribution with merge-aware landing identity.

## Production and processing

### Production jobs

`ProcessDefinition` owns immutable process identity and generic typed provider requirements. Material eligibility,
recipe/yield, duration, energy, wear, and dynamic batch semantics belong to the execution-family resolver that can
interpret the selected matter; production does not maintain a second recipe.

`ProcessResolution` binds one concrete operation to exact selected inputs, duration, outputs, and finite resource
consequences. Production start transfers selected matter into durable work-in-process, reserves output capacity,
binds provider occupancy and required owner revisions, and records any modeled energy consequences needed for
continuation. Storage exposure remains wall-clock based while matter is in process.

Completion is revision-bound and moves exact outputs to their reserved destinations, applies resolved wear/energy
effects, releases occupancy/reservations, and emits merge-aware landing receipts. Loss of required support/provider
availability may suspend a job while preserving work-in-process, reservations, and remaining active duration.
Manual production releases player attention while suspended and must reacquire labor/survival budget before
resumption.

Manual shaping conserves material identity/mass and cannot change phase. Optional equipment may reduce attention
through canonical condition-adjusted throughput; required equipment rejects the equipment-free route. Equipment
changes throughput/wear, not recipe yield. Scrap/chip outputs remain represented matter until an explicit authored
recovery process consumes them.

### Physical resolvers

Powered ore definitions share one profile for throughput capability, batch limit, carrier, mass-specific work,
and active-tick wear. Runtime resolvers bind current provider/energy state and exact selected matter.

`assess_powered_ore_mass_envelope` exposes current monotonic scale bounds shared by powered comminution,
screening, and separation. It combines condition-adjusted equipment capacity, finite stored work, transfer power,
and remaining condition life. Replenishment views remain capped by the same store's authored capacity and do not
imply an external unlimited source. These envelopes are disposable planning evidence; exact resolver/validator
semantics remain authoritative.

`project_powered_ore_order` projects bounded multi-batch work over immutable definitions, sequential wear,
whole-tick timing, finite per-batch energy, and an optional declared maintenance policy. It does not claim future
feed, replacement material, stored energy, labor/survival, support, output capacity, or authorization.

`project_manual_ore_duration` is the equipment-free direct-labor counterpart. Optional tool/workstation support
uses the normal equipment boundary and changes attention/wear only; transformation/recovery physics remain with
the manual ore definition.

Implemented transformation families:

- **Comminution:** validates feed/particle state, batch/throughput, finite work, duration, and wear.
- **Dry screening:** partitions fully resolved particle classes around an authored aperture; no-op passes are
  rejected.
- **Constituent separation:** applies authored target/non-target recovery to liberated particulate feed while
  preserving unrecovered residue and exact composition.
- **Thermal:** sensible heating, pure-material melting, and casting use exact selected matter, finite energy,
  equipment limits, phase boundaries, and latent heat. Exact trace energies are accumulated before narrowing to
  the whole-nanojoule transaction boundary. Phase-change definitions bind explicit material/form identities and
  persisted jobs replay the admitted physical resolution.

## Equipment, labor, survival, energy, and fluids

### Equipment and maintenance

Capabilities use typed values plus explicit `AtLeast`/`AtMost` requirements. Provider capability is evaluated
through current condition; failed equipment supplies no productive capability.

Equipment owns identity, condition, embodied traces, occupancy, world location through logistics, and optional
structural support. Fixed machinery requires valid support for new work and contributes its own structural-load
channel.

Assembly consumes exact traces. Additive upgrades preserve identity, condition, prior embodiment, location, and
maintenance semantics while adding authored matter; inherited capabilities may not regress at the preserved
condition. Disassembly and worn recovery follow authored material routes.

Maintenance is physical. Admission commits the exact replacement/component exchange and represented spent matter,
then player work owns the service interval; condition recovery occurs only at completion. Component replacement
affects only the authored working component and preserves unrelated embodiment/upgrades.

### Player work and survival

`PlayerWorkState` permits at most one attention-owning operation across manual production, prospecting, mining,
manual power, eating, and drinking. Admission binds the exact metabolic/hydration budget required by that work.
Suspended operations that release attention must reacquire it and revalidate remaining survival budget before
continuing.

Manual power binds one portable provider and compatible finite energy destination. Runtime duration/work is limited
by provider capability, destination input power, physiology, store capacity, and condition; generated work stays in
player-work custody until completion, then moves into `EnergyState` with wear and physiological expenditure.
`project_manual_power` compares authored future configurations without authorizing ownership/current charge.
Current-state manual-power assessments reuse canonical provider/store physics and remain disposable until fresh
validation.

`SurvivalState` owns metabolic energy, hydration, vitality, nutrition, exact pending direct-consumption custody,
and terminal consumed matter/fluid totals. Eating/drinking move accepted quantities into pending survival custody
at admission and distribute earned benefit across their timed attention interval using cumulative exact integer
allocation. Basal/exertion expenditure happens before same-tick intake capacity is applied; intake may first cover
that tick's deficit and only residual benefit refills reserve. If work is interrupted by terminal survival state,
unearned pending benefit is not released.

Diet quality is limited by the weakest tracked food-group reserve. Fractional vitality-recovery carry persists so
continuation does not depend on tick partitioning.

### Energy and fluids

Energy stores own carrier, capacity, directional power limits, stored energy, revision, optional embodied traces,
and optional passive dissipation; logistics owns their world location. Consumers/producers use validated owner
operations. Passive dissipation is an environmental sink and applies from the pre-tick store snapshot after
same-tick ingress. Generic store-to-store transfer/carrier conversion is absent.

Material-backed energy-store upgrades are additive: registry validation requires carrier compatibility, non-
regressing capacity/transfer semantics, non-increasing passive loss, and exact base-plus-additions embodiment.
Runtime upgrade requires an empty unoccupied store and preserves store identity/location/creation history.
Disassembly is the inverse custody route for empty idle stores.

Fluid stores own identity, exact volume, temperature, capacity, revision, optional structural support, and
homogeneous fluid identity; logistics owns their world location. Runtime supports exact withdrawal and support
changes; generic transfer, pumping, mixing, and pressure networks are absent. Shared owner projections convert
represented volume/density into exact mass for structural load and thermal accounting. Fluid sensible/latent
energy remains read-only accounting; passive fluid heat transport is absent.

## Structures

Structural members own geometry, topology, embodied material, self-weight, external source-separated
loads, lifecycle, and damage. Analysis models axial tension/compression and deterministic
stable/strained/cracked/failed transitions with support-loss cascades. Support edges require positive-area
voxel contact: overlapping bounds qualify, as do face-abutting bounds with positive overlap on the other two
axes. Edge-only and corner-only touches do not carry support; sub-voxel joint geometry is outside the model.

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
- persisted schedules and operation-specific physical replay.
