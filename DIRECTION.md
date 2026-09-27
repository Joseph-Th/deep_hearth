# Direction

This page owns future system-integration priority and accretion strategy. It does not claim that planned
capabilities are implemented. Use [`STATUS.md`](STATUS.md) for current reality, [`GAME_DESIGN.md`](GAME_DESIGN.md)
for intended player experience, [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) for implemented contracts, and
[`README.md`](README.md) for routing.

The objective is not maximum feature count. The objective is a dense, comprehensible simulation graph in which
new capability reuses existing physical/state abstractions, closes real control loops, and increases the number
of meaningful interactions without multiplying rulesets.

This is strategic direction, not an executable task queue. The user's current request and any explicitly
authorized task remain the work authority. Re-evaluate the sequence against [`STATUS.md`](STATUS.md) after each
substantial vertical slice rather than preserving stale priority for its own sake.

## Planning map

| Planning question | Read |
| --- | --- |
| What kinds of work have the most connective leverage? | [Accretion objective](#accretion-objective) |
| Which agent/player control-surface debts are worth paying down, and in what refinement order? | [Control-surface program](#control-surface-program) |
| What broad integration order best reuses existing abstractions? | [Default integration sequence](#default-integration-sequence) |
| When is a vertical slice actually complete and accretive? | [Vertical-slice completion contract](#vertical-slice-completion-contract) |
| Which tempting abstractions should not be added? | [What not to accrete](#what-not-to-accrete) |

## Accretion objective

Prefer work with high connective leverage:

1. closes a missing edge between existing authoritative owners;
2. turns controlled setup or an implicit assumption into an ordinary canonical path;
3. exposes a missing observation, blocker, projection, or outcome needed to control an existing system;
4. lets one existing resource, capability, or infrastructure investment participate in more systems;
5. replaces repeated direct attention with physical logistics, delegation, storage, maintenance, or automation;
6. creates a recoverable feedback loop rather than a terminal special case;
7. can be proved locally at its owner boundaries before requiring broad scenario evidence.

Prefer a smaller connected graph over a larger disconnected catalog. A feature that adds many definitions but
no new owner interaction, decision surface, or recovery path has low priority unless it is required to close a
specific vertical slice.

## Control-surface program

When a slice touches an owner, make the existing control path easier to operate before adding parallel
abstractions. Legitimate callers should be able to find:

- current relevant state or a canonical assessment;
- legal action prerequisites and typed blockers;
- a useful projection for costly choices when production already knows the controlling physics;
- the canonical authorization/mutation boundary;
- the committed outcome and stable identity needed to continue, inspect, claim, repair, or reverse work.

Prefer improvements in this order:

1. consume an existing projection/outcome instead of rescanning state;
2. propagate an owner result that a crossed edge currently discards;
3. add a typed immutable reverse index or read-side projection for repeated domain reconstruction;
4. expose a production-owned feasible bound when repeated probing varies only one physical dimension;
5. batch canonical `advance_tick` calls only when callers repeatedly hand-roll the same bounded wait;
6. add freshness metadata only when a useful read-side result is actually retained across mutations.

Keep strategy outside production. Candidate ranking, goals, risk tolerance, and search budgets belong to the caller;
production owns legality, physics, and authoritative outcomes. Do not build a universal action bus, AI facade,
reflection layer, or mutable availability cache for agent convenience.

Prioritize future slices that close a real current loop, reuse existing owners and physical currencies, replace
controlled setup or duplicated reasoning with an ordinary path, connect several existing investments, expose
recoverable failure, and admit a focused proof. Re-evaluate after each substantial slice because closing one edge
changes the value of the next.

## Default integration sequence

This is a dependency-oriented planning order, not a release promise. A smaller vertical slice may move earlier
when it closes a stronger loop with less machinery.

Agent-operability is not a separate roadmap phase. Apply the control-surface and semantic-entropy programs inside
each slice when concrete friction is exposed, so the repository becomes easier to extend as the graph grows.

### 1. Close existing control loops

Before opening major domains, finish ordinary authorization around implemented physical transitions when a small
missing edge blocks use. Typical high-value slices include ordinary acquisition, player-authorized
construction/deconstruction or recovery, and production read surfaces that replace controlled setup or caller
reconstruction. Preserve meaningful scale boundaries: portable generation should not stand in for industrial
power infrastructure, and chemical reduction should enter only when authored material chemistry requires a
reductant/byproduct model.

Completion criterion: the capability can move from controlled/capability-only evidence toward ordinary play
without adding an alternate semantic path.

### 2. Establish world-space action and logistics

Build world-space action and logistics on the persistent custody/location semantics defined in
[`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md). Logistics should own placement, carrying/haulage, delivery, access,
path cost, and transport time/energy/labor without turning inventory into a movement authority. Extend the same
model to movement/path authorization, ordinary source acquisition, mounted-production site/contact geometry,
fluid transport/pumping, and explicit mounted-to-mounted equipment transport. Prefer physical placement and
transport over generic proximity predicates.

Its direct-player surface should deliberately use familiar block-survival grammar: an active hotbar item,
primary break/use-tool action, secondary place/interact/consume action, ordinary pickup/drop, and slot/container
transfer semantics. The logistics owner may still charge exact mass, volume, encumbrance, path, time, exertion,
temperature, spoilage, and preservation consequences underneath those interactions. Internal lot selection,
reservations, claims, and commit boundaries should be composed behind the familiar action unless an intermediate
choice is itself meaningful gameplay.

This layer should connect geology, stockpiles, structures, production, maintenance, and later settlement labor.
It should reuse persistent spatial identity/bounds rather than introducing a parallel coordinate model. Carried
inventory should be a real custody/location state with explicit capacity rather than an unbounded alias for every
nearby stockpile; world containers should reuse the same underlying material/storage facts while presenting
familiar inventory slots and quick-transfer operations.

Completion criterion: important material transitions can state not only what moves between owners, but how the
world authorizes and pays for that movement, while ordinary direct manipulation still feels like a conventional
block-survival inventory and interaction loop.

### 3. Close the familiar wilderness shell

Once direct world interaction and carrying exist, prioritize the ordinary first-session survival loop before
adding more industrial-network depth. A fresh player should be able to acquire visibly available wood/stone and
forage, carry them through the hotbar/inventory shell, make the first primitive tools, place and open basic
storage, establish simple shelter/light/fire, eat/drink, and recover from ordinary early mistakes without
controlled delivery or hidden-state tooling.

This slice should reuse the existing material, crafting, equipment, storage, survival, fluid, thermal, and
structural owners. Familiar direct actions should be the entry point; deeper properties such as exact mass,
spoilage, temperature, hydration, tool wear, structure load, and fuel/heat should become consequences of those
actions rather than prerequisites for learning a special interaction language.

Completion criterion: from a fresh ordinary world and empty carried inventory, the player can complete a
recognizable block-survival first-day loop through canonical world actions, including basic acquisition, tool
crafting, storage, food/water, and shelter/fire, without harness-only material injection.

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
