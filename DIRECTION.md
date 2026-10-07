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

### 2. Establish world-space action and logistics

Build world-space action on the existing custody/location model. Logistics owns placement, carrying/haulage,
delivery, access, path cost, and transport resource cost while inventory retains matter custody. Use the same
model for movement, source acquisition, mounted-production geometry, fluid transport, and equipment transport.

The direct-player surface uses familiar block-survival actions: active hotbar context, primary tool/break action,
secondary place/interact/consume action, pickup/drop, and slot/container transfer. Exact mass, capacity,
encumbrance, path, exertion, temperature, spoilage, reservations, and commit boundaries remain underneath those
interactions. Reuse persistent spatial identity and make carried inventory explicit finite custody.

Completion criterion: important material transitions can state not only what moves between owners, but how the
world authorizes and pays for that movement, while ordinary direct manipulation still feels like a conventional
block-survival inventory and interaction loop.

### 3. Close the familiar wilderness shell

After direct world interaction and carrying exist, close the ordinary first-session survival loop before adding
more industrial-network depth. A fresh player should acquire visible wood/stone/forage, carry them through the
normal inventory shell, make primitive tools, use basic storage, establish shelter/light/fire, eat/drink, and
recover from ordinary mistakes without controlled delivery or hidden-state tooling. Reuse existing material,
crafting, equipment, storage, survival, fluid, thermal, and structural owners. Familiar direct actions should be
the entry point; exact physical consequences should remain consequences of those actions rather than
prerequisites for learning a special interaction language.

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
quantities.

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
