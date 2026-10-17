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
| `wilderness` | Production fresh-world generation supplies a deterministic five-edge opening opportunity graph with seed-varied finite forage and water quantities plus a finite fallen-timber reserve whose minimum exceeds the unaided body-carrying envelope. Their minimum food and water opportunities each exceed one resting day of authored basal need, giving the opening enough runway for non-subsistence decisions without claiming equivalent active-work duration. The maintained mobile actor observes the exact forage opportunity only after reaching it, gathers a 500 g portable ration rather than stripping the patch, skips an unnecessary water branch without learning its exact volume, and reaches a material junction through two lived continuous-body legs; each executed leg starts from a settled slow boundary, uses the production walk projection, then must match its projected control ticks and exact next-boundary energy/hydration charge before there stone and timber become simultaneous adjacent choices rather than scripted checkpoints. Maintained coverage supplies three disclosed **camp goals** independently of world generation and hidden world quantities: stay mobile and defer both materials when no workload exists; establish a worksite cache at the material junction by hauling field-box construction timber back from its leaf, building the box at camp, retaining its material-backed woodworking residue with provisions, then making a second canonical timber trip for a low grounded cache support plus 6 kg of staged reserve; from the timber leaf the actor hand-sets that support into the adjacent junction, returns with the remaining 6 kg, mounts the existing field box onto the active member, and fills it so stored matter becomes a real structural load; or protect a longer food reserve whose eight-day target exceeds the field box's seven-day berry horizon and therefore justifies hauling stone back to camp for the stone crock's ten-day horizon. The goal is evaluator policy, not claimed emergent strategy; the resulting route, load, source depletion, storage choice, provisioning, and retained world reserve are lived outcomes. Selected storage is actually fabricated, placed at the junction, and filled through ordinary runtime paths; the worksite branch additionally uses ordinary grounded structural construction and stockpile mounting, so capacity, preservation, camp centralization, retained fabrication residue, and structural support are physical consequences rather than catalog comparisons. The same long worksite project creates an ordinary warmth problem: unsheltered exposure lowers the persisted thermal-comfort reserve into the chilly band. The actor budgets the known fire response before committing to that project, makes a canonical round trip to gather the campfire's exact local stone input, assembles a stone-ring campfire at the junction, burns one authored 1 kg wood charge from the staged timber reserve, and remains co-located while the fire's finite passive thermal loss raises comfort back above the authored warning threshold. Burned fuel enters an explicit terminal matter account, the thermal store retains only finite remaining heat, and a remote burning fire does not warm the player. Hunger and hydration pressure are crossed with project work in maintained coverage: the actor prices canonical gathering, crafting, load-sensitive camp travel, the fire stone trip/recovery window when selected, and the remaining route before deciding its working reserves. Lived camp travel now executes through generated-terrain 20 Hz body motion; future legs whose exact source/body context has not yet been reached stay deliberately labeled as conservative project upper bounds rather than being misreported as exact path projections. Optional-water routing is derived from observed hydration plus that disclosed project budget rather than from the evaluator's start-profile label, and food is provisioned before work when the same projection would cross the hunger warning; needless water/food remains deferrable. The maintained policy actor therefore uses canonical continuous-body locomotion for its actual forage, optional-water, junction, material-haul, cache, and campfire logistics rather than the legacy timed-step work path. A separate maintained direct-control witness additionally traverses all five unique opening edges through canonical 20 Hz continuous-body locomotion, visits both material leaves after the forage/water branch, settles at each interaction voxel, and issues one-shot empty-hand secondary use for forage, water, and the first timber batch. Before leaving the already-observed water detour, the actor projects its known two-edge return through forage to the work-path junction with the production-owned continuous walk projection; lived control execution must match the projected control ticks exactly. It deliberately tops that timber parcel to 5 kg at the legal slow boundary and carries 5.5 kg total back to the junction through the same continuous body controller. If slow work cannot admit on the sampled control tick, the request survives through neutral continuation and re-resolves once at the next legal boundary; the report retains those control-tick queue waits alongside finite gathering, finite water withdrawal, hydration change, carried load/capacity, and timed-work proof. This witness does not replace the policy actor's smaller deliberate forage or project-aware hydration choices. A single field box remains below the early adze's five-board-batch attention crossover, preserving rational tool restraint even in the worksite-cache branch. A separate fresh-world **long-stay shelter** witness starts with empty carried custody, uses the same continuous-body opening routes to gather its own stone and timber, and derives its tool decision from the actual roof geometry rather than a synthetic workload. Three 3.25 kg timber members are hand-set into a real support chain; the 6.5 kg 1 m² roof requires nine board batches, so acquiring the stone adze reduces total board-shaping attention by 90 ticks relative to bare work. The selected boards are then placed by ordinary secondary use, Planned roofing gives no benefit, activation changes overhead assessment, and the next survival tick observes the authored exposure loss fall from 1000 to 500 ppm while matter remains conserved. Exploratory worlds vary generation seed, physiology pressure, and camp goal independently. Evidence proves need-sensitive routing, purposeful material choice, finite runway, selective harvesting, local source opportunity cost, continuous-body camp logistics, finite material-backed fire/warmth, a fresh-start material-backed overhead refuge, survival/project budgeting, workload-sensitive restraint and investment, one-shot direct-use reliability, exact costing of executed and already-known bounded continuous paths, a finite loaded continuous-body timber return, and continuous-body reachability across the complete bounded five-edge opening topology. It does not prove one single-state first-day sequence containing every opening action, general pathfinding/route generation, hydrology, walls/enclosure, weather, or climate. |
| `survival` | Hunger, thirst, recovery, preservation investment, storage recovery, and work/provisioning interaction through ordinary runtime paths. Rested worlds exercise varied proactive provisioning after bounded passive exposure; warning-boundary worlds own urgent hunger/thirst response rather than forcing every rested case into crisis. The provisioning world may inherit an already-owned preservation enclosure and exercises its lived food-rotation consequence. Organic diet policy uses the actor-visible coarse recovery signal plus currently available dietary categories: it pays the extra meal mass for balanced recovery only when visible recovery has materially fallen and the missing variety is actually available, rather than assigning diet preference from behavior entropy. The matched diet-recovery counterfactual still executes both policies to measure smaller exact consequences; it creates a real vitality deficit through canonical exposure, then establishes a material-backed stone-ring campfire through ordinary assembly and finite combustion, warms above the authored recovery threshold, and keeps that finite heat source fueled while matched compact and balanced meals recover for the same horizon. Reports expose warmup, fuel/refuels, minimum thermal comfort, diet quality, and realized vitality change so cold stress cannot silently masquerade as nutrition evidence. Preservation build-or-decline economics are evaluated in a separate matched episode with the same protected-reserve scale, a disclosed finite raw-material opportunity, and a seed-varied disclosed reserve-use horizon that can fall shortly before or after ambient spoilage. Investment value is the enclosure's incremental edible lifetime over the no-build outcome at that same horizon, so food that would still be usable without construction is not credited to preservation. Reports must label the continuity boundary explicitly so the selected enclosure is never misread as replacing or downgrading the inherited enclosure in one continuous state. |
| `progression` | Post-wilderness primitive copper progression: evidence-gated mining, workload-sensitive manual-versus-mechanized processing, maintenance, and reinvestment from a disclosed prepared primitive context. The maintained long order selects mechanization only after the same actor-visible decision state proves short work still favors the manual route and reports the first player-attention crossover plus the real embodied processing-line capital. Organic and explicit-replay cases additionally re-execute the generated scarce-copper campaign through first mechanized convergence in a bounded exact-local corridor using the same four geological clues, reserve/grade pressures, direct-copper scarcity, and prepared raw opportunity as the long campaign. The actor must physically enter each regional/clue region before prospecting, choose regional/surface/sample techniques through the same player-facing region catalog instead of evaluator method identity, fabricate its stone pick and geological hammer after admission, rediscover and carry them, use that hammer only when it is a currently usable local catalog instrument, mine samples into carried custody, haul them back to the workshop, and make the same evidence-driven pick-first/crank-first decision. The chosen first investment then continues physically: the actor builds the crank/flywheel/crusher/separator line at the local workshop, charges it canonically, starts the first crush from actually hauled feed, and uses part of that autonomous processing window for a real field trip that travels to known bulk ore, mines the next feed into carried custody, and returns it to the workshop. Powered separation must recover the same exact second-reinforcement copper parcel selected by the matched spatial-proxy campaign, after which the second reinforcement executes with ordinary custody rules. Any mass-changing pick reinforcement requires drop, local modification, rediscovery, and repickup. Both sequencing branches must end with the reinforced pick and reinforced crank plus a physically returned hard-seam sample. The same seed must match the spatial-proxy campaign on regional evidence, detailed clue evidence, owned bulk assay, direct-copper shortage, information-refinement outcome, first investment decision, selected processing feed, recovered second-upgrade copper, and convergence. Sustained stockpiling, later maintenance/service, and reinvestment remain spatial-proxy, so the whole progression scope is not exact-local. It does not establish fresh-spawn pacing or ordinary first-day acquisition; opening food/water/fire evidence belongs to the wilderness scope, while shelter/weather remain absent. |
| `liberation` | Material-backed carryover of the progression processing line and a workload-dependent choice between low-infrastructure manual ore recovery and acquiring the incremental quern/riddle extension from the ordinary generated wilderness graph. The build projection prices the minimum material-leaf route, exact-local gathering, and fabrication before the actor freezes its choice; optional water is not charged when current physiology does not require it. A selected build now composes the same terrain-aware opening used by fresh wilderness play, places the inherited workshop at that generated surface spawn, traverses every chosen graph edge through canonical 20 Hz continuous-body locomotion while preserving the one-slow-tick-per-edge acquisition-attention model, gathers stone/timber into carried custody, returns loaded to the inherited workshop, and executes matched recovery without retroactive reselection. The processing line remains a disclosed separate-episode handoff from spatial-proxy progression. |
| `settlement` | A lived lumber episode freezes its first frame-saw/sawmill investment against disclosed demand, but the powered upgrade identity is no longer evaluator-supplied: after admission the actor discovers the exact-local frame-saw upgrade through `player_equipment_upgrade_options`, then discovers the compatible powered board process from the authored powered-variant/topology surface before any upgrade parts are fabricated. The same observed row is later revalidated at actual upgrade time, so discovery is not an authorization shortcut. The episode then reveals a later order in the same worn state so owned machinery can be reused or later reinvestment can become rational. Maintained witnesses use matched repeat demand for stable crossover regression, while organic follow-up demand is independently generated from the remaining disclosed lumber opportunity using world entropy only; it must not read the first investment result, current wear, live crossover, or actor policy. This lets the same workshop face shrinking or growing demand after its first decision and makes wear-driven capacity loss distinguishable from ordinary attention payback. Copper working has an explicit capital ladder: direct hand work can remain best for a short job, repeated work can justify a treadle hammer that improves player-active throughput without delegation, and later helve-hammer investment can convert that earned workshop capability into powered delegated work. Separate executed crossover witnesses cover spindle drill, wire drawbench, helve hammer, and other machine families; the report labels those separately so they do not masquerade as one continuous settlement. A separate single-state five-family workshop portfolio proves that delegated work is usable agency rather than idle fast-forward: while a powered spindle-drill job continues on the shared finite flywheel bank, the player canonically dresses stone-chip stock into the drill's real replacement bit, and the report records the fully overlapped player-attention window before the machine job completes. |
| `foundry-bootstrap` | First-foundry build/defer choice after inherited settlement workshop and powered ore-dressing capability plus already-owned assayed ore from the same primitive copper-progression mining/assay vocabulary. Pre-foundry ore recovery reuses the copper-reinforced hand crank earned by primitive progression; the electrical prime mover is not inherited. A selected foundry bootstrap must fabricate the timber treadle base and convert it to the treadle dynamo before electrical melting, so the electrical transition is paid and observable. The episode also covers bounded recovery of a visible copper shortfall through the inherited dressing line when executable, casting, thermal recovery, and cast-stock reinvestment proved by a real settlement-size cast when owned supply permits it. |
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
- **Initial wilderness edge:** production world creation must generate a bounded five-edge horizontal opportunity graph from an explicit seed before admission: forage one step from spawn; at forage, finite drinkable water is one adjacent optional branch and a second edge continues to a material junction; at that junction stone and timber are simultaneous adjacent leaves in seed-selected assignment. Forage mass and water volume vary deterministically by seed, and their minimum generated quantities each exceed one resting day of authored basal need. The generated timber opportunity must remain finite and seed-varied but its minimum must exceed the authored unaided body-carrying envelope, making it a known local reserve rather than a one-trip pocket inventory. Generation commits surface custody, fluid custody, water location, and all normalized traversal edges only after owner snapshots precheck, and fresh generation rejects any already-populated logistics world rather than layering the opening over preexisting custody. After admission the actor must begin with no exact-local opening source, orient only through traversal-gated adjacent surface/drink observation, observe only current-voxel movement choices, and prove an unlisted geometric neighbor is rejected unchanged. Player carry capacity is authored physiology and must not be resized to the plan. The maintained mobile actor learns exact forage mass locally, gathers a 500 g ration, leaves the finite surplus in world custody, skips unnecessary water without learning its exact volume, reaches the material junction in two movement steps, and defers both visible raw-material leaves when no current workload justifies collection. A hydration-pressure mobile actor must instead visit the water branch, learn exact-local volume, drink canonically to a working target that includes the disclosed remaining route/project hydration cost, return, and reach the junction; the route decision itself must come from actor-visible reserves and the project projection, not the setup-profile enum. Maintained camp coverage must execute both primitive storage goals from the same actor-visible junction rather than merely compare definitions. A disclosed worksite-cache goal sends the actor to the timber leaf for field-box construction material, returns that load to the junction, fabricates and places the box there, and retains the fabrication residue and provisions instead of carrying or deleting the byproduct. The actor then returns to the same timber source for enough material to hand-set a low grounded cache support into the face-adjacent junction plus 6 kg of staged reserve, constructs that member through ordinary player structural work, returns carrying the remaining 6 kg, mounts the field box to the active support, and fills it. The mounted cache must create nonzero structure-owned stored-matter load, and the report must expose support mass/work, retained residue, camp logistics steps, and selected-source opportunity versus remainder so the worksite is evidenced as material-backed leverage rather than a policy label. The long worksite path must also prove the opening fire/warmth loop. Its ordinary elapsed work must drive the persisted thermal-comfort reserve below the authored warning without relying on a cold-start fixture; before committing, the actor budgets the campfire stone round trip and a bounded warmth-recovery window alongside the storage project. After the cache is loaded, the actor gathers the campfire's exact local stone input, returns to the junction, assembles the stone-ring hearth, consumes one authored 1 kg wood charge from the staged timber through the canonical combustion action, and remains beside the fire until passive thermal loss restores comfort above the warning threshold. Fuel mass must move into the explicit combustion terminal account, finite heat must remain in the store after recovery, and owner-level evidence must prove the same burning fire does not warm a remote player. This is fire/warmth coverage, not evidence of walls, enclosure, weather protection, ambient climate, or general environmental heat transport. A disclosed preservation goal gathers stone, returns it to the junction, and fabricates the carved stone crock because its eight-day food horizon exceeds the field box's seven-day berry horizon while fitting the crock's ten-day horizon. The actor must price canonical gathering/crafting exertion and load-sensitive project travel before either project and provision first when the projected budget would cross a survival warning. The first timber box must remain capable of deferring the adze while its two board batches are below the production-backed five-batch crossover. A separate long-stay fresh-world witness must derive the roof workload from authored geometry, acquire its own stone and timber, build every roof-height support through ordinary timed structural work, acquire the adze only when that real board workload repays it, place selected boards through ordinary direct control, prove Planned roofing gives no benefit, and observe the completed roof reduce canonical exposure loss while conserving matter. Exploratory wilderness cases independently vary generated world seed, physiology pressure, and camp goal; the goal is explicit evaluator policy and must not read hidden world quantities, the live decision threshold, or a desired route split. The larger local patches remain opportunities rather than obligations to harvest. It does not claim terrain/ecology geometry, general pathfinding/route generation, hydrology/flow/recharge, walls/enclosure, weather/climate, arbitrary remote discovery, or one complete single-state first day containing every opening action.
- **Ordinary-scope boundary:** aggregate player-fantasy reporting must match current reachability. Report the distributed surface-plus-water generator, persisted five-edge opening opportunity graph, actor-safe current-voxel step choices, traversal-gated immediate source observation, physiology-sensitive optional-water routing, material-junction choice, edge-gated movement, ordinary grounded cache-support construction plus stockpile mounting at the worksite, the fresh-start long-stay support-chain/roof episode with workload-earned adze and causal exposure reduction, reversible isolated grounded-member recovery, exact-local stone-ring campfire assembly and staged-timber combustion with causal co-located warmth recovery, liberation-kit stone/timber acquisition from that graph, the bounded exact-local progression campaign through first mechanized convergence with local prospecting/tool handling/carried sample haulage/workshop machinery and one real autonomous-feed trip, and bounded exact-local field orders with post-admission tool fabrication, local surveying, carried extraction, and camp haulage as ordinary runtime capabilities. Generated exact-local progression campaigns and field orders must reuse the same independent world pressures as their matched spatial-proxy cases and preserve the actor-visible evidence and selected result they claim to validate. A bounded authored graph, progression corridor, and camp-to-target field route are still not general hydrology, terrain, pathfinding, route cost, world-scale discovery, or arbitrary haulage. Progression retains its spatial-proxy scope label because sustained stockpiling, later maintenance/service, reinvestment locality, and broader cross-site material movement remain abstracted even though its first mechanized processing/convergence episode is now exact-local. Fieldwork likewise retains its spatial-proxy scope label: one maintained depletion-driven secondary search/retool/haul path is exact-local, but broad survey-campaign travel, arbitrary subsequent-site exploration, and the general multi-site search campaign remain abstracted.
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
