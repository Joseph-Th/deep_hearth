# Gameplay Evaluation

This page owns automated-player information boundaries, gameplay-harness evidence semantics, focused scope
contracts, and exploration/replay policy. Use [`TESTING.md`](TESTING.md) for test selection and completion,
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

Actor policy may choose among observable alternatives using attention, material demand, survival reserve,
throughput, capacity, condition, and acquired evidence. Production owns legality and physics; actor code owns
candidate generation, ranking, search order, stopping rules, and policy ties. Shared harness helpers may implement
that evaluation policy but do not gain simulation authority.

### Decision-frame contract

Material choices should be explainable from one bounded frame:

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
| Ordinary/runtime, exact-local | focused `survival`; maintained `primitive-liberation`; report `woodworking`, `power-provider`, first-foundry episode | Automated-player outcomes through ordinary acquisition and the same logistics-locality admission used by the runtime. |
| Ordinary-system spatial proxy | focused `progression`; report `fieldwork`; preassembled `primitive-liberation` variation | Ordinary geology, production, equipment, survival, evidence, and investment semantics with actor movement intentionally abstracted because the runtime has no movement/path authority yet. These episodes support knowledge/tool/resource-loop conclusions, not travel/locality conclusions. |
| Controlled capability | `workshop`, `ore`, `foundry` | Canonical mechanics under disclosed prearranged infrastructure, not ordinary reachability. |
| Counterfactual | matched branches | Action-attributable differences from one actor-visible starting state over one fixed comparison horizon. |
| Exploratory | `python ci.py report`, explicit replays/sweeps | Bounded discovery and diagnostics; exploration does not create a routine pass/fail requirement. |

Automation can establish mechanical consequences, production blockers, conservation, persistence, replay,
relative treatment effects, and behavior of the declared automated policy. It does not establish human
comprehension, enjoyment, subjective fairness, visual quality, likely human strategy, or frequencies beyond the
evaluated worlds and horizons.

## Decision evidence

A material automated decision should be explainable from one coherent frame: world/variation seed, behavior
seed where applicable, tick, actor perspective, important legitimate observations, bounded candidate set,
representative production blockers, selected action or explicit no-action, policy rationale, typed result, and
important immediate/delayed consequences.

Diagnostic truth may explain a decision after the fact but must not feed back into that decision. Keep committed
runtime values distinct from projected next-decision values.

Decision diagnostics should preserve the control coordinate of the choice: owning authority/contract,
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

All focused targets use the `test-gameplay` feature contract. Broad gameplay verification uses one consolidated
`gameplay_audit` target so the shared harness module graph is compiled and linked once; the small focused targets
remain the fast iteration surfaces. Scoped reports reuse ignored report tests in those focused binaries where
available; cross-system reporting and workshop/agency exploration remain explicit examples.

Focused probe targets expose the routine gate/probe without compiling the larger owner contract suites.
Owner-specific contract targets remain available for exact-test iteration and are selected automatically by
`tools/run_test.py` when they are the smallest matching Cargo target. Report-capable probe roots add one ignored
exploratory entry. Generator, topology, counterfactual, and other cross-cutting contracts stay in the broad
contract/audit targets.

| Scope | Contract |
| --- | --- |
| `survival` | Hunger, thirst, recovery, preservation investment, storage recovery, and work/provisioning interaction through ordinary runtime paths. |
| `progression` | Evidence-gated mining, primitive processing, mechanization, maintenance, reinvestment, and first-foundry decision/execution coverage. |
| `settlement` | Repeated-work investment contracts for spindle drill, wire drawbench, helve hammer, sawmill, lathe, and grindstone conversions. |
| report `primitive-liberation` | Raw-material acquisition, primitive-kit construction, ore processing, payback, and first-foundry opportunity evidence. |
| report `woodworking` | Bare-hand/adze/frame-saw investment, wear, maintenance, and attention/material payback. |
| report `fieldwork` | Sampling, bounded search, depletion, retooling, salvage, survey investment, and evidence-driven extraction adaptation. |
| report `power-provider` | Human-power provider and accumulator investment across primitive and settlement workloads with full lifecycle costs. |
| `workshop` | Installed industrial operation under finite work, survival, wear, maintenance, structural pressure, and recovery. Capability-only. |
| `ore` | Installed crush/grind/screen/regrind/concentrate flow with exact constituent accounting and terminal tailings. Capability-only benchmark. |
| `foundry` | Installed industrial pure-copper heating/melting/casting with finite energy, adaptive batches, remelting, and sink recovery. Capability-only benchmark. |

Repository-owned gameplay gates and audits run maintained deterministic cases only. They do not consume ambient
gameplay seed variables, so routine verification is repeatable and cannot silently widen because a previous
exploration left replay state in the environment. `python ci.py report` uses the same fixed anchors with a bounded
fresh organic slice. Printed report roots are replay evidence; explicit variation or behavior roots reproduce a
sample exactly. Maintained anchor/coverage cases prove contracts and must not be read as prevalence. When
frequency matters, interpret the separately reported organic slice as bounded sampled-world evidence, not as a
population estimate. Full episodes are reserved for behavior that requires executed cross-system consequences. A
world may succeed, adapt, or stop at a canonical constraint; every partial or blocked outcome must preserve
trusted-load validity and relevant conservation.
Selected-path summaries must never count an unselected coverage branch, diagnostic counterfactual, or forced
negative control as something the player experienced. Those branches stay explicitly labeled as counterfactual or
coverage evidence in both verbose narration and concise aggregation.

### Coverage contracts

These contracts state what gameplay evidence must prove; harness module docs own step-by-step execution.

- **Liberation frontier:** the maintained anchor bootstraps only raw stone/logs plus the disclosed ore/storage world before actor admission, then canonically picks up, builds, and uses the primitive kit in the same state. Organic worlds that choose the kit must do the same; branch-targeted maintained coverage may use a preassembled ordinary kit but cannot stand in for acquisition continuity. Reporting must distinguish the pre-admission raw-source fixture from runtime same-voxel pickup and from ordinary world gathering; until [`STATUS.md`](STATUS.md) says otherwise, acquisition continuity begins at the disclosed raw world source rather than claiming terrain gathering. Kit construction is legal only when that world's complete disclosed campaign executes through carried machine/provider condition and finite-store loss and clears attention payback; campaign body economics remain visible as supporting evidence. Batch-demand and full-buffer charging run from matched states through the same canonical chain and must show identical recovery, conserved matter, and trusted-load validity. Concentrate remains physically distinct from usable copper, but the authored elemental-copper model gives rich concentrate a finite-recovery mechanical cleanup route to native metal. Separate first-foundry evidence may begin from a disclosed ordinary-material opportunity, but it must label that state boundary explicitly and freeze the recovery requirement before construction. Direct cold rework and foundry recovery are legitimate alternatives: the actor skips the foundry when lossy rework satisfies the disclosed order and builds it only when the material shortfall justifies full recovery. A selected foundry branch must exercise the treadle-to-dynamo conversion and conserve matter/energy ownership across charge, melt, cast, and ingot rework. Industrial foundry reachability is outside this contract and belongs to [`STATUS.md`](STATUS.md).
- **Catalog continuity:** the authored reinforcement input must reach sampling, woodworking, power, crushing, grinding, and separation equipment. The reinvestment branch executes crusher and separator upgrades, including an above-base separator batch. The same reinforcement raises flywheel capacity through the energy owner without regressing carrier, limits, loss, or recovery, and the expanded envelope funds a larger processing batch.
- **Woodworking continuity:** hewing, sawing, and turning stay physically distinct. The adze accelerates hewing without changing its recovery stream and cannot satisfy sawing or turning. Handles and timber flywheels keep their hand fallback but gain a dedicated spring-pole lathe route; the lathe improves attention rather than material yield. The frame saw needs its authored blade and frame, has no equipment-free fallback, and its payback includes embodied timber, blade copper, wear, and maintenance. Investment intent freezes before branches run; executed counterfactuals assess but never revise that choice. Exact masses, yields, and timings live in content/production definitions.
- **Settlement mechanization:** the sash sawmill, helve hammer, timber spindle drill, flywheel lathe, and flywheel toolroom grindstone are additive conversions of workshop investments, not replacement recipes that discard material or machine identity. Each powered process references its manual transform for exact material input/yield authority, consumes finite mechanical work through the energy owner, applies condition-based machine wear, and does not claim player attention during execution. Each maintained conversion must fit its intended finite accumulator through authored energy values. Matched short/project workloads must keep manual precursors rational below disclosed attention crossovers and prove conversion payback above them. Upgraded machines retain manual capability where authored so energy shortage changes the dominant cost instead of changing material transforms.
- **Preservation continuity:** every authored enclosure stays ordinarily producible and recoverable with a distinct capacity/preservation/material/attention tradeoff. Feasibility precedes ranking; projection, selection, and execution share one finite disclosed opportunity. The actor may decline construction when edible-horizon return does not pay attention cost. Each branch projects its food lot through the survival-owned freshness projection, then proves that forecast against canonical construction and ticks. Dismantling runs through the timed player-work path before salvage; each body exposes a same-material salvage route without creating a cheaper construction cycle.
- **Maintenance and automation:** stone scrap retains a manual zero-machine recovery path with exact conservation; the dedicated toolroom route may spend its authored consumable and infrastructure investment to improve material efficiency, and its powered upgrade delegates the same recovery through finite mechanical work. Ore grinding capability cannot satisfy the toolroom route. Contaminated or mixed-temperature scrap rejects atomically. The progression pick-vs-crank counterfactual compares only opportunities present in that decision state. Processing decisions compare canonical hand-processing attention with complete mechanized construction and charging attention; overlap-only setup recovery remains diagnostic. Settlement machine conversions use the same rule: setup and charging are priced before selection, powered execution is delegated world time rather than free work, and identical transforms prevent automation-only yield inflation. Autonomous feed work must distinguish a successfully prepared next-cycle buffer from a true destination-capacity blocker; the former is evidence that delegation returned player attention, not evidence of a bottleneck. The stockpiling-plus-forced-service branch is an unselected negative control, not a peer recommendation. Mining may use acquired conservative resource-scale evidence to size investment, but exact hidden reserve never feeds policy. Service occupies exclusive player work and restores condition only at completion.
- **Demand-sized search investment:** after a known local mining opportunity ends short, survey-capital decisions size their disclosed follow-up horizon from remaining demand and the acquired conservative resource-scale estimate, capped by the bounded candidate-site set. They do not price an upgrade against sites the current order does not plausibly need, and exact hidden reserves never enter the choice.

## Counterfactual and replay discipline

Counterfactual evaluation may compute a shared observation horizon outside actor policy, then replay treatment
and baseline from the same decision state to that fixed horizon. Future controlled events and branch outcomes
never become actor inputs. When production treats internal representations as equivalent, compare their
aggregate observable contract rather than incidental internal identity.

`DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` controls report physical-world variation;
`DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` controls report actor-policy variation where applicable; and
`DEEP_HEARTH_GAMEPLAY_SEEDS` remains a low-level explicit focused-world replay input. Routine `ci.py` gameplay
gates and audits clear these variables and execute maintained cases only. `python ci.py report` generates fresh
variation/behavior roots when none are supplied, keeps the maintained anchors fixed, and bounds every organic
sample. Report failure and success summaries retain replay input. `--variation-seed <u64>` is report-only;
report scopes with actor-policy variation also accept `--behavior-seed <u64>`. These flags are the validated CLI
equivalents of the report environment variables and take precedence over ambient values.

`python ci.py report` is the bounded exploration surface. Its default concise view keeps the measured player
loop and loop dynamics, one summary per ordinary probe, ordinary integration frontiers exposed by those probes,
and compact controlled workshop/ore/foundry summaries. It prioritizes lived pacing, repeated-work reuse,
provisioning/maintenance interruptions, adaptation, delegation, and source-boundary honesty over diagnostic
provenance labels that do not change the experienced route. Registry inventory and acquisition-edge catalog counts are
verbose diagnostics rather than default player-experience evidence; ordinary reachability is reported through the
episodes and explicit frontier summaries that exercise it.
Controlled summaries stay explicitly labeled and do not imply ordinary reachability. `python ci.py report
--verbose` restores the complete capability diagnostics, blockers, tradeoffs, counterfactuals, per-world
comparisons, and replay evidence. `DEEP_HEARTH_GAMEPLAY_VERBOSE` remains the environment-level equivalent for
tooling. Blocked selected continuations retain their actual elapsed time, inventory, and partial upgrades rather
than rolling back to the decision state.
`DEEP_HEARTH_GAMEPLAY_TRACE` adds operation-level workshop narration. Increase breadth through explicit
report/replay inputs.

Agency exploration retains its three unfiltered organic worlds, then searches at most 24 further deterministic
worlds for up to two with executed policy differences. The report separates unfiltered outcomes, qualified
worlds, unqualified attempts, and the search bound. Qualification uses diagnostic matched-branch outcomes only
for evaluator sampling, never actor choice; qualified samples do not estimate prevalence. An incomplete search
is valid bounded evidence. Routine agency gates do not perform this exploration search.
