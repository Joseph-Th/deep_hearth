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

Agent/player legibility is an ongoing architectural program, not a separate subsystem. When an owner is touched,
check whether callers can answer its important control questions through production surfaces:

- current relevant state or assessment;
- legal action families or concrete action prerequisites;
- representative blockers with typed reasons;
- projected physical/cost consequence where planning materially benefits;
- committed typed outcome;
- stable identity needed to continue, inspect, claim, repair, or reverse work.

Add the narrowest missing projection or outcome at the owner. Do not create a universal action bus, generic
reflection layer, harness-only legality model, or privileged AI state surface merely to make automation easier.
Typed domain operations remain the authority.

### Control-surface maturity

Do not pursue API symmetry for its own sake. Mature each control path only as far as its real callers need:

| Need | Preferred production surface | Current strong examples |
| --- | --- | --- |
| Understand current pressure/state | canonical read-only assessment or owner record accessor | `SurvivalAssessment`, `GeologicalKnowledgeAssessment`, `StructuralAssessment` |
| Compare a costly action before committing | deterministic `Resolved*` / decision object exposing material bottlenecks and consequences | powered ore `Resolved*`, thermal `Resolved*`, equipment maintenance resolution |
| Protect consequential mutation | revision/state-bound `Validated*` token with typed rejection and a consuming commit | production start, mining, equipment/storage/energy lifecycle operations |
| Continue delayed work | stable runtime identity plus persisted job/custody/schedule state, with admission schedule returned when callers need it | `ProductionJobId`, `MiningJobId`, `PlayerWorkState`, `EatOutcome::completes_at`, `DrinkOutcome::completes_at` |
| Observe committed consequence | typed outcome/completion/claim result containing stable identity and material deltas needed downstream | `TickOutcome`, `ProcessCompletion::landings`, `MiningClaimReceipt`, maintenance/disassembly/support outcomes |
| Act on hidden truth safely | opaque evidence-derived authorization that withholds the hidden owner identity | `MiningTargetResolution` |
| Adapt a scalable request | production-derived feasible bound or bottleneck when the domain already knows the monotonic limiting quantities | equipment/process batch limits and typed finite-energy/condition blockers are ingredients; do not force callers to rediscover the combined envelope by repeated failure |
| Retain planning safely | narrow dependency/revision or equally explicit invalidation semantics when a read-side result is expensive enough to keep | revision-bound production completion and validated owner operations show the pattern; do not add a global world revision solely for convenience |
| Discover a route toward a goal | goal-directed immutable topology lookup that exposes authored producers/providers/assembly/upgrade/recovery relationships without claiming current availability | today several gameplay catalog/progression helpers repeatedly scan registries to reconstruct these relationships |
| Wait for a known temporal boundary | bounded batched stepping over canonical `advance_tick`, preserving ordered outcomes and optional actor-visible stop conditions | many gameplay/tests hand-roll `advance_exact`; workshop already uses `TickOutcome` to stop on completion or suspension |
| Reason safely from absence | query result with explicit scope and completeness: exhaustive, bounded-partial with continuation, or sampled/replayable | gameplay evaluation already distinguishes bounded search from unavailability; future production/topology discovery should make the same distinction structurally |

Treat raw-state access as control-surface debt only when a legitimate caller must reconstruct domain meaning,
legality, or prediction that a production owner already knows how to derive. Reading an exact record field for
reporting, identity, or already-authoritative state is not itself debt. This distinction prevents agent
ergonomics from turning into redundant facade construction.

When a harness, adapter, or future autonomous actor contains copied thresholds, physical formulas, provider
matching, hidden-state workarounds, or before/after inference solely because production exposes no semantic
answer, move the derivation to the narrowest production owner and reuse it. When the consumer merely formats or
aggregates canonical values, leave that work outside the owner.

Repeated legality calls are not automatically debt. It is legitimate for an actor to evaluate genuinely
different alternatives. Debt exists when the alternatives differ only along a monotonic dimension and the
production resolver already computes the limits needed to answer that dimension directly.

Use increasing claim strength when planning control surfaces: **direct authored edge -> authored path -> ordinary
reachability -> current opportunity -> authorization -> committed consequence**. Registry-derived topology should
make the first two cheap; it must not silently upgrade them into the stronger claims. Local
`has_authored_acquisition_edge` and `has_authored_assembly_edge` predicates are deliberately direct-edge
declarations. Keep transitive authored-path and ordinary-reachability claims in topology/reachability authorities
instead of pushing them into individual definitions.

### Current operability signals

Use concrete callers to distinguish useful future work from theoretical API symmetry:

| Signal | Current evidence | Direction |
| --- | --- | --- |
| Some `()` commits are already semantically complete | storage-enclosure construction changes the caller-named stockpile immediately; controlled structural materialization changes the caller-named element; field-prospecting validation already exposes `ProspectingWork` with its schedule and completion later emits `FieldProspectingOutcome`. | Preserve `()` where no new continuation fact is created. Improve callers that discard an existing token/outcome before adding signature ceremony. |
| Canonical time advancement is repeatedly wrapped by caller loops | progression, survival, thermal, ore, and workshop code repeatedly loop `advance_tick`; workshop's job advance helper already consumes typed completion/suspension outcomes, while many `advance_exact` helpers discard outcomes. | Prefer one bounded batch/stop abstraction only if it serves multiple legitimate callers. It must call canonical ticks and preserve outcome order. Do not treat known schedules as permission for semantic fast-forward. |
| Structural relocation can be inspected before mutation | validated equipment relocation exposes the resulting structural analysis used by the workshop actor before commit. | Preserve this pattern: prediction and authorization share production semantics, and the actor does not clone or mutate the world to preview legality. |
| Deterministic lot choice remains harness policy | `tests/gameplay_harness/material_selection.rs` chooses exact lots from observable stockpile order to satisfy an actor-selected mass. | Do not move this into production merely for reuse. The inventory owner supplies authoritative lots; tie-breaking among otherwise legal observable inputs belongs to actor policy unless product semantics require a canonical choice. |

Keep this table small. Remove a signal when its underlying friction disappears; it is a planning aid, not a
second status page.

### Operability refinement order

When an authorized implementation slice exposes these debts, prefer the least-semantic-cost improvement first:

1. **Consume existing semantics correctly.** Replace caller rescans or duplicated checks with already-available
   typed outcomes/projections before adding production API. Example: use `TickOutcome` completion identity rather
   than polling for job disappearance.
2. **Propagate discarded owner results.** If a lower owner already computes a continuation fact, carry it through
   the crossed edge. Current examples are the inventory-owned landing identities now propagated through
   production completion/mining claim and the direct-consumption completion schedule returned by admission.
3. **Compress immutable topology.** Add typed registry-derived reverse indexes for repeated producer/provider/
   construction/upgrade/recovery discovery. Prove exact derivation from definitions; do not add mutable world
   availability or actor preference.
4. **Expose production-owned feasible projections.** Replace repeated request probing with domain-specific
   bounds where production already derives the limiting dimensions. Powered ore follows this pattern through
   `PoweredOreMassEnvelope`. Homogeneous pure-material melting and casting now use thermal lot-mass envelopes;
   casting evaluates exact transfer-duration buckets because deferred passive sink recovery makes global mass
   feasibility non-monotonic. Do not simplify a coupled physical constraint merely to obtain a searchable scalar.
5. **Batch canonical continuation.** If multiple legitimate callers still hand-roll time loops, provide bounded
   stepping that executes `advance_tick` and preserves ordered outcomes. Do not implement semantic fast-forward
   as a convenience optimization.
6. **Add freshness metadata only on demonstrated retention need.** Expose narrow revision/dependency stamps when
   a useful planning result is expensive enough to retain across other actions. Do not publish every owner
   revision or introduce a global world revision preemptively.

At every step, remove the superseded caller reconstruction and its tests/diagnostics rather than retaining two
ways to derive the same semantic answer. This order is a refinement heuristic, not an executable task queue;
[`STATUS.md`](STATUS.md) changes only when runtime scope/reachability actually changes.

For new discovery surfaces, prefer **narrow exhaustive queries** before generic pagination. A topology query for
one commodity/provider relationship should usually be complete and cheap. Add continuation only when measured
result size justifies it, and never use truncation as a hidden resource-control mechanism.

### Agent-operability program

Every substantial slice should make the system at least as easy for the next agent to address correctly as it
was before. Treat repeated reasoning cost as design evidence, not merely as inconvenience.

High-leverage improvements are usually local:

- a canonical projection that replaces repeated raw-state reconstruction;
- a typed blocker that names the actionable owner/precondition instead of forcing broad diagnosis;
- a stable work identity/outcome that removes whole-state before/after inference;
- propagation of an existing destination-owner landing identity through a crossed custody edge;
- an explicit cross-owner edge that makes custody and stale dependencies followable;
- a routed authority/source/proof entry that prevents repository-wide search;
- a focused regression that turns a previously broad investigation into a cheap falsification.

Do not build a universal AI facade, reflection schema, duplicated action catalog, or generic planner API unless
the product itself genuinely needs that abstraction. Agent ergonomics should emerge from a more coherent domain
system, not a second semantic layer laid over an incoherent one.

### Semantic entropy budget

New capability has an information cost as well as an implementation cost. Before introducing a new concept,
ask whether it necessarily adds any of these: a new durable owner, a new operation lifecycle shape, a new flow
kind, a new generic status vocabulary, a new cache/invalidation rule, a new authority document, a new test lane,
or a new compatibility branch. Each addition must correspond to genuinely new semantics that cannot be expressed
cleanly through the existing tower.

Prefer changes that increase capability faster than they increase the number of concepts an agent must keep in
working memory. Reusing one owner or edge for another physically equivalent operation is usually more accretive
than adding a neighboring abstraction whose main benefit is local convenience.

Repeated friction is a useful prioritization signal. If several unrelated tasks repeatedly require the same
wide search, copied derivation, hidden-state workaround, broad audit, or manual state-diff reconstruction, fixing
that semantic bottleneck can have more connective leverage than adding another content branch.

For long-horizon agent control, prioritize **topology compression before planner centralization**. It is useful
for production to expose stable authored relationships and current semantic blockers that every legitimate
caller would otherwise rediscover. It is not useful for production to decide the actor's goal, route ordering,
risk tolerance, resource valuation, or stopping policy. A richer possibility/opportunity vocabulary should make
many planners easier to build without making one planner authoritative.

### Integration triage

Choose among otherwise valid future slices lexicographically rather than collapsing the design into one opaque
score. Earlier questions dominate later ones:

1. **Does it close a real current loop?** Prefer an absent edge that blocks ordinary progression, recovery,
   delegation, or control of already-implemented capability over a disconnected new content family.
2. **Can it reuse current owners and currencies?** Prefer matter/energy/labor/information/support/time flows that
   deepen existing abstractions over a feature that needs a parallel state model.
3. **Does it remove privileged setup or duplicated reasoning?** Prefer replacing capability-fixture injection,
   hidden reconstruction, or copied legality with an ordinary production route or canonical projection.
4. **Does it reduce future semantic cost?** Prefer work that turns a recurring wide search, copied derivation,
   ambiguous blocker, or broad diagnostic into a stable owner/edge/projection/proof reusable by later slices.
5. **How many existing investments become more useful?** Prefer edges that connect several current materials,
   machines, stores, structures, knowledge records, or recovery routes rather than one narrow endpoint.
6. **Is failure actionable and recoverable?** Prefer a slice with visible blockers and repair/resume/reroute
   semantics over one that terminates in an unexplained dead end.
7. **Can it be proved cheaply?** Among similarly valuable slices, prefer the one whose owner/boundary contract
   can be established with bounded deterministic evidence before broad gameplay evaluation.

This ordering is a decision aid, not a permanent roadmap score. Re-evaluate after each substantial slice because
closing one edge can change which later edge has the highest connective leverage.

## Default integration sequence

This is a dependency-oriented planning order, not a release promise. A smaller vertical slice may move earlier
when it closes a stronger loop with less machinery.

Agent-operability is not a separate roadmap phase. Apply the control-surface and semantic-entropy programs inside
each slice when concrete friction is exposed, so the repository becomes easier to extend as the graph grows.

### 1. Close existing control loops

Before opening major new domains, preferentially finish ordinary authorization around already-modeled physical
transitions and capability-only systems where a small missing edge is the blocker. High-value examples are
player-authorized construction/deconstruction or recovery steps, ordinary acquisition paths, and production
read surfaces that currently require controlled setup or specialized harness knowledge. The existing copper
progression also has a high-connectivity material gap between prepared ore and pure copper; a physically explicit
reduction/smelting route belongs here when its required heat, reductant, byproduct, and equipment ownership can
be modeled coherently through existing or deliberately introduced owners.

Completion criterion: the capability can move from controlled/capability-only evidence toward ordinary play
without adding an alternate semantic path.

### 2. Establish world-space action and logistics

Matter currently has strong local custody but limited general movement authority. Build the world-space action
substrate that can own placement, carrying/haulage, delivery, access, path cost, and transport time/energy/labor
without turning inventory into a universal movement authority.

This layer should connect geology, stockpiles, structures, production, maintenance, and later settlement labor.
It should reuse persistent spatial identity/bounds rather than introducing a parallel coordinate model.

Completion criterion: important material transitions can state not only what moves between owners, but how the
world authorizes and pays for that movement.

### 3. Add explicit physical networks

Once world-space movement and infrastructure placement have a coherent substrate, extend the same graph to
carrier networks: mechanical transmission, electrical distribution, and fluid transport. Network state should
own topology and losses; endpoint stores remain the authority for stored quantities.

Completion criterion: energy/fluid transfer is a physical routed operation with capacity, loss, occupancy,
failure, inspection, and recovery rather than generic store-to-store mutation.

### 4. Delegate through the same action model

Workers, animals, schedules, and automation should consume the same observable tasks, production legality, world
movement, and physical costs as direct player action. Delegation should change who supplies attention and how
work is organized, not create a second simulation path.

Completion criterion: a solved manual loop can be assigned, observed, interrupted, recovered, and audited while
preserving the same owner transitions as direct execution.

### 5. Deepen environmental feedback

Climate, hydrology, environmental heat, sanitation, ecology, and agriculture should enter after they have
owners and control surfaces capable of affecting existing survival, storage, structures, logistics, energy, and
production loops. Prefer environmental state that creates actionable forecasts and infrastructure responses over
ambient complexity with no practical lever.

Completion criterion: environmental variation changes decisions through explicit signals, flows, and recovery
actions rather than opaque periodic penalties.

### 6. Expand industrial transformation depth

After current progression gaps have ordinary physical routes, add alloying, forging, machining, broader
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
