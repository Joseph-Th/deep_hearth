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
initialize both survival and player logistics before actor work begins. The runtime's no-logistics-player locality
fallback is valid only for controlled-capability fixtures or explicitly movement-abstracted evidence; it cannot
support an exact-local ordinary-play claim.

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
pre-inventory route discovery comes from the production commodity handbook. Exact definitions and physics still come
from their owning registries/resolvers. Explicit lot selection remains valid only when the physical lot is itself a
meaningful player choice. Repeated stateful projections carry each production result into the next step; never
multiply the first-step cost across a lifecycle whose condition, reserves, occupancy, or other state can change.

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
`ProcessCompletion::landings()` / `ProcessParcelLanding`, `MiningClaimReceipt`, and admitted direct-consumption
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

### Temporal observation horizon

An actor may choose a bounded horizon from legitimate information such as a current schedule, policy deadline, or
fixed experiment duration. Multi-tick harness helpers are valid only when they execute canonical
`advance_tick` semantics and preserve ordered actor-visible outcomes that could change policy before the horizon.

A known completion tick is an upper bound, not guaranteed completion. Helpers must stop on declared
decision-relevant events or return intervening outcomes. Semantic fast-forward is outside the actor contract
unless production owns and proves the equivalent authoritative transition.

## Evidence modes

| Mode | Surface | Supported conclusion |
| --- | --- | --- |
| Ordinary/runtime, exact-local | focused `survival`, `primitive-liberation`, `woodworking`, `power-provider`, `settlement`, `foundry-bootstrap` | Automated-player outcomes through ordinary acquisition and the same logistics-locality admission used by the runtime. |
| Ordinary-system spatial proxy | focused `progression`; report `fieldwork` | Ordinary geology, production, equipment, survival, evidence, and investment semantics with actor movement intentionally abstracted because the runtime has no movement/path authority yet. These episodes support knowledge/tool/resource-loop conclusions, not travel/locality conclusions. |
| Controlled capability | `workshop`, `ore`, `foundry` | Canonical mechanics under disclosed prearranged infrastructure, not ordinary reachability. |
| Counterfactual | matched branches | Action-attributable differences from one actor-visible starting state over one fixed comparison horizon. |
| Exploratory | `python ci.py report`, explicit replays/sweeps | Bounded discovery and diagnostics; exploration does not create a routine pass/fail requirement. |

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
| `survival` | Hunger, thirst, recovery, preservation investment, storage recovery, and work/provisioning interaction through ordinary runtime paths. |
| `progression` | Evidence-gated mining, primitive processing, mechanization, maintenance, and reinvestment. |
| `liberation` | Material-backed carryover of the progression processing line, ordinary same-voxel acquisition and incremental quern/riddle fabrication, ore processing, wear, and post-extension lifecycle payback through the native-copper route. Its matched campaign proves whether the executed extension repaid over the disclosed workload; it is not pre-action build/defer evidence. |
| `settlement` | A lived lumber episode freezes its first frame-saw/sawmill investment against disclosed demand, then reveals a repeat order in the same worn state so owned machinery can be reused or later reinvestment can become rational. Separate executed crossover witnesses cover spindle drill, wire drawbench, helve hammer, and other machine families; the report labels those separately so they do not masquerade as one continuous settlement. |
| `foundry-bootstrap` | First-foundry build/defer choice after inherited settlement workshop and powered ore-dressing capability plus already-owned assayed ore, bounded recovery of a visible copper shortfall through that inherited line when executable, casting campaign, thermal recovery, and cast-stock reinvestment proved by a real settlement-size cast when owned supply permits it. |
| report `woodworking` | Bare-hand/adze/frame-saw investment, wear, maintenance, and attention/material payback. |
| report `fieldwork` | Sampling, bounded search, depletion, retooling, salvage, survey investment, and evidence-driven extraction adaptation. |
| report `power-provider` | Human-power provider and accumulator investment across primitive and settlement workloads with full lifecycle costs. |
| `workshop` | Installed industrial operation under finite work, survival, wear, maintenance, structural pressure, and recovery. Capability-only. |
| `ore` | Installed crush/grind/screen/regrind/concentrate flow with exact constituent accounting and terminal tailings. Capability-only benchmark. |
| `foundry` | Installed industrial pure-copper heating/melting/casting with finite energy, adaptive batches, remelting, and sink recovery. Capability-only benchmark. |

Routine focused gameplay gates and the broad gameplay audit pair maintained deterministic witnesses with one fresh,
replayable organic case. Explicit replay roots replace that fresh case when reproducing a run. Reports use a broader
bounded organic sample and agency qualification searches. Maintained cases prove contracts, not prevalence;
organic samples are bounded evidence, not population estimates. A named qualitative regime must have a maintained
witness; exploratory organic cases probe generator, policy, and integration drift between those anchors without
turning qualitative coverage into a probabilistic requirement. Organic generator ranges that are intended to
straddle an authored threshold, equipment scale, storage capacity, or production crossover derive that scale from
the current production definitions/projections. Fixed numeric pressure is appropriate only when the number itself
is the disclosed world condition or player goal, not when it is standing in for an authored requirement.
Direct Cargo invocation also includes one deterministic organic case; repository-owned runners replace its root
with a fresh replayable root so routine iteration varies without sacrificing reproduction.
An organic seed must materially affect actor-visible world pressure, actor policy, or both; changing only a replay
label is not variation. For bounded generators whose purpose depends on spanning distinct pressures or decision
regimes, keep a cheap direct generator contract that samples enough seeds to detect collapse without replacing the
end-to-end focused episode.
Partial or blocked outcomes must preserve trusted-load validity and relevant conservation. Selected-path summaries
must not count counterfactual, coverage-only, or negative-control branches as player experience.

### Coverage contracts

Harness modules own execution detail. These are the evidence obligations:

- **Liberation frontier:** prove acquisition continuity from the disclosed raw-source boundary through primitive equipment and processing. Keep controlled raw-source setup distinct from terrain gathering. Report executed-kit payback as later lifecycle feedback, not as a pre-action investment decision reconstructed from comparison outcomes. Label any first-foundry starting boundary explicitly, and conserve matter and energy across selected foundry work.
- **Ordinary-scope boundary:** until world-source acquisition is ordinarily reachable, aggregate player-fantasy reporting must say that its ordinary evidence begins after the disclosed bootstrap and must report fixture sourcing, runtime pickup, and world gathering separately. Runtime pickup does not prove ordinary gathering.
- **Catalog continuity:** exercise reinforcement relationships across sampling, woodworking, power, crushing, grinding, and separation through their owning systems.
- **Woodworking continuity:** keep hewing, sawing, and turning physically distinct. Tool investment may change attention or recovery, but not invent alternate material transforms. Freeze investment intent before matched branches execute.
- **Settlement mechanization:** powered conversions preserve manual material/yield authority, consume finite work and wear, return player attention, and show workload-dependent payback without making manual capability irrational at every scale. Freeze the first investment before any follow-up order is disclosed. When later demand appears, carry actual equipment condition forward and reassess how much work the worn manual route and any feasible upgrade can finish; greater useful completion outranks attention savings, while equal-capacity routes use the shared minimum-return policy. Reinvestment, partial wear-limited work, and wear-blocked work are valid lived outcomes and must remain explicit. Separate capital/delegation witnesses stay labeled as separate executed projects rather than one continuous settlement.
- **Foundry continuity:** the post-settlement episode inherits material-backed, co-located frame-saw, treadle-hammer, treadle-drive, copper-reinforced crusher/separator, and copper-banded flywheel infrastructure plus a bounded already-owned assayed ore parcel from prior copper progression instead of rebuilding or ignoring prior capability. New foundry components must reuse canonically resolved workshop actions where useful. Before treating a native-copper shortfall as terminal, the actor evaluates the inherited powered dressing line first; manual breaking/sorting remains a physical fallback rather than the default reset after mechanization. Batch-aware planning must respect the same per-batch whole-mass recovery groups and finite stored-work limits as runtime, and must reacquire live processing/power envelopes after each completed batch because wear and remaining work can change the next legal batch. Maintained coverage includes both a terminal insufficient-ore witness and a recover-through-inherited-line witness; organic/replay cases vary healthy inherited condition as disclosed prior use. The actor builds the first foundry only when owned copper covers the foundry capital, mold-upgrade cast stock, and at least one real settlement-size follow-up batch. Cast-stock reinvestment is consequential only when that larger batch executes through the still-owned first-foundry thermal sink; insufficient owned copper after legitimate or partial recovery must defer the investment without fixture top-up.
- **Survival continuity:** direct food and drink evidence uses ordinary serving-sized runtime actions. A target reserve that exceeds one serving must execute and report repeated canonical actions rather than presenting the aggregate target as one use. Dietary breadth may improve nutrition-supported recovery rate without being forced to dominate calorie density in realized vitality over every finite horizon; report both the causal nutrition effect and the realized outcome.
- **Preservation continuity:** enclosure choices remain distinct capacity, preservation, material, and attention tradeoffs. Feasibility precedes ranking; freshness projections agree with canonical execution; dismantling uses timed work and conservative salvage.
- **Maintenance and automation:** manual recovery remains physically valid while mechanization may improve attention or material efficiency through authored inputs, finite work, wear, and service. Routine component-replacement maintenance must replace the authored wear component actually being serviced rather than an unrelated structural assembly mass. Automation does not create yield. Mining policy uses acquired conservative evidence, never exact hidden reserve.
- **Demand-sized search investment:** survey investment sizes its bounded follow-up horizon from remaining demand and acquired conservative resource-scale evidence. Exact hidden reserve is not an input.

## Counterfactual and replay discipline

Counterfactual evaluation may compute one shared observation horizon outside actor policy, then replay treatment
and baseline from the same decision state to that fixed horizon. Future controlled events and branch outcomes
never become actor inputs. Compare aggregate observable contracts when production treats internal representations
as equivalent.

Actor provisioning follows the same information boundary. A current task may budget its known survival cost,
but a possible follow-up action discovered only after prospecting or another observation cannot influence an
earlier provisioning choice. After new evidence arrives, reassess the newly disclosed work and provision from
that state if needed.

Project-owned routine gameplay verification ignores ambient replay state, preserves maintained witnesses, and adds
one fresh replayable organic case. Explicit roots replay one requested case exactly. Reports reuse the same replay
contract while generating a broader fresh sample. [`TESTING.md`](TESTING.md) owns command selection; command help
owns exact option syntax.

Concise and verbose report modes may change diagnostics, not evidence semantics. Controlled summaries remain
labeled as capability evidence and never imply ordinary reachability. Blocked selected continuations report their
actual resulting state rather than a rolled-back decision state.

Agency exploration may use a bounded deterministic qualification search. Qualification is evaluator sampling
only, never actor input or prevalence evidence; an incomplete search remains valid bounded evidence.
