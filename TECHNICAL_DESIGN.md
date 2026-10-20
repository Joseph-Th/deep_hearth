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

`advance_control_tick(registries, state)` is the canonical 20 Hz direct-control mutation. It advances one
`ControlTick`, integrates any active generated-world player body, and every 72nd call executes the same
slow-simulation transaction used by `advance_tick`.

`advance_tick(registries, state)` is the atomic headless/process helper for exactly one next slow boundary only
when no generated-world player body has active control-rate physics. It fails with
`ControlSteppingRequired` rather than skip gravity/collision. Direct-control adapters and generated-world body
continuation therefore advance through `advance_control_tick`.

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
| Root simulation and time | core definitions, typed quantities/time | `AppState::control_tick()`, `AppState::tick()`, immutable owner accessors | per-phase `decide_*` is crate-owned orchestration | `advance_control_tick` is the canonical 20 Hz step and owns active body physics; at a slow boundary it settles prior locomotion exertion into the slow transaction, then decides current body motion from the post-transaction state before direct interactions observe that committed body state; a fatal slow tick clears any buffered direct intent inside the slow transaction before state validation, and the corresponding control step reports the canceled intent as `PlayerDead`; `advance_tick` atomically advances one slow boundary only when no generated-world body requires those intervening control steps; direct clock/owner applies stay crate-private |
| Registries and built-in content | `Registries` plus domain registries; `build_registries` validates cross-references | public immutable `Registries::*()` accessors; derived authored topology may provide goal-directed reverse lookup | callers inspect authored possibilities; topology never claims current legality or ordinary reachability | none; registries and derived definition indexes are immutable after construction |
| Block world | `WorldRegistry` block/state definitions, excavation profiles, canonical `CubeFace`, fixed-point `ViewDirection`, authored eye/reach geometry, plus persisted `WorldGenerationProfile` | `AppState::world()`, generated chunk/section records, exact block lookup/represented mass, and bounded actor-safe `raycast_player_block` | `validate_generate_chunk` / `validate_generate_player_neighborhood` derive deterministic terrain; targeting derives stance-specific eye origin; exact excavation admission binds the first hit, authored mining physics, portable tool, batch mass, and one durable output-custody route | excavation reserves tool occupancy and attention, then chooses reserved carried custody only when the complete batch fits; otherwise completion creates one finite loose surface-resource drop at the player's admitted work voxel. Sparse cell mass preserves partial excavation, the start/completion receipts expose selected/landed custody, and trusted load validates future surface revision/identity headroom plus persisted drop position/kind. Local player-created drops auto-transfer as much mass as currently fits on control time, preserving any remainder in the same world custody. `PlayerDirectControlInput` can switch one of nine persisted semantic quick-access slots before resolving primary/use/drop on the same control sample; slot selection and item binding require living player admission, and their commits reject survival-stale authorizations; every requested primary, secondary, or drop action checks live player admission before contextual resolution and reports typed unadmitted/dead outcomes instead of guessing missing equipment or selection. Primary excavation derives applicable canonical mining methods from the newly selected carried tool and pointed target, then selects the best authorized throughput; when the full tool batch exceeds current labor, condition, schedule, or output-custody limits, it searches for the largest admissible positive mass through the same exact admission validator rather than rejecting a smaller feasible extraction. Secondary use shares one generated-air/body/object placement-target resolver: finished storage uses its storage owner, while one visible wood-log stack on an adjacent terrain top face derives the authored basic timber-post geometry and delegates to atomic grounded construction. Broader block placement, side-supported structures, rendered item entities, richer shapes, and residency/unload remain open |
| Inventory and storage | material/form/storage definitions | `AppState::inventory()`, stockpile/lot records, revision-bound `MaterialStackProjection`, and `project_player_inventory` for carried stacks/equipment/hotbar plus exact-local stationary equipment and containers | feature owners construct exact lot selections internally; `validate_quick_move_player_stack` checks canonical ground locality and player liveness before reading destination capacity, then derives the maximum fitting carried↔local mass and delegates deterministic lot slicing and relocation to canonical owners; equipment pickup/drop remains its own carry validator | no second inventory/item owner exists: feature-specific validators still own matter transfer and equipment custody; adapter presentation consumes projections/actions only |
| Surface gathering | `LaborRegistry` gathering methods plus finite loose surface-resource records; each method separates a familiar direct-use batch from its larger legal batch ceiling | `AppState::available_surface_resources()` for exact-local resources, `available_player_world_drops` for exact-local dropped-inventory presentation, and `available_adjacent_surface_resources` for traversal-gated orientation | `validate_start_player_surface_gathering` derives carried custody for deliberate gathering. Empty-hand secondary use resolves exactly one co-located natural opportunity to exactly one authored method, clamps its direct batch to remaining carried capacity, and rejects ambiguous local contexts rather than choosing silently; the drop projection excludes natural opportunities by construction | `advance_tick` atomically depletes deliberate gathering into reserved inventory; control ticks transfer up to current carried capacity from one local `Drop`, leaving any remainder conserved under the same surface identity. Generated opportunities never auto-pick |
| Geological knowledge | prospecting methods plus hidden finite geology | `AppState::geological_knowledge()`, `assess_geological_knowledge`, knowledge map, `player_prospecting_options` region-scoped technique catalog | player/evaluator selects from authored footprint, evidence quality, projected work cost, accepted instrument definitions, and exact-local usable instruments; field prospecting authorization still revalidates runtime state | `validate_start_field_prospecting` -> tick records observations |
| Mining | `MiningRegistry` methods and physical hardness/tool constraints | `AppState::mining()` plus acquired geological knowledge; hidden `GeologyState` is not public | `resolve_mining_target` binds localized evidence and the best acquired physical hardness band when present | `validate_start_mining` uses acquired conservative hardness when present; unsampled localized attempts use hidden hardness only for opaque tool-sufficiency admission -> tick automatically lands ordinary output and exposes `TickOutcome::mining_output()`; physically blocked output remains internal recovery custody and retries on later ticks |
| Production | `ProductionRegistry`, `ProcessDefinition` | `AppState::production()`, job records, reservations/occupancy | operation-specific resolvers produce `ProcessResolution` / `Resolved*` | ordinary player execution uses `validate_start_player_process*` to require complete player admission and prove exact-local endpoint/resource access before capability-level `validate_start_process*`; controlled capability probes may enter `validate_start_process*` directly -> tick completion |
| Equipment | `EquipmentRegistry`, capability/maintenance/upgrade profiles | `AppState::equipment()`, equipment records; `AppState::available_local_equipment()` resolves stationary or player-carried equipment at the actor voxel | `resolve_equipment_provider`, `resolve_equipment_maintenance` | `validate_pickup_equipment` / `validate_drop_equipment` move portable equipment between stationary logistics custody and the physiology-sized player carrying envelope; direct control composes the same drop validator for the active carried selection and exposes success/rejection without another custody path; assembly, upgrade, disassembly, and mount/unmount/relocate remain owner-level capability boundaries for controlled setup; once ordinary player admission exists they bind the living survival snapshot before mutation; mass-changing upgrade/disassembly and structural mounting require carried equipment to be dropped first; maintenance additionally reserves timed player work |
| Player labor | `LaborRegistry`, manual-power/prospecting/structural-construction definitions | `AppState::player_work()` | `project_manual_power` projects immutable future provider/store physics; `assess_manual_power_energy_envelope` and `assess_manual_power_destination_target` bind current provider/store state for exact read-only generation and post-work stored-energy planning; structural construction derives duration from exact member mass; runtime owner commands still own authorization, attention, survival budget, and revisions | manual power/prospecting/manual production/structural-construction commands -> tick; attention lifecycle is crate-owned |
| Player admission | survival physiology plus root survival/logistics/inventory/world owners | pre-admission owner state only | `validate_admit_player` binds authored full reserves, fixed-point body placement, collision-free current world state, and physiology-sized carried capacity against one snapshot | `ValidatedPlayerAdmission::commit` prechecks survival/logistics/inventory/world freshness, then establishes survival, body state, and carried inventory custody atomically; controlled spatial-proxy fixtures may seed survival without claiming locality |
| Survival | `SurvivalRegistry`, physiology, food/drink definitions | `AppState::survival()`, `assess_survival`, `assess_food_freshness` | consumption validators derive bounded direct intake and physiological schedule; `validate_eat_stack_to_full` resolves one revision-bound projected food cohort to private lot slices | `validate_eat` / `validate_drink` -> tick; ordinary initialization is owned by `validate_admit_player`, while survival-only initialization is fixture-scoped |
| Energy | `EnergyRegistry`, store definitions, carrier/power contracts | `AppState::energy()`, store records, explicit energy accounting | process/manual-power resolvers use `validate_energy_supply` / `validate_energy_sink` as part of their plan | assembly/upgrade/disassembly remain owner-level capability boundaries for controlled setup; once ordinary player admission exists they bind the living survival snapshot before mutation; reserved consumption/release and passive loss apply through canonical owners/tick |
| Fluids | `FluidRegistry`, fluid definitions | `AppState::fluid()`, store records, fluid accounting | consumers validate exact egress internally; no generic routing planner exists | support validators and canonical consumers; generic transfer/pumping/mixing absent |
| Structures | `StructuralRegistry`, profiles and geometry plus `LaborRegistry` hand-setting method | `AppState::structures()`, `analyze_structure`, `StructuralAssessment` | owner-specific support/load validation plans final aggregate load; the two-step plan/start API remains available, while stack-facing player actions compose one revision-bound carried stack with either reachable grounded geometry or a non-empty stable set of face-contacting active supports and precheck planning, exact materialization, survival/attention, and timed work before mutation | support/load commits through inventory/equipment/fluid/structural owners; atomic carried-stack construction inserts the inert member, embodies exact matter, and starts hand-setting only after all prechecks succeed. Structural construction/dismantling uses the same authored eye-to-target interaction-reach envelope as familiar first-person targeting, still rejects body overlap, and does not grant remote world access. Supported framing establishes every validated support edge in the same plan revision, so unsupported non-grounded state is never exposed; in-progress supported work survives trusted save/load, remains Planned/non-load-bearing, and activates on completion. Selected logs place ordinary framing; selected boards may place one 1 m² × 10 mm, 6.5 kg supported timber cover panel from the same ray/stack path. Load analysis still uses the existing axial model; broader panel families/joints, plate bending, and general terrain-support physics remain absent. |
| Initial wilderness generation | surface-resource records, fluid-store records, logistics locations/opening-access edges, one kobold colony patrol, and `WorldGenerationSeed` as explicit fresh-world input rather than persisted random state | adjacent resource and drink projections expose only opening-edge immediate-neighbor opportunity, while `available_nearby_kobold_scouts` exposes scout position, noticed-drop history, and any presently carried parcel's material/mass only near the admitted player; exact surface mass and fluid volume remain exact-local | `validate_generate_initial_wilderness` derives a bounded six-edge horizontal opportunity graph: forage and a water branch lead to a work junction with stone and timber leaves; a scout den on one leaf begins moving after 180 slow ticks (10.8 minutes), thereafter stepping every five slow ticks (18 seconds), via the junction toward the other leaf | commit prechecks surface, fluid, logistics, and pristine kobold state before establishing custody, locations, traversal, and colony. Movement uses persisted route edges, and on generated terrain each destination must retain a solid floor and two-block air clearance. An obstructed, unsupported, or unavailable destination pauses the scout without advancing its route cursor. Any positive exposed loose parcel can induce an early return when den capacity remains; a portable parcel at most 1 kg and fitting the den moves whole, while larger piles are split into at most 250 g daughter parcels at one physically adjacent return step. Existing portable lots take priority, and stable surface identity selects among exposed piles. A bulky pile on a side branch does not attract a remote detour; the scout must physically reach it on its route. Along its return the scout moves at most one parcel per step through the canonical surface owner, preserving ID, mass, composition, provenance, age, and recovery through ordinary pickup. Splitting a larger Drop also uses the canonical surface owner: two reserved revisions create one source-derived daughter parcel at the adjacent destination and subtract the exact mass from the original ID, leaving both lots recoverable and without duplicating matter. A living player within one horizontal voxel of the scout forces it toward the den without new theft and makes it abandon any already-carried parcel at its present location, even if its retreat path is blocked. Either this encounter or local bright firelight establishes a persisted 40-slow-tick caution period, refreshed on further danger, during which the scout stays in the den rather than immediately renewing its outbound patrol. Caution expires and ordinary scavenging resumes even if the player left; local scout sightings expose wary behavior without leaking the deadline or remote position. This provides a noncombat defense without revealing remote patrol events. One persisted parcel ID connects consecutive steps; trusted load checks route and surface custody even after legitimate player interception. Arrival at the den releases the parcel into local surface custody. In the absence of portable drops or a nearby living player, the scout also takes up to 100 g from one finite food opportunity on its adjacent return leg and creates a new source-derived loose parcel at the den. Source depletion and new loose custody are reserved as one conserved surface transaction, including material profile, provenance, and ambient storage age; a persisted next-forage deadline enforces a minimum 100 slow ticks (six minutes) between successful catches. The den has 500 g of physical staging space measured from all available `Drop` matter at its voxel, including player-created parcels. The scout will not take cargo that cannot fit; a den filled during a return trip forces the scout to leave the original parcel at its last reachable location. Foraged food is truncated to remaining den space; a full den halts further source depletion without consuming cooldown. Player recovery through normal local automatic pickup frees space and permits later forage. Guarding the source physically obstructs access, and the loot is recoverable with ordinary pickup. The same scout movement uses `assess_local_illumination`: entering any cell at light level 3 or more is blocked on an outbound patrol, and an already-illuminated scout retreats toward its den and relinquishes existing dragged matter. Block-aware occlusion and dynamically fading combustion energy therefore make a player-built, fueled campfire a temporary alternative to standing guard; no durable light or enemy-vision state is invented, and fading brightness eventually admits return foraging. From the junction, a scout first searches adjacent authored traversal edges for a portable unattended loose parcel that fits its den; failing that it may plan one further edge toward a similarly portable camp cache. Stable source ID selects the target, and an admissible unlit, physically clear intermediate branch is retained as an explicit route leg. The authored outer patrol stop remains immutable. Stable surface-resource identity selects among candidates; the destination must also have standing clearance, no living-player occupancy, and illumination below the deterrent threshold. Only a successfully moved step starts the separately persisted detour, including an optional second destination; the scout can reach step three and then return cell by cell with the same source parcel, clearing the plan at the den and restoring the authored patrol. The same player, terrain, illumination and den-capacity gates apply at each leg; heavy distant piles do not attract an unliftable two-edge raid. Trusted load rejects missing, untraversable, or cursor-inconsistent two-edge detours; required goal and caution fields advance the save schema to 106 without legacy migration. Trusted load checks nonempty caution deadlines for five-tick alignment, earliest possible encounter and no more than 40 ticks beyond current authoritative time. A nearby player can still scare the scout off before it lifts the parcel, and remote steps do not emit a player event. This is a bounded two-edge graph search rather than general pathfinding, combat, pursuit, ecology, hydrology, or unrestricted remote discovery |
| Manual crafting overlay | `CraftingRegistry`, manual craft definitions, optional-or-required equipment profile | inventory, survival, authored craft definitions, optional equipment instance; `player_manual_craft_options` projects carried recipe/batch availability | `project_manual_craft_hand_work` projects authored fallback duration; ordinary durable recipe inputs are resolved privately from carried custody while freshness-sensitive inputs require one explicit revision-bound `MaterialStackProjection`; `resolve_manual_craft` remains exact matter/equipment physics authority | `validate_start_player_manual_craft` / `_from_stack` compose familiar recipe + amount + optional tool into in-place `validate_start_manual_craft` -> equipment-reserving production job + player work -> tick |
| Powered crafting overlay | `CraftingRegistry`, `PoweredCraftDefinition` referencing one manual material transform, required MassFlow capability, carrier, specific energy, and wear | inventory, equipment, energy, production state; referenced manual transform remains material/yield authority | `resolve_powered_craft` binds exact selected matter, reuses the referenced transform's batch/output construction, resolves condition-adjusted machine throughput, and validates finite carrier-compatible stored work | ordinary starts use `validate_start_powered_craft`, which requires a living fully admitted player and exact-local access to both stockpiles, the machine, and the energy store before creating a production job with reserved equipment/energy and no player-work claim; capability-only evidence resolves first and enters generic production admission directly -> tick |
| Ore-processing overlay | `OreProcessingRegistry`, manual/powered process profiles | inventory, equipment, energy, production state | `assess_powered_ore_mass_envelope` for one-batch current scale bounds; `project_powered_ore_order` for bounded replenished-work orders with carried wear and optional critical-band service; `resolve_comminution_process`, `resolve_screening_process`, `resolve_constituent_separation_process` for exact selected-lot legality; manual counterparts preserve an equipment-free fallback and may bind one authored condition-adjusted MassFlow tool/workstation | ordinary powered resolutions enter player-authorized `validate_start_player_process*`; capability-only evidence may enter generic production admission; manual start validators bind player work and reserve optional equipment through the same production job |
| Thermal overlay | `ThermalRegistry`, heating/melting/casting definitions | inventory, equipment, energy, production state | `assess_melting_lot_mass_envelope` and `assess_casting_lot_mass_envelope` for current homogeneous-lot scale bounds; `resolve_sensible_heating_process`, `resolve_melting_process`, `resolve_casting_process` for exact selected-lot legality | ordinary resolved work enters player-authorized `validate_start_player_process*`; capability-only evidence may enter generic production admission; tick applies outputs, wear, and energy consequences |
| Conservation/accounting | authored material/fluid/energy properties | `calculate_matter_accounting`, `calculate_fluid_volume_accounting`, `calculate_explicit_energy_accounting` | read-only reconciliation only | none; accounting never mutates or authorizes custody |
| Persistence | current save schema + registry schema | `SaveEnvelope` for output, decoded `LoadedSaveEnvelope` before trust | exact-version admission plus deterministic index rebuild/graph validation | `LoadedSaveEnvelope::into_state`; adapters own bytes/storage, not state promotion |
| Presentation definitions | texture/shader registries and authored assets | immutable definition access and deterministic bake/assembly results | deterministic renderer-neutral assembly | graphics resources/frame effects belong to adapters, outside `AppState` |

If a caller needs a surface not shown here, determine whether the missing surface belongs to the canonical owner
or whether the caller is crossing an ownership boundary it does not control.

## Global runtime facts

- `ControlTick` is the persisted high-frequency direct-control/kinematic clock at 20 Hz (50 ms). `SimulationTick` is the persisted slow physical/accounting/calendar clock; exactly 72 control ticks form one 3.6-second simulation quantum. `TickSpan` remains relative slow-simulation duration.
- The built-in calendar maps 24,000 simulation ticks to 86,400 seconds. Calendar acceleration therefore does not define the direct-control timestep.
- Rate-authored slow physics integrate against the 3.6-second physical simulation quantum. Slow-owned survival, production, mining-job, thermal, storage-age, and similar `TickSpan` schedules remain on that exact accounting clock. Responsive block excavation is an explicit exception: its authored `TickSpan` duration, exertion, and wear are converted to exact `ControlTick` substeps with cumulative rounding that preserves the whole authored total.
- Work scheduled directly in `TickSpan` may be admitted only on an exact control/simulation boundary. Validation tokens that create such slow work bind the exact `ControlTick`, so a validate/commit race across a 50 ms control advance cannot shorten the authored interval. Responsive operations that explicitly convert an authored span to exact control time, currently block excavation, may admit off-boundary while preserving the same physical total.
- Familiar one-shot direct input is not discarded merely because its slow owner cannot admit work on that control sample. Logistics may persist exactly one off-boundary direct-action request while reserving the revision required to clear it. Contextual use stores request tick, current voxel, active selection, and fixed-point view direction; structural break additionally stores the exact targeted member so delayed admission cannot turn into terrain excavation or a replacement-member action. The next slow boundary clears the buffer and re-resolves the canonical action against current owner state; changed locality, selection, or structural target yields a context-change outcome rather than stale authorization. Fresh same-kind boundary input supersedes older buffered intent. A persistable state may contain that buffer only before its due boundary.
- Runtime has no stochastic owner; authoritative results derive from persisted state and explicit inputs.
- The first terrain chunk uses an explicit caller-provided seed/state pair and persists that immutable generation
  profile. Later player-neighborhood demand derives missing chunks from the same identity without ambient RNG or
  caller-retained policy. Initial wilderness generation separately persists distributed opening resources/edges.
- Implemented authoritative physical calculations use checked integer arithmetic, not floating point.
- Dynamic scheduled work persists as explicit records.
- Known due simulation ticks may bound batched caller stepping, but do not authorize skipping intervening canonical control-tick semantics.
- `advance_control_tick` is the high-frequency mutation surface. It advances the control clock and, when the admitted player's current footprint has generated chunk truth, applies the authoritative vertical body step; every 72nd call also executes one complete slow-simulation transaction. `advance_tick` invokes that slow transaction atomically only when no generated-world body requires intervening control steps.
- The slow transaction decides all fallible phase work against one pre-tick snapshot. Its application stage
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
| `FluidState` | `AppState::fluid()` plus `AppState::available_local_fluid_stores()` | public read plus exact-player-local observation | Finite homogeneous fluid stores and support assignments, including the one generated finite opening-water opportunity; this does not imply general hydrology, refill, or remote source generation |
| `EquipmentState` | `AppState::equipment()` plus `AppState::available_local_equipment()` | public read plus exact-player-local observation | Equipment instances, condition, embodied traces, support assignments; actor-safe local observation excludes remote and unlocated instances |
| `StructureState` | `AppState::structures()` | public read | Members, topology, embodied matter, source-separated loads, damage |
| `WorldState` | `AppState::world()` | public read | Generated 16x16 chunk columns containing sorted 16-cubed sections with local block-state palettes, bit-packed indices, exact non-air/represented-mass summaries, sparse partial-cell mass overrides, and a revision; missing chunk means no generated world truth and a missing section inside a generated chunk means canonical air |
| `SurfaceResourceState` | `AppState::available_surface_resources()` plus `available_adjacent_surface_resources` | public exact-player-local observation plus traversal-gated immediate-neighbor orientation | Finite loose world-space matter, persisted `Opportunity`/`Drop` interaction kind, lifecycle, and depletion, including generated opening-graph forage/stone/timber and player-created excavation drops; exact remote state, arbitrary-voxel queries, and persistence remain privileged |
| `KoboldState` | `available_nearby_kobold_scouts` plus `TickOutcome::kobold_scout()` | public player-local sighting and event only | One generated kobold den, deterministic three-cell back-and-forth scout patrol, delayed first outing, and persisted evidence of observed player-created drops. The scout does not attack, take custody of materials, or advance technology. Blocked generated terrain prevents a patrol step. No renderer/creature body yet |
| `GeologyState` | `AppState::geology()` | core-only hidden truth | Finite hidden geological deposits and depletion; actor code must not enumerate this owner |
| `GeologicalKnowledgeState` | `AppState::geological_knowledge()` | public actor-safe evidence | Acquired bounded observations only |
| `InventoryState` | `AppState::inventory()` plus `AppState::available_local_ground_stockpiles()` | public read plus exact-player-local ground-storage observation | Stockpiles, material lots, reservations, routing, preservation, material-backed storage enclosures, stockpile support |
| `LogisticsState` | `AppState::logistics()` plus `AppState::available_adjacent_traversal_destinations()` | public read plus current-player opening-graph neighbors | Fixed-point player body with derived voxel locality, normalized persisted opening-access edges, finite carried-stockpile identity, sorted player-carried portable-equipment identities, nine semantic quick-access bindings plus selected slot, at most one slow-boundary direct-action buffer, stationary stockpile/equipment/energy/fluid locations, and world-space custody revision |
| `ProductionState` | `AppState::production()` | public read | Active jobs, schedules, routing, exclusive resource occupancy |
| `MiningState` | `AppState::mining()` | public read without hidden deposit identity | Mining work-in-process, blocked-output recovery custody, and schedules |
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
player locality + carried rigid matter -> grounded planned structural member -> timed hand-setting -> active load-bearing member
finite loose surface matter -> timed same-voxel gathering -> inventory lot
explicit fresh-world seed + spawn voxel -> deterministic opening-graph forage/stone/timber -> SurfaceResourceState
explicit fresh-world seed + spawn voxel -> finite optional-branch water + five traversal edges -> FluidState + LogisticsState
explicit fresh-world seed + chunk coordinate + authored solid block state + material density -> deterministic mass-backed terrain chunk -> WorldState
player-eye target + carried tool + mining physics -> timed block excavation -> reduced cell mass + carried inventory lot or local loose surface matter
pointed face + selected finished storage body -> validated adjacent generated-air placement -> located ground storage; exact-local pickup/drop <-> player-carried inventory lot
derived player voxel + persisted opening-access edge -> actor-visible adjacent opening destination/source orientation; generated world + control input + carried load + physiology -> 20 Hz collision-resolved logistics body motion + settled locomotion exertion
generated chunk truth + persisted player body + gravity -> 20 Hz vertical sweep/contact -> logistics body + pending fall impact -> survival vitality
inventory traces -> equipment @ player voxel -> optional structural support
exact-local equipment observation -> player tool/machine selection -> canonical owner action
inventory traces -> finite energy store @ player voxel
finite fluid store @ world voxel -> exact-local observation -> survival-filtered drinking / structurally supported vessel
embodiment -> authored maintenance, disassembly, dismantling, or salvage -> inventory traces
finite energy store -> reserved/consumed process energy -> modeled work/heat or explicit loss sink
hidden geology -> bounded prospecting observation -> geological knowledge -> mining authorization
structure -> support assignment -> source-separated load -> availability/failure consequence
survival reserves + PlayerWorkState -> timed direct labor -> physical operation consequence
survival + logistics + carried inventory -> atomic ordinary player admission
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
| Survival + logistics + carried inventory -> ordinary player | `validate_admit_player` | Admission authorizes survival reserves, player world position, and the physiology-sized inventory-owned carried stockpile against one pre-admission snapshot. Commit prechecks survival, logistics, and inventory revisions before any mutation. Spatial-proxy gameplay fixtures may seed survival without logistics only when they explicitly do not claim ordinary locality. |
| Explicit terrain input -> generated block world | `validate_generate_chunk` -> `ValidatedChunkGeneration::commit` | A seed, representable chunk coordinate, and registered non-air block state deterministically resolve one compact terrain column. Full-cube represented mass is derived from authoritative material density, persisted section summaries are recomputed at trusted load, and whole-world matter accounting includes generated terrain. Validation rejects existing chunks, unknown states, coordinate overflow, and exhausted revision capacity. Commit binds the world revision and refuses replacement. Generated chunks persist as authoritative world truth; no ambient randomness or generic public cell setter is involved. This proves storage/generation, not streaming, geology/biome integration, richer shapes, or placement. |
| Persisted player body + generated world -> visible block target | `ViewDirection::new` -> `raycast_player_block` | The adapter supplies only a normalized millionth-unit direction. The world owner derives eye origin from the admitted player's persisted body stance and authored 1.62/1.27 m eye height, traverses generated cells with checked integer DDA up to the authored 4.5 m reach, and returns the first targetable `FullCube` state with canonical entry `CubeFace`. Exact reach boundary contact is admitted; corner/edge ties have deterministic axis precedence. Missing chunks or vertical authority fail closed rather than becoming transparent air. This is actor-safe observation only and does not authorize breaking, placement, or remote state enumeration. |
| Visible block + portable tool + labor -> partial terrain excavation -> carried inventory or local loose surface matter | `validate_start_player_block_excavation` -> control/slow tick | Admission reruns bounded targeting, proves an authored excavation profile, resolves existing mining hardness/throughput/batch/wear physics, requires exact-local unmounted equipment, and persists target state, represented source mass, tool trace, output custody, and schedule in `PlayerWorkState`. When the complete batch fits carried custody, admission reserves inbound mass plus future inventory/lot headroom; otherwise it reserves future surface revision/resource identity and anchors a `Drop` to the admitted player's work voxel. Completion atomically creates the exact fragmented lot or finite surface record, applies full-interval tool wear, and subtracts the same mass from `WorldState`; sparse cell mass keeps the full-cube collision state until represented mass reaches zero, when the cell becomes air. Fatal interruption before completion releases a carried reservation when one exists, extracts no matter, and charges elapsed tool wear. Trusted load replays target mass, output-route legality, tool occupancy, physics, schedule, route-specific future capacity, and survival budget. Local drop pickup fills current carried capacity and preserves any remainder under the same surface identity. This is a conserved excavation owner path with control-rate progress and persisted selected-tool context; rendered item entities, adapter feedback, and general block placement remain absent. |
| Player body + generated world + carried load -> continuous locomotion | `advance_control_tick_with_input` -> logistics kinematics/body commit -> next slow survival settlement | Logistics owns the persisted fixed-point body and derives voxel locality from it; generated `WorldState` supplies collision authority; survival owns load-sensitive locomotion cost. Each 20 Hz control sample resolves horizontal acceleration/sprint/crouch, vertical jump/gravity/contact, and deterministic world collision before committing one body update. Realized locomotion prices total body load, including carried material and portable equipment, and fractional exertion accumulates until the slow survival boundary. Crossing into another voxel additionally fails closed while exclusive player work is active, while moving carried custody would violate a retained production site or pending mining settlement, or while carried equipment is occupied by work. Persisted opening-access edges are not a movement prerequisite. General pathfinding and autonomous haul routing remain absent. |
| Settled generated-world body + explicit local path -> continuous travel projection | `project_player_walk_path` / `project_player_walk_path_for_load` | Logistics accepts only a caller-supplied sequence of horizontal face-adjacent destinations from a living, admitted, standing, grounded, motionless, idle player at a slow boundary. It reuses the same 20 Hz acceleration, swept collision, gravity, braking, and fractional locomotion accumulation as execution: commanded horizontal movement accrues realized-speed exertion, while passive neutral-input braking may move the body without charging active locomotion effort. Every leg must reach and settle in its requested voxel, the complete supplied path must fit before the next slow boundary, and unexpected falls/overshoot or unavailable world truth fail closed. The current-load form prices authoritative carried custody; the explicit-load form can price a hypothetical future haul within the supplied body capacity without fabricating inventory, because ordinary walking trajectory is load-independent while exertion remains load-sensitive. Both preserve the player's settled fractional exertion remainders, expose exact control ticks plus the basal-and-locomotion charge that will settle at the next boundary, and mutate nothing. This is read-only costing of an already chosen local path, not route discovery, traversability generation, path ranking, or general pathfinding. |
| Generated-world fall -> landing impact -> survival vitality | `advance_control_tick` body sweep -> `PlayerFallImpactOutcome` / persisted logistics pending loss -> next slow survival tick | Vertical collision derives impact speed from the downward pre-collision velocity. Physiology authors an 8 m/s safe landing threshold and a deterministic linear vitality-loss curve to lethal loss at the 55 m/s terminal fall speed. A landing emits the impact speed/loss immediately, but logistics persists any nonzero loss until the next slow survival boundary so fatality, active-work cancellation, and other death effects remain inside the existing atomic survival/death transaction. Save/load preserves and bounds that normalized pending loss; `advance_tick` cannot skip it. A landing occurring on a slow boundary is therefore settled at the following boundary rather than retroactively mutating the transaction that preceded contact. Ordinary jump landings remain below the safe threshold. This is fall-impact injury only, not armor/protection, fire/suffocation damage, corpse/respawn, or a general damage model. |
| Opaque overhead geometry -> survival thermal exposure | `assess_player_overhead_cover` -> `advance_simulation_tick` -> `decide_survival_tick_with_local_heat` | Logistics requires generated world truth above every body-footprint cell, then accepts either an opaque block or an Active structural member whose cross-section spans that member's full horizontal voxel footprint; Planned framing does not protect the player. Missing world truth fails closed as open exposure. Survival owns the authored consequence: cover reduces per-tick comfort loss, local heat remains a separate additive source, and cover never creates warmth. The lived fresh-world worksite carries its existing field box and fire forward into construction: the player consumes staged logs first, gathers the remaining timber/stone shortfall, earns the adze from the nine board batches justified by the 6.5 kg panel, builds a three-member support chain, and places that roof through ordinary direct control; a continuation from the completed camp state observes the authored 1000 -> 300 ppm/t exposure reduction after finite fire warmth is gone. Its covered hour remains comfortable without refueling, whereas matched unbuilt/no-fuel exposure becomes chilly; one additional real fuel charge in an unroofed camp is warmer in that same hour. This is overhead refuge, not walls, insulation, weather, or enclosure. |
| Local loose surface matter <-> carried inventory | `player_surface_gathering_options` -> `validate_start_player_surface_gathering_option` -> tick; `available_player_world_drops` -> automatic control pickup; `validate_drop_player_material` for direct carried-material release | Surface custody preserves exact material profile, provenance, ambient-equivalent storage age, remaining mass, lifecycle, and position. Local `Opportunity` records project into actor-facing gathering options that keep authored method identity private while exposing commodity, remaining opportunity, direct-use batch, maximum batch, exertion, and duration for a chosen amount. Options are surface-revision-bound; start rechecks the selected option before canonical gathering admission proves exact locality, method/commodity compatibility, batch size, carried capacity, lot/revision headroom, and player attention while reserving destination capacity. Slow completion restores the exact parcel into inventory and depletes the source. `Drop` records are excluded from the gathering catalog because their drop-only exact-local projection supports control-rate transfer of `min(remaining drop mass, current carried available capacity)`, leaving any conserved remainder under the same identity. Direct material drop consumes the selected semantic commodity as exact current lot slices, preserves temperature/composition/particle state/provenance/storage history, and derives a fixed two-second automatic-pickup delay from persisted control time. Natural opportunities never auto-pick; the explicit-method gathering wrapper remains only as a lower-level subsystem boundary. |
| Hidden geology -> acquired knowledge | `validate_start_field_prospecting` -> tick | Player work owns the timed action; completion records bounded actor-safe observations in `GeologicalKnowledgeState`. Hidden deposit identity never enters the observation. |
| Acquired knowledge -> extraction authorization | `resolve_mining_target` | Produces actor-safe `MiningTargetResolution` from legitimate evidence without transferring custody or exposing hidden deposit identity. |
| Geology + equipment + labor -> mining work -> inventory | `validate_start_mining` -> `advance_tick` | Admission binds requested effort, tool, destination, labor, logistics, and output reservation in `MiningJobRecord`. Completion extracts the bound matter and ordinarily settles it directly into the reserved inventory destination; `MiningOutputReceipt` on `TickOutcome::mining_output()` reports the exact contribution plus its merge-aware surviving lot identity. |
| Mining recovery custody -> inventory | crate-owned `advance_tick` settlement retry | If a post-admission physical blocker or same-tick inventory owner prevents landing, extracted matter remains in durable mining-owned awaiting-settlement custody with its reservation and future headroom. Later ticks retry the internal settlement automatically after the blocker clears. Recovery yields for that tick when production, surface gathering, enclosure dismantling, or fatal-work cancellation already owns an inventory mutation, then retries afterward rather than invalidating another owner's revision-bound plan. If the reserved destination is carried custody, player movement waits for that retry so unsettled matter cannot change world locality or enter the moving load after locomotion cost was projected. This boundary is not a second player command. |
| Local ground storage <-> player-carried inventory | `validate_place_player_storage[_at]` / `AppState::available_local_ground_stockpiles` -> `project_material_stacks` -> `validate_pickup_stack_from_ground` / `validate_drop_stack_to_ground`; exact-lot variants remain available to lower-level callers | Finished authored storage placement consumes its exact enclosure body from carried custody, derives authored capacity, and applies the storage profile as one player action; direct contextual placement first raycasts a visible block, derives the adjacent face voxel, requires generated air plus no player-body or stationary-object overlap, then enters the crate-owned exact-position storage path. The empty target allocation is an internal transaction rather than a visible placement step. Observation exposes stationary stockpiles only at that voxel and excludes carried custody. Inventory projects revision-bound familiar stacks using the same physical merge compatibility as authoritative lot coalescing, keeping lot membership private while exposing meaningful profile/provenance facts; projected partial-mass requests resolve back to deterministic exact lot slices and reject stale inventory revisions. Logistics proves same-voxel transfer access, then delegates canonical relocation/storage semantics to inventory. When complete player admission exists these direct mutations require a living player and bind the survival revision through commit; controlled logistics-only fixtures retain subsystem-local admission. Carried-load changes take effect immediately in custody; subsequent 20 Hz locomotion samples price the resulting current load instead of relying on a reserved multi-tick movement budget. Coordinate-taking ground allocation and existing stockpile/fluid location are crate-owned world/bootstrap boundaries; maintained external gameplay fixtures can reach them only through pre-admission `gameplay_fixture` adapters. |
| Canonical carried custody -> persisted quick-access context | `validate_set_quick_access_slot` / `validate_select_quick_access_slot`; `PlayerDirectControlInput::{with_quick_access_slot,with_primary,with_secondary,with_drop_active_selection}` composes canonical owner operations | Logistics stores nine `PlayerActiveSelection` bindings and one selected slot. Equipment bindings reference exact carried identities; material bindings reference authored `CommodityKey` rather than `MaterialLotId`, so lot split/merge cannot invalidate intent and temporary depletion may preserve a material binding. Rebinding moves a semantic selection instead of duplicating it, equipment drop clears any bound slot, and trusted load rejects invalid/duplicate bindings. A control sample applies slot switching before primary/use/drop, so ordinary number-key plus action input observes the newly selected context. Selected-material secondary use can also discover a unique authored single-input combustion store to assemble from carried material, or fuel one unique exact-local combustion store whose authored fuel matches the selected commodity; ambiguity rejects instead of choosing a store/definition by ID, while energy/inventory owners still validate exact material, locality, capacity, conservation, and revisions. When a contextual secondary action resolves to slow work off-boundary, logistics also retains exactly one one-shot request until the next slow boundary, reserving its clear revision and snapshotting voxel, active selection, and direction so execution can reject changed context and re-resolve current owner facts. `project_player_inventory` joins hotbar bindings with carrying assessment, carried equipment, familiar stacks and exact-local containers without owning custody. Primary, secondary and drop still delegate to the same excavation, consumption/placement, equipment and material owners. |
| Projected food stack -> timed eating | `project_player_inventory` / `MaterialStackProjection` -> `validate_eat_stack_to_full` -> `validate_eat` | The caller chooses one visible physical/freshness cohort rather than a lot ID. The revision-bound projection resolves only the requested mass into deterministic exact lot selections, then canonical survival validation owns freshness, temperature, intake bounds, attention, schedule, physiology and conservation. A stale projection fails before admission. |
| Carried inventory -> personal manual craft | `player_manual_craft_options` -> `validate_start_player_manual_craft` / `validate_start_player_manual_craft_from_stack` -> `validate_start_manual_craft` | The player-facing catalog derives current batch availability from the admitted player's carried stockpile. Durable homogeneous input uses deterministic owner-private lot selection from recipe + nonzero batch count; age-sensitive/freshness-relevant input instead requires one current `MaterialStackProjection`, whose inventory revision and commodity are rechecked before its private lot slices are consumed. Optional equipment is an exact ID and passes the existing locality/capability/occupancy checks. Outputs route in place to carried custody, with production reservations, player attention, survival budget, schedule, conservation, and stale-state rejection unchanged. |
| Local equipment -> player operation | `AppState::available_local_equipment` -> operation-specific projection/resolver/validator | Observation exposes equipment only at the admitted player's exact voxel, including portable equipment currently in player custody, and does not authorize use. Prospecting composes this into `player_prospecting_options`: required techniques remain visible by accepted authored instrument definition, while currently usable instances are limited to exact-local, idle, unmounted equipment whose condition can survive the full survey. Actor policy may select a local tool or machine from current definition/condition state without retaining a fixture or assembly identity; the owning operation rechecks locality, occupancy, condition, support, and capability at admission. A carried tool occupied by active work is unavailable for another operation, and active slow player work prevents voxel departure. |
| Inventory + providers -> production work | resolver-specific `Resolved*` / `ProcessResolution` -> `validate_start_process*` | Start moves exact input into production work-in-process and binds output reservations, provider occupancy, finite-energy consequences, and site coherence in the durable job. |
| Initial wilderness input -> opening world custody | `validate_generate_initial_wilderness` | Before admission at tick zero, an explicit seed and spawn voxel deterministically authorize a bounded six-edge opening graph with finite food, material, water opportunities, and one kobold den on a material leaf. Validation requires pristine surface, fluid, logistics, and kobold states; commit prechecks material owners and logistics before applying their changes and the initial patrol. The scout follows two authored graph edges starting at simulation tick 600, moves once per 20 slow ticks when the next generated voxel is free, and notices but never consumes or removes player-created drops. Current evidence establishes a persistent local scout encounter, not creature collision, combat, ecological production, theft, or general pathfinding. |
| Opening water -> player locality -> survival hydration | `available_adjacent_drink_sources` -> `advance_control_tick_with_input` -> `AppState::available_local_fluid_stores` / `available_local_drink_sources` -> `validate_drink*` -> tick | The adjacent opening observation surface is bound to the admitted player's actual voxel and reveals only edge-linked drinkable source identity/fluid/position, not exact volume or temperature. Continuous body movement pays canonical load-sensitive locomotion cost while generated terrain/collision controls locality. Exact-local observation then exposes finite contents; drink admission rechecks locality, volume, serving limits, physiology, and attention. A generated finite pool proves ordinary opening water access, not general hydrology or remote source discovery. |
| Production work -> inventory / equipment / energy | `advance_tick` completion | Completion routes exact outputs, applies equipment/energy consequences, releases occupancy/reservations, and emits `ProcessCompletion` with inventory landing identities. |
| Fatal survival -> active player work | `advance_tick` fatal-work planning | Work due on the fatal tick completes first. Otherwise each work owner applies its declared suspend/cancel/release semantics, settles physical equipment consequences for elapsed active ticks, and preserves represented custody plus trusted-load validity. |
| Inventory -> equipment embodiment | `validate_assemble_equipment` / `validate_upgrade_equipment` | Exact material traces become equipment embodiment; assembly establishes world custody when logistics exists, while upgrade preserves equipment identity/location and adds the authored trace. The owner boundary remains available to controlled actorless/partial fixtures, but a fully admitted player must be living and the authorization remains bound to that survival revision until commit. |
| Equipment embodiment -> inventory recovery/service | `validate_player_disassemble_equipment` -> `validate_disassemble_equipment`; `player_equipment_maintenance_options` -> `validate_start_player_equipment_maintenance` -> `validate_equipment_maintenance` | Player recovery sends one visible stationary local equipment instance to carried custody while canonical disassembly still decides exact intact-versus-worn recovery, capacity, locality, occupancy, structural consequences, survival, and stale-state legality; worn component-replacement tooling therefore returns its working component as authored scrap rather than pristine stock. The player service catalog exposes idle exact-local worn equipment, authored replacement/spent commodities, required replacement mass, target condition, duration, and whether current carried stock can satisfy the canonical resolver. Starting service uses carried custody for replacement supply and spent-material return while the existing maintenance validator still owns locality, purity, occupancy, survival, exact material exchange, timing, interruption, and condition recovery. Actorless/partial controlled disassembly remains an owner capability boundary. |
| Inventory -> finite energy-store embodiment | energy-store assemble/upgrade/disassemble validators | Exact traces cross between inventory and `EnergyState`; lifecycle operations preserve or retire identity/location according to the validated transition. Controlled actorless/partial lifecycle operations remain valid; once ordinary player admission exists the validators require living survival and bind that revision through commit. Disassembly recovery into carried custody rechecks exact player-local destination access, capacity, and owner revisions; later locomotion prices the changed carried load through the continuous body path. |
| Enclosure matter <-> storage profile | storage-enclosure build/dismantle validators -> tick | Inventory remains matter owner; timed dismantling checkpoints exposure, restores ambient storage, and lands exact enclosure matter in the reserved recovery destination. |
| Inventory/equipment/fluid -> structural load | owner-specific support validators | Endpoint owner retains object custody; `StructureState` owns source-separated load and support consequences after final aggregate-load validation. Controlled support setup remains owner-level before ordinary admission; once a full player exists, direct support changes require living survival and reject stale survival snapshots before mutation. |
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
Mining completion likewise returns its exact mined `MaterialLotSpec` plus one merge-aware surviving identity in
`MiningOutputReceipt`. A landing identity may therefore refer to an existing lot when compatible matter
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
| `Speed` | nanometer/second | `u64` |
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
particle-state policy, cohesion, and whether their physical form is residual; material identities may also be
residual (notably slag). `MaterialRegistry::is_residual_commodity` classifies authored chips, scrap, tailings,
exhausted tailings, and slag without converting them into another custody type. Residue can still be recovered
where a real recipe consumes it. Rigid infrastructure inputs must be consolidated non-particulate solids.
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

An authored `residual_only` storage enclosure uses the same material-backed stockpile owner and finite inbound
reservations. The clay-lined spoil bay is an ambient dry-storage site with 80 kg capacity, constructed from a
3.2 kg timber bulk-crate body and 4 kg packed-clay lining. Unlike preserving food containers, it admits only
authored residual commodities (including recoverable chips and offcuts). Canonical ingress, relocation, and
production-output reservation reject ordinary products; enclosure construction checks already-owned contents,
and trusted load rechecks retained lots against the immutable enclosure definition. It is containment, not
deletion, treatment, hydrology, leachate control, or a pollution simulation. Recoverable residues can be moved
back to ordinary storage and fed to existing real recovery processes.
The explicit `SecondaryControlIntent::place_storage(definition, direction)` resolves a pointed,
terrain-supported voxel and then performs the same owner-validated, atomic material-backed
placement as single-body selected-material storage. This is necessary for multi-input structures
such as the spoil bay: selected material alone cannot distinguish several constructions sharing
timber and clay inputs. It requires carried construction stock and produces the existing
`StoragePlaced` or typed placement rejection outcome; no additional material or state is granted.

Supported stockpiles contribute stored matter plus enclosure matter as structural load. Stored-mass and support
consequences commit atomically.

### Logistics

`LogisticsState` owns the player's persisted fixed-point body state, the inventory-owned carried stockpile
identity, stationary stockpile locations, equipment locations, energy-store locations, fluid-store locations,
fractional direct-locomotion exertion awaiting slow survival settlement, at most one off-boundary one-shot
contextual-use request, and a monotonic revision. The body stores
a bottom-center `WorldPoint` in nanometres plus signed horizontal/vertical velocity and standing/crouching stance; existing voxel-locality
rules derive `PlayerLogisticsRecord::position()` from that point rather than persisting a second position.
Logistics does not duplicate lot contents, equipment condition, stored energy/fluid, or capacities.

Built-in physiology authors a 0.6 m body width; 1.8 m standing and 1.5 m crouching heights; 1.3/4.3/5.6 m/s
crouch/walk/unloaded-sprint speeds; 20 m/s^2 horizontal acceleration; 5.0 m/s unloaded and 4.6 m/s fully loaded
jump launch speeds; and a 55 m/s fall-speed cap. Full carry continuously removes the sprint bonus, reaching the
4.3 m/s walking ceiling at capacity. `advance_control_tick_with_input` consumes normalized fixed-point world-space
intent, moves horizontal velocity toward the stance/load target, applies grounded jump impulses and gravity over
the exact 50 ms interval, and sweeps the active-height body X then Z then vertically against `FullCube` collision.
Standing up is refused when the taller envelope collides; crouching refuses a horizontal move that would remove
support under the current foot plane. Missing destination chunk truth is a hard collision boundary rather than air.
Trusted load bounds persisted velocity, locomotion fractional state, and pending normalized fall injury;
trusted load/chunk generation reject body/terrain overlap, and player admission/chunk-generation tokens bind
the opposite owner revisions so a validate/commit race cannot create an overlap. Downward landings expose the
pre-collision impact speed. Impacts above the authored 8 m/s safe threshold queue deterministic vitality loss,
scaling linearly to lethal loss at the 55 m/s fall-speed cap, and survival settles that persisted loss at the next
slow boundary so existing death cleanup remains atomic. Step-up, partial shapes, surface traction, protection,
and demand streaming remain absent.

Direct horizontal locomotion reuses the authored load-sensitive effort rather than bypassing survival. Each moved
control step scales that effort by realized speed, so sprinting costs more than walking at the same load, and accumulates exact control-rate fractions in logistics,
and settles the accumulated energy/hydration cost into the existing slow survival tick every 72nd control step.
No second metabolism clock exists. If the same boundary kills the player or claims slow work, the current direct
input is discarded in favor of the neutral body plan so slow-state consequences cannot be followed by a stale
post-transaction movement impulse.

Player-context operations use shared access checks: carried inventory is implicitly local; other targeted
stockpiles/equipment/energy/fluid stores require an explicit location at the player's voxel once player logistics
is initialized. Controlled capability fixtures may remain locationless only when they are not claiming ordinary
player access.

Ground pickup/drop requires player and loose stockpile at the same voxel and delegates exact matter relocation to
inventory. Carried-load changes are immediate custody changes; continuous locomotion prices the resulting current
load on subsequent 20 Hz control samples rather than reserving a multi-tick movement budget. Equipment and
energy-store disassembly recovery likewise uses current carried capacity and owner revisions. Mount/unmount changes
support without teleporting an existing location. Assembly establishes player-local world custody for newly created
equipment/energy stores; disassembly removes that custody.

Pointed storage placement additionally requires a generated empty destination voxel, clearance from the player,
stationary assets, and structural elements, and a `FullCube` terrain block directly beneath the destination.
This contact rule belongs to player interaction, not the general-purpose stockpile custody allocator. It checks
support at placement time only: later terrain removal and general terrain-dependent structural collapse are not
currently modeled.

Slow `TickSpan` player work may start in generated-world body mode only at an exact control/simulation boundary
while the body is grounded and horizontally stopped. Ordinary player locomotion has no separate slow-work movement
variant: generated-world movement stays on the continuous body authority, while persisted opening-access edges are
used only for bounded local observation and planning.

Prospecting requires the player inside the survey region and any instrument at the player voxel. Mining requires
the player inside the acquired resolved target region plus local tool/output access. Manual power requires local provider
and destination store. Production uses an actor-independent same-site rule: every explicitly located continuing
endpoint in one admitted job must agree on a voxel. Validation tokens bind logistics revision where location can
invalidate admission. Before continuous body motion crosses into another voxel, logistics projects the player's
carried stockpile at that destination and refuses a move that would split a retained production site. Voxel departure
also waits while completed mining output remains in durable awaiting-settlement custody for the carried stockpile,
and while any carried equipment is occupied by active work. These are custody/site-coherence constraints on the
continuous body path, not a second movement owner.

General pathfinding, automatic proximity search, generic stockpile/energy/fluid transport, arbitrary equipment
teleportation, and mounted-production site/contact geometry are absent. Explicit player carrying still provides
exact-local pickup/drop; the persisted opening-access graph remains a bounded observation/planning surface and is
not an alternate locomotion path.

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

Opening world generation also places a finite seed-varied wild-nut opportunity at the existing
work junction, without a new route edge. The normal gathering catalog, carried-stack meal
action, and survival nutrition owner handle a mixed fruit/protein ration; this is not farming
or wildlife simulation. An independent clay outcrop beyond the drinkable water voxel adds
a sixth access edge without making empty-hand drinking ambiguous. Finite loose stone and clay
supplies support a fresh-world project that gathers, hand-packs binder, shapes timber boards,
and assembles the clay-faced casting bed through the player equipment catalog. This supplies
casting infrastructure, not a complete powered foundry or general geological generation.
Stone excavated into chips can be recovered through ordinary surface
custody and reknapped through a slower, conserving manual craft into tool-head stock. The
faster intact-lump recipe remains a reason to collect loose rock.

Prospecting is exclusive player work over an authored method and bounded region. Completion may inspect hidden
geology only to produce method-bounded observations; in-progress work persists through `PlayerWorkState`.
One-point hand sampling produces a coarse 5 kg reserve band. A player who already owns a stone
geological hammer can instead commit 60 slow ticks and extra instrument wear to an extended
chip assay with a 1 kg band, before fabricating a mining tool. The copper indexed-channel method
also resolves 1 kg bands across a wider footprint; the expensive core drill still provides the
most precise 0.25 kg band. All of these are catalogued choices with different costs, not
automatic confidence upgrades or extractions. A matched fieldwork witness executes the
extended assay from the same pre-tool decision snapshot and reports its narrower evidence
without claiming the unexecuted subsequent search as a completed outcome.

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
temperature limit cannot be used as a read-only probe of hidden geology. Geology retains matter custody during
labor. Completion extracts the slice and ordinarily settles it directly into the reserved inventory destination,
returning the exact contribution and merge-aware landing identity through `TickOutcome::mining_output()`. If a
post-admission structural condition or same-tick inventory owner prevents that landing, the slice remains in
mining-owned awaiting-settlement custody with its reservation and future headroom until a later tick can retry the
crate-owned settlement. Retry examines waiting jobs in stable identity order and settles the first physically
eligible output; an unrepaired structural load on one destination does not hold up independent destinations.
Carried destinations cannot move during that interval. The recovery phase is durable for
continuation safety but is not exposed as a separate player action.

## Production and processing

### Production jobs

`ProcessDefinition` owns immutable process identity and generic typed provider requirements. Material eligibility,
recipe/yield, duration, energy, wear, and dynamic batch semantics belong to the execution-family resolver that can
interpret the selected matter; production does not maintain a second recipe.

`ProcessResolution` binds one concrete operation to exact selected inputs, duration, outputs, and finite resource
consequences. Production start transfers selected matter into durable work-in-process, reserves output capacity,
binds provider occupancy and required owner revisions, and records any modeled energy consequences needed for
continuation. Ordinary player-facing production authorization additionally requires a fully admitted living player,
exact-local access to every retained endpoint, and an unchanged survival revision through commit; capability-level
setup remains separate for controlled fixtures.
Storage exposure remains wall-clock based while matter is in process.

Completion is revision-bound and moves exact outputs to their reserved destinations, applies resolved wear/energy
effects, releases occupancy/reservations, and emits merge-aware landing receipts. Future revision headroom is
reserved conservatively per durable job for production completion and every cross-owner completion mutation it
will eventually require, rather than relying on current due-tick coalescing. Same-tick completions may still batch
at execution, but suspension can separate schedules, so availability transitions may consume only revision slack
beyond the final-completion obligations retained by jobs that remain. Output support mount/unmount reprojects the
retained structural obligation atomically. Loss of required support/provider availability may suspend a job while
preserving work-in-process, reservations, remaining active duration, and those future completion obligations.
Manual production releases player attention while suspended and must reacquire labor/survival budget before
resumption.

**Physical process residue:** Form and material definitions classify chips, offcuts/scrap, ore tailings and slag
without deleting their mass or suppressing recovery recipes. Manual shaping and powered crafting retain their
existing one-destination behavior by default: both useful output and residual lots occupy that destination's
finite capacity. Their opt-in residue-destination admission (`ManualCraftStartRequest::with_residue_destination`,
`validate_start_player_manual_craft_with_residue_destination`, and
`validate_start_powered_craft_with_residue_destination`) partitions the exact resolved lot specs into stable
`PRIMARY` and `RESIDUE` process streams. The production owner checks local access, independent capacity,
support and future reservations for **both** destinations before consuming inputs/energy. A full waste stockpile
rejects another segregated job; the player can use a different local stockpile, move the physical waste using
existing inventory/ground transfer or drop paths, or reclaim it through an authored process. Jobs persist both
routes and exact output profiles, and trusted load validates the split against current material definitions.
Ore constituent separation already resolves two independent target/residue streams with tailings containing the
remaining physical constituents; segregation does not grant an extra separation process. Slag identity is
defined, but chemical ore reduction and the generation of new smelting slag are not implemented. Environmental
dispersion, soil/water contamination, spoil heaps, and bulk waste conveyance are not yet simulated.

**Residue as useful fuel:** Campfire combustion supports explicit alternative solid fuel charges,
not an implicit material substitution. Alongside its original 1 kg log charge (12 MJ usable
heat), the stone-ring campfire now takes 500 g of wood-processing chips for 5 MJ usable heat.
`validate_combust_selected_fuel` binds that commodity at admission; selected-material direct use
chooses the matching charge, while the original `validate_combust_fuel` retains the log default.
Both paths share finite heat capacity, locality, occupancy, revision checks, structural loads,
inventory egress and the existing terminal combusted-mass account. A short chip supply rejects
without consuming logs, and an old log charge can still be added after burning chips. This is
wood-waste heat recovery, not complete combustion chemistry, air emissions, or modeled ash.

**Exposed residue footprint:** Player-created loose solid residue drops retain their exact
voxel, mass and authored host-material density. Their combined solid-equivalent volume on the
one-square-metre voxel footprint yields a conservative local layer depth (including across
distinct material lots); at 25 mm or more, rigid player-placed ground storage refuses the
unstable footing with its position and estimated depth. Natural gathering opportunities,
non-residual drops and matter held inside any stockpile do not count as exposed spoil. The
same canonical storage-placement validator checks this for both local and pointed placement;
its commit token binds the surface revision, preventing a late spill from validating against
an obsolete clear site. Real material pickup can restore firm footing without erasing mass,
and the ordinary interaction reports rejection without consuming construction matter. This
is one static footprint/contact rule, not simulated particulate scattering, changing terrain,
generic structural foundations, or soil/water contamination.

Manual shaping conserves material identity/mass and cannot change phase. Pure durable input lots may span exact
temperatures: form-only shaping retains every selected thermal cohort and apportions each authored output across
the remaining cohort distribution at whole-milligram precision, so crafting neither averages heat nor performs
free temperature separation. Powered crafting delegates the same material/yield transform. Optional equipment may
reduce attention through canonical condition-adjusted throughput; required equipment rejects the equipment-free
route. Equipment changes throughput/wear, not recipe yield. Scrap/chip outputs remain represented matter until an
explicit authored recovery process consumes them. Cast copper ingots also feed three direct
manual-work routes: a 60 g ingot becomes a 54 g saw blade or electrical winding plus 6 g scrap,
and a 20 g ingot becomes an 18 g pierced sizing plate plus 2 g scrap. They retain the existing
component forms, equipment requirements, and embodied downstream consumers instead of a
parallel foundry-only parts vocabulary; the original native/reinforcement routes remain available.
Settlement helve hammers can delegate the same cast-ingot-to-blade and cast-ingot-to-winding
transforms with finite mechanical energy; the powered spindle drill similarly delegates cast
screen-plate piercing. Each consumes the exact manual input and produces identical products and
recoverable offcuts. Powered work requires an assembled, accessible machine, charged energy
store, and consumes mechanical energy and equipment condition; no additional casting route or
free processing bypass exists. Player-facing powered-work discovery now groups local machine
variants under each known manual transform: `player_powered_craft_options` combines the exact-local
owned machinery, compatible finite energy stores, material batch availability, required work, and
condition-adjusted processing time. It distinguishes missing stock and missing charge, but a
selected option is not authority: `validate_start_player_powered_craft_option` reselects exact
physical lots and validates current access, energy, wear, reservations and output custody.
The first-foundry continuation also permits reinvesting one 80 g cast into the inherited treadle
hammer: an observed finite 2 kg fallen-timber source is gathered through player-owned surface
work and sawn into 1.8 kg boards. Cutting 500 g of those boards yields 400 g handles and 100 g
chips. After the 800 g helve frame investment, 500 g boards remain. The fabricated parts and
20 g of cast-stock copper reinforcement add the helve upgrade, saving 1 kg of fallen timber
compared with individually turning two whole logs into handles. Both the hand-crafting owner
and the existing powered-lathe family can cut these same board handles, with identical 400 g
handle and 100 g chip yields; the latter consumes finite mechanical work and machine condition.
The upgraded, identical helve equipment can then form a 54 g winding from the remaining 60 g ingot charge using the inherited hand-cranked
flywheel. That actual winding is then installed to expand the existing electrical buffer from
15 kJ to 30 kJ. This is a matched alternate use of the same cast, not extra copper or an
unearned machine awarded by the fixture.

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

Dietary contribution is independent of metabolic calories: each food category gains reserve
from its consumed mass relative to an authored full-reserve serving (500 g in built-in content).
Thus 150 g of berries restores the same fraction of fruit nutrition as 150 g of grain restores
of grain nutrition, while energy and hydration still depend on the actual food. Unreplenished
dietary reserves lose 600,000 ppm per authored day in built-in physiology.

Diet quality combines the average reserve of three broad food groups with the weakest reserve,
weighted equally. Missing one category slows natural vitality recovery instead of preventing it;
balanced intake retains the highest benefit. Fractional vitality-recovery carry persists so
continuation does not depend on tick partitioning.

### Energy and fluids

Energy stores own carrier, capacity, directional power limits, stored energy, revision, optional embodied traces,
and optional passive dissipation; logistics owns their world location. Consumers/producers use validated owner
operations. Passive dissipation is an environmental sink and applies from the pre-tick store snapshot after
same-tick ingress. A definition may also mark nonempty finite storage as a renderer-neutral local light source. `project_local_light_sources` exposes stable active emitters and `assess_local_illumination` derives deterministic face-connected falloff through non-opaque generated world cells, without persisted light state. Missing chunks and opaque blocks obstruct propagation. Combustion-source brightness decreases with remaining finite heat relative to its primary authored fuel charge; shorter alternative fuel charges begin dimmer and refueling can restore brightness. This covers local emitted light only, not skylight, general visibility, or meshing. Generic store-to-store transfer/carrier conversion is absent.

Material-backed energy-store upgrades are additive: registry validation requires carrier compatibility, non-
regressing capacity/transfer semantics, non-increasing passive loss, and exact base-plus-additions embodiment.
Runtime upgrade requires an empty unoccupied store and preserves store identity/location/creation history.
`player_energy_store_upgrade_options` enumerates eligible exact-local empty stores, reports
material-ready/unready candidate additions, names and directional transfer limits, and never
reserves material. Selecting a row revalidates through `validate_upgrade_energy_store`; moving,
charging or consuming inputs between observation and action cannot bypass ownership.
The foundry's 15 kJ copper-plate buffer accepts one 54 g winding to become a 30 kJ wound
buffer with greater transfer limits; the subsequent 60 kJ copper rack adds 240 g
reinforcement and 3.2 kg of boards while retaining that winding. The intermediate buffer can
hold two small melts' electrical demand at once, but the upgrade requires an empty idle store,
and the increased capacity neither creates free power nor distributes it remotely.
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

Persistent spatial references use checked 64-bit voxel coordinates and half-open bounds. `WorldState` partitions
generated terrain into 16x16 chunk columns and sorted 16-cubed sections for compact storage, while other domain
records remain addressed by world coordinates rather than renderer/ECS identities. Chunk residency, streaming,
scene graphs, renderer objects, and frame scheduling remain adapter/orchestration concerns rather than simulation
authority.

Texture and shader definitions are immutable registry content. Texture baking is deterministic and
renderer-neutral. WGSL libraries/programs have typed identities, deterministic assembly, validated
dependencies, explicit entry points/pipeline requirements, and bounded work. Graphics-resource creation
and frame scheduling belong to adapters. [`assets/shaders/README.md`](assets/shaders/README.md) owns the
concrete shader binding contract.

## Trusted load

`LoadedSaveEnvelope::into_state(registries)` is the public decoded-save promotion boundary and
`validate_loaded_state(registries, state)` is its exhaustive graph validator. Raw `AppState` does not
implement public deserialization, so adapters cannot bypass schema/reconstruction checks by decoding the
runtime root directly. Validation recomputes rather than trusting cached claims. Generic graph validation may
also audit explicitly controlled in-memory spatial proxies, but save promotion additionally requires player
admission to be either wholly absent or complete across survival and logistics. A survival-only or
logistics-only actor cannot become trusted runtime continuation through persistence. Admission:

1. validates save and registry versions;
2. rebuilds derived indexes;
3. validates each local owner and all authored/runtime references;
4. validates cross-owner occupancy, reservations, support, provenance, and ownership, including operation-specific physical replay where persisted work depends on it;
5. rejects incomplete persisted player admission before runtime use;
6. returns `AppState` only after both the generic graph and save-specific promotion contract are valid.

Cross-owner validation covers, as applicable:

- authored/runtime references and monotonic identity cursors;
- forward/reverse indexes and derived caches;
- material profile, provenance, containment, and reservations;
- production, mining, and player-work lifecycle/occupancy;
- exact represented matter, fluid, and modeled-energy ownership;
- structural topology, embodiment, damage, and source-owned load channels;
- support assignments and independently recomputed loads;
- persisted schedules and operation-specific physical replay.
