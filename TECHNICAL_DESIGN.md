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
| Root simulation and time | core definitions, typed quantities/time | `AppState::control_tick()`, `AppState::tick()`, immutable owner accessors | per-phase `decide_*` is crate-owned orchestration | `advance_control_tick` is the canonical 20 Hz step and owns active body physics; `advance_tick` atomically advances one slow boundary only when no generated-world body requires those intervening control steps; direct clock/owner applies stay crate-private |
| Registries and built-in content | `Registries` plus domain registries; `build_registries` validates cross-references | public immutable `Registries::*()` accessors; derived authored topology may provide goal-directed reverse lookup | callers inspect authored possibilities; topology never claims current legality or ordinary reachability | none; registries and derived definition indexes are immutable after construction |
| Block world | `WorldRegistry` block/state definitions, excavation profiles, canonical `CubeFace`, fixed-point `ViewDirection`, authored eye/reach geometry, plus persisted `WorldGenerationProfile` | `AppState::world()`, generated chunk/section records, exact block lookup/represented mass, and bounded actor-safe `raycast_player_block` | `validate_generate_chunk` / `validate_generate_player_neighborhood` derive deterministic terrain; targeting derives stance-specific eye origin; exact excavation admission binds the first hit, authored mining physics, portable tool, batch mass, and one durable output-custody route | excavation reserves tool occupancy and attention, then chooses reserved carried custody only when the complete batch fits; otherwise completion creates one finite loose surface-resource drop at the player's admitted work voxel. Sparse cell mass preserves partial excavation, the start/completion receipts expose selected/landed custody, and trusted load validates future surface revision/identity headroom plus persisted drop position/kind. A whole local player-created drop auto-transfers on control time when carried capacity and future owner headroom permit; oversized drops remain for manual partial gathering. `PlayerDirectControlInput` can hold a primary excavation intent; any idle 20 Hz control step selects the largest target/tool batch, surfaces typed rejection through `PrimaryActionOutcome`, and can chain the next batch on the exact completion step. The durable excavation schedule is in `ControlTick`, exposes normalized progress, and apportions authored slow-tick exertion/wear without changing the total physical cost. Placement, generic item/drop projection, hotbar context, richer shapes, and residency/unload remain open |
| Inventory and storage | material/form/storage definitions | `AppState::inventory()`, stockpile/lot records and stable iterators | feature owners construct explicit lot selections; enclosure validators derive storage consequences | no generic public transport command; feature-specific validators own ingress/egress/reform, enclosure, and support transitions |
| Surface gathering | `LaborRegistry` gathering methods plus finite loose surface-resource records | `AppState::available_surface_resources()` for exact-local resources and `available_adjacent_surface_resources` for traversal-gated orientation | `validate_start_player_surface_gathering` derives carried custody for deliberate gathering; resource kind distinguishes manual world opportunities from player-created drops | `advance_tick` atomically depletes deliberate gathering into reserved inventory; control ticks may transfer one whole fitting local `Drop` automatically. Generated opportunities never auto-pick, oversized drops remain in world custody, and fragmented stone retains an authored partial-recovery method |
| Geological knowledge | prospecting methods plus hidden finite geology | `AppState::geological_knowledge()`, `assess_geological_knowledge`, knowledge map | field prospecting authorization; evidence combination remains actor-safe | `validate_start_field_prospecting` -> tick records observations |
| Mining | `MiningRegistry` methods and physical hardness/tool constraints | `AppState::mining()` plus acquired geological knowledge; hidden `GeologyState` is not public | `resolve_mining_target` binds localized evidence and the best acquired physical hardness band when present | `validate_start_mining` uses acquired conservative hardness when present; unsampled localized attempts use hidden hardness only for opaque tool-sufficiency admission -> tick automatically lands ordinary output and exposes `TickOutcome::mining_output()`; physically blocked output remains internal recovery custody and retries on later ticks |
| Production | `ProductionRegistry`, `ProcessDefinition` | `AppState::production()`, job records, reservations/occupancy | operation-specific resolvers produce `ProcessResolution` / `Resolved*` | ordinary player execution uses `validate_start_player_process*` to require complete player admission and prove exact-local endpoint/resource access before capability-level `validate_start_process*`; controlled capability probes may enter `validate_start_process*` directly -> tick completion |
| Equipment | `EquipmentRegistry`, capability/maintenance/upgrade profiles | `AppState::equipment()`, equipment records; `AppState::available_local_equipment()` resolves stationary or player-carried equipment at the actor voxel | `resolve_equipment_provider`, `resolve_equipment_maintenance` | `validate_pickup_equipment` / `validate_drop_equipment` move portable equipment between stationary logistics custody and the physiology-sized player carrying envelope; assembly, upgrade, disassembly, and mount/unmount/relocate remain owner-level capability boundaries for controlled setup; once ordinary player admission exists they bind the living survival snapshot before mutation; mass-changing upgrade/disassembly and structural mounting require carried equipment to be dropped first; maintenance additionally reserves timed player work |
| Player labor | `LaborRegistry`, manual-power/prospecting/structural-construction definitions | `AppState::player_work()` | `project_manual_power` projects immutable future provider/store physics; `assess_manual_power_energy_envelope` and `assess_manual_power_destination_target` bind current provider/store state for exact read-only generation and post-work stored-energy planning; structural construction derives duration from exact member mass; runtime owner commands still own authorization, attention, survival budget, and revisions | manual power/prospecting/manual production/grounded structural-construction commands -> tick; attention lifecycle is crate-owned |
| Player admission | survival physiology plus root survival/logistics/inventory/world owners | pre-admission owner state only | `validate_admit_player` binds authored full reserves, fixed-point body placement, collision-free current world state, and physiology-sized carried capacity against one snapshot | `ValidatedPlayerAdmission::commit` prechecks survival/logistics/inventory/world freshness, then establishes survival, body state, and carried inventory custody atomically; controlled spatial-proxy fixtures may seed survival without claiming locality |
| Survival | `SurvivalRegistry`, physiology, food/drink definitions | `AppState::survival()`, `assess_survival`, `assess_food_freshness` | consumption validators derive bounded direct intake and physiological schedule | `validate_eat` / `validate_drink` -> tick; ordinary initialization is owned by `validate_admit_player`, while survival-only initialization is fixture-scoped |
| Energy | `EnergyRegistry`, store definitions, carrier/power contracts | `AppState::energy()`, store records, explicit energy accounting | process/manual-power resolvers use `validate_energy_supply` / `validate_energy_sink` as part of their plan | assembly/upgrade/disassembly remain owner-level capability boundaries for controlled setup; once ordinary player admission exists they bind the living survival snapshot before mutation; reserved consumption/release and passive loss apply through canonical owners/tick |
| Fluids | `FluidRegistry`, fluid definitions | `AppState::fluid()`, store records, fluid accounting | consumers validate exact egress internally; no generic routing planner exists | support validators and canonical consumers; generic transfer/pumping/mixing absent |
| Structures | `StructuralRegistry`, profiles and geometry plus `LaborRegistry` hand-setting method | `AppState::structures()`, `analyze_structure`, `StructuralAssessment` | owner-specific support/load validation plans final aggregate load; `validate_plan_player_grounded_structural_element` requires a living fully admitted player before admitting a local grounded inert blueprint, and `validate_start_player_structural_construction` binds exact carried matter plus timed work | support/load commits through inventory/equipment/fluid/structural owners; direct support mutations bind the living survival snapshot when ordinary player admission exists; ordinary grounded materialization remains Planned/non-load-bearing during work and activates on its completion tick; non-grounded/support-linked construction, joints/panels, and general deconstruction remain absent |
| Initial wilderness generation | surface-resource records, fluid-store records, logistics locations/traversal edges, and `WorldGenerationSeed` as an explicit fresh-world input rather than persisted random state | after admission, `available_adjacent_surface_resources` and `available_adjacent_drink_sources` expose only traversal-authorized immediate-neighbor identity/commodity-or-fluid/position; exact surface mass and fluid volume/temperature remain exact-local; `AppState::available_player_steps()` exposes only traversal destinations from the actor's current voxel | `validate_generate_initial_wilderness` derives a bounded five-edge horizontal opportunity graph: forage is one step from spawn; forage exposes an optional finite-water branch and a work-junction continuation; that junction exposes loose stone and timber on separate adjacent leaves in seed-selected assignment | commit prechecks surface, fluid, and logistics snapshots before atomically establishing generated custody/location/access. Movement and immediate observation require persisted edges; unknown adjacent moves fail closed. The graph permits physiology-sensitive routing without claiming terrain geometry, general pathfinding/route generation, ecology, hydrology/flow/recharge, or arbitrary remote discovery |
| Manual crafting overlay | `CraftingRegistry`, manual craft definitions, optional-or-required equipment profile | inventory, survival, authored craft definitions, optional equipment instance | `project_manual_craft_hand_work` projects authored fallback duration plus the shared physiological budget without authorizing current state; `resolve_manual_craft` binds selected matter and uses fixed authored duration when fallback is permitted, otherwise requires canonical condition-adjusted MassFlow | `validate_start_manual_craft` -> equipment-reserving production job + player work -> tick |
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
- Rate-authored slow physics integrate against the 3.6-second physical simulation quantum. Existing survival, wear, production, mining, thermal, storage-age, and other `TickSpan` schedules remain on that exact accounting clock rather than being rounded across 50 ms steps.
- Work expressed in `TickSpan` may be admitted only on an exact control/simulation boundary. Validation tokens that can create such work bind the exact `ControlTick`, so a validate/commit race across a 50 ms control advance cannot shorten the authored interval. Direct movement, collision, targeting, block interaction, and similar responsive mechanics should use `ControlTick` instead of weakening this boundary.
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
| `GeologyState` | `AppState::geology()` | core-only hidden truth | Finite hidden geological deposits and depletion; actor code must not enumerate this owner |
| `GeologicalKnowledgeState` | `AppState::geological_knowledge()` | public actor-safe evidence | Acquired bounded observations only |
| `InventoryState` | `AppState::inventory()` plus `AppState::available_local_ground_stockpiles()` | public read plus exact-player-local ground-storage observation | Stockpiles, material lots, reservations, routing, preservation, material-backed storage enclosures, stockpile support |
| `LogisticsState` | `AppState::logistics()` plus `AppState::available_player_steps()` | public read plus current-player traversal choices | Fixed-point player body with derived voxel locality, normalized persisted traversable-neighbor edges, finite carried-stockpile identity, sorted player-carried portable-equipment identities, stationary stockpile/equipment/energy/fluid locations, and world-space custody revision |
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
local ground storage observation -> same-voxel player allocation / pickup / drop <-> player-carried inventory lot
derived player voxel + persisted traversal edge + carried mass + physiology -> legacy timed adjacent step -> logistics body snapped to destination voxel floor
generated chunk truth + persisted player body + gravity -> 20 Hz vertical sweep/contact -> logistics body
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
| Visible block + portable tool + labor -> partial terrain excavation -> carried inventory or local loose surface matter | `validate_start_player_block_excavation` -> control/slow tick | Admission reruns bounded targeting, proves an authored excavation profile, resolves existing mining hardness/throughput/batch/wear physics, requires exact-local unmounted equipment, and persists target state, represented source mass, tool trace, output custody, and schedule in `PlayerWorkState`. When the complete batch fits carried custody, admission reserves inbound mass plus future inventory/lot headroom; otherwise it reserves future surface revision/resource identity and anchors a `Drop` to the admitted player's work voxel. Completion atomically creates the exact fragmented lot or finite surface record, applies full-interval tool wear, and subtracts the same mass from `WorldState`; sparse cell mass keeps the full-cube collision state until represented mass reaches zero, when the cell becomes air. Fatal interruption before completion releases a carried reservation when one exists, extracts no matter, and charges elapsed tool wear. Trusted load replays target mass, output-route legality, tool occupancy, physics, schedule, route-specific future capacity, and survival budget. A whole fitting drop auto-picks on control time; an oversized drop remains conserved and supports partial gathering. This is a conserved excavation owner path, not yet complete hold-to-break UX: generic item/drop projection, hotbar context, control-rate progress, and placement remain absent. |
| Player locality + world access + carried load -> adjacent movement | `AppState::available_player_steps` -> `validate_start_player_step` -> `advance_tick` | Logistics owns both player voxel and normalized persisted traversal edges; `PlayerWorkState` owns the one-tick attention interval; survival owns basal plus load-sensitive locomotion cost. Admission requires a face-adjacent edge already authorized by logistics and reserves the completion revision; unknown adjacent coordinates fail closed. The player remains at the origin until the tick atomically moves locality and releases work. Locomotion prices total body load: carried material plus the embodied mass of player-carried portable equipment against the authored physiology capacity. Occupied carried equipment blocks movement, and a carried tool cannot start new work while a step is already active. Instantaneous actions cannot increase carried load after the survival budget was admitted; existing load-reducing material transfers remain valid because they only reduce the remaining locomotion burden, while equipment pickup/drop itself requires an idle player. This proves immediate access only: terrain geometry/collision, slope, pathfinding, route cost, and automatic multi-step route planning remain unmodeled. |
| Local loose surface matter -> carried inventory | `AppState::available_surface_resources` -> deliberate `validate_start_player_surface_gathering` / automatic control pickup | `Opportunity` records require explicit gathering: admission proves exact locality, method/commodity compatibility, batch size, carried capacity, lot/revision headroom, and player attention while reserving destination capacity; slow completion lands the parcel and depletes the source. `Drop` records additionally support one whole-drop control-rate transfer when the complete remaining mass fits current carried capacity and future inventory/surface/lot headroom. Capacity failure leaves the drop unchanged for later pickup or partial deliberate gathering. The explicit-destination gathering boundary remains for deliberate routing and subsystem evidence. |
| Hidden geology -> acquired knowledge | `validate_start_field_prospecting` -> tick | Player work owns the timed action; completion records bounded actor-safe observations in `GeologicalKnowledgeState`. Hidden deposit identity never enters the observation. |
| Acquired knowledge -> extraction authorization | `resolve_mining_target` | Produces actor-safe `MiningTargetResolution` from legitimate evidence without transferring custody or exposing hidden deposit identity. |
| Geology + equipment + labor -> mining work -> inventory | `validate_start_mining` -> `advance_tick` | Admission binds requested effort, tool, destination, labor, logistics, and output reservation in `MiningJobRecord`. Completion extracts the bound matter and ordinarily settles it directly into the reserved inventory destination; `MiningOutputReceipt` on `TickOutcome::mining_output()` reports the exact contribution plus its merge-aware surviving lot identity. |
| Mining recovery custody -> inventory | crate-owned `advance_tick` settlement retry | If a post-admission physical blocker or same-tick inventory owner prevents landing, extracted matter remains in durable mining-owned awaiting-settlement custody with its reservation and future headroom. Later ticks retry the internal settlement automatically after the blocker clears. Recovery yields for that tick when production, surface gathering, enclosure dismantling, or fatal-work cancellation already owns an inventory mutation, then retries afterward rather than invalidating another owner's revision-bound plan. If the reserved destination is carried custody, player movement waits for that retry so unsettled matter cannot change world locality or enter the moving load after locomotion cost was projected. This boundary is not a second player command. |
| Local ground storage <-> player-carried inventory | `validate_place_player_storage` / `AppState::available_local_ground_stockpiles` -> `validate_pickup_from_ground` / `validate_drop_to_ground` | Finished authored storage placement consumes its exact enclosure body from carried custody, derives authored capacity, allocates the target at the admitted player's exact voxel, and applies the storage profile as one player action; the empty target allocation is an internal transaction rather than a visible placement step. Observation exposes stationary stockpiles only at that voxel and excludes carried custody. Logistics proves same-voxel transfer access; inventory owns exact selected-lot relocation and storage semantics. When complete player admission exists these direct mutations require a living player and bind the survival revision through commit; controlled logistics-only fixtures retain subsystem-local admission. Pickup is rejected while a player step is active so carried mass cannot increase after locomotion admission; drop remains compositional because it reduces the moving load. Coordinate-taking ground allocation and existing stockpile/fluid location are crate-owned world/bootstrap boundaries; maintained external gameplay fixtures can reach them only through pre-admission `gameplay_fixture` adapters. |
| Local portable equipment <-> player carrying envelope | `AppState::available_local_equipment` -> `validate_pickup_equipment` / `validate_drop_equipment` | Logistics owns whether a portable equipment identity is stationary or player-carried; inventory owns the carried stockpile capacity that represents the remaining material share of the same physiology-sized body envelope. Pickup requires a living idle admitted player, exact-local stationary equipment, no occupancy/support requirement/mount, and enough remaining envelope after committed material plus reservations. Commit binds survival, equipment, logistics, and inventory revisions before shrinking material headroom and moving custody. Drop restores the same capacity and places the equipment at the player's current voxel. Persisted carried IDs are strictly ordered, may not also be stationary or mounted, and the carried-stockpile capacity plus carried equipment mass must equal authored body capacity. |
| Local equipment -> player operation | `AppState::available_local_equipment` -> operation-specific resolver/validator | Observation exposes equipment only at the admitted player's exact voxel, including portable equipment currently in player custody, and does not authorize use. Actor policy may select a local tool or machine from current definition/condition state without retaining a fixture or assembly identity; the owning operation rechecks locality, occupancy, condition, support, and capability at admission. A carried tool is unavailable for new work while player movement is in progress. |
| Inventory + providers -> production work | resolver-specific `Resolved*` / `ProcessResolution` -> `validate_start_process*` | Start moves exact input into production work-in-process and binds output reservations, provider occupancy, finite-energy consequences, and site coherence in the durable job. |
| Initial wilderness input -> opening world custody | `validate_generate_initial_wilderness` | Before admission at tick zero, an explicit seed and spawn voxel deterministically authorize one bounded five-edge horizontal opportunity graph. Forage is one step from spawn. From forage, finite non-refilling drinkable water is an optional adjacent branch while a second edge continues to a material junction; that junction exposes stone and timber on separate adjacent leaves in seed-selected assignment. Validation requires pristine surface, fluid, and logistics owners and logistics revision headroom; commit prechecks all three owners before applying any mutation. Generated records, water location, and normalized traversal edges then persist as ordinary world truth. This proves only the bounded opening opportunity, not terrain, ecology, hydrology/flow/recharge, general pathfinding/route generation, or remote generation. |
| Opening water -> player locality -> survival hydration | `available_adjacent_drink_sources` -> `validate_start_player_step` -> `AppState::available_local_fluid_stores` / `available_local_drink_sources` -> `validate_drink*` -> tick | The adjacent observation surface is bound to the admitted player's actual voxel and reveals only immediately face-adjacent drinkable source identity/fluid/position, not exact volume or temperature. Movement pays canonical load-sensitive survival cost and changes logistics locality only at completion. Exact-local observation then exposes finite contents; drink admission rechecks locality, volume, serving limits, physiology, and attention. A generated finite pool proves ordinary opening water access, not general hydrology or remote source discovery. |
| Production work -> inventory / equipment / energy | `advance_tick` completion | Completion routes exact outputs, applies equipment/energy consequences, releases occupancy/reservations, and emits `ProcessCompletion` with inventory landing identities. |
| Fatal survival -> active player work | `advance_tick` fatal-work planning | Work due on the fatal tick completes first. Otherwise each work owner applies its declared suspend/cancel/release semantics, settles physical equipment consequences for elapsed active ticks, and preserves represented custody plus trusted-load validity. |
| Inventory -> equipment embodiment | `validate_assemble_equipment` / `validate_upgrade_equipment` | Exact material traces become equipment embodiment; assembly establishes world custody when logistics exists, while upgrade preserves equipment identity/location and adds the authored trace. The owner boundary remains available to controlled actorless/partial fixtures, but a fully admitted player must be living and the authorization remains bound to that survival revision until commit. |
| Equipment embodiment -> inventory recovery/service | `validate_disassemble_equipment` / `validate_equipment_maintenance` | Disassembly returns authored recovery and removes equipment custody. Actorless/partial controlled disassembly remains an owner capability boundary; ordinary admitted-player disassembly requires living survival and rechecks its bound survival snapshot at commit. Recovery into carried custody is rejected while a step is active, with the same guard repeated at commit so a validated disassembly cannot race a later step start. Maintenance commits its material exchange at admission; player work owns the service interval, interruption retains proportional completed recovery, and scheduled completion reaches the full service target. |
| Inventory -> finite energy-store embodiment | energy-store assemble/upgrade/disassemble validators | Exact traces cross between inventory and `EnergyState`; lifecycle operations preserve or retire identity/location according to the validated transition. Controlled actorless/partial lifecycle operations remain valid; once ordinary player admission exists the validators require living survival and bind that revision through commit. Disassembly recovery into carried custody uses the same active-step load-increase guard at validation and commit. |
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

`LogisticsState` owns the player's persisted fixed-point body state, the inventory-owned carried stockpile
identity, stationary stockpile locations, equipment locations, energy-store locations, fluid-store locations,
fractional direct-locomotion exertion awaiting slow survival settlement, and a monotonic revision. The body stores
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
Trusted load bounds persisted velocity and locomotion fractional state, trusted load/chunk generation reject
body/terrain overlap, and player admission/chunk-generation tokens bind the opposite owner revisions so a
validate/commit race cannot create an overlap. Fall damage, step-up, partial shapes, surface traction, and demand
streaming remain absent.

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
inventory. An active step freezes instantaneous carried-load increases until completion. This shared guard covers
ground pickup plus equipment and energy-store disassembly recovery into carried custody, and each validated token
rechecks the rule at commit. Load-reducing drop remains valid while the player is still authoritatively at the
origin. Mount/unmount changes support without teleporting an existing location. Assembly establishes player-local
world custody for newly created equipment/energy stores; disassembly removes that custody.

Slow `TickSpan` player work may start in generated-world body mode only at an exact control/simulation boundary
while the body is grounded and horizontally stopped. The older timed face-adjacent step remains available only
where generated-world body physics is inactive; it is rejected once the player's footprint is backed by generated
chunks, so ordinary terrain movement cannot accidentally continue through the legacy traversal-edge path.

Prospecting requires the player inside the survey region and any instrument at the player voxel. Mining requires
the player inside the acquired resolved target region plus local tool/output access. Manual power requires local provider
and destination store. Production uses an actor-independent same-site rule: every explicitly located continuing
endpoint in one admitted job must agree on a voxel. Validation tokens bind logistics revision where location can
invalidate admission. Player movement projects carried output destinations at the destination voxel and rejects a
step that would split an admitted production site; process admission and commit likewise cannot add a new output
reservation to carried custody while that custody is already in transit. Movement also rejects carried custody
with completed mining output still awaiting automatic settlement, preserving both extraction locality and the
load-sensitive locomotion budget until the reserved parcel has actually landed.

General pathfinding, automatic proximity search, generic stockpile/energy/fluid transport, arbitrary equipment
teleportation, and mounted-production site/contact geometry are absent. Explicit
player carrying still provides exact-local pickup/drop; the traversal-edge step remains a bounded legacy/planning
surface outside generated-world body mode rather than the ordinary terrain locomotion path.

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
temperature limit cannot be used as a read-only probe of hidden geology. Geology retains matter custody during
labor. Completion extracts the slice and ordinarily settles it directly into the reserved inventory destination,
returning the exact contribution and merge-aware landing identity through `TickOutcome::mining_output()`. If a
post-admission structural condition or same-tick inventory owner prevents that landing, the slice remains in
mining-owned awaiting-settlement custody with its reservation and future headroom until a later tick can retry the
crate-owned settlement. Carried destinations cannot move during that interval. The recovery phase is durable for
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

Manual shaping conserves material identity/mass and cannot change phase. Pure durable input lots may span exact
temperatures: form-only shaping retains every selected thermal cohort and apportions each authored output across
the remaining cohort distribution at whole-milligram precision, so crafting neither averages heat nor performs
free temperature separation. Powered crafting delegates the same material/yield transform. Optional equipment may
reduce attention through canonical condition-adjusted throughput; required equipment rejects the equipment-free
route. Equipment changes throughput/wear, not recipe yield. Scrap/chip outputs remain represented matter until an
explicit authored recovery process consumes them.

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
