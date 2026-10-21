# Gameplay Evaluation

**Role:** Automated-player information, evidence, scope, and replay authority.

Use [`TESTING.md`](TESTING.md) for test selection and completion,
[`STATUS.md`](STATUS.md) for reachability, and [`README.md`](README.md) for project routing.

`tests/gameplay_harness/` evaluates player-facing behavior through production APIs. Controlled setup may create
capability-only state; setup never establishes ordinary reachability.

## Evaluation map

| Question | Read |
| --- | --- |
| What information and production surfaces may the automated actor use? | [Actor contract](#actor-contract) |
| What can each evaluation mode legitimately establish? | [Evidence modes](#evidence-modes) |
| How should decisions, blockers, no-action, and diagnostics be explained? | [Decision evidence](#decision-evidence) |
| What does each maintained gameplay scope cover? | [Focused scopes](#focused-scopes) |
| How are matched branches, seeds, replay, and exploration constrained? | [Counterfactual and replay discipline](#counterfactual-and-replay-discipline) |

## Actor contract

After controlled setup, automated actors use the same production resolvers, validators, commits, and simulation
ticks as ordinary runtime behavior. They may read legitimate observable state, explicit actor policy, and
canonical production projections. They may not inspect hidden geology, future controlled events, setup
authorization, or comparison-branch outcomes.

`src/content/gameplay_fixture.rs` owns pre-admission fixture construction and opaque authorization for disclosed
controlled events. Pre-admission ends when the survival or logistics player is initialized. After that boundary,
all consequential changes use canonical runtime operations. Fixture authorization is never actor-visible evidence.
Ordinary exact-local episodes locate every disclosed pre-existing stationary endpoint before that boundary, then
atomically admit the player, establishing survival, world position, and carried custody before actor work begins.
The runtime's no-logistics-player locality fallback is valid only for controlled-capability fixtures or explicitly
movement-abstracted evidence; it cannot
support an exact-local ordinary-play claim. After admission, route-driving inherited equipment and finite energy
stores are reacquired through exact-local actor observation rather than fixture-retained identities. Shared
admission checks require actor-visible local equipment, energy stores, fluid stores, and ground stockpiles to agree
with the admitted exact-local runtime state; fixture identities may remain only as post-hoc continuity checks.

Actor policy may choose among observable alternatives using attention, material demand, survival reserve,
throughput, capacity, condition, and acquired evidence. Production owns legality and physics; actor code owns
candidate generation, ranking, search order, stopping rules, and policy ties. Shared harness helpers may implement
that evaluation policy but do not gain simulation authority.

Controlled setup owns scenario pressure, not copied game facts. Auxiliary fixture capacities that are not the
pressure under evaluation derive from the disclosed finite resource/workload envelope or authored definitions;
they must not become hidden blockers when content is retuned. Pre-existing equipment may represent disclosed
prior use by varying condition only before admission, within current authored maintenance bands, after its embodied
matter and location are established normally. Maintained witnesses may remain pristine; organic/replay cases should
vary inherited condition when that state materially affects the evaluated decision. Scenario generators may vary
authored opportunities by replayable seed, but equal physical opportunities are not resolved by registry identity
when that would change the generated world.

World feedback may also be negative. A paid search, survey, or reroute is allowed to reveal that a bounded area
contains no useful target; ordinary evaluation represents that as acquired evidence and a recoverable stop or
continued search, not a panic or a hidden guarantee that the next area is productive. Primary maintained
opportunities may remain deliberate starting conditions, but unexplored follow-up space does not owe the player a
resource merely because an order remains unfinished.

Choice-rich organic probes keep physical-world and actor-policy roots independent. Maintained witnesses use a
stable deterministic policy attached to the maintained case; coverage witnesses may intentionally exercise a
different bounded preference when the actor-choice boundary is part of the contract. Organic/replay cases derive
policy from the independent behavior root without changing world generation, authored rules, production
projections, or action legality. Reports expose the applied policy so a cold agent can distinguish world pressure
from actor preference.

When production exposes a player/UI planner for an ordinary choice, the harness uses that planner instead of
reconstructing its selection rules. Live manual-craft discovery comes from the production stockpile recipe catalog;
field prospecting uses the region-scoped production technique catalog to compare footprint, evidence quality,
projected attention/resource cost, and usable exact-local instruments; pre-inventory route discovery comes from the
production commodity handbook. Exact definitions and physics still come from their owning registries/resolvers.
Explicit lot selection remains valid only when the physical lot is itself a meaningful player choice. Repeated
stateful projections carry each production result into the next step; never multiply the first-step cost across a
lifecycle whose condition, reserves, occupancy, or other state can change.

### Decision-frame contract

Material choices use one bounded decision frame:

```text
observable state -> bounded candidates -> production resolution/blocker
                 -> actor policy -> selected action or explicit no-action
                 -> typed result -> later feedback
```

Candidate sets must state what their absence means. An exhaustive search over a declared observable domain may
support “no candidate in that domain now.” Budgeted, sampled, heuristic, or truncated search supports only the
bounded search result. Preserve the search bound or continuation in diagnostics.

Authored topology is legitimate actor input for declared transformations/providers/routes, but it establishes
possibility only. Current opportunity requires actor-visible state and canonical production resolution;
authorization still requires validation. Do not turn registry order, implementation identity, or hidden truth
into policy inputs.

Freeze an investment decision before running matched comparison branches. Later lifecycle cost and terminal
inventory assess that decision; they do not retroactively reselect it. Refresh current resource/condition
assessments before later service, fallback, or adaptation decisions.

Use owner-provided continuation data instead of reconstructing it. In particular, consume
`ProcessCompletion::landings()` / `ProcessParcelLanding`, `MiningOutputReceipt`, and admitted direct-consumption
completion ticks rather than rescanning destinations or rereading unrelated state to rediscover identities or
schedules.

### Adaptive search and freshness

Bounded search is valid when alternatives are genuinely distinct or no direct production envelope exists. Keep
it deterministic/replayable and retain the offered request, selected request, and typed production blocker.

Repeated probing that varies only one monotonic quantity is evidence for a production-owned feasible bound, not
for a harness-side formula. The actor still owns what to do with that bound: reduce scale, recharge, switch
provider, use a fallback, or abandon the goal.

Candidate frames and projections are disposable. Reuse them only while all authoritative dependencies they rely
on remain unchanged; otherwise reacquire the narrow production assessment/resolution. A cloned future branch or
remembered validation success never authorizes current state.

When inventory exposes multiple physical/freshness stacks for one food commodity, automated actors choose from
those projected stacks. They must not recover or rank private lot IDs; the production action resolves exact lot
slices only after the visible stack choice is made.

Ordinary grounded-construction evidence should likewise begin from an actor-visible carried stack and use the
atomic grounded-member action. Do not commit an inert plan first and then treat later material/work admission as
inevitable; the two-step owner API remains valid when blueprint planning itself is the behavior under evaluation,
but the familiar build gesture must fail without leaving an orphan planned member. When evaluating the direct
player shell, exercise the pointed top-face timber-post secondary action rather than bypassing its target/vacancy
checks with geometry supplied directly to the structural owner. Recovery evidence for that shell must likewise
exercise primary-break pointed dismantling with timed exact carried salvage rather than deleting the member through
a fixture or generic structural mutation.

### Temporal observation horizon

An actor may choose a bounded horizon from legitimate information such as a current schedule, policy deadline, or
fixed experiment duration. Multi-tick harness helpers are valid only when they execute canonical
`advance_tick` semantics and preserve ordered actor-visible outcomes that could change policy before the horizon.

A known completion tick is an upper bound, not guaranteed completion. Helpers must stop on declared
decision-relevant events or return intervening outcomes. Semantic fast-forward is outside the actor contract
unless production owns and proves the equivalent authoritative transition.

Passive elapsed time is temporal pressure, not player activity. A harness may advance ordinary ticks to expose
food aging, reserve drawdown, or another time-dependent state, but reports must label that interval as passive
exposure unless the actor actually took actions during it. Do not insert hidden maintenance, eating, drinking, or
other policy decisions merely to force an otherwise rested case into a warning state. Survival exploration keeps
rested cases on varied proactive provisioning horizons and uses separately authored warning-boundary starts for
urgent response, so routine preparation and emergency recovery remain distinct experiences.

## Evidence modes

| Mode | Surface | Supported conclusion |
| --- | --- | --- |
| Ordinary/runtime, controlled-local opening | focused `wilderness` | Fresh-player food/water stabilization and early investment restraint after admission, using a fixed authored body carry limit and ordinary exact-local gathering/consumption over disclosed finite local sources. Source generation remains controlled, but carried capacity and post-admission decisions are gameplay rather than fixture-sized. |
| Ordinary/runtime, exact-local | focused `survival`, `primitive-liberation`, `woodworking`, `power-provider`, `settlement`, `foundry-bootstrap` | Automated-player outcomes after disclosed starting-state setup, using ordinary production/acquisition operations and the same logistics-locality admission used by the runtime. This does not establish a missing world-source generation/acquisition edge. |
| Ordinary-system spatial proxy | focused `progression`; report `fieldwork` | The full scopes still contain campaigns whose movement is intentionally abstracted. Their spatial-proxy episodes omit a logistics player and support knowledge/tool/resource-loop conclusions only. Separate paired exact-local witnesses may validate bounded movement, custody, and locality slices against those proxy decisions; those witnesses do not promote the remaining abstract campaigns to exact-local. |
| Controlled capability | `workshop`, `ore`, `foundry` | Canonical mechanics under disclosed prearranged infrastructure, not ordinary reachability. |
| Counterfactual | matched branches | Action-attributable differences from one actor-visible starting state over one fixed comparison horizon. |
| Exploratory | `python ci.py report`, explicit replays/sweeps | Bounded discovery and diagnostics; exploration does not create a routine pass/fail requirement. |

Cross-episode continuity must say what actually persists. Reconstructing conserved carried assets, embodied matter,
and inherited condition at an episode boundary is modeled handoff evidence, not literal same-state continuity.
Reports name modeled handoffs explicitly so a cold reader can distinguish one continuous runtime history from an
adjacent episode exercising the next economic decision. Counterfactual branches likewise remain visibly marked as
counterfactual even when they execute canonical runtime operations and are useful for calibrating an actor decision.

Automation can establish mechanical consequences, production blockers, conservation, persistence, replay,
relative treatment effects, and behavior of the declared automated policy. It does not establish human
comprehension, enjoyment, subjective fairness, visual quality, likely human strategy, or frequencies beyond the
evaluated worlds and horizons.

## Decision evidence

A material automated decision records one coherent frame: world/variation seed, behavior
seed where applicable, tick, actor perspective, important legitimate observations, bounded candidate set,
representative production blockers, selected action or explicit no-action, policy rationale, typed result, and
important immediate/delayed consequences.

Diagnostic truth may explain a decision after the fact but must not feed back into that decision. Keep committed
runtime values distinct from projected next-decision values.

The concise fieldwork report must include a maintained, seed-bound player experience alongside
the distribution of outcomes: acquired reserve interval, intended order, selected tool, elapsed
search and tool preparation before first ore, realized output, and any multi-site response.
These are actor observations and executed consequences, not the evaluator's hidden deposit
diagnostic. One maintained episode demonstrates what occurred there, not how often it occurs.
A longer hand assay is replayed from that same actor-visible pre-investment state;
it must use the ordinary prospecting catalog and live work validator, charge the extra attention
and tool wear, and report whether the reserve interval actually narrows. Keep the comparison
branch separate from the original decision. When the narrower upper bound disqualifies the site,
the alternate actor searches another bounded site. That search may be barren; unexecuted
extraction never counts as recovered ore.
The concise report also carries a **separate** generated-world expedition that actually walks
between survey sites, encounters barren ground, mines an observed reserve, and returns its ore
through finite player carrying capacity. That expedition is not a continuation of the spatial-proxy
maintained campaign; its distinct identity avoids attributing simulated search and actual
transport costs to one fictitious actor.
Concise reports retain one maintained exact-local wilderness player's first provisions,
restraint, carried burden, remaining finite sources, and elapsed time. A separately executed
cold-response continuation shows how that restraint changes when comfort becomes scarce.
The compact maintained journey includes the later actual campfire intervention and a clearly
marked matched earlier-preparation alternative, rather than repeating the entire verbose event
or implying the early fire was built by the actual late-reacting player.
Neither witness is a population estimate or a guaranteed resource opportunity.
The generated two-food wilderness meal reports the live fruit and protein nutrition offered
by its actual carried-stack portions. Broad reports summarize those offers in percentages,
separately from caloric restoration; do not infer nutrition from food energy density.
For long exploratory output, a focused verbose seed selects one world's tagged events, not
necessarily a single actor history. Wilderness runs can reuse a seed under different goals and
physiological starts; an explicit sample/start/goal episode key is required to claim one witness.
Filtering may suppress untagged lines but must not reduce simulation coverage or silently
attribute another episode's events to the selected actor. When the same actor continues after
the initial opening, that continuation retains the original episode key so a focused replay
shows the new need, chosen intervention, and consequences alongside the initial decision.
The scavenger encounter likewise begins with an ordinary gathered and deliberately exposed
ration in one generated world. It reports only the actor-visible patrol warning, timed opportunity
to collect the drop, identity-preserving enemy relocation, and physical retrieval from the den.
Matched branches may choose to collect the exposed ration, approach without retrieving it to
force a scout retreat, or neglect it and later recover the actual displaced parcel. The actor may
not read a retreat event after the scout leaves local observation; the probe must physically
re-approach to find its new position and the parcel. A separate core continuation proves that a
nearby player makes a loaded scout abandon its parcel. The evidence distinguishes an avoidable
loss of local access from destroyed matter and does not represent deterrence as combat or pursuit.
Focused replays accept the seed/episode-tagged scavenger experience as a complete witness
independently of the opening-survival episodes. A second generated-world episode follows
one witnessed natural food opportunity: the player may protect it, or leave it unattended,
return to observe the changed finite quantity, find the recovered food through an adjacent
den projection, and pick it up. The probe must use normal player movement and local
resource observation; it must not read hidden enemy route or surface truth to choose a plan.
Successful food-foraging intervals are persisted, so repeated harvesting cannot deplete a
patch on every scout circuit. A third matched choice builds and fuels a real stone-ring
campfire using observed sources. The player can leave the burning fire unattended and
verify the guarded food remains unchanged, save/load the active fire, then return after
its finite light falls below the deterrence threshold and witness forage losses resume.
A player must travel back to inspect exact source mass; renderer-neutral illumination
is used as canonical behavior input rather than a scripted timer or free safe zone. A separate
longer generated-world den episode waits for the scout's 500 g loose cache to fill, verifies
that the unattended natural source retains its surplus, approaches the den through local
observations, recovers each finite parcel using normal control-time pickup, then leaves and
rechecks the source after scavenging resumes. Save/load and owner tests also establish the
carry limit, partial foraging into remaining capacity, and custody when the den fills during
an interrupted return trip. Another generated opening tests a route deviation: an actor
gathers a real ration, leaves it on the forage branch off the scout's initial patrol,
and physically withdraws off-trail. The scout takes one traversable detour toward the
exposed parcel, carries it one step at a time to the den, and the player later follows
local material observations to recover the same parcel. A subsequent observer-local
sighting proves the scout resumed its originally authored patrol destination instead of
becoming stuck on the decoy branch. Temporary detour state survives trusted save/load,
while a separate test proves player proximity still deters theft, missing or tampered
detour fields fail load, and oversized parcels cannot lure scouts into false detours.
An additional generated-world bulk episode collects a real 2 kg stone lot, leaves it at
the work junction, and compares carrying it securely with abandoning it in loose surface
custody. The scout takes two separate 250 g daughter parcels to its finite den cache
without moving the bulk source itself. The actor returns and reclaims both the 1.5 kg
original remainder and the 0.5 kg den loot through ordinary body movement and pickup;
remote mass is not inspected during absence. Owner-level replay verifies the split
transaction retains material profile, age, provenance, exact matter accounting and
save/load continuation. Enclosed storage, not simply a heavy exposed drop, remains
the credible defense against ongoing scavenging.

The camp-cache episode extends the same scout to one extra authored traversal edge.
A player gathers 250 g of food, leaves it exposed at the original camp, then walks
two ordinary body steps off-trail to avoid deterring the scout. The scout crosses
the intermediate forage cell, visibly arrives at the camp, and returns one real
parcel through that cell and the junction to its den. Only local sightings and
pickup disclose the outcome; a trusted save/load halfway through yields identical
steps and custody. In the matching guarded branch, standing on the intermediate
trail keeps the camp cache safe without directly picking it up. Owner tests cover
two-edge saved topology, restoration of the original patrol, invalid goal admission
and conserved source-local splitting of bulky remote caches. Further owner-level evidence follows actual two-edge travel to a natural fresh food patch, retains the newly generated recoverable harvest parcel on each intermediate return step, and replays deterministically across save/load. A patch expiring between discovery and arrival cancels the last edge without taking matter; a starving scout still hauls inert loot if there is no locally edible alternative. The full-den clearing regression prevents that fallback from overriding fresh food. Save schema 115 stores
optional route goals, the caution deadline, sparse resident meal scheduling,
missed-meal hunger, distinct locally witnessed machinery and physically delivered den-report times, the den watchman's ration-paid deployment, recuperation clock, locally witnessed pending warning, reported counterpicket order and real outward/return route leg, material-keyed consumed-food custody, twelve bounded scout footprints and eight separate watchman footprints, all tagged by resident role; older saves are not migrated. Owner tests contrast a genuine locally sighted mechanical store with ordinary unalerted foraging/scavenging: the same finite 750 g stone cache yields a 500 g rather than 250 g recoverable salvage daughter, while a 500 g wild source supplies 200 g rather than 100 g and a 50- rather than 100-tick next harvest window. Unseen distant electrical equipment or nearby thermal heat does not alert the scout. A machine witnessed at the junction must be reported by a real return to the den before larger batches apply; a player occupying that den entrance delays the report and mobilization. Trusted save/load replay during the pending report, forged report arrival and missing knowledge rejection are checked; there is no unlocked population or invented equipment. A new paired mobility regression gives both scouts nearby 750 g exposed stone and a real copper electrical winding two edges out. Only the physically reported, mobilized scavenger undertakes the longer winding raid; the unalerted scout chooses adjacent stone. The recovered copper keeps its surface identity and 120 g mass across every real return leg and a mid-raid trusted save/load, while world matter totals remain unchanged. In a player-interception branch, ordinary automatic pickup moves the winding into player stockpile custody and the mobilized scout falls back to stone. No secret machine theft or unreachable inventory probing is introduced. An industrial cache-exchange regression additionally fills the den with 500 g of stone and offers an actual 120 g copper winding at the adjacent branch. On its regular meal deadline the alerted scout moves the same stone parcel to the junction, ignores that ejected cargo while retrieving the manufactured winding, and brings the winding home. Trusted save/load between eviction and salvage gives identical custody and steps; ordinary unalerted dens, empty salvage opportunities and player-occupied exits keep their original cache. Every branch checks conserved world matter. Mobilization also coordinates an existing second den resident: owner regressions show a 20 g fresh meal deducted from real den custody before an industrial watchman occupies the adjacent route for twenty slow ticks, then returns on a bounded recovery clock without charging repeated meals. Unalerted colonies and incomplete rations retain the resident at home; nearby players block departure and can trap a deployed watchman outside by occupying its return edge. Nearby sightings distinguish the watchman from the scout only after its first true deployment; save replay and malformed deployment/cooldown rejection are verified. No new troops or materials are created by the report. The guard now actually occupies the junction, stopping a scout from entering that cell while watched and preventing guard deployment when the scout occupies it. Matched real-play regressions place an admitted player near the deployed guard but two voxels from the sheltered scout: the guard retreats one real edge, conveys its witnessed danger at the den, and the scout delays its patrol on its existing forty-tick caution deadline; without a player, the scout resumes when the normal watch ends. A player occupying the den instead traps the watchman outside with its report persistently pending, including across strict save/load; fabricated impossible pending reports are rejected. Local sighting exposes a threatened guard only within established occluded sight range. Additional paired watchman regressions verify that only a physically reported player intrusion authorizes a subsequent, separately rationed trip all the way to the outer authored patrol tip and back via every real intermediate edge; this costs a second complete 20 g den meal, and the watcher never shares a cell with the scout. Insufficient stored food postpones the wider patrol without granting free duty. Player control of the outer tip forces the guard home and retains the unused counterpicket order. Strict save/load across the occupied tip and crafted invalid route-leg or premature alarm-order fields are covered; all measured world matter totals are unchanged. Additional owner evidence observes the rationed watchman's actual den-to-junction walk and player-triggered fleeing return as two separate role-tagged local traces; its subsequent outer counterpicket leaves four canonical steps in the saved watch history. A distant observer sees none of the evidence; old tracks disappear from local sight by time without a world scan. Trusted-load tests reject wrong roles, false directions, fabricated activity, unearned early marks, overflow, or absent watch history. Stationary or player-blocked guards leave no false evidence, and save/load continues identically. Den-care regression additionally fills the real cache with 500 g of inert stone: at the scheduled missed meal, a nearby edible patch motivates one recoverable stone relocation, then real forage and a 20 g catch-up meal with exact matter conservation and save/load replay. No reachable food keeps the hoard intact; a player blocking the exit stops the action; a nearby unobstructed player can witness the relocation without receiving remote information.

In the matched live scout encounter, a nearby player can also see an actual retreat trace left at the work junction, pointing along the scout's physical escape edge. This spoor persists through trusted save/load and is distinct from a sighting: it does not reveal the remote scout's current position or cargo. Owner regression also covers carried-parcel scuffs, fading old tracks, obstruction of a blocked retreat, and forged trace rejection. Walking to guard the junction without pickup
makes the scout retreat and stay wary for forty simulation ticks. The player
then withdraws and can return by tick 215 to recover the still-unmoved ration;
waiting until tick 235 instead exposes the same parcel to renewed theft and den
recovery. A trusted save/load after the frightening encounter replays both
continuations. This makes guarding a time-limited choice rather than an instant
reset or permanent ward; caution is observable only through a nearby scout
sighting, and remote material mass is not read during the wait.

Generated-terrain enemy perception tests now distinguish distance from line of sight:
a scout two voxels away is visible across clear air, hidden when the intervening
junction cell becomes opaque stone, and revealed when that cell is cleared.
A separately positioned player observer also receives no scout-movement event
behind opaque stone although the physical scout moves normally. A diagonal
corner cannot disclose a scout around a single opaque intervening voxel. These
tests use actual generated block truth and strict local projections; they do not
invent enemy detection, remote telemetry, or a permanent terrain effect.

Generated-terrain owner encounters distinguish a blocked first step from an
outbound obstruction: the first makes a den-bound scout wait, while stone blocking
an outer patrol tip after the junction causes a visible, one-edge return to the
den. Unblocking the tip restores the original patrol without mutating its route or
creating phantom cargo. In a longer cache raid, obstructing the second destination
after the scout reaches its intermediate cell cancels its temporary detour and
preserves the original 250 g parcel; clearing the stone later permits its recovery
by the scout through the real return route. Both continuations survive trusted
save/load and conserve represented matter. The blocked cells are fixture terrain,
not a claim that the player has a terrain-block placement control.

A time-sensitive camp-cache branch now reclaims the actual 250 g dropped ration
through ordinary body controls after the scout starts its two-edge route. The
player withdraws before the next scout decision; on arrival at the intermediate
cell, the scout observes that the neighboring cache is empty and returns home
without creating a second parcel or proceeding to the abandoned goal. The
control-interval pickup is contrasted with full unattended theft and passive
trail guarding in one generated opening. Owner tests also demonstrate that a
den filled during a distant approach cancels the raid. The original patrol,
trusted save/load continuation, and exact matter accounting remain intact.

A separate generated-world continuation exhausts the junction's natural wild nuts
through ordinary player gathering and compares guarding the neighboring berry
source with withdrawing beyond the scout's deterrence range. Once the junction
source is gone, a scout takes a real one-edge detour to berries, extracts a
finite 100 g source-derived catch into an adjacent loose parcel on its return,
and transports that same lot to the den. The player inspects the exact berry
source mass locally and recovers the conserved den parcel using normal pickup.
Trusted save/load replays the in-flight catch, while owner tests verify profile,
provenance, age, 100-tick cooldown, and the last 50 g of den capacity.

A competing loose-loot encounter tests scout preference independently of creation
order: the actor gathers both edible berries and inert stone, drops stone first
and fruit second at the junction, and withdraws. The scout takes the later
food parcel first, leaving the earlier stone recoverable at its original
location. Staying on the neighboring trail instead guards both parcels.
Normal player pickup reclaims the stone and the same food parcel from the den;
save/load and matter accounting remain stable. Owner tests also establish that
a physically nearer inert parcel outranks distant food and that edible bulk
matter takes priority when neither heavy pile can be carried whole.

Close visible player presence deters a scout or watchman from its present clearing,
including diagonal adjacency. A generated opaque corner blocks that intimidation;
distance alone does not disclose the player through terrain. The den-cache experience
therefore physically withdraws from the guarded area along an actor-observed water/clay
route, searches a short off-graph route with canonical body-walk projections when terrain
obstructs direct retreat, and walks back to reclaim conserved food. Merely standing at
the authored spawn does not establish that a nearby source is unattended.

The scout also assesses one *proposed outbound step* for a directly visible
living player guarding the destination. If that clearing is threatened, it
withdraws along its real route before approaching, leaves fleeing spoor, drops
any actual carried parcel in place, and retains its finite caution deadline.
It does not collect exposed food or scrap while fleeing. An opaque corner must
conceal the player from the scout's **current** cell even when the proposed
destination would be adjacent: use both present line of sight and reachable
destination geometry, not hypothetical vision from the next step. Matched
owner continuations compare a guarded approach with actual unattended salvage,
save/replay of the caution window, and covered versus uncovered flanks.
The same current-cell observation is used when ranking nearby scavenging
alternatives: a scout avoids a visibly guarded preferred food cache and instead
travels to a safe reachable scrap cache, then revisits the food when the player
withdraws. Far destinations it cannot currently see remain uncertain and are
rechecked from the next real route step. This remains a surface-indexed
two-edge neighborhood search, not a world-wide threat map or full pathfinder.
Returning scouts apply the same sightline-and-proximity test before each
homeward edge, not only while approaching outward loot. A player visibly
guarding the return junction can halt a laden scout short of that clearing:
the frightened resident abandons its original loose parcel at its actual
position, without traversing the guarded edge. Once the player withdraws,
the scout can pick that same parcel up again and complete its real return
route. A matched opaque-corner/generated-terrain encounter verifies that an
unseen guard cannot preempt a return leg; the creature reacts only when it
can see the player from where it actually stands. Already frightened scouts
still prioritize immediate flight along a physical escape edge, dropping
carried material rather than freezing in proximity to danger. All checks
share one local visibility rule on the existing five-slow-tick cadence.
Bright terrain now guards homeward edges as well as outbound approaches:
when the returning scout can see a newly lit next cell but its current cell
remains below the deterrence threshold, it refuses to drag cargo into the
light and abandons that same recoverable parcel at its actual position.
A matched owner encounter uses real generated opaque terrain, an admitted
actor assembling a stone-ring campfire from 1 kg of staged stone, and
canonical combustion of a 500 g finite woodworking-chip fuel charge.
The ordinary surface parcel and scout tracks remain unchanged during the
blocked step, while an unlit comparison hauls the parcel normally. An alternate
admitted-player control continuation reclaims all 250 g through ordinary
local surface pickup while the light remains active, without changing the
world matter total. After the source's represented heat passively dissipates
below the junction's deterrence threshold, the scout reclaims the identified
parcel and completes the route with true hauling spoor; strict save/load and
matter totals agree.
Light checks do not suppress a scout's immediate escape from danger already
at its own cell. No virtual fire, new path search, or extra scout schedule.
Den capacity is reassessed on every actual return step. A scout already hauling
a recoverable parcel continues carrying it over the intermediate edges even
when another surface transaction fills the den in its absence. If the parcel
cannot fit on the final den edge, the creature returns empty-handed and leaves
that same identified parcel at the approach; player pickup can reclaim it.
With capacity available, a matched continuation brings the same parcel home.
The scout also declines to eject spoiled food or inert cache material toward
a living player visibly guarding the exit, instead of offering free loot through
an otherwise unsafe den-care step. These changes create no extra den storage,
new material, or additional resident update cadence.

A deployed watchman can recognize a living player at its two-voxel sighting
range without conflating mere observation with adjacent deterrence. The warning
stays with that resident until it physically returns to the den; only then
can the scout enter a wary period and the colony plan a later food-funded outer
watch. These checks run on the existing five-slow-tick resident cadence.
Outbound guard destinations also require a currently visible player before
anticipatory deterrence; a player hidden by an opaque corner cannot freeze an
otherwise provisioned outer picket. A matched generated-terrain watch encounter
checks both sides of that corner after the guard's real den warning and second
food-funded departure.
The same sight-bound rule now applies when a calm watchman tries to return to
its den. A player visibly guarding the entrance from beyond the guard's direct
fear radius can delay that actual return and the resident's pending report.
The warning remains with the watchman until the player withdraws and its
physical return leg completes; only then does the scout receive caution and
authorize a later rationed counterpatrol. An unguarded matched watchman returns
on schedule, and the delayed one spends no extra food while waiting. Immediate
flight from a threat at the current post still takes priority over anticipation.
The same emergency distinction now applies to finite local firelight: a
watchman already exposed at its own post can flee one real edge home even if
the den approach is also bright, instead of remaining stranded by the
destination-light check. A matched generated-stone encounter uses a physically
reported electrical store, a real 20 g deployment ration, assembled campfires,
and canonically combusted timber or woodworking chips. Full-strength light at
the junction provokes a fleeing footprint and safe return through a lit den;
a weaker chip fire at the den lights only the destination, so the calm sentry
waits at its real junction post. If a player occupies the den instead, even a
frightened sentry cannot pass through that body: its local warning remains
undelivered. All three branches conserve represented matter, spend no
unearned second ration, and validate/replay through trusted save/load.
No new per-control-tick sensing, omniscient colony state, or detached raid
timer is introduced by these reactions.

The compact settlement-workshop experience distinguishes one executed shared-power
worksite from separately evaluated machine-investment projects. It reports both human
charging effort and delegated machine runtime, but counts only *executed parallel
player work* as demonstrated reuse of freed attention. Delegated runtime without an
observed secondary action is explicitly unobserved opportunity, not productive work.

Broad cross-domain summaries identify the player role and evidence modes represented; they are
not fantasy-achievement scores. Counts of scripted goals, successful fixtures, or passed
contracts cannot establish whether an individual experienced an interesting tradeoff. Inspect
seed-bound causal journeys and competing production outcomes before making gameplay claims.

Keep hidden fixture truth in a separate diagnostic representation from actor-playable state. Player-facing
decision and experience evidence contains only legitimately observable state, acquired evidence, and explicit
policy. Exact hidden reserve, grade, hardness, generated role identity, or other fixture truth may be retained for
post-action evaluator checks, but belongs only in explicitly diagnostic output marked as non-policy input.

Decision diagnostics preserve the control coordinate of the choice: owning authority/contract,
authoritative owner or crossed edge, operation stage reached, relevant flow/currency, and evidence mode. This
lets failures route back to production semantics instead of becoming actor-specific archaeology.

When nothing happens, classify it rather than collapsing all absence into failure:

| Classification | Meaning |
| --- | --- |
| Unobserved | Enabling state did not occur in the evaluated horizon. |
| Generator gap | Actionable state existed but candidate generation missed it. |
| Policy gate | A viable candidate existed but explicit actor policy rejected it. |
| Validation gate | A concrete candidate reached production validation and was rejected. |
| Information gap | Relevant authoritative truth existed but was not legitimately observable. |
| Execution failure | Accepted operation failed to produce its contracted result. |
| Inconsequential | Operation succeeded without a material consequence for the evaluated claim. |
| Dormant | No detected opportunity existed and none was expected. |
| Insufficient data | Seeds, horizon, or search bound cannot support a stronger conclusion. |

Bounded search bounds evidence, not production legality. An unsampled candidate is unverified, not unavailable;
one rejected candidate does not prove an entire action family unavailable unless the production rule or an
exhaustive check establishes that conclusion.

## Focused scopes

[`TESTING.md`](TESTING.md) owns Cargo target layout and command selection. This section owns what each gameplay
scope may establish.

| Scope | Contract |
| --- | --- |
| `wilderness` | Production fresh-world generation supplies a deterministic six-edge opening opportunity graph with seed-varied finite forage and water quantities plus a finite fallen-timber reserve whose minimum exceeds the unaided body-carrying envelope. Their minimum food and water opportunities each exceed one resting day of authored basal need, giving the opening enough runway for non-subsistence decisions without claiming equivalent active-work duration. The maintained mobile actor observes the exact forage opportunity only after reaching it, gathers a 500 g portable ration rather than stripping the patch, skips an unnecessary water branch without learning its exact volume, and reaches a material junction through two lived continuous-body legs; each executed leg starts from a settled slow boundary, uses the production walk projection, then must match its projected control ticks and exact next-boundary energy/hydration charge before there stone and timber become simultaneous adjacent choices rather than scripted checkpoints. The same rested, initially goal-free mobile state also continues under canonical physical time until an actor-visible chilly warning changes what matters. The production-owned conditional unheated-comfort runway forecasts warning onset for the observed open exposure; roof or heat changes invalidate that assumption. Matched reactive and forecast-led actors begin in that same unbuilt camp. Both gather local stone and timber and assemble/fuel a finite campfire through ordinary body movement, gathering, selected secondary use, and control-boundary settlement. The forecast-led policy budgets the visible four-leg route, authored gathering times, and direct-use settlement before reserving a one-tick contingency; it schedules work early enough to prevent chilly exposure. The reactive player waits for the observed warning and retains more fuel runway, but ends colder. A third no-intervention branch compares actual warmth at the same physical endpoint. The short opening snapshot and later adaptation are separately labeled in detailed and concise reports, with no evaluator-supplied construction goal and no hidden future-duration oracle. Maintained coverage supplies four disclosed **camp goals** independently of world generation and hidden world quantities: stay mobile and defer both materials when no workload exists; establish a worksite cache at the material junction by hauling field-box construction timber back from its leaf, building the box at camp, retaining its material-backed woodworking residue with provisions, then making a second canonical timber trip for a low grounded cache support plus 6 kg of staged reserve; from the timber leaf the actor hand-sets that support into the adjacent junction, returns with the remaining 6 kg, mounts the existing field box onto the active member, and fills it so stored matter becomes a real structural load; extend that same worksite state into a long stay by consuming cached timber first, gathering only the shelter shortfall, earning a stone adze from the actual roof workload, and building overhead refuge; or protect a longer food reserve whose eight-day target exceeds the field box's seven-day berry horizon and therefore justifies hauling stone back to camp for the stone crock's ten-day horizon. The goal is evaluator policy, not claimed emergent strategy; the resulting route, load, source depletion, storage choice, provisioning, and retained world reserve are lived outcomes. First-session collection rates are tuned for a short, purposeful ration or material haul rather than extended exclusive waiting; the opening review uses duration ceilings, not minimum waiting quotas. In verbose mode a compact causal player journey complements the underlying ownership and accounting ledger. Selected storage is actually fabricated, placed at the junction, and filled through ordinary runtime paths; the worksite branch additionally uses ordinary grounded structural construction and stockpile mounting, so capacity, preservation, camp centralization, retained fabrication residue, and structural support are physical consequences rather than catalog comparisons. The same long worksite project creates an ordinary warmth problem: unsheltered exposure lowers the persisted thermal-comfort reserve into the chilly band. The actor budgets the known fire response before committing to that project, makes a canonical round trip to gather the campfire's exact local stone input, assembles a stone-ring campfire at the junction, burns one authored 1 kg wood charge from the staged timber reserve, and remains co-located while the fire's finite passive thermal loss raises comfort back above the authored warning threshold. Burned fuel enters an explicit terminal matter account, the thermal store retains only finite remaining heat, and a remote burning fire does not warm the player. The report must distinguish the production-owned single-charge passive-loss runway, the much shorter comfort-recovery interval, and the remaining heat and burn runway at the end of the actual project; a 1 kg charge must provide at least one authored physical hour so the fire functions as sustained camp infrastructure rather than a recovery pulse. Hunger and hydration pressure are crossed with project work in maintained coverage: the actor prices canonical gathering, crafting, load-sensitive camp travel, the fire stone trip/recovery window when selected, and the remaining route before deciding its working reserves. Lived camp travel now executes through generated-terrain 20 Hz body motion; future legs whose exact source/body context has not yet been reached stay deliberately labeled as conservative project upper bounds rather than being misreported as exact path projections. Optional-water routing is derived from observed hydration plus that disclosed project budget rather than from the evaluator's start-profile label, and food is provisioned before work when the same projection would cross the hunger warning; needless water/food remains deferrable. The maintained policy actor therefore uses canonical continuous-body locomotion for its actual forage, optional-water, junction, material-haul, cache, and campfire logistics without a discrete movement-work fallback. A separate maintained direct-control witness additionally traverses all five unique opening edges through canonical 20 Hz continuous-body locomotion, visits both material leaves after the forage/water branch, settles at each interaction voxel, and issues one-shot empty-hand secondary use for forage, water, and the first timber batch. Before leaving the already-observed water detour, the actor projects its known two-edge return through forage to the work-path junction with the production-owned continuous walk projection; lived control execution must match the projected control ticks exactly. It deliberately tops that timber parcel to 5 kg at the legal slow boundary and carries 5.5 kg total back to the junction through the same continuous body controller. If slow work cannot admit on the sampled control tick, the request survives through neutral continuation and re-resolves once at the next legal boundary; the report expresses those control-tick admission delays in seconds alongside finite gathering, finite water withdrawal, hydration change, carried load/capacity, and timed-work proof. This witness does not replace the policy actor's smaller deliberate forage or project-aware hydration choices. A single field box remains below the early adze's five-board-batch attention crossover, preserving rational tool restraint even in the worksite-cache branch. The maintained **long-stay shelter** coverage continues that same field-box-and-fire actor state, uses the remaining 5 kg of staged logs before gathering its 14.75 kg fresh-timber shortfall plus 1 kg of stone, and derives its tool decision from the actual roof geometry rather than a synthetic workload. Three 3.25 kg timber members are hand-set into a real support chain; the 6.5 kg 1 m² roof requires nine board batches, so acquiring the stone adze saves 106 ticks of combined tool-acquisition and board-shaping attention relative to bare work. The selected boards are then placed by ordinary secondary use. Planned roofing gives no benefit and activation changes overhead assessment. When the roof is complete, the actor discloses another hour at camp and compares a cautious usable fire horizon with the production-owned covered, unheated comfort runway. The roof can preserve comfort without another fuel charge for that first hour. As the fire expires and the next hour becomes relevant, the actor checks its actual carried woodworking chips before deciding whether to harvest new timber. A real 500 g chip charge recovered from roof fabrication yields 5 MJ of finite fire heat through selected direct use, saving the two-leg timber trip and leaving another kilogram in the finite resource source; if sufficient chips are not carried, the older real timber haul remains the fallback. The journey reports the chip reuse, avoided hauling, source preservation, and finite shorter heat runway rather than treating all fuels as equivalent. Matched first-hour branches contrast the completed roof without extra fuel, no roof without extra fuel, and no roof with an additional actual kilogram of staged timber. The fuel-only route provides greater immediate warmth at lower construction cost; roofing buys a fuel-free refuge interval, not a universal short-job advantage. Later matched refueled and unfueled covered continuations prove that shelter does not eliminate fuel needs. All comparisons retain canonical custody and same-time endpoints; outcomes do not retroactively decide the actor's choice. An independent clone of the completed roof state observes the authored exposure loss fall from 1000 to 300 ppm after finite fire warmth is exhausted while matter remains conserved. Exploratory worlds vary generation seed, physiology pressure, and the three lighter camp goals independently; long-stay shelter remains one maintained heavy report case, with two further generated seeds exercised by its focused contract. Evidence proves need-sensitive routing, purposeful material choice, finite runway, selective harvesting, local source opportunity cost, continuous-body camp logistics, finite material-backed fire/warmth, a same-state food/cache/fire/tool/shelter opening, survival/project budgeting, workload-sensitive restraint and investment, one-shot direct-use reliability, exact costing of executed and already-known bounded continuous paths, a finite loaded continuous-body timber return, and continuous-body reachability across the complete bounded six-edge opening topology. It does not force the optional water branch into the rested long-stay state and therefore does not prove one single-state sequence containing every need branch; nor does it prove general pathfinding/route generation, hydrology, walls/enclosure, weather, or climate. |
| `survival` | Hunger, thirst, recovery, preservation investment, storage recovery, and work/provisioning interaction through ordinary runtime paths. Rested worlds exercise varied proactive provisioning after bounded passive exposure; warning-boundary worlds own urgent hunger/thirst response rather than forcing every rested case into crisis. The provisioning world may inherit an already-owned preservation enclosure and exercises its lived food-rotation consequence. Organic diet policy uses the actor-visible coarse recovery signal plus currently available dietary categories: it pays the extra meal mass for balanced recovery only when visible recovery has materially fallen and the missing variety is actually available, rather than assigning diet preference from behavior entropy. The matched diet-recovery counterfactual still executes both policies to measure smaller exact consequences; it creates a real vitality deficit through canonical exposure, then establishes a material-backed stone-ring campfire through ordinary assembly and finite combustion, warms above the authored recovery threshold, and keeps that finite heat source fueled while matched compact and balanced meals recover for the same horizon. Reports expose warmup, fuel/refuels, minimum thermal comfort, diet quality, and realized vitality change so cold stress cannot silently masquerade as nutrition evidence. Preservation build-or-decline economics are evaluated in a separate matched episode with the same protected-reserve scale, a disclosed finite raw-material opportunity, and a seed-varied disclosed reserve-use horizon that can fall shortly before or after ambient spoilage. Investment value is the enclosure's incremental edible lifetime over the no-build outcome at that same horizon, so food that would still be usable without construction is not credited to preservation. Reports must label the continuity boundary explicitly so the selected enclosure is never misread as replacing or downgrading the inherited enclosure in one continuous state. |
| `progression` | Post-wilderness primitive copper progression: evidence-gated mining, workload-sensitive manual-versus-mechanized processing, maintenance, and reinvestment from a disclosed prepared primitive context. The maintained long order selects mechanization only after the same actor-visible decision state proves short work still favors the manual route and reports the first player-attention crossover plus the real embodied processing-line capital. Organic and explicit-replay cases additionally re-execute the generated scarce-copper campaign through first mechanized convergence in a bounded exact-local corridor using the same four geological clues, reserve/grade pressures, direct-copper scarcity, and prepared raw opportunity as the long campaign. The actor must physically enter each regional/clue region before prospecting, choose regional/surface/sample techniques through the same player-facing region catalog instead of evaluator method identity, fabricate its stone pick and geological hammer after admission, rediscover and carry them, use that hammer only when it is a currently usable local catalog instrument, mine samples into carried custody, haul them back to the workshop, and make the same evidence-driven pick-first/crank-first decision. The chosen first investment then continues physically: the actor builds the crank/flywheel/crusher/separator line at the local workshop, charges it canonically, starts the first crush from actually hauled feed, and uses part of that autonomous processing window for a real field trip that travels to known bulk ore, mines the next feed into carried custody, and returns it to the workshop. Powered separation must recover the same exact second-reinforcement copper parcel selected by the matched spatial-proxy campaign, after which the second reinforcement executes with ordinary custody rules. Any mass-changing pick reinforcement requires drop, local modification, rediscovery, and repickup. Both sequencing branches must end with the reinforced pick and reinforced crank plus a physically returned hard-seam sample. The same seed must match the spatial-proxy campaign on regional evidence, detailed clue evidence, owned bulk assay, direct-copper shortage, information-refinement outcome, first investment decision, selected processing feed, recovered second-upgrade copper, and convergence. Sustained stockpiling, later maintenance/service, and reinvestment remain spatial-proxy, so the whole progression scope is not exact-local. It does not establish fresh-spawn pacing or ordinary first-day acquisition; opening food/water/fire evidence belongs to the wilderness scope, while shelter/weather remain absent. |
| `liberation` | Material-backed carryover of the progression processing line and a workload-dependent choice between low-infrastructure manual ore recovery and acquiring the incremental quern/riddle extension from the ordinary generated wilderness graph. The build projection prices the minimum material-leaf route, exact-local gathering, and fabrication before the actor freezes its choice; optional water is not charged when current physiology does not require it. A selected build now composes the same terrain-aware opening used by fresh wilderness play, places the inherited workshop at that generated surface spawn, traverses every chosen graph edge through canonical 20 Hz continuous-body locomotion while preserving the measured one-slow-interval-per-adjacent-leg acquisition-attention contract, gathers stone/timber into carried custody, returns loaded to the inherited workshop, and executes matched recovery without retroactive reselection. The processing line remains a disclosed separate-episode handoff from spatial-proxy progression. |
| `settlement` | A lived lumber episode freezes its first frame-saw/sawmill investment against disclosed demand, but the powered upgrade identity is no longer evaluator-supplied: after admission the actor discovers the exact-local frame-saw upgrade through `player_equipment_upgrade_options`, then discovers the compatible powered board process from the authored powered-variant/topology surface before any upgrade parts are fabricated. The same observed row is later revalidated at actual upgrade time, so discovery is not an authorization shortcut. The episode then reveals a later order in the same worn state so owned machinery can be reused or later reinvestment can become rational. Maintained witnesses use matched repeat demand for stable crossover regression, while organic follow-up demand is independently generated from the remaining disclosed lumber opportunity using world entropy only; it must not read the first investment result, current wear, live crossover, or actor policy. This lets the same workshop face shrinking or growing demand after its first decision and makes wear-driven capacity loss distinguishable from ordinary attention payback. Copper working has an explicit capital ladder: direct hand work can remain best for a short job, repeated work can justify a treadle hammer that improves player-active throughput without delegation, and later helve-hammer investment can convert that earned workshop capability into powered delegated work. Separate executed crossover witnesses cover spindle drill, wire drawbench, helve hammer, and other machine families; the report labels those separately so they do not masquerade as one continuous settlement. A separate single-state five-family workshop portfolio proves that delegated work is usable agency rather than idle fast-forward: while a powered spindle-drill job continues on the shared finite flywheel bank, the player canonically dresses stone-chip stock into the drill's real replacement bit, and the report records the fully overlapped player-attention window before the machine job completes. |
| `foundry-bootstrap` | First-foundry build/defer choice after inherited settlement workshop and powered ore-dressing capability plus already-owned assayed ore from the same primitive copper-progression mining/assay vocabulary. Pre-foundry ore recovery reuses the copper-reinforced hand crank earned by primitive progression; the electrical prime mover is not inherited. A selected foundry bootstrap must fabricate the timber treadle base and convert it to the treadle dynamo before electrical melting, so the electrical transition is paid and observable. The episode also covers bounded recovery of a visible copper shortfall through the inherited dressing line when executable, casting, thermal recovery, and cast-stock reinvestment proved by a real settlement-size cast when owned supply permits it. From the same actual 80 g cast, the player then manufactures a 54 g replacement saw blade in the chosen continuation, while a separate executed matched branch forms a 54 g electrical winding. Each consumes 60 g of ingot stock, retains 20 g unused, and returns 6 g recoverable scrap; the harness confirms that these forms are demanded by existing woodworking and electrical machinery. The third cast-stock option pierces an 18 g screen plate using a real player-owned pump drill in the settlement contract. In the winding counterfactual the same actor upgrades the empty copper-plate buffer through its additive wound stage and charges it for two primitive melts with finite treadle labor. Identity, consumed winding, remaining copper and conservation stay visible; one coil cannot complete the two-coil dynamo. In the controlled settlement-foundry contract, an actual 80 g cast can alternatively be finished by a physically assembled powered helve hammer and spindle drill, using a separately assembled treadle and finite flywheel. Matched branches produce a 54 g saw blade or winding plus an 18 g pierced plate and 8 g represented scrap; they require charged mechanical work and leave the player free during delegated jobs. The actor finds each powered alternative from its known hand recipe and exact-local machines, observes missing flywheel charge, performs human charging, and submits the refreshed option through canonical selection/admission without explicit lot identities. Stale or uncharged rows cannot start work. A third alternative now reuses the actual first-foundry actor state and only the pre-existing treadle hammer and frame saw, plus a finite fallen-timber source disclosed in the local world before admission. After a real 80 g cast, the actor gathers 2 kg of timber, saws 1.8 kg of boards, and cuts 400 g of handles from a 500 g board blank, retaining 100 g chips. The helve's 800 g frame leaves 500 g of dressed boards for later work. The actor converts 20 g cast copper into the bearing, upgrades the inherited hammer, charges its existing flywheel, and delegates the remaining 60 g of copper to produce a 54 g winding. Installing that winding upgrades the original electrical buffer from 15 to 30 kJ. It proves the powered-hammer and electrical-storage reinvestment through real custody, time, wear and conservation inside the same exact-local continuation. The matched report compares actual capital-plus-first-job time against direct winding time without assuming future tuning must preserve the same payback order. The current route favors direct work for one order, while the upgraded station is available for further jobs. The original saw and buffer alternatives remain matched forks, not cumulative player rewards. This does not establish fresh-world acquisition of the entire inherited settlement workshop, later spindle drilling, or industrial power distribution. |
| report `woodworking` | Bare-hand/adze/frame-saw investment, wear, maintenance, and attention/material payback. Organic copper opportunity is inherited from primitive copper-work scale independently of saw affordability or protected-reserve policy; maintained witnesses own named affordability regimes. |
| report `fieldwork` | Sampling, bounded search, depletion, retooling, salvage, survey investment, and evidence-driven extraction adaptation. Survey actions are chosen through the player-facing region-scoped prospecting catalog rather than by evaluator-supplied method identity: exact-local episodes require the selected instrument to be a currently usable local instance, while explicitly movement-abstracted proxy witnesses may match only the disclosed authored instrument definition. Point inspection is intentionally quicker but coarser than a broader local transect; repeated multi-site work can instead repay copper reinforcement for indexed channel sampling, and maintained coverage requires the same funded actor to prefer point search for one future site but indexed search for a five-site campaign. Organic reports vary demand horizon, geological reserve scale, and hardness on independent replayable channels; exact reserve, hardness, grade, location, follow-up geology, and policy entropy remain fresh, hidden geology reaches the actor only through acquired evidence, and tool investment is an emergent outcome rather than a generated world class. A maintained exact-local expedition places its camp, barren survey site, and target on deterministic generated terrain and traverses every survey and haul leg through canonical 20 Hz continuous-body locomotion, while preserving carried tools, survey evidence, mining into carried custody, repeated camp hauling, and load-sensitive exertion. Its disclosed 30 kg order is independent of the authored body-carrying envelope: carrying capacity constrains trip size and therefore produces one full and one lighter return trip rather than defining the extraction goal. Organic and explicit-replay cases additionally re-execute their independently generated demand, reserve, hardness, grade, and raw-material opportunity in a bounded exact-local camp/barren/target episode embedded in a deterministic generated-terrain corridor. After admission the actor fabricates the sampling hammer and selected mining tool from local stock, rediscovers those tools through local observation, carries them through canonical 20 Hz continuous-body movement, surveys only while physically present, mines into carried custody, and returns ore to the camp cache through real haul trips. The same seed must produce the same acquired evidence band, extraction-tool choice, extracted mass, and supply-stop result in this bounded exact-local episode and the richer spatial-proxy episode. Maintained adaptive coverage also executes one depletion-driven secondary reroute on the same generated-terrain continuous-body corridor: a repeat order physically exhausts the known site, the actor walks candidate sites while performing the same transects and point inspections as the proxy search, returns to camp for any evidence-driven salvage/retooling, then travels back to the localized site, mines into carried custody, and hauls the recovery home. That witness must match the proxy on depleted-repeat output/stop, secondary evidence, prospecting work, tool/retool/salvage decision, and geological extraction result; body-scale carrying may split mining across more admissions and therefore change elapsed extraction ticks. Broad multi-site survey campaigns, arbitrary subsequent-site exploration, and general search travel remain spatial-proxy, so the whole fieldwork scope remains labeled spatial-proxy. |
| report `power-provider` | Human-power provider and accumulator investment across primitive and later-settlement workloads with full lifecycle costs. The settlement arm reuses the same disclosed lumber-demand generator as lived settlement play but intentionally evaluates a later scaled workshop behind the timber flywheel bank, not the first sawmill's stone flywheel; reports label that stage explicitly so its provider economics are not mistaken for the lived first-settlement cadence. Provider-frontier search is diagnostic only and never sizes organic demand. Primitive play charges only to the next planned consumer batch's required stored work, including passive loss during charging, rather than wasting attention filling a lossy buffer to capacity. |
| `workshop` | Installed industrial operation under finite work, survival, wear, maintenance, structural pressure, and recovery. Capability-only. |
| `ore` | Installed crush/grind/screen/regrind/concentrate flow with exact constituent accounting and terminal tailings. Capability-only benchmark. |
| `foundry` | Installed industrial pure-copper heating/melting/casting with finite energy, adaptive batches, remelting, and sink recovery. Capability-only benchmark. |

Routine focused gameplay gates execute maintained witnesses plus one fresh replayable organic case. Broad gameplay
audits keep all maintained witnesses but spend the same organic budget on one replay-stable rotating scope rather
than multiplying variation across every probe. Exact focused-probe execution uses the same maintained witnesses plus
one fresh replayable organic case, so repeated cold-agent repairs exercise slightly different playable pressure
without widening the Cargo target or rebuilding unrelated code. Exact owner/unit/contract tests remain deterministic.
Reports own the broader bounded organic sample and agency qualification searches. Maintained cases prove contracts, not prevalence; organic
samples are bounded evidence, not population estimates. A named qualitative regime that must never regress belongs
in maintained coverage. Exploratory organic cases probe generator, policy, and integration drift between those
anchors without turning qualitative coverage into a probabilistic requirement. Organic world generation must not
read the live decision threshold, selected route, comparison winner, or market frontier merely to manufacture a
balanced branch split. Generate plausible actor-visible demand and independent physical pressure first, then let
current production economics and actor policy produce the decision. Production definitions may still size real
physical envelopes such as equipment capacity, authored storage limits, or finite opportunity bounds; they are not
permission to condition the sampled world on a desired decision result. Fixed numeric pressure is appropriate only
when the number itself is the disclosed world condition or player goal, not when it is standing in for an authored
requirement. Bounded organic reports may stratify a few low world-seed bits across independent physical pressure
classes that are either visible initially or legitimately discoverable during the episode. Stratification controls
the sampled world class, never the resulting route or actor knowledge: hidden values still reach policy only
through their ordinary evidence path. The remaining seed entropy stays fresh, and actor-policy roots remain
independent from those world strata. Supplying roots to an exact focused probe adds and reproduces that sampled
case instead of drawing a new one.
Generator contracts assert variation and independent pressure coverage over an appropriately broad direct sample;
they do not require every tiny report sample to hit both sides of an investment frontier. Route/outcome guarantees
belong to maintained witnesses. Likewise, later executed feedback is not projection calibration unless the
pre-action model actually predicted that quantity; reports label policy thresholds and hindsight outcomes
separately rather than calling their agreement "exact."
An organic seed must materially affect actor-visible world pressure, actor policy, or both; changing only a replay
label is not variation. For bounded generators whose purpose depends on spanning distinct pressures or decision
regimes, keep a cheap direct generator contract that samples enough seeds to detect collapse without replacing the
end-to-end focused episode.
Partial or blocked outcomes must preserve trusted-load validity and relevant conservation. Selected-path summaries
must not count counterfactual, coverage-only, or negative-control branches as player experience.

### Coverage contracts

Harness modules own execution detail. These are the evidence obligations:

- **Liberation frontier:** prove that the low-infrastructure manual route remains rational for short disclosed work while longer work can justify acquiring the incremental quern/riddle extension. Freeze the build/manual choice before either matched branch executes, using production-backed projections rather than hindsight. Price the ordinary generated opening graph's minimum material-leaf round trip, exact-local stone/timber gathering, and fabrication in that choice; do not charge the optional water branch when current physiology does not require it. A selected build must discover and traverse the graph through canonical movement, gather into finite carried custody, return to the inherited workshop, and conserve matter; counterfactual kit execution may calibrate the same route but must remain labeled as such. The inherited processing line remains a disclosed separate-episode handoff from spatial-proxy progression, not evidence of one continuous cross-era `AppState`. Label any first-foundry starting boundary explicitly and conserve matter and energy across selected foundry work.
- **Initial wilderness edge:** production world creation must generate a bounded six-edge horizontal opportunity graph from an explicit seed before admission: forage one opening-graph edge from spawn; at forage, finite drinkable water is one adjacent optional branch and a second edge continues to a material junction; at that junction stone and timber are simultaneous adjacent leaves in seed-selected assignment. Forage mass and water volume vary deterministically by seed, and their minimum generated quantities each exceed one resting day of authored basal need. The generated timber opportunity must remain finite and seed-varied but its minimum must exceed the authored unaided body-carrying envelope, making it a known local reserve rather than a one-trip pocket inventory. Generation commits surface custody, fluid custody, water location, and all normalized traversal edges only after owner snapshots precheck, and fresh generation rejects any already-populated logistics world rather than layering the opening over preexisting custody. After admission the actor must begin with no exact-local opening source, orient only through traversal-gated adjacent surface/drink observation, observe only current-voxel opening-graph choices, and prove an unlisted geometric neighbor remains hidden from that bounded observation surface. Player carry capacity is authored physiology and must not be resized to the plan. The maintained mobile actor learns exact forage mass locally, gathers a 500 g ration, leaves the finite surplus in world custody, skips unnecessary water without learning its exact volume, reaches the material junction in two lived continuous-body legs, and defers both visible raw-material leaves when no current workload justifies collection. A hydration-pressure mobile actor must instead visit the water branch, learn exact-local volume, drink canonically to a working target that includes the disclosed remaining route/project hydration cost, return, and reach the junction; the route decision itself must come from actor-visible reserves and the project projection, not the setup-profile enum. Maintained camp coverage must execute both primitive storage goals from the same actor-visible junction rather than merely compare definitions. A disclosed worksite-cache goal sends the actor to the timber leaf for field-box construction material, returns that load to the junction, fabricates and places the box there, and retains the fabrication residue and provisions instead of carrying or deleting the byproduct. The actor then returns to the same timber source for enough material to hand-set a low grounded cache support into the face-adjacent junction plus 6 kg of staged reserve, constructs that member through ordinary player structural work, returns carrying the remaining 6 kg, mounts the field box to the active support, and fills it. The mounted cache must create nonzero structure-owned stored-matter load, and the report must expose support mass/work, retained residue, camp logistics steps, and selected-source opportunity versus remainder so the worksite is evidenced as material-backed leverage rather than a policy label. The long worksite path must also prove the opening fire/warmth loop. Its ordinary elapsed work must drive the persisted thermal-comfort reserve below the authored warning without relying on a cold-start fixture; before committing, the actor budgets the campfire stone round trip and a bounded warmth-recovery window alongside the storage project. After the cache is loaded, the actor gathers the campfire's exact local stone input, returns to the junction, assembles the stone-ring hearth, consumes one authored 1 kg wood charge from the staged timber through the canonical combustion action, and remains beside the fire until passive thermal loss restores comfort above the warning threshold. Fuel mass must move into the explicit combustion terminal account, finite heat must remain in the store after recovery, and owner-level evidence must prove the same burning fire does not warm a remote player. This is fire/warmth coverage, not evidence of walls, enclosure, weather protection, ambient climate, or general environmental heat transport. A disclosed preservation goal gathers stone, returns it to the junction, and fabricates the carved stone crock because its eight-day food horizon exceeds the field box's seven-day berry horizon while fitting the crock's ten-day horizon. The actor must price canonical gathering/crafting exertion and load-sensitive project travel before either project and provision first when the projected budget would cross a survival warning. The first timber box must remain capable of deferring the adze while its two board batches are below the production-backed five-batch crossover. A separate long-stay fresh-world witness must derive the roof workload from authored geometry, acquire its own stone and timber, build every roof-height support through ordinary timed structural work, acquire the adze only when that real board workload repays it, place selected boards through ordinary direct control, prove Planned roofing gives no benefit, and observe the completed roof reduce canonical exposure loss while conserving matter. Exploratory wilderness cases independently vary generated world seed, physiology pressure, and camp goal; the goal is explicit evaluator policy and must not read hidden world quantities, the live decision threshold, or a desired route split. The larger local patches remain opportunities rather than obligations to harvest. It does not claim terrain/ecology geometry, general pathfinding/route generation, hydrology/flow/recharge, walls/enclosure, weather/climate, arbitrary remote discovery, or one complete single-state first day containing every opening action.
- **Ordinary-scope boundary:** aggregate player-fantasy reporting must match current reachability. Report the distributed surface-plus-water generator, persisted six-edge opening opportunity graph, actor-safe current-voxel opening-graph choices, traversal-gated immediate source observation, physiology-sensitive optional-water routing, material-junction choice, continuous terrain-backed movement alongside edge-gated opening observation, ordinary grounded cache-support construction plus stockpile mounting at the worksite, the fresh-start long-stay support-chain/roof episode with workload-earned adze and causal exposure reduction, reversible isolated grounded-member recovery, exact-local stone-ring campfire assembly and staged-timber combustion with causal co-located warmth recovery, liberation-kit stone/timber acquisition from that graph, the bounded exact-local progression campaign through first mechanized convergence with local prospecting/tool handling/carried sample haulage/workshop machinery and one real autonomous-feed trip, and bounded exact-local field orders with post-admission tool fabrication, local surveying, carried extraction, and camp haulage as ordinary runtime capabilities. Generated exact-local progression campaigns and field orders must reuse the same independent world pressures as their matched spatial-proxy cases and preserve the actor-visible evidence and selected result they claim to validate. A bounded authored graph, progression corridor, and camp-to-target field route are still not general hydrology, terrain, pathfinding, route cost, world-scale discovery, or arbitrary haulage. Progression retains its spatial-proxy scope label because sustained stockpiling, later maintenance/service, reinvestment locality, and broader cross-site material movement remain abstracted even though its first mechanized processing/convergence episode is now exact-local. Fieldwork likewise retains its spatial-proxy scope label: one maintained depletion-driven secondary search/retool/haul path is exact-local, but broad survey-campaign travel, arbitrary subsequent-site exploration, and the general multi-site search campaign remain abstracted.
- **Primitive processing investment:** evaluate manual recovery and the processing-line investment from the same actor-visible state across a bounded known-workload sweep before committing the maintained long order. One short order must remain capable of favoring the manual route; the first workload where the conservative production-backed machine projection wins is reported specifically as the **player-attention crossover**, never as a complete economic break-even. Report the line's shaped-input and embodied material commitment alongside that crossover because several kilograms of reusable crusher, separator, crank, and flywheel capital are not made economically free merely because their assembly attention repays quickly. The current progression scope begins with those primitive stone/timber materials already owned as part of its disclosed prepared context, so the report must label that capital source instead of implying the episode acquired it ordinarily. Organic/replay exact-local campaign evidence must reuse that same generated prepared context and geology, execute physical prospecting and sample hauling, preserve the direct-copper shortage that makes the first reinforcement exclusive, and match the long campaign's first pick-versus-crank decision without using hidden geology or later outcomes. It must then build and charge the selected primitive processing line at the workshop, crush the same actor-selected feed, recover the same second-reinforcement copper through canonical powered separation, and converge on both upgrades. The first autonomous crush must overlap a productive exact-local replenishment trip rather than merely elapse while the actor idles: the player travels to a known source, mines into carried custody, returns, and stages that new feed locally while the crusher window is accounted as overlap plus any residual idle time. Any mass-changing pick reinforcement, whether first or second, requires drop-before-mass-change followed by local rediscovery and repickup. The selected maintained order must lie on the mechanized side of the attention market and have the required material already owned, but the report must not convert material capital into an invented attention currency. The long-order scenario is evidence for that disclosed workload conditional on prepared material affordability, not permission to label primitive processing universally mechanized or ordinary acquisition-funded. Workload generation must remain independent of the live crossover threshold, and later execution may evaluate the frozen choice without retroactively resizing demand. Sustained stockpiling, maintenance/service, and reinvestment remain explicitly outside this exact-local campaign witness.
- **Catalog continuity:** exercise reinforcement relationships across sampling, woodworking, power, crushing, grinding, and separation through their owning systems.
- **Woodworking continuity:** keep hewing, sawing, and turning physically distinct. Tool investment may change attention or recovery, but not invent alternate material transforms. In exact-local woodworking worlds, material-backed adzes and frame saws assembled after actor admission must be rediscovered through the local equipment observation surface before actor execution instead of treating the assembly commit receipt as player knowledge. Organic worlds span small work, short queues, and project-scale demand while inherited native-copper quantity varies independently from saw/reinforcement affordability thresholds. Named blocked, reserve-sensitive, payback, maintenance, and fallback regimes belong to maintained witnesses; a small organic report may legitimately omit one or more routes. Freeze investment intent before matched branches execute.
- **Settlement mechanization:** powered conversions preserve manual material/yield authority, consume finite work and wear, return player attention, and show workload-dependent payback without making manual capability irrational at every scale. Freeze the first investment before any follow-up order is disclosed. When later demand appears, carry actual equipment condition forward and reassess how much work the worn manual route and any feasible upgrade can finish; greater useful completion outranks attention savings, while equal-capacity routes use the shared minimum-return policy. Reinvestment, partial wear-limited work, and wear-blocked work are valid lived outcomes and must remain explicit. Separate capital/delegation witnesses stay labeled as separate executed projects rather than one continuous settlement.
- **Foundry continuity:** the post-settlement episode inherits material-backed, co-located frame-saw, treadle-hammer, treadle-drive, copper-reinforced crusher/separator, and copper-banded flywheel infrastructure plus a bounded already-owned assayed ore parcel from prior copper progression instead of rebuilding or ignoring prior capability. Organic inherited ore uses the same primitive mining-cycle scale and soft/hard assay bands as progression, independently of foundry capital or the feed required to close a shortfall; after admission the actor re-observes live ore mass/composition from runtime state. New foundry components must reuse canonically resolved workshop actions where useful. Before treating a native-copper shortfall as terminal, the actor evaluates the inherited powered dressing line first; manual breaking/sorting remains a physical fallback rather than the default reset after mechanization. Batch-aware planning must respect the same per-batch whole-mass recovery groups and finite stored-work limits as runtime, and must reacquire live processing/power envelopes after each completed batch because wear and remaining work can change the next legal batch. Maintained coverage includes both a terminal insufficient-ore witness and a recover-through-inherited-line witness; organic/replay cases vary healthy inherited condition as disclosed prior use. The actor builds the first foundry only when owned copper covers the foundry capital, mold-upgrade cast stock, and at least one real settlement-size follow-up batch. Cast-stock reinvestment is consequential only when that larger batch executes through the still-owned first-foundry thermal sink; insufficient owned copper after legitimate or partial recovery must defer the investment without fixture top-up.
- **Survival continuity:** direct food and drink evidence uses ordinary serving-sized runtime actions. A target reserve that exceeds one serving must execute and report repeated canonical actions rather than presenting the aggregate target as one use. Dietary breadth may improve nutrition-supported recovery rate without being forced to dominate calorie density in realized vitality over every finite horizon; report both the causal nutrition effect and the realized outcome.
- **Preservation continuity:** enclosure choices remain distinct capacity, preservation, material, and attention tradeoffs. Bounded organic worlds span current authored capacity pressure so ordinary evidence includes both a real enclosure market and genuine bulk-storage constraint instead of collapsing every sample to one feasible box. The provisioning episode's inherited enclosure is lived in that main state; the actor-selected build-or-decline path is a separate matched investment episode and must be labeled as such rather than presented as a same-state replacement decision. That matched episode executes its selected path and canonical later freshness consequence before evaluator-only candidate comparisons. Feasibility precedes ranking; freshness projections agree with canonical execution; dismantling uses timed work and conservative salvage.
- **Maintenance and automation:** manual recovery remains physically valid while mechanization may improve attention or material efficiency through authored inputs, finite work, wear, and service. Primitive power investment must be calibrated against fully executed matched projects: a higher-capital treadle may win only when its realized attention saving clears the same declared capital-return floor used pre-action. Settlement mechanical-provider reports distinguish current mechanical decision-frontier equipment from additive dynamo conversions whose extra capital buys electrical capability for later foundry work; a dynamo need not win a lumber-only comparison to be useful. Runtime charging targets the next known consumer batch instead of manufacturing surplus work for a dissipative accumulator. Routine component-replacement maintenance must replace the authored wear component actually being serviced rather than an unrelated structural assembly mass. Automation does not create yield. Mining policy uses acquired conservative evidence, never exact hidden reserve.
- **Demand-sized search investment:** survey investment sizes its bounded follow-up horizon from remaining demand, acquired conservative resource-scale evidence, and the actor's explicit productive-site prior. The horizon budgets enough attempts for the expected number of productive sites rather than pretending every candidate area realizes the observed upper reserve bound. Exact hidden reserve and generated barren-site frequency are not inputs.

## Counterfactual and replay discipline

Counterfactual evaluation may compute one shared observation horizon outside actor policy, then replay treatment
and baseline from the same decision state to that fixed horizon. Future controlled events and branch outcomes
never become actor inputs. Compare aggregate observable contracts when production treats internal representations
as equivalent.

Actor provisioning follows the same information boundary. A current task may budget its known survival cost,
but a possible follow-up action discovered only after prospecting or another observation cannot influence an
earlier provisioning choice. After new evidence arrives, reassess the newly disclosed work and provision from
that state if needed.

Project-owned routine gameplay verification ignores ambient replay state. Focused gates add one fresh replayable
organic case; broad audits route one fresh case to a single root-selected scope while preserving maintained coverage
everywhere. Reports reuse the replay contract while generating a broader fresh sample. [`TESTING.md`](TESTING.md) owns command selection;
command help owns exact option syntax.

Concise and verbose report modes may change diagnostics, not evidence semantics. The default concise report must
retain enough cross-system evidence to judge the player fantasy directly: name the player role, preserve
observe/infer/extract evidence, and expose world feedback such as depletion-driven rerouting or retooling rather
than reducing the experience to investment-loop counts. Decision-frontier summaries must label workload entry
thresholds explicitly instead of presenting frontier order as though it were a universal ranking. Cross-system
projections must identify themselves as projections when they have not been lived as one canonical episode, even
when every projected leg uses authoritative physics. When a maintained single-state episode later lives the same
interaction, summaries prefer that lived evidence and retain any consequential blocker it exposes; aggregate
delegated-time totals must not hide a shared-resource serialization boundary or imply concurrency the runtime did
not authorize. If lived operation uses a route justified by a separately executed market/frontier witness, label
that decision basis instead of implying the route selection itself was lived in the same state. Controlled
summaries remain labeled as capability evidence and never imply ordinary reachability.
Blocked selected continuations report their actual resulting state rather than a rolled-back decision state.

Agency exploration may use a bounded deterministic qualification search. Qualification is evaluator sampling
only, never actor input or prevalence evidence; an incomplete search remains valid bounded evidence.
