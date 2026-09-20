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
integrator must first prove equivalence across all tick phases it skips. Static `PeriodicSchedule` next-due
queries and persisted dynamic completion ticks identify boundaries; they do not establish that the interval
between those boundaries is semantically empty.

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
| Root simulation and time | core definitions, `WorldSeed`, typed quantities/time | `AppState::tick()`, immutable owner accessors | per-phase `decide_*` is crate-owned orchestration | `advance_tick`; direct clock/owner applies stay crate-private |
| Registries and built-in content | `Registries` plus domain registries; `build_registries` validates cross-references | public immutable `Registries::*()` accessors; derived authored topology may provide goal-directed reverse lookup | callers inspect authored possibilities; topology never claims current legality or ordinary reachability | none; registries and derived definition indexes are immutable after construction |
| Inventory and storage | material/form/storage definitions | `AppState::inventory()`, stockpile/lot records and stable iterators | feature owners construct explicit lot selections; enclosure validators derive storage consequences | no generic public transport command; feature-specific validators own ingress/egress/reform, enclosure, and support transitions |
| Geological knowledge | prospecting methods plus hidden finite geology | `AppState::geological_knowledge()`, `assess_geological_knowledge`, knowledge map | field prospecting authorization; evidence combination remains actor-safe | `validate_start_field_prospecting` -> tick records observations |
| Mining | `MiningRegistry` methods and physical hardness/tool constraints | `AppState::mining()` plus acquired geological knowledge; hidden `GeologyState` is not public | `resolve_mining_target` binds localized evidence and the best acquired physical hardness band when present | `validate_start_mining` requires acquired hardness evidence and uses its conservative upper bound -> tick -> `validate_claim_mining_output` |
| Production | `ProductionRegistry`, `ProcessDefinition` | `AppState::production()`, job records, reservations/occupancy | operation-specific resolvers produce `ProcessResolution` / `Resolved*` | `validate_start_process` / `validate_start_process_routed` -> tick completion |
| Equipment | `EquipmentRegistry`, capability/maintenance/upgrade profiles | `AppState::equipment()`, equipment records | `resolve_equipment_provider`, `resolve_equipment_maintenance` | assembly, upgrade, maintenance, disassembly, mount/unmount/relocate validators |
| Player labor | `LaborRegistry`, manual-power/prospecting definitions | `AppState::player_work()` | owner commands calculate/bind required attention and resource budget | manual power/prospecting/manual production commands -> tick; attention lifecycle is crate-owned |
| Survival | `SurvivalRegistry`, physiology, food/drink definitions | `AppState::survival()`, `assess_survival`, `assess_food_freshness` | consumption validators derive bounded direct intake and physiological schedule | `validate_eat` / `validate_drink` -> tick; `initialize_player_survival` is the ordinary initialization boundary |
| Energy | `EnergyRegistry`, store definitions, carrier/power contracts | `AppState::energy()`, store records, explicit energy accounting | process/manual-power resolvers use `validate_energy_supply` / `validate_energy_sink` as part of their plan | assembly/upgrade/disassembly validators; reserved consumption/release and passive loss apply through canonical owners/tick |
| Fluids | `FluidRegistry`, fluid definitions | `AppState::fluid()`, store records, fluid accounting | consumers validate exact egress internally; no generic routing planner exists | support validators and canonical consumers; generic transfer/pumping/mixing absent |
| Structures | `StructuralRegistry`, profiles and geometry | `AppState::structures()`, `analyze_structure`, `StructuralAssessment` | owner-specific support/load validation plans final aggregate load | support/load commits through inventory/equipment/fluid/structural owners; general player construction remains absent |
| Manual crafting overlay | `CraftingRegistry`, manual craft definitions, optional-or-required equipment profile | inventory, survival, authored craft definitions, optional equipment instance | `resolve_manual_craft` uses fixed authored duration when fallback is permitted, otherwise requires canonical condition-adjusted MassFlow | `validate_start_manual_craft` -> equipment-reserving production job + player work -> tick |
| Ore-processing overlay | `OreProcessingRegistry`, manual/powered process profiles | inventory, equipment, energy, production state | `assess_powered_ore_mass_envelope` for shared current scale bounds; `resolve_comminution_process`, `resolve_screening_process`, `resolve_constituent_separation_process` for exact selected-lot legality; manual counterparts expose their own resolutions | powered resolutions enter `validate_start_process*`; manual start validators also bind player work |
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
- `WorldSeed` is deterministic creation and persistence input, not public actor-readable runtime state; replay
  metadata retains the seed outside actor policy when diagnostics need it.
- `RandomState` is persisted and owns typed RNG streams derived from the world seed.
- Implemented authoritative physical calculations use checked integer arithmetic, not floating point.
- Dynamic scheduled work persists as explicit records. `PeriodicSchedule` is for static clock-derived
  phase scheduling.
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
| Hidden geology -> acquired knowledge | `validate_start_field_prospecting` -> simulation tick | `PlayerWorkState` holds exclusive prospecting labor; completion records one aggregate `GeologicalObservationRecord` or one bounded, spatially ordered per-voxel batch in `GeologicalKnowledgeState`, according to the authored method. All methods record bounded abundance. Authored physical-sampling methods may additionally record a conservative excavation-hardness band only when their material evidence has a positive lower bound; uncertain/negative evidence therefore cannot reveal hidden presence through hardness metadata. `TickOutcome::field_prospecting()` exposes the actor-safe first observation identity, count, and scope without deposit identity. |
| Acquired knowledge -> extraction authorization | `resolve_mining_target` | Read-only `MiningTargetResolution` proves that legitimate evidence currently resolves one extractable owner while keeping the geological deposit identity crate-private. No custody changes yet. |
| Geology + equipment + labor -> mining work | `validate_start_mining` -> simulation tick | Start binds the requested excavation effort, tool, destination, player attention, and a durable `MiningJobRecord`. Pre-admission batch/capacity/labor/wear feasibility is derived from the requested mass, not hidden remaining reserve, so read-only validation cannot be used to measure a deposit. The hidden output slice is `min(requested, remaining)` and is bound internally; after commit its reservation and eventual claim may legitimately reveal a short recovery. Geology keeps that output matter during labor; completion removes it from `GeologyState`, applies wear for the requested effort, releases attention, and places the physical output in mining-owned claim custody. |
| Mining claim custody -> inventory | `validate_claim_mining_output` | A ready job retains its reserved destination capacity until claim. Claim moves the exact output into `InventoryState`, retires mining custody without a second extraction decision, and returns `MiningClaimReceipt` with the exact contribution plus its merge-aware surviving lot identity. |
| Inventory + providers -> production work | resolver-specific `Resolved*` / `ProcessResolution` -> `validate_start_process` or `validate_start_process_routed` | Start consumes exact selected input into `ProductionState` work-in-process, reserves routed output capacity, binds provider occupancy, and records modeled finite-energy consequences needed for replay. |
| Production work -> inventory / equipment / energy | `advance_tick` completion planning and apply | Due completion routes exact material streams to reserved inventory destinations, applies condition consequences, releases/consumes modeled energy as resolved, clears occupancy/reservations, and emits `ProcessCompletion` through `TickOutcome`; each stream has merge-aware inventory landing identities. |
| Fatal survival -> active player-work disposition | `advance_tick` fatal-work planning | A work item due on the fatal tick completes normally before labor is released. Otherwise direct manual production is suspended with its work-in-process and output reservations intact; working mining is canceled without extraction or completion wear and returns its exact output reservation; storage-enclosure dismantling is canceled without removing the enclosure and returns its exact recovery reservation. Unfinished manual power and prospecting are pure `PlayerWorkState` continuations, so fatal interruption releases them without generated energy, acquired evidence, or completion wear. Maintenance has already committed its exact material/component exchange at admission, so interrupted service keeps that represented matter transition but receives no deferred condition recovery. The resulting idle/dead player state must pass the same trusted-load reconciliation as persisted continuation. |
| Inventory -> equipment embodiment | `validate_assemble_equipment` / `validate_upgrade_equipment` | Exact material traces leave inventory custody and become `EquipmentState` embodiment. Upgrade preserves equipment identity and prior embodiment/condition while adding only the authored trace; inherited capabilities cannot regress at the preserved condition according to each capability definition's explicit improvement direction. |
| Equipment embodiment -> inventory recovery | `validate_disassemble_equipment` / `validate_equipment_maintenance` | Disassembly returns authored recoverable traces as inventory lots. Maintenance admission commits the exact replacement/component exchange and represented spent matter immediately while `PlayerWorkState` owns the timed service interval; equipment condition recovery occurs only at completion. If labor terminates before that completion, the committed material exchange remains and no condition gain is granted. |
| Inventory -> finite energy-store embodiment | `validate_assemble_energy_store` / `validate_upgrade_energy_store` | Exact material traces become `EnergyState` embodiment; upgrade preserves store identity and carrier/transfer semantics while changing the authored store definition. `validate_disassemble_energy_store` is the exact reverse custody route for empty idle stores. |
| Inventory enclosure matter <-> inventory storage profile | `validate_build_storage_enclosure` / `validate_start_storage_enclosure_dismantling` -> simulation tick | Inventory remains the material owner while `PlayerWorkState` owns the timed dismantling interval. Start reserves the recovery destination and binds survival/labor; the enclosure and its preservation profile remain authoritative until completion, when exposure is checkpointed, ambient storage is restored, and exact enclosure matter enters the recovery stockpile. |
| Inventory / equipment / fluid -> structural load | owner-specific mount/unmount validators | The mounted owner retains object custody while `StructureState` owns its source-separated load. Final aggregate load is validated before support assignment changes, and returned support outcomes expose the structural consequence when the represented load actually changes; force-rounded load no-ops remain revision-bound without synthesizing an analysis. |
| Player physiology + equipment -> stored mechanical work | `validate_start_manual_power` -> simulation tick | `PlayerWorkState` owns pending generation during direct labor; admission binds survival budget, equipment wear, and energy-store capacity. Completion deposits exact work into `EnergyState`, applies wear/physiological expenditure, releases attention, and exposes `ManualPowerOutcome`. |
| Inventory / fluid -> terminal survival consumption | `validate_eat` / `validate_drink` -> simulation tick | Admission transfers selected matter/fluid into `SurvivalState` pending-consumption custody, reserves exclusive attention, and returns the exact completion tick with the accepted intake outcome. Tick installments release only earned physiological benefit; terminal consumed totals retain represented custody after the explicit food/fluid simulation boundary. |
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
General stockpile transport is not implemented. The gameplay harness may authorize controlled conserved
transfers only as setup or controlled-event infrastructure.

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
The target and recovery stockpiles must be unmounted; the target must have no reserved inbound work and remain
valid under the ambient storage profile. Admission reserves exact recovery capacity and starts exclusive timed
player work with authored survival exertion. The enclosure remains installed throughout the interval, so retained
lots continue aging under its current preservation multiplier. Completion checkpoints that exposure, restores
ambient storage, and returns the enclosure traces to the distinct recovery stockpile with their exact temperature,
composition, particle state, and provenance. Recovery capacity, lot-ID space, inventory revisions, and competing
delayed-output ownership are validated before admission/completion mutation. General world-space demolition,
access, and dismantling tools remain outside this transition.

Detached enclosure bodies may then be reused intact or entered into explicit manual salvage. Timber salvage
conserves the full body mass as boards plus represented chips; the stone crock converts its exact body into
reworkable stone scrap for the reknapping route. Exact salvage yields and attention costs live in the authored
definitions.

Structural support links require positive-area voxel contact: overlapping bounds are admissible, as are bounds
that abut on one axis while overlapping on the other two. Edge-only and corner-only contact cannot carry a load
path. Trusted-load validation replays the same rule, so persisted topology cannot bypass runtime support
admission.

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

Physical sampling is also the information boundary for extraction resistance. A resolved target carries the best
acquired excavation-hardness band that covers its localized evidence region. Mining start rejects a target with
no acquired hardness band rather than probing hidden geological resistance, and compares the selected tool only
against the conservative acquired upper bound. The exact hidden hardness remains authoritative physical truth for
geology ownership and trusted continuation validation, but it is not a read-only planning oracle. Trusted load
rejects a persisted physical hardness band that excludes any still-live matching deposit in its sampled region;
depleted historical observations remain valid because they can no longer authorize extraction.

`resolve_mining_order` is a bounded read-only effort projection over authored method/equipment definitions,
initial condition, an acquired hardness upper bound, requested mass, caller-selected batch mass, and a maximum
batch count. It reuses admission physics sequentially, including wear, remainder batches, per-batch tick rounding,
and checked total duration. It rejects invalid inputs, exceeded search bounds, and typed batch physics failures.
It neither reads hidden reserves nor promises supply, survival, destination capacity, or authorization; callers
must refresh inputs after changes and admit each actual batch normally.

Mining start validates authorization, acquired hardness, tool, labor, capability, wear, destination, and
reservation constraints.
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
reservations, and exact remaining active time. Production schedules also persist completed wall-clock suspension
time so trusted load can replay `completes_at = started_at + active_duration + completed_suspension_time` instead
of trusting an arbitrary due tick; the currently open suspension is excluded until resume. Suspended manual
production releases `PlayerWorkState`; resumption must reacquire labor and pass the remaining survival-budget
admission.

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
physical authoring. The envelope is disposable guidance, not authorization: a later owner mutation invalidates
its assumptions, and the exact process-specific resolver must still validate the selected matter before any
production start can be authorized.

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
in player-work custody until completion. Sink-capacity admission credits only passive dissipation guaranteed
before the release tick. Trusted load reprojects the same rule from current stored energy and remaining work.

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
- persisted RNG, schedules, and operation-specific physical replay.

New systems define an immutable authored contract where appropriate, one owner for each consequential
fact, one canonical mutation path, persistence semantics, typed failures, and invariant coverage before
`STATUS.md` lists the capability as implemented.
