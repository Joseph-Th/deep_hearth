# Direction

**Role:** Future system-integration priority and accretion strategy.

This page owns future integration priority, not current capability. Use [`STATUS.md`](STATUS.md) for implemented
scope, [`GAME_DESIGN.md`](GAME_DESIGN.md) for intended experience, and [`README.md`](README.md) for routing.
Prefer a dense, comprehensible simulation graph over feature count. The sequence is dependency guidance, not a
task queue; re-evaluate it when the integration frontier changes.

## Planning map

| Planning question | Read |
| --- | --- |
| What kinds of work have the most connective leverage? | [Accretion objective](#accretion-objective) |
| What block-survival substrate must exist before deeper breadth is the main frontier? | [Block-world parity gate](#block-world-parity-gate) |
| Which agent/player control-surface debts are worth paying down, and in what refinement order? | [Control-surface program](#control-surface-program) |
| What broad integration order best reuses existing abstractions? | [Default integration sequence](#default-integration-sequence) |
| When is a vertical slice actually complete and accretive? | [Vertical-slice completion contract](#vertical-slice-completion-contract) |
| Which tempting abstractions should not be added? | [What not to accrete](#what-not-to-accrete) |

## Accretion objective

Prefer work that closes an existing owner edge, turns controlled setup into an ordinary path, exposes a missing
control surface, reuses existing infrastructure across more systems, replaces repetitive attention with physical
delegation, or closes a recoverable feedback loop. Favor slices that can be proved at owner boundaries before
requiring broad scenario evidence.

Prefer a smaller connected graph over a larger disconnected catalog. A feature that adds many definitions but
no new owner interaction, decision surface, or recovery path has low priority unless it is required to close a
specific vertical slice.

## Block-world parity gate

The immediate integration frontier is the missing block-survival substrate identified in
[`BLOCK_SURVIVAL_PARITY.md`](BLOCK_SURVIVAL_PARITY.md). Deep Hearth's deeper simulation owners are not a substitute
for the ordinary world shell that makes those systems playable.

Before additional industrial breadth becomes the default priority, ordinary play should have one coherent route
through these foundations:

1. a high-frequency authoritative game tick suitable for direct movement, with calendar progression no longer
   defining the kinematic timestep;
2. compact runtime block states in streamed chunk/section ownership with deterministic seeded terrain;
3. continuous player transform, gravity, collision, jump/crouch/sprint and terrain contact;
4. camera targeting plus primary break/attack and secondary place/use interaction composition;
5. world-drop rendering plus adapter inventory/container/crafting interaction;
6. block/sky lighting, section meshing and enough renderer/input/save adapters to experience the real generated
   world rather than a harness-only abstraction.

This gate is mechanical, not a demand to clone Minecraft's implementation or content. Deep Hearth should keep
its stronger conservation, material, structure, survival, process, determinism and ownership models underneath
the familiar shell. A difference must have a concrete reason and cannot remove the basic player verb.

## Control-surface program

A usable owner control path exposes current relevant state or a canonical assessment, legal prerequisites and
typed blockers, useful production-owned projections for costly choices, the canonical authorization/mutation
boundary, and the committed outcome or continuation identity. When a slice touches an owner, improve that path
before adding another abstraction. Prefer, in order: reuse an existing projection or receipt; propagate a
discarded owner result; add a narrow reverse index or semantic projection; expose a production-owned feasible
bound; batch canonical ticks only for bounded repeated waits; add freshness metadata only for retained read-side
state.

Strategy remains outside production. Candidate ranking, goals, risk tolerance, and search budgets belong to
callers; production owns legality, physics, and authoritative outcomes. Avoid generic action buses, AI facades,
reflection layers, and mutable availability caches.

## Default integration sequence

This is a dependency order, not a release promise. A smaller slice may move earlier when it closes a stronger
loop with less machinery. Improve control surfaces within each slice when concrete friction is exposed, so the
repository becomes easier to extend as the graph grows rather than treating operability as a separate phase.

### 1. Close existing control loops

Close small authorization or acquisition gaps around implemented physical transitions before opening major new
domains. Typical slices are ordinary acquisition, player-authorized construction/recovery, and production read
surfaces that replace controlled setup or caller reconstruction. Preserve scale boundaries: portable generation
does not substitute for industrial power, and chemical reduction requires an explicit reductant/byproduct model.

Completion criterion: the capability can move from controlled/capability-only evidence toward ordinary play
without adding an alternate semantic path.

### 2. Establish the block world and first-person action

Build a real persistent voxel world around the existing spatial, material, custody and location vocabulary.
Introduce compact block definitions/states, chunk/section ownership, deterministic terrain generation, streaming,
world alteration and chunk-granular persistence. Keep block identity, physical material identity and domain-owner
attachments distinct so ordinary cells stay compact while exceptional temperature, damage, moisture, embodied
matter or machine state can live in sparse owner-controlled state.

The first storage slice now exists: `WorldState` owns persisted 16x16 chunk columns containing palette-compressed
16-cubed sections, `WorldRegistry` owns authored block/state identity, and explicit-seed generation can commit
deterministic stone terrain through a revision-bound transaction. That is infrastructure, not ordinary terrain
play. The body path now persists a fixed-point bottom-center transform plus horizontal/vertical velocity in
logistics and runs 20 Hz normalized walking input, authored acceleration/braking, gravity, and swept full-cube
wall/floor/ceiling collision wherever the current footprint has generated chunk truth. Missing destination chunks
remain hard authority boundaries rather than air, but the first terrain seed/state pair is now persisted as world
generation identity and `validate_generate_player_neighborhood` can atomically fill the missing 3x3 neighborhood
around the admitted player in one revision. The same body path owns stance, ledge-safe crouching, load-limited
sprinting, and load-sensitive jumping. Adapter residency/unload remains open; the next gameplay slice can move to
direct block action composition while streaming composition is added around this demand boundary. The core now
accepts a normalized fixed-point view direction and deterministically raycasts from authored stance-specific eye
height through generated full-cube terrain, with a 4.5 m authored reach and missing world truth treated as a hard
authority boundary.

The authoritative-time foundation separates a persisted 20 Hz `ControlTick` from the existing 3.6-second
physical/accounting `SimulationTick`. `advance_control_tick` now owns both high-frequency progression and active
generated-world body gravity/collision, running the slow transaction exactly every 72 control steps. `advance_tick`
fails closed when that body physics is active instead of skipping intervening control semantics. Existing
`TickSpan` work remains boundary-gated and generated-world slow work additionally requires ground contact.

Direct player movement is now on the target authority path for basic terrestrial locomotion: fixed-point body
position, horizontal/vertical velocity, persisted standing/crouching stance, walking, load-limited sprinting,
grounded load-sensitive jumping, gravity, swept full-cube collision, crouched ledge protection, save/load
continuation, and body/terrain race rejection are authoritative. Realized horizontal effort is speed- and
load-sensitive and settles through the existing slow survival owner. Step-up, partial shapes, surface traction,
fall injury, and adapter residency/unload remain open; deterministic player-neighborhood terrain demand now exists,
and the timed one-voxel labor step is rejected once generated-world body
physics is active. Deterministic player-eye targeting now feeds a conserved timed terrain-excavation owner path:
authored mining physics binds portable tool throughput/hardness/wear, partial represented mass persists per cell,
and completed fragments land in reserved carried custody when the complete batch fits, otherwise in finite loose
surface custody at the player's work position. Player-created drops are persisted separately from natural
opportunities and have an exact-local drop-only projection. Control-time pickup transfers as much mass as current
carried capacity permits and leaves any remainder under the same conserved world identity; natural opportunities
still require deliberate gathering.
`PlayerDirectControlInput` now composes held primary excavation intent over that same owner path: any idle control
step can choose the largest target/tool batch, typed rejection is observable, and a held action can start the next
legal batch on the exact control step that completes the previous one. Excavation persists 20 Hz start/completion
schedule and normalized progress while preserving authored mining duration, exertion, wear, custody, and save/load
continuation. Logistics now persists nine semantic quick-access slots plus one selected slot over canonical carried
custody. Equipment bindings still drive primary excavation and one-step equipment drop through the existing carry
owner. Material bindings name authored carried commodities rather than transient lot identities; contextual secondary use
composes that selection into canonical food consumption when one physical cohort is unambiguous, and can place
an authored finished storage body into the generated-air voxel adjacent to the pointed block face. Placement
rejects missing world authority, terrain occupancy, player-body overlap, and existing stationary object custody.
Direct drop transfers the selected carried commodity into exact finite surface parcels without flattening
temperature, composition, particulate state, provenance, or storage age, and uses a short control-time self-pickup
delay before ordinary automatic recovery. Inventory now also owns a revision-bound familiar material-stack
projection derived from the same physical compatibility rules as lot coalescing, including current physical
storage-age exposure for freshness presentation. A projected stack can drive partial same-voxel carried↔ground/storage
transfer by mass while inventory privately resolves exact lot slices and rejects stale projections. A single
player-facing inventory projection composes carrying state, all nine hotbar bindings, carried equipment/stacks,
and only exact-local containers. `validate_quick_move_player_stack` now composes the familiar maximum-fit transfer
gesture between carried custody and one local container while exact relocation remains authoritative. Direct
control can switch slots before primary/use/drop on the same 20 Hz sample.
Personal manual crafting now composes the same carried custody into a stable recipe catalog plus recipe/batch
start action. Durable homogeneous inputs resolve to exact lots privately; freshness-sensitive inputs keep one
explicit revision-bound stack choice, optional equipment remains an exact revalidated provider, and output returns
to carried custody through ordinary timed production. Adapter inventory/container/crafting rendering and input bindings,
rendered/spatial drop presentation, arbitrary block/object placement, and generic moving item entities remain open.

Logistics still owns placement, carrying/haulage, delivery, access, path cost, and transport resource cost while
inventory retains matter custody. AI routing and haulage should derive traversability from world geometry and
movement capability rather than requiring hand-authored traversal edges.

The current timed face-adjacent step fails closed behind persisted logistics-owned traversal edges. Fresh world
generation now authors a short five-edge opening opportunity graph, while `AppState::available_player_steps` exposes only
choices from the actor's current voxel and immediate source observation is gated by those same edges. This
remains useful bounded evidence that locality, physiology and carried load affect decisions. It is not the target
ordinary movement architecture and should not be extended into a hand-authored world graph. Once continuous
world motion owns direct locomotion, retain or replace the step abstraction only where logistics/AI planning still
needs a discrete route-cost representation.

The direct-player surface uses familiar block-survival actions: active hotbar context, primary tool/break action,
secondary place/interact/consume action, pickup/drop, and slot/container transfer. Exact mass, capacity,
encumbrance, path, exertion, temperature, spoilage, reservations, and commit boundaries remain underneath those
interactions. Reuse persistent spatial identity and make carried inventory explicit finite custody.

Completion criterion: from a generated spawn, the player can look, walk, sprint, crouch, jump, fall and collide
through streamed block terrain; raycast and break a visible world block; receive or leave its conserved matter as
a world drop; manipulate it through a familiar hotbar/inventory shell; and place/use a basic block or object
through canonical owner operations. Save/reload preserves the altered world and player continuation. No authored
traversal edge is required merely to walk across otherwise traversable terrain.

### 3. Close the familiar wilderness shell

After direct world interaction and carrying exist, close the ordinary first-session survival loop before adding
more industrial-network depth. A fresh player should acquire visible wood/stone/forage, carry them through the
normal inventory shell, make primitive tools, use basic storage, establish shelter/light/fire, eat/drink, and
recover from ordinary mistakes without controlled delivery or hidden-state tooling. Reuse existing material,
crafting, equipment, storage, survival, fluid, thermal, and structural owners. Familiar direct actions should be
the entry point; exact physical consequences should remain consequences of those actions rather than
prerequisites for learning a special interaction language.

The current explicit-seed opening generator closes a bounded five-edge horizontal opportunity graph. Forage
appears one step from spawn. From forage, a finite non-refilling water branch competes with a continuation to a
material junction, where loose stone and timber are simultaneously visible on two adjacent leaves. The lived
wilderness episode proves that current physiology changes route choice: a rested actor skips the water detour,
while a hydration-pressure actor chooses it and drinks before returning to the work path. Both reach the material
junction through traversal-gated immediate observation and pay every load-sensitive step. Generated forage and
water quantities are finite and seed-varied; their minimum opportunities exceed one resting day of basal need,
while the lived actor deliberately gathers only a portable ration and leaves local surplus in world custody.
Exact water quantity remains unknown unless the actor visits its branch. This closes a real
distributed opening slice without claiming terrain geometry, general route generation/pathfinding, ecology,
hydrology/flow/recharge, or broader world discovery.

The worksite path also closes the first bounded fire/warmth edge. Ordinary unsheltered time drains a persisted
thermal-comfort reserve; the existing long cache project naturally enters the chilly band. The actor prices the
known fire response before committing, gathers exact local stone, assembles a stone-ring campfire, and burns one
material-backed charge from staged timber into finite thermal storage. The same passive heat that leaves that
co-located store restores player warmth, while remote heat does not. Fuel mass remains conserved in an explicit
combustion terminal account. This is a primitive local heat source, not weather, environmental heat transport,
atmospheric chemistry, light, or shelter semantics.

The structural owner now also has one ordinary direct-player construction path below shelter semantics. An
admitted player may plan a grounded member only in one face-adjacent voxel that does not contain the player, commit exact carried
rigid material into that planned member, and spend authored hand-setting work plus survival reserves before it
becomes load-bearing. In-progress work survives save/load and production-owned same-tick structural loads are
projected before activation. This is a construction substrate, not a shelter implementation: non-grounded
framing, joints, panels, deconstruction, weather/exposure, sleep, enclosure, and shelter benefit remain open.

Treat this as a substantive wilderness era, not a short bootstrap into copper. A normal opening should spend its
first quarter-hour on the immediate camp economy: local acquisition, food/water, fire/shelter, storage, and a
small stone-and-timber toolkit. Geological clues may establish a future objective, but copper extraction and ore
processing are later work. Keep that pacing physical rather than adding an unlock timer: gathering, fabrication,
survival needs, movement, construction, and actual geological opportunity should create the delay.

Completion criterion: from a fresh ordinary world and empty carried inventory, the player can complete a
recognizable block-survival first-day loop through canonical world actions, including basic acquisition, tool
crafting, storage, food/water, and shelter/fire, without harness-only material injection. The fresh-start proof
must begin before copper progression and must not seed copper, prebuilt tools, provisions, or shelter components
as a substitute for those opening actions.

### 4. Add explicit physical networks

Once world-space movement, the familiar wilderness shell, and infrastructure placement have a coherent
substrate, extend the same graph to carrier networks: mechanical transmission, electrical distribution, and
fluid transport. Network state should own topology and losses; endpoint stores remain the authority for stored
quantities. The current finite-store model intentionally reserves one selected store for a production job, so a
shared flywheel bank can reduce recharge frequency without acting as a parallel workshop bus. Preserve that
physical bottleneck until an explicit network owns distribution; do not obtain concurrency by weakening endpoint
occupancy.

Completion criterion: energy/fluid transfer is a physical routed operation with capacity, loss, occupancy,
failure, inspection, and recovery rather than generic store-to-store mutation.

### 5. Delegate through the same action model

Workers, animals, schedules, and automation should consume the same observable tasks, production legality, world
movement, and physical costs as direct player action. Delegation should change who supplies attention and how
work is organized, not create a second simulation path.

Completion criterion: a solved manual loop can be assigned, observed, interrupted, recovered, and audited while
preserving the same owner transitions as direct execution.

### 6. Deepen environmental feedback

Climate, hydrology, environmental heat, sanitation, ecology, and agriculture should enter after they have
owners and control surfaces capable of affecting existing survival, storage, structures, logistics, energy, and
production loops. Prefer environmental state that creates actionable forecasts and infrastructure responses over
ambient complexity with no practical lever.

Completion criterion: environmental variation changes decisions through explicit signals, flows, and recovery
actions rather than opaque periodic penalties.

### 7. Expand industrial transformation depth

After progression gaps have ordinary physical routes, add alloying, forging, machining, broader
separation, combustion/thermal plant depth, chemistry, and advanced power as extensions of the existing
material, thermal, energy, capability, equipment, maintenance, and logistics abstractions. Each process stage
must own a distinct physical transformation or control problem.

Completion criterion: added industrial depth increases material choice, throughput, recovery, energy/logistics
demand, maintenance, precision, or automation leverage without becoming recipe-only nesting.

## Vertical-slice completion contract

A planned capability is not complete merely because its core calculation exists. Before promoting it in
[`STATUS.md`](STATUS.md), the slice should answer:

| Question | Required result |
| --- | --- |
| Intent | The player/system decision or obligation is clear in [`GAME_DESIGN.md`](GAME_DESIGN.md) or follows an existing law there. |
| Owner | Each consequential fact has one authoritative lifecycle owner. |
| Observation | Legitimate callers can inspect the state/blocker needed to choose an action without private-state reconstruction. |
| Authorization | Legal action derives from production rules with typed rejection. |
| Mutation | One canonical path performs the consequential transition atomically at its promised boundary. |
| Flows | Matter, fluid, energy, labor, information, support, capacity, identity, and time transfers are explicit where applicable. |
| Persistence | Future-affecting custody and schedule state survive; derived data rebuilds deterministically. |
| Outcome | The committed consequence is legible enough for continuation, diagnostics, presentation, and tests. |
| Recovery | Important failure/blockage has an explicit repair, resume, reroute, reclaim, or terminal-boundary story. |
| Proof | Focused owner/boundary evidence exists; cross-system/gameplay evidence exists when the claim crosses those boundaries. |
| Reachability | [`STATUS.md`](STATUS.md) classifies ordinary, capability-only, implemented infrastructure, or absent scope truthfully. |
| Addressability | The slice has one obvious owner/control path, explicit crossed edges, stable semantic search anchors, and no undocumented relationship required to operate it correctly. |

An accretive slice should reduce or preserve future reasoning cost and need no proof broader than the behavior
actually claimed. If it works only because its implementer remembers undocumented relationships, it is not yet
agent-accretive.

## What not to accrete

Do not add these merely to reduce short-term implementation friction:

- a second state store for a fact already owned elsewhere;
- generic mutable service locators or managers that obscure lifecycle ownership;
- UI-, harness-, or AI-specific copies of legality, costs, timing, or physical formulas;
- global action/event abstractions that erase typed domain identity and failure semantics;
- compatibility/migration machinery without an active supported compatibility contract;
- broad caches whose invalidation owner is less clear than recomputation;
- new content tiers whose required transport, construction, maintenance, information, or recovery loops are
  still absent.

The governing heuristic is simple: make the graph denser, the control surface clearer, and the proof cheaper
before making the catalog wider.
