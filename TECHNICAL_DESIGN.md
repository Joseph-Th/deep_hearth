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
| Cross-owner custody, reservations, and continuation | Cross-owner edge atlas |
| Definitions, reads, planning, and command surface by subsystem | Subsystem control index |

## System control model

Implemented subsystems form one control graph rather than independent simulations. The common currencies and
constraints are:

| Flow | Typical owners and transitions |
| --- | --- |
| Matter | geology -> mining custody -> inventory -> production/infrastructure -> inventory/recovery or terminal survival consumption |
| Fluid | fluid stores -> validated withdrawal -> terminal survival consumption; generic transport is not yet implemented |
| Energy | finite stores -> durable work/process custody -> physical result or explicit sink/loss; manual labor can generate stored mechanical work through its own validated path |
| Attention/labor | `PlayerWorkState` arbitrates exclusive player attention while `SurvivalState` supplies the physiological budget of active work |
| Information | hidden world truth -> authorized observation -> `GeologicalKnowledgeState` -> evidence-based action authorization; hidden truth never becomes an actor shortcut |
| Support/load | structures provide support; inventory, equipment, and fluid owners contribute source-separated load; support state can gate productive availability |
| Capacity/exclusivity | reservations and occupancy bind future outputs/resources so delayed work cannot double-book them |
| Time | persisted schedules and active/suspended durations make future consequences replayable rather than implicit |

When extending a cross-system mechanic, start from the changed flow and follow custody from owner
to owner. Every handoff is a canonical operation or durable work record.

### Truth classes

Keep these classes distinct when reading or extending the system. Many expensive agent mistakes come from
treating one class as another:

| Class | Meaning | May persist? | May authorize mutation? |
| --- | --- | --- | --- |
| Authored definition | Immutable possibility, identity, physical/capability limit, or reference. | Registry identity/version only as required | No, except as one input to validation. |
| Authoritative runtime fact | Generated state required for continuation. | Yes | Owner commands read it. |
| Derived projection | Recomputable interpretation, aggregate, assessment, or index. | Only when explicitly rebuildable/validated semantics justify it | No by itself. |
| Resolution/plan | Concrete predicted consequence for one request against current facts. | Normally no | No; it feeds authorization. |
| Validated authorization | State-bound proof that a consequential commit is currently legal. | No | Yes, exactly through its consuming canonical commit/apply path. |
| Durable work/custody | In-flight ownership, reservation, schedule, provider trace, or pending consequence. | Yes | Governs continuation through its owner/tick path. |
| Diagnostic/evaluation evidence | Explanation, sample, counterfactual result, accounting report, or replay metadata. | Outside authoritative simulation as needed | Never. |

If a new value seems to belong to two rows, separate the concepts before storing or exposing it.

### Planning topology

The implemented definition set forms typed authored topology. `CraftingRegistry` owns deterministic reverse
indexes for direct manual producers and consumers, while `Registries::new` derives the cross-registry
`ProcessTopology` projection after validating all domain references. That projection also enforces that one
`ProcessId` cannot own more than one crafting/ore/thermal execution family and records each process's execution
family, typed equipment role, typed energy role, nominal provider definitions, and compatible energy-store definitions. Equipment and
energy-store assembly/upgrade ancestry plus maintenance/recovery material relationships remain with their
domain definitions. Runtime `resolve_equipment_provider` remains separate because a known equipment instance
adds mutable condition and structural-support facts that immutable topology cannot prove.

Treat three levels separately:

1. **Possibility:** immutable definition relationships. A reverse lookup such as producers of one commodity or
   nominal providers of one capability requirement is a derived registry projection. It may answer authored
   connectivity but not current legality or ordinary reachability.
2. **State:** current owner records, custody, quantities, condition, support, occupation, knowledge, and
   schedules. Hidden owners remain hidden from actor-facing queries.
3. **Opportunity:** a request-scoped projection that intersects relevant possibility with legitimate observable
   state and canonical domain resolution. It may report candidates or typed blockers, but only validation can
   authorize mutation.

Do not persist possibility/opportunity indexes as world truth. A registry-derived reverse index must rebuild
deterministically from the exact validated definitions. An opportunity result must either be consumed
immediately as read-side guidance or carry explicit freshness semantics if retaining it is materially useful.

Topology lookup should normally be exhaustive for its narrow declared key: for example all authored manual
producers of one `CommodityKey`, all process definitions assigned to one exact execution family, or all nominal
equipment definitions satisfying one exact typed requirement set. Return results in stable domain-identity order
unless another domain ordering is itself authoritative. If a future topology domain becomes too large for one
bounded response, expose explicit continuation and completeness rather than a hidden `take(N)`.

Opportunity discovery may be intentionally bounded because current candidate combinations can be larger. Such a
surface must distinguish "no candidate in this bounded search" from an exhaustive proof of unavailability, name
the searched domain/budget, and bind continuation to the same relevant state dependencies. Validation remains
the only authorization even when discovery reports an exhaustive current candidate set.

Goal-directed discovery should preserve domain shape. Useful examples are producers for one commodity,
construction/upgrade ancestry for one infrastructure definition, providers satisfying one typed capability
profile, or processes using one energy carrier. Avoid an untyped `Node -> Edge -> Node` API that would force
callers to rediscover whether an edge means material transformation, provider compatibility, upgrade ancestry,
or recovery.

Likewise, shared planning constraints belong at the narrowest physical abstraction that actually shares them.
Powered ore operations share throughput, batch, carrier/work, and wear profile concepts; the powered-ore mass
envelope derives their current scale bounds without inspecting the selected material batch. Process-specific form,
composition, particle-state, and output legality remain with the canonical comminution/screening/separation
resolver. Destination storage remains an inventory-owned constraint observed through available capacity; canonical
reservation/ingress validation remains the authorization. Homogeneous melting and casting use separate thermal
lot-mass envelopes over canonical phase-change energy, equipment, finite energy, and transfer timing. All envelopes
remain disposable planning evidence; canonical resolution still binds exact selected matter before authorization.

Use claim-strength terms consistently when exposing or interpreting this topology:

- a definition's assembly/upgrade/recovery field establishes a **direct authored edge**;
- recursive traversal over such edges establishes an **authored path** only for the declared roots and edge
  families included by that traversal;
- **ordinary reachability** is a stronger current-scope claim owned by [`STATUS.md`](STATUS.md), not inferred
  from a local direct-edge predicate;
- a **current opportunity** requires legitimate observable runtime state plus canonical domain resolution;
- **authorization** requires validation against the current mutable dependencies.

Equipment `has_authored_acquisition_edge()` and energy `has_authored_assembly_edge()` are deliberately local
definition classifications. `CraftingRegistry::manual_producers()` and `manual_consumers()` are immutable direct
material-edge reverse indexes. `Registries::process_topology()` is the aggregate cross-registry projection for a
process's unique execution family, typed energy role, nominal equipment definitions, and compatible energy-store
definitions. None of these surfaces performs recursive reachability or current-state planning.

### Temporal stepping contract

`advance_tick(registries, state)` remains the only authoritative simulation-time mutation. Current gameplay and
tests often know a work record's `completes_at` and call it repeatedly until that horizon. A future batched
stepping surface may wrap that exact operation for caller efficiency, but it must preserve the same state and
the same ordered sequence of material `TickOutcome` events as the equivalent repeated calls.

Batching may stop early on caller-declared observable events such as production completion, suspension/resume,
manual-power completion, prospecting completion, mining output readiness, or a survival condition. It may not
peek at hidden geology, controlled future events, or later outcomes to choose an earlier stopping point.

No semantic interval-skipping/fast-forward contract is currently implemented. Any future optimized interval
integrator must first prove equivalence across all tick phases it skips. Persisted dynamic completion ticks may
identify boundaries, but they do not establish that the interval between those boundaries is semantically empty.

### Cross-owner edge contract

For a new materially distinct handoff, document and implement enough of this contract that an agent can trace
the edge without reconstructing the transaction from incidental fields:

1. source owner and destination owner;
2. canonical admission boundary and stable identities involved;
3. exact quantity/relationship transferred, reserved, or newly owned;
4. capacity, exclusivity, support, information, and lifecycle prerequisites;
5. mutable dependencies bound for stale-state protection;
6. custody or schedule owner between admission and completion, if delayed;
7. atomic rejection boundary and any intentionally modeled failure-side mutation;
8. typed committed outcome or continuation identity;
9. trusted-load reconstruction/validation obligations;
10. smallest owner/boundary/continuation proof that distinguishes the edge from a nearby invalid transfer.

The [cross-owner edge atlas](#cross-owner-edge-atlas) is the routing index for implemented instances of this
contract. Add a row only for genuinely new ownership semantics, not every new operation over an existing edge.

## Subsystem contract card

Use this compact schema when reading or adding a subsystem. Existing sections below provide the concrete facts;
source and adjacent tests remain the edge-case authority.

1. **Definitions:** immutable authored identities, limits, and physical/capability references.
2. **Authoritative state:** generated facts, identity cursors, revisions, lifecycle, custody, and schedules.
3. **Observable projections:** canonical read models suitable for callers, presentation, and automated actors.
4. **Decisions/resolution:** deterministic derivation of consequences from definitions plus current state.
5. **Authorization:** typed rejection and stale-state binding before consequential mutation.
6. **Mutation:** one owner or cross-owner commit/apply path.
7. **Flows:** exact quantities, reservations, occupancy, support, information, and time that cross owner boundaries.
8. **Persistence:** what must survive, what can rebuild, and what trusted load re-derives.
9. **Evidence:** local invariant tests plus the smallest cross-system/gameplay proof when behavior crosses owners.
10. **Addressability:** stable semantic landmarks identify the owner, canonical request/resolution/validation/
    commit path, durable identity when present, typed error family, outcome/assessment, trusted-load validator,
    and adjacent proof.
11. **Freshness/continuation:** which authoritative dependencies invalidate retained planning and which stable
    identity, revision, schedule, or outcome lets a caller continue without reconstructing unrelated state.
12. **Feasibility, when scalable:** which production surface exposes the controlling bound or bottleneck when a
    caller would otherwise have to probe many nearby requests to discover one monotonic feasible envelope.
13. **Query completeness, when discoverable:** whether candidate/topology queries are exhaustive, bounded with
    continuation, or sampled, including deterministic ordering and the claim an empty result actually supports.

New subsystem design is incomplete until these questions have explicit answers. Not every answer requires a new
type or file; the goal is one legible ownership/control story, not ceremony. Addressability describes semantic
landmarks, not mandated file names, and feasibility requires no extra API when the operation has no meaningful
scalable planning dimension.

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
| Production | `ProductionRegistry`, `ProcessDefinition` | `AppState::production()`, job records, reservations/occupancy | operation-specific resolvers produce `ProcessResolution` / `Resolved*` | `validate_start_process` / `validate_start_process_routed` -> tick completion |
| Equipment | `EquipmentRegistry`, capability/maintenance/upgrade profiles | `AppState::equipment()`, equipment records | `resolve_equipment_provider`, `resolve_equipment_maintenance` | assembly, upgrade, maintenance, disassembly, mount/unmount/relocate validators |
| Player labor | `LaborRegistry`, manual-power/prospecting definitions | `AppState::player_work()` | `project_manual_power` projects immutable future provider/store physics; `assess_manual_power_energy_envelope` and `assess_manual_power_destination_target` bind current provider/store state for exact read-only generation and post-work stored-energy planning; runtime owner commands still own authorization, attention, survival budget, and revisions | manual power/prospecting/manual production commands -> tick; attention lifecycle is crate-owned |
| Survival | `SurvivalRegistry`, physiology, food/drink definitions | `AppState::survival()`, `assess_survival`, `assess_food_freshness` | consumption validators derive bounded direct intake and physiological schedule | `validate_eat` / `validate_drink` -> tick; `initialize_player_survival` is the ordinary initialization boundary |
| Energy | `EnergyRegistry`, store definitions, carrier/power contracts | `AppState::energy()`, store records, explicit energy accounting | process/manual-power resolvers use `validate_energy_supply` / `validate_energy_sink` as part of their plan | assembly/upgrade/disassembly validators; reserved consumption/release and passive loss apply through canonical owners/tick |
| Fluids | `FluidRegistry`, fluid definitions | `AppState::fluid()`, store records, fluid accounting | consumers validate exact egress internally; no generic routing planner exists | support validators and canonical consumers; generic transfer/pumping/mixing absent |
| Structures | `StructuralRegistry`, profiles and geometry | `AppState::structures()`, `analyze_structure`, `StructuralAssessment` | owner-specific support/load validation plans final aggregate load | support/load commits through inventory/equipment/fluid/structural owners; general player construction remains absent |
| Manual crafting overlay | `CraftingRegistry`, manual craft definitions, optional-or-required equipment profile | inventory, survival, authored craft definitions, optional equipment instance | `project_manual_craft_hand_work` projects authored fallback duration plus the shared physiological budget without authorizing current state; `resolve_manual_craft` binds selected matter and uses fixed authored duration when fallback is permitted, otherwise requires canonical condition-adjusted MassFlow | `validate_start_manual_craft` -> equipment-reserving production job + player work -> tick |
| Powered crafting overlay | `CraftingRegistry`, `PoweredCraftDefinition` referencing one manual material transform, required MassFlow capability, carrier, specific energy, and wear | inventory, equipment, energy, production state; referenced manual transform remains material/yield authority | `resolve_powered_craft` binds exact selected matter, reuses the referenced transform's batch/output construction, resolves condition-adjusted machine throughput, and validates finite carrier-compatible stored work | `validate_start_powered_craft` -> ordinary production job with reserved equipment/energy and no player-work claim -> tick |
| Ore-processing overlay | `OreProcessingRegistry`, manual/powered process profiles | inventory, equipment, energy, production state | `assess_powered_ore_mass_envelope` for one-batch current scale bounds; `project_powered_ore_order` for bounded replenished-work orders with carried wear and optional critical-band service; `resolve_comminution_process`, `resolve_screening_process`, `resolve_constituent_separation_process` for exact selected-lot legality; manual counterparts preserve an equipment-free fallback and may bind one authored condition-adjusted MassFlow tool/workstation | powered resolutions enter `validate_start_process*`; manual start validators bind player work and reserve optional equipment through the same production job |
| Thermal overlay | `ThermalRegistry`, heating/melting/casting definitions | inventory, equipment, energy, production state | `assess_melting_lot_mass_envelope` and `assess_casting_lot_mass_envelope` for current homogeneous-lot scale bounds; `resolve_sensible_heating_process`, `resolve_melting_process`, `resolve_casting_process` for exact selected-lot legality | resolved work enters `validate_start_process*`; tick applies outputs, wear, and energy consequences |
| Conservation/accounting | authored material/fluid/energy properties | `calculate_matter_accounting`, `calculate_fluid_volume_accounting`, `calculate_explicit_energy_accounting` | read-only reconciliation only | none; accounting never mutates or authorizes custody |
| Persistence | current save schema + registry schema | `SaveEnvelope` for output, decoded `LoadedSaveEnvelope` before trust | exact-version admission plus deterministic index rebuild/graph validation | `LoadedSaveEnvelope::into_state`; adapters own bytes/storage, not state promotion |
| Presentation definitions | texture/shader registries and authored assets | immutable definition access and deterministic bake/assembly results | deterministic renderer-neutral assembly | graphics resources/frame effects belong to adapters, outside `AppState` |

If a caller appears to need a surface not shown here, first determine whether it is a missing canonical
projection/command or whether the caller is trying to cross an ownership boundary it should not control.

## Global runtime facts

- `SimulationTick` is absolute world time; `TickSpan` is relative duration.
- The built-in calendar maps 24,000 ticks to 86,400 seconds; one tick is 3.6 seconds.
- Rate-authored physics integrate against physical tick duration. Per-tick gameplay costs use world ticks.
- No runtime stochastic owner is currently implemented. Add persisted random state only with a concrete system
  whose outcomes require authoritative stochastic continuation.
- Implemented authoritative physical calculations use checked integer arithmetic, not floating point.
- Dynamic scheduled work persists as explicit records. Add static clock-derived schedule machinery only when an
  implemented owner has a concrete recurring-phase contract that requires it.
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

These are control paths as well as accounting paths. Planning code should inspect the canonical projection at
each edge when the owner exposes one and invoke the canonical transition, not reach through to a later owner.
If a legitimate caller lacks the read surface needed to control an edge without reconstructing private domain
meaning, treat that as control-surface debt under [`DIRECTION.md`](DIRECTION.md), not permission for a parallel
rules implementation.

### Cross-owner edge atlas

Use this atlas when a task is about an interaction rather than a local calculation. The named boundary is the
semantic entry point; inspect its implementation and adjacent tests before reading every endpoint owner.

| Edge | Canonical boundary | Authoritative handoff and continuation |
| --- | --- | --- |
| Hidden geology -> acquired knowledge | `validate_start_field_prospecting` -> simulation tick | `PlayerWorkState` holds exclusive prospecting labor; completion records one aggregate `GeologicalObservationRecord` or one bounded, spatially ordered per-voxel batch in `GeologicalKnowledgeState`, according to the authored method. All methods record bounded abundance. Authored physical-sampling methods may additionally record a conservative excavation-hardness band and, only when the acquired observation footprint exactly localizes one unambiguous body, a coarse observation-time resource-mass interval. Both require a positive lower abundance bound, neither carries deposit identity, and partial/ambiguous regions emit no resource-scale claim. Hardness is immutable for a live deposit and aggregate reads prefer precision. Remaining resource mass changes with extraction, so aggregate reads prefer the latest acquired resource-mass observation and use precision only to break same-tick ties. The assessment exposes that selected resource observation's own tick separately from the latest observation of any kind, and contradictory abundance evidence withholds resource scale rather than presenting an older positive reserve estimate as current. `TickOutcome::field_prospecting()` exposes the actor-safe first observation identity, count, and scope without deposit identity. |
| Acquired knowledge -> extraction authorization | `resolve_mining_target` | Read-only `MiningTargetResolution` proves that legitimate evidence currently resolves one extractable owner while keeping the geological deposit identity crate-private. A candidate body must already have existed when every observation contributing to the authorization was acquired, so older evidence cannot retroactively bind a later-generated body. No custody changes yet. |
| Geology + equipment + labor -> mining work | `validate_start_mining` -> simulation tick | Start binds the requested excavation effort, tool, destination, player attention, and a durable `MiningJobRecord`. With initialized player logistics, the player must be inside the resolved deposit bounds and the tool/output stockpile must have exact logistics-owned locations at the player voxel; the start token binds logistics revision and trusted load replays those conditions while the job is working. Pre-admission batch/capacity/labor/wear feasibility is derived from the requested mass, not hidden remaining reserve, so read-only validation cannot be used to measure a deposit. The hidden output slice is `min(requested, remaining)` and is bound internally; after commit its reservation and eventual claim may legitimately reveal a short recovery. Geology keeps that output matter during labor; completion removes it from `GeologyState`, applies wear for the requested effort, releases attention, and places the physical output in mining-owned claim custody. |
| Mining claim custody -> inventory | `validate_claim_mining_output` | A ready job retains its reserved destination capacity until claim. Claim moves the exact output into `InventoryState`, retires mining custody without a second extraction decision, and returns `MiningClaimReceipt` with the exact contribution plus its merge-aware surviving lot identity. |
| Ground inventory <-> player-carried inventory | `validate_pickup_from_ground` / `validate_drop_to_ground` | `LogisticsState` proves the player and loose stockpile occupy the same voxel, then delegates exact selected-lot movement to inventory relocation. Inventory remains authoritative for capacity, containment, temperature, provenance, storage history, lot identity, and structural-load consequences. Ground/carried custody cannot simultaneously be structurally mounted. Movement, path cost, haulage, and world-source creation remain separate absent authorities. |
| Inventory + providers -> production work | resolver-specific `Resolved*` / `ProcessResolution` -> `validate_start_process` or `validate_start_process_routed` | Start consumes exact selected input into `ProductionState` work-in-process, reserves routed output capacity, binds provider occupancy, and records modeled finite-energy consequences needed for replay. Every endpoint with an explicit logistics-owned voxel—source, routed destinations, equipment, consumed-energy store, or released-energy sink—must agree on one production site; the start token binds logistics revision so location assignment or relocation cannot race admission. |
| Production work -> inventory / equipment / energy | `advance_tick` completion planning and apply | Due completion routes exact material streams to reserved inventory destinations, applies condition consequences, releases/consumes modeled energy as resolved, clears occupancy/reservations, and emits `ProcessCompletion` through `TickOutcome`; each stream has merge-aware inventory landing identities. |
| Fatal survival -> active player-work disposition | `advance_tick` fatal-work planning | A work item due on the fatal tick completes normally before labor is released. Otherwise direct manual production is suspended with its work-in-process and output reservations intact; working mining is canceled without extraction or completion wear and returns its exact output reservation; storage-enclosure dismantling is canceled without removing the enclosure and returns its exact recovery reservation. Unfinished manual power and prospecting are pure `PlayerWorkState` continuations, so fatal interruption releases them without generated energy, acquired evidence, or completion wear. Maintenance has already committed its exact material/component exchange at admission, so interrupted service keeps that represented matter transition but receives no deferred condition recovery. The resulting idle/dead player state must pass the same trusted-load reconciliation as persisted continuation. |
| Inventory -> equipment embodiment | `validate_assemble_equipment` / `validate_upgrade_equipment` | Exact material traces leave inventory custody and become `EquipmentState` embodiment. With initialized player logistics, assembly also creates equipment world custody at the player voxel. Upgrade preserves equipment identity, world location, prior embodiment/condition, and adds only the authored trace; player-context equipment and material-source endpoints must be locally accessible. Inherited capabilities cannot regress at the preserved condition according to each capability definition's explicit improvement direction. |
| Equipment embodiment -> inventory recovery | `validate_disassemble_equipment` / `validate_equipment_maintenance` | Disassembly returns authored recoverable traces as inventory lots and removes the equipment logistics location. Maintenance admission requires exact-local equipment, replacement source, and spent-material destination whenever player logistics is initialized, regardless of whether the equipment is structurally supported; it commits the exact replacement/component exchange and represented spent matter immediately, then `PlayerWorkState` owns the timed service interval. Trusted load replays active-service equipment access. Equipment condition recovery occurs only at completion. If labor terminates before that completion, the committed material exchange remains and no condition gain is granted. |
| Inventory -> finite energy-store embodiment | `validate_assemble_energy_store` / `validate_upgrade_energy_store` | Exact material traces become `EnergyState` embodiment. With initialized player logistics, assembly also creates energy-store world custody at the player voxel; upgrade requires exact-local store/material access and preserves store identity, location, and carrier/transfer semantics while changing the authored store definition. `validate_disassemble_energy_store` requires exact-local store/recovery access, removes the store location, and is the exact reverse custody route for empty idle stores. |
| Inventory enclosure matter <-> inventory storage profile | `validate_build_storage_enclosure` / `validate_start_storage_enclosure_dismantling` -> simulation tick | Inventory remains the material owner while `PlayerWorkState` owns the timed dismantling interval. Start reserves the recovery destination and binds survival/labor; the enclosure and its preservation profile remain authoritative until completion, when exposure is checkpointed, ambient storage is restored, and exact enclosure matter enters the recovery stockpile. |
| Inventory / equipment / fluid -> structural load | owner-specific mount/unmount validators | The mounted owner retains object custody while `StructureState` owns its source-separated load. Final aggregate load is validated before support assignment changes, and returned support outcomes expose the structural consequence when the represented load actually changes; force-rounded load no-ops remain revision-bound without synthesizing an analysis. Existing stockpile, equipment, and fluid-store world locations survive support assignment changes and must lie inside the selected support bounds; support tokens bind logistics revision so location changes cannot race admission. |
| Player physiology + equipment -> stored mechanical work | `validate_start_manual_power` -> simulation tick | `PlayerWorkState` owns pending generation during direct labor; admission binds survival budget, equipment wear, energy-store capacity, and logistics revision. With initialized player logistics, provider equipment and the destination store must have exact locations at the player voxel; trusted load replays both access conditions while work is active. Completion deposits exact work into `EnergyState`, applies wear/physiological expenditure, releases attention, and exposes `ManualPowerOutcome`. |
| Inventory / fluid -> terminal survival consumption | `validate_eat` / `validate_drink` -> simulation tick | Admission transfers selected matter/fluid into `SurvivalState` pending-consumption custody, reserves exclusive attention, and returns the exact completion tick with the accepted intake outcome. With initialized player logistics, eating requires the source stockpile to be carried or explicitly located at the player voxel, and drinking requires the fluid store to be explicitly located there; both tokens bind logistics revision. Tick installments release only earned physiological benefit; terminal consumed totals retain represented custody after the explicit food/fluid simulation boundary. |
| Authoritative owners -> whole-system accounting | `calculate_matter_accounting`, `calculate_explicit_energy_accounting`, `calculate_fluid_volume_accounting` | Read-only accounting recomputes from owners and never becomes another custody store. Use it to prove conservation/reconciliation, not to drive mutation. |

If a new feature creates a materially new row, first decide whether it is a new owner edge or merely another
operation over an existing edge. Prefer reusing an existing edge contract when ownership, custody, and failure
semantics are genuinely the same.

#### Destination landing identity

Inventory ingress already resolves merge-aware persistent identity. `apply_material_ingress` returns one
surviving `MaterialLotId` per admitted parcel, reusing an existing identity when compatible matter coalesces.
Reserved delayed ingress uses the same rule: `apply_reserved_deposits` returns one inventory-owned receipt per
reserved request, containing one surviving identity per admitted parcel in request order.

Production completion composes those receipts into `ProcessOutputLanding` values keyed by stream/destination;
each `ProcessParcelLanding` pairs the exact `MaterialLotSpec` contribution with the surviving lot identity.
Mining claim likewise returns its exact claimed `MaterialLotSpec` plus one merge-aware surviving identity in
`MiningClaimReceipt`. A landing identity may therefore refer to an existing lot when compatible matter
coalesced. Other delayed custody edges needing the same answer should propagate this inventory-owned result
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

Materials are immutable definitions. Forms define phase, particle-state policy, and physical cohesion.
Only consolidated non-particulate solids may directly become rigid infrastructure components or structural
embodiment; loose forms require an explicit shaping or consolidation process first. Infrastructure embodiment
does not carry stockpile preservation history, so any material with an authored edible form is excluded from
equipment, energy-store, and storage-enclosure assembly/upgrade inputs in every form until embodied
perishability aging is modeled.
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
`LogisticsState` supplies that authorization for exact selected-lot pickup/drop when the player and a loose
ground stockpile share one voxel. `validate_player_stockpile_access`, `validate_player_equipment_access`,
`validate_player_energy_store_access`, and `validate_player_fluid_store_access` are the shared admission rules for
player actions over inventory, equipment, energy-store, and fluid-store endpoints. Once player logistics is
initialized, an existing endpoint must be carried where applicable or have an explicit logistics-owned location
at the player voxel. The carried
inventory is itself one finite `InventoryState` stockpile; logistics owns its carried role/location rather than
duplicating material state. General stockpile/equipment/energy/fluid-store transport, player movement,
haulage/path cost, and world-source acquisition are not implemented. The gameplay harness may
still authorize other controlled conserved transfers only as setup or controlled-event infrastructure.

### Logistics

`LogisticsState` persists the local player's `VoxelCoord`, the identity of one inventory-owned carried
stockpile, stationary stockpile voxel locations, equipment voxel locations independent of support assignment,
finite-energy-store voxel locations, finite-fluid-store voxel locations, and a monotonic revision. Initialization allocates carried capacity through
inventory's revision-bound empty-stockpile allocator; logistics never stores duplicate lot, equipment condition,
stored energy, fluid contents, or capacity totals. `assess_player_carrying` joins the location record to inventory's current
capacity and mass.

`validate_place_ground_stockpile` is the low-level world/bootstrap location boundary for an existing unsupported
stockpile; `validate_allocate_ground_stockpile` composes the same owner with inventory's empty-stockpile
allocator without creating matter. Placement rejects carried or structurally mounted custody, active storage
dismantling occupancy, and stockpiles with reserved inbound work, so a delayed output target cannot be moved
after admission. Placement tokens recheck those facts at commit as well as logistics/support revisions.
Same-voxel `validate_pickup_from_ground` / `validate_drop_to_ground` compose explicit lot selection with canonical
inventory relocation; relocation preserves temperature, composition, particle state, provenance, storage
exposure, finite capacity, merge-aware identity, and structural-load accounting.

`validate_place_fluid_store` is the bootstrap location boundary for an existing finite fluid store. It binds one
previously unlocated store to a voxel without moving fluid, changing support, or implying a transport/pumping
mechanism. If the store is already structurally supported, that voxel must lie inside the support bounds; trusted
load replays the same relation. Structural mount admission applies the inverse check for already located stores and
binds logistics revision so a late placement cannot race support assignment.

`validate_player_stockpile_access`, `validate_player_equipment_access`,
`validate_player_energy_store_access`, and `validate_player_fluid_store_access` require exact local custody for
existing endpoints whenever player logistics is initialized. Carried inventory is implicitly co-located; every
other targeted stockpile, equipment instance, energy store, or fluid store must have an explicit voxel equal to
the player's. Controlled capability fixtures with no player logistics may remain locationless because they are
not claiming direct-player world access. Manual crafting and manual ore processing check their stockpile
endpoints; eating checks its food source; drinking checks its fluid-store source; storage construction/dismantling
checks its material, target, and recovery endpoints; equipment/energy-store lifecycle operations check the
endpoints they actually touch. Player-context equipment and energy-store assembly create world custody at the
player voxel. Equipment mounting and unmounting change support assignment without deleting or recreating the
equipment location, so support changes cannot teleport equipment. Located stockpile mounts follow the same rule.
Equipment/energy-store disassembly removes the corresponding world location. Trusted load rejects location
records whose owner identity is absent and rejects located supported stockpiles, equipment, and fluid stores whose
voxel lies outside the support bounds.

Prospecting additionally requires a known player to stand inside the requested survey region and any required
sampling instrument to have a location at that player voxel. Mining requires a known player to stand inside the
resolved deposit's actual bounds and requires its tool/output-stockpile endpoints at that voxel. Their
validate/commit tokens bind logistics revision, and trusted-load validation replays the same spatial
requirements for active prospecting, working mining, active equipment maintenance, and active manual power.
Manual power additionally requires provider equipment and the destination energy store at the
player voxel. Production uses a different, actor-independent rule: start admission requires every explicitly
located source, destination, equipment, and energy endpoint to share one voxel. Consumed source matter and
consumed energy then leave those owners at admission, so trusted load does not freeze their historical locations;
while a job remains unsuspended it replays co-location only for continuing equipment, released-energy sinks, and
reserved output destinations. Suspended production deliberately skips that running-site check so recovery or
relocation can be resolved by its own continuation path. Stockpile mounting rejects player-carried custody but
preserves an existing stationary location, requiring it to lie inside the selected support. Equipment support
changes likewise preserve world location. Both mount paths bind logistics revision so location changes cannot
race validation. No player movement, terrain/path legality, haulage duration/exertion, automatic proximity
search, mounted-production site/contact geometry, or physical rule for direct mounted-to-
mounted equipment relocation is implied by this slice. No generic fluid-store relocation, pumping, mixing, or
pressure network is implied by fluid world custody.

Same-material reform may change form without changing material phase. It preserves temperature, composition,
and particle state; phase transitions remain owned by thermal processing. Within inventory custody, storage
exposure is checkpointed only when the effective preservation multiplier changes. Equal-rate relocation, reform,
and lot coalescing preserve the selected cohort's existing history representation so equivalent physical
histories cannot age differently because they were split across inventory transaction boundaries. Any material
with an authored edible form retains exact future-equivalent exposure cohorts in every form, so non-edible
intermediate storage cannot erase perishability history before a later same-material food reform. Freshness
reporting derives the remaining shelf-life horizon from the retained rational projection phase, not only from
rounded current age, so the reported final fresh tick is the tick immediately before authoritative spoilage.
`project_food_freshness_after_storage_transition` applies that same history arithmetic read-only across one
future authored enclosure transition and a caller-chosen later assessment tick. It is disposable planning
evidence: it does not validate enclosure capacity, containment, construction inputs, player work, or any
intervening mutation, and callers must project again if authoritative state changes before acting.

Storage enclosures are immutable definitions with a capacity limit, storage profile, and exact consolidated
assembly profile. Construction transfers selected traces from inventory into persistent stockpile-owned
enclosure matter. Before commit, every existing lot must satisfy the completed enclosure's phase, temperature,
material-phase, and particle-state containment rules. Existing lots checkpoint accumulated exposure before the
new preservation multiplier takes effect, so improved storage affects future spoilage only. Trusted load
validates enclosure definition, construction time, storage profile, and embodied traces.

Built-in ordinary provisions storage separates construction cost, usable capacity, preservation,
and raw-material family instead of forming a linear upgrade ladder. Exact capacities, preservation factors,
embodied bodies, raw routes, and attention costs live in the authored content definitions. Capacity and available
raw material constrain feasibility before actor preference. Construction delay precedes the improved storage rate,
so the prospective freshness projection lets callers inspect that tradeoff without duplicating storage-aging rules.

Enclosure dismantling is the inverse custody transition for that exact embodied matter, not generic demolition.
The target and recovery stockpiles must be unmounted; when either has logistics-owned world custody it must be at
the player's voxel. The target must have no reserved inbound work and remain valid under the ambient storage
profile. Admission reserves exact recovery capacity and starts exclusive timed player work with authored survival
exertion. The enclosure remains installed throughout the interval, so retained lots continue aging under its
current preservation multiplier. Completion checkpoints that exposure, restores ambient storage, and returns the
enclosure traces to the distinct recovery stockpile with their exact temperature, composition, particle state,
and provenance. Recovery capacity, lot-ID space, inventory revisions, logistics revision, and competing delayed-
output ownership are validated before admission/completion mutation; trusted load rejects active dismantling whose
persisted known locations no longer match player access. General pathing, demolition, and dismantling tools remain
outside this transition.

Detached enclosure bodies may then be reused intact or entered into explicit manual salvage. Timber salvage
conserves the full body mass as boards plus represented chips; the stone crock converts its exact body into
reworkable stone scrap for the reknapping route. Exact salvage yields and attention costs live in the authored
definitions.

Structural support links require positive-area voxel contact: overlapping bounds are admissible, as are bounds
that abut on one axis while overlapping on the other two. Edge-only and corner-only contact cannot carry a load
path. Trusted-load validation replays the same rule, so persisted topology cannot bypass runtime support
admission.

`project_prismatic_member_load` is the structural owner's read-only projection for a pristine prismatic member
under an external load. It composes density-derived member mass, gravity self-weight, authored material/profile
capacity, and utilization so planning, presentation, and gameplay evaluation do not reproduce that physical
composition outside the structural subsystem. Its exact utilization-limit predicate compares load against the
requested fraction of capacity directly rather than treating floor-rounded display ppm as a legality result.

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

Physical sampling is also the planning boundary for extraction resistance. A resolved target carries the best
acquired excavation-hardness band that covers its localized evidence region. With such evidence, mining compares
the selected tool against the conservative acquired upper bound before commitment. Without a hardness band, a
localized target may still be attempted; admission uses hidden hardness only to determine whether the physical
attempt is possible and reports the tool's hardness ceiling rather than revealing the hidden value. The exact
hidden hardness remains authoritative physical truth for geology ownership and trusted continuation validation,
but it is not a read-only planning oracle. Trusted load
first requires every persisted observation to be compatible with at least one currently authored prospecting
method: evidence kind, observation footprint, abundance uncertainty, and any physical hardness/resource metadata
must not claim precision the method could not produce. Persisted abundance must remain conservative for every body
that was physically available at the observation tick; a positive nonphysical abundance claim additionally requires
that the region could have been fully covered by bodies available then. Geological deposits persist generation and
depletion ticks, with same-tick depletion still observable under pre-tick snapshot semantics, so trusted load can
reconstruct historical availability instead of substituting current lifecycle. Later-generated bodies do not
retroactively validate older samples, and bodies depleted before acquisition do not constrain later samples. A
hardness band must include every matching body available at acquisition and at least one such body must exist. A
resource-mass interval requires exactly one matching acquisition-time body intersecting the observation, with exact
footprint and a physically possible observation-time remaining mass between current remaining mass and immutable
initial mass. Depleted bodies remain persisted, so legitimate pre-depletion physical observations continue to
validate without pretending their measured reserve scale is current. Mining target resolution separately refuses
to bind a live body generated after any evidence used to localize that authorization.

`resolve_mining_order` is a bounded read-only effort projection over authored method/equipment definitions,
initial condition, an acquired hardness upper bound, requested mass, caller-selected batch mass, and a maximum
batch count. It reuses admission physics sequentially, including wear, remainder batches, per-batch tick rounding,
and checked total duration. It rejects invalid inputs, exceeded search bounds, and typed batch physics failures.
It neither reads hidden reserves nor promises supply, survival, destination capacity, or authorization; callers
must refresh inputs after changes and admit each actual batch normally.

Mining start validates authorization, available hardness evidence or opaque resistance failure, player location
inside the resolved deposit bounds when logistics is initialized, tool/destination access when those endpoints
are located, labor, capability, wear, destination storage, and reservation constraints. Its token binds logistics
revision alongside inventory/equipment/mining/support owners so a later location change cannot authorize stale
work; trusted-load mining-job validation replays spatial access while the job remains working.
Geology retains ownership of the selected batch during labor. Completion removes the batch from geology,
applies wear, releases player work, and creates an explicit durable claim boundary. Completed output remains
mining-owned with its destination capacity reserved until claim succeeds, so unrelated simulation time and work
can continue while a blocked claim is repaired without losing, duplicating, or silently storing matter.
Admission therefore reserves the eventual claim's material-lot identity and inventory/mining revision headroom.
A structurally supported destination also reserves one conservative structural revision per retained claim.
Mounting and unmounting a destination reproject those structural obligations atomically, so support recovery may
consume headroom that the affected claims no longer need after the move. Claim retirement consumes its remaining
reserved obligations rather than double-counting them as unrelated immediate work. Trusted load rejects persisted
mining custody that no longer has the headroom implied by its current support assignments.

## Production and processing

### Production jobs

`ProcessDefinition` owns immutable process identity and typed generic provider-capability requirements.
Exact material eligibility, quantity, and recipe semantics belong only to the execution-family resolver definition
that can physically interpret the selected lots; production does not author a second feed recipe. Resolver-owned
equipment capability IDs also appear as `AtLeast` process requirements so generic provider discovery and resolver
admission use the same capability dimensions. Operation-specific duration, yield, energy, wear, and dynamic batch
limits likewise belong in resolver output.

`ProcessResolution` binds one concrete operation to exact selected inputs, duration, output streams, and finite
resource consequences. Production reserves output capacity at start and owns consumed matter and modeled
in-process energy until completion. Inherited material storage exposure remains wall-clock based while work is
in process, including any suspension interval, so a blocked operation cannot preserve perishable matter merely
by remaining incomplete. Completion is revision-bound and conserves represented matter and modeled energy
across all streams.

Manual shaping conserves material identity and mass, preserves temperature, cannot change phase, and only emits
forms whose particle-size state is untracked. A manual definition may additionally expose a MassFlow-based
equipment profile. Optional profiles retain the authored fixed attention duration when no provider is supplied;
required profiles reject equipment-less work. A supplied provider resolves condition-adjusted throughput through
the normal equipment boundary, becomes production-occupied, wears for the exact active duration, and persists
its validated post-condition with the job. Equipment never mutates a process's yield: materially different yields
are separate authored transformations. Particulate
output requires an owner that defines particle-size state. `chip` and `scrap` outputs remain represented matter; no owner may reinterpret them as fuel or fresh
components without an explicit recovery process. Clean built-in copper scrap has two such routes: a slower
manual cold-work process reforms an exact reinforcement mass without phase change, while pure-copper melting
accepts authored copper ingot, reinforcement, native-metal, and scrap forms and resolves all of them through the
same conserved fusion physics. Pure stone scrap has a separate cold reknapping route with slower attention than
fresh lump knapping. Reknapping preserves the selected scrap temperature and cannot combine lots at
different temperatures because no thermal-mixing owner exists. Wood scrap and stone/wood chips remain
represented terminal matter; ore, crushed ore, and concentrate require a separate reduction/smelting owner.

Loss of required equipment or output support may suspend a job. Suspension preserves work-in-process,
reservations, and exact remaining active time. Production availability tracks persisted support-assignment epochs
from inventory and equipment plus the structural owner revision, so unrelated lot mutations, equipment wear, and
other owner changes do not force a full physical-availability reconsideration. The derived production dependency
snapshot itself is not persisted; trusted load therefore performs a conservative first availability pass after
rebuilding indexes. Production schedules also persist completed wall-clock suspension time so trusted load can
replay `completes_at = started_at + active_duration + completed_suspension_time` instead of trusting an arbitrary
due tick; the currently open suspension is excluded until resume. Suspended manual production releases
`PlayerWorkState`; resumption must reacquire labor and pass the remaining survival-budget admission.

### Physical resolvers

Powered ore-processing definitions share `PoweredOreProcessProfile` for throughput capability, batch limit,
energy carrier, mass-specific work, and active-tick wear. Runtime admission and trusted-load replay derive those
consequences from the shared profile; each resolver owns only its material transformation.

`assess_powered_ore_mass_envelope` is the read-only current-state planning surface for the scale dimensions
shared by powered comminution, screening, and constituent separation. It resolves the current equipment
provider, generic process capability requirements, condition-adjusted throughput and batch capacity, current
energy-supply access, stored work, transfer power, and condition lifetime. Its `maximum_mass()` is the minimum of
equipment capacity, finite-work capacity, and the mass whose throughput/energy duration still fits usable
condition life. `constraint_for` reports the first shared scale constraint in canonical powered-resolution
order. A caller-selected condition floor can further reduce the mass while remaining policy rather than
physical authoring. Replenishment projections keep the same currently bound store and therefore remain capped by
its authored total energy capacity as well as equipment capacity and condition lifetime; they never treat
"replenished" as an unlimited external source. The envelope can report that replenished constraint, the exact
stored work required by a requested mass, and the mass supported by a caller-supplied currently available energy
level without reconstructing mass-specific-energy arithmetic. The envelope is disposable guidance, not
authorization: a later owner mutation invalidates its assumptions, and the exact process-specific resolver must
still validate the selected matter before any production start can be authorized.

`project_powered_ore_order` is the bounded immutable-definition companion for workload decisions that span many
replenished powered batches. It sequentially reuses the same condition-adjusted capabilities, batch ceiling,
energy requirement, transfer timing, whole-tick rounding, and wear physics as runtime admission. The optional
`ServiceAtCritical` policy restores authored condition between projected batches so planning can account for the
batch fragmentation caused by a declared maintenance policy. It reports the exact projected batch energies,
service count, active duration, and final condition, but does not claim feed legality, future stored energy,
replacement material, service labor, survival reserve, occupancy, structural support, output capacity, or
runtime authorization. Those remain current-state owner facts and must be revalidated at action time.

`project_manual_ore_duration` is the narrower equipment-free direct-labor planning surface. It applies the same
authored manual-process batch envelope and whole-tick throughput rounding used by runtime manual comminution
without claiming that selected matter, output capacity, player attention, or survival reserve is currently
available. Manual ore profiles may additionally declare one optional MassFlow equipment capability. The
operation-specific resolver binds a current provider through the ordinary equipment boundary, derives
condition-adjusted duration and wear through the shared equipment throughput scheduler, and persists that
provider/outcome in the production job. Equipment never changes the manual transformation or recovery physics;
it only changes attention time and incurs wear. This keeps hand fallback, tool-assisted dressing, and powered
machinery as distinct investment scales rather than recipe aliases.

Implemented resolver contracts:

- **Comminution:** validates feed and output particle state, batch limits, condition-adjusted throughput,
  finite work energy, duration, and wear. Direct-labor and powered routes share the same material projection.
- **Dry screening:** partitions fully resolved particle classes around an authored aperture without inventing
  unresolved fractions. Same-form batches already wholly classified to one side of the aperture are rejected
  instead of consuming work energy and equipment condition for a no-op pass.
- **Constituent separation:** applies authored target and non-target recovery to liberated particulate feed.
  Unrecovered constituents remain in physical residue. Sorting and concentration preserve exact composition,
  use deterministic remainder allocation, and emit forms that prevent unsupported repeat-processing loops.
- **Thermal processing:** sensible heating, pure-material melting, and casting use exact selected matter, finite
  energy sources/sinks, equipment limits, phase boundaries, and latent heat. Ppm-weighted sensible heat is first
  resolved at femtojoule precision. Multi-lot heating sums exact trace energies before narrowing once at the
  aggregate energy-transaction boundary, so physically identical work does not become invalid merely because
  matter is split across lots. A runtime transfer whose aggregate is still not exactly representable in whole
  nanojoules is rejected rather than rounded down, while read-only material thermal accounting retains the
  remainder. Each
  pure phase-change definition
  binds one exact authored material rather than inferring material identity from the first selected lot. Melting
  owns a canonical nonempty set of accepted solid feed forms for that material and one liquid output form, so
  physically equivalent recovery feeds can share one fusion resolver without recipe aliases. Casting binds one
  liquid-to-solid form pair for its exact material and also owns the completed-solid temperature. Melting and
  casting both reject feed hotter than the equipment maximum temperature, so a warm solid below its melting
  point but above the furnace limit cannot be admitted. Persisted jobs
  replay the same material, accepted forms, and physical resolution used at admission.

## Equipment, labor, survival, energy, and fluids

### Equipment and maintenance

Capabilities use typed values and explicit `AtLeast`/`AtMost` requirements. Runtime providers expose
condition-adjusted capability through the same evaluation boundary as nominal definitions; failed equipment
provides no productive capability.

Equipment owns identity, condition, embodied traces, occupancy, and optional structural support. Fixed
machinery requires active support before new work starts. Mounted equipment contributes its own structural-load
channel.

Assembly consumes exact traces. Additive upgrades preserve identity, condition, prior embodiment, and the exact
maintenance profile that gives preserved condition its physical meaning while adding authored matter. An
upgrade therefore cannot move accumulated wear onto another component or silently change its service cost.
Disassembly and worn recovery are allowed only through authored routes. Maintenance is physical: aggregate
replacement consumes an exact commodity and emits conserved spent matter; traced component service replaces one
complete authored component, restores pristine condition because no residual component wear is separately
represented, and preserves unrelated traces and upgrades. Phase or particle transformations remain owned by
their physical process.

Built-in primitive copper reinforcement uses that additive path for extraction, geological sampling,
woodworking, manual power, crushing, grinding, and separation equipment. Reinforced equipment improves its
authored capability without replacing the instance or prior wear; processors may increase both flow and batch
capacity. Their condition curves degrade both productive flow and safe batch capacity;
component service replaces only the authored working component and leaves unrelated handles, frames, and copper
reinforcement embodied. Worn disassembly follows that same component ownership: the worn component enters its
authored spent form while unrelated embodied traces are recovered exactly. Stone working-component service and disassembly emit
the exact worn mass as stone scrap. Compatible pure scrap reknaps into reusable tool components through an authored
route; remaining scrap and chips stay represented.

### Player work and survival

`PlayerWorkState` allows at most one active player-attention operation across manual production, prospecting,
mining, manual power, eating, and drinking. Work admission binds the required metabolic-energy and hydration
budget. Registry assembly rejects authored exertion-bound work that has no physically executable full-reserve
route: fixed-duration work must fit complete reserves, manual ore processing must fit its maximum authored batch,
and manual power must have at least one pristine portable provider that can fully charge a compatible finite
store before condition or survival limits are exhausted. Suspended manual production releases attention;
resumption must reacquire it and revalidate the exact remaining budget.

Direct manual power requires portable unmounted equipment and a compatible finite energy destination. Duration
is limited by provider capability, destination input power, sustainable metabolic output, and requested work.
Energy creation, physiological cost, and equipment wear share one validated operation. Generated work remains
in player-work custody until completion, with the resolved per-tick exertion persisted as part of that durable
schedule so tick execution does not rederive admitted physics. Sink-capacity admission credits only passive
dissipation guaranteed before the release tick. Trusted load reprojects the same rule from current stored energy
and remaining work and rejects a persisted exertion value that disagrees with the canonical schedule.
`project_manual_power` is the read-only future-configuration surface for comparing authored provider/store
investments before those instances exist. It shares provider capability, destination input-power, schedule,
physiology, and wear physics with runtime admission, but deliberately assumes an empty or sufficiently free
future store and does not authorize current ownership, occupancy, stored energy, survival reserve, or revisions.
For existing instances, `assess_manual_power_energy_envelope` reuses the same current provider/store bindings and
canonical start semantics to find exact feasible generation below a caller limit while respecting optional
survival reserve floors. Because passive sink loss can make adjacent generated-energy requests non-monotonic, the
labor owner searches exact duration buckets rather than requiring callers to probe admission. It separately
reports the greatest generated work and the greatest post-work destination-store energy, since those need not be
the same request. `assess_manual_power_destination_target` finds the least exact generated work that reaches a
requested post-work store level, explicitly accounting for passive loss of preexisting stored energy during the
work interval and for completion-before-passive-loss ordering on the release tick. Both current-state projections
are disposable planning evidence; execution still requires a fresh `validate_start_manual_power` token.

`SurvivalState` owns metabolic energy, hydration, vitality, recent nutrition, terminal consumed matter/fluid
totals, and exact pending direct-consumption custody. Eating and drinking transfer selected physical quantities
into survival ownership at admission, then release their physiological energy, hydration, and nutrition over
the authored exclusive-attention interval. Uptake is allocated from cumulative integer fractions so intermediate
ticks cannot receive future benefit and the final tick recovers every whole-unit remainder. Each installment is
capped against the capacity remaining after that tick's basal/exertion expenditure. If starting reserves cannot
fully pay that expenditure, the installment first pays the exact energy/hydration shortfall; only its residual may
refill stored reserves, and starvation/dehydration damage is applied only when the installment cannot cover that
shortfall. Nutrition intake is available for same-tick recovery before decay. Admission therefore does not require
pre-action reserve headroom, because physiological expenditure during consumption can create capacity. Meals and
drinks are rejected outside their authored direct-consumption serving bounds; the built-in minimum meal aligns
the smallest serving with one full consumption tick instead of allowing milligram-scale micro-meals. A drink is
also rejected when it resolves to no whole-unit hydration benefit. Minimum serving bounds prevent physically
meaningless micro-consumption actions while remaining part of the same canonical duration, custody, and
physiological calculation. If the player
dies while an intake is pending, no further physiological benefit is released; pending survival custody and its
eating/drinking attention record are canceled together on the next authoritative tick.
Current-schema saves persist pending intake identity and timing, and trusted load validates that custody against
the matching player-work interval. Each pending intake also persists the cumulative terminal-consumption
baseline from immediately before admission, so trusted load requires `baseline + pending intake = current
cumulative total` rather than allowing unrelated historical consumption to satisfy pending custody.

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

The built-in copper-banded stone flywheel adds authored copper reinforcement to the ordinary stone-plus-wood
accumulator. It preserves carrier, transfer limits, and passive loss while raising stored-work capacity; exact
masses and capacities live in the authored definitions. The expanded reserve funds larger primitive-processing
batches through the same canonical energy path.

Fluid stores own identity, volume, temperature, capacity, revision, and optional structural support. A material
has at most one fluid identity in the current homogeneous-fluid model. Runtime supports exact withdrawal and
support changes; generic transfer, pumping, and mixing are absent. One fluid-owner mass projection converts
represented volume and authored density to exact micrograms; structural loading and thermal accounting consume
that same physical projection rather than re-deriving density arithmetic independently. Stored-fluid sensible
energy is projected exactly from represented mass, temperature, and specific heat; when the backing material has
authored fusion properties, the ledger also includes liquid latent heat without rounding sub-nanojoule remainders. Passive fluid heat transport
is absent. The thermal fate of food or fluid after either crosses the terminal survival-consumption boundary
is outside the explicit-energy ledger.

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

New systems define an immutable authored contract where appropriate, one owner for each consequential
fact, one canonical mutation path, persistence semantics, typed failures, and invariant coverage before
`STATUS.md` lists the capability as implemented.
