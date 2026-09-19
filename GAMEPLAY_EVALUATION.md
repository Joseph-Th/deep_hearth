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

After setup, actor code must use production resolvers, validators, commits, and simulation ticks; read only
legitimate observable state, explicit actor policy, and canonical projections; never inspect hidden geology,
future controlled events, setup authorization, or comparison-branch outcomes; and preserve ordinary ownership,
persistence, conservation, capability, and survival rules. `src/content/gameplay_fixture.rs` owns controlled
scenario construction before actor admission.

The actor may reason from observable attention, material demand, survival cost, throughput, capacity, condition,
and acquired evidence. Registry order, implementation identity, hidden truth, and future outcomes are not policy
inputs. Observable ties require an explicit actor rule.

Prefer canonical assessments and typed outcomes when production already knows the semantic answer. If actor code
must reproduce a domain formula, threshold, provider rule, or hidden-state inference solely because no
production read surface exists, treat that as possible control-surface debt. Exact record reads for identity,
reporting, or already-authoritative values are not debt.

Shared harness support may own evaluation policy such as deterministic lot selection, bounded sampling,
scenario variation, or report formatting. Such policy stays outside production unless it is also a legitimate
product-domain answer. Reuse does not by itself confer simulation authority.

### Decision-frame contract

For material actor choices, prefer one bounded decision frame assembled from canonical production surfaces:

```text
legitimate observation
    -> bounded candidate families
    -> production-derived resolution / blocker
    -> policy comparison
    -> selected action or explicit no-action
    -> typed result and later feedback
```

Candidate generation is actor policy, but it should exploit domain structure rather than brute-force every
identity combination when production already exposes a narrower semantic route. Production owns legality and
physics; the actor owns search order, stopping rules, preferences, and uncertainty tolerance.

Every actor candidate set should have an evidence-strength interpretation. If generation exhaustively traverses
one declared observable domain, an empty set may support "no candidate in that domain now". If generation is
budgeted, heuristic, sampled, or stops after enough acceptable candidates, emptiness/absence supports only a
generator/search result and must retain its bound. This is the distinction behind `Generator gap` versus a
production `Validation gate` or canonical unavailability proof.

If a production/topology query itself is bounded, actor diagnostics preserve the query scope, completeness flag,
continuation/budget, and freshness basis. Do not collapse a partial production query into an exhaustive actor
claim merely because the actor used every item that happened to be returned.

Immutable authored topology is legitimate actor input when it is exposed through production registries or a
canonical registry-derived projection. It can answer what transformations/providers/routes are authored in
principle. It cannot establish that the route is ordinarily reachable, that the actor currently owns its
prerequisites, or that hidden world truth will satisfy it. Current candidates must be grounded in actor-visible
state and canonical resolution before policy compares them.

Keep topology discovery separate from route choice. A shared reverse index may return all manual producers of a
commodity, all nominal providers of a capability requirement, or the assembly ancestry of an infrastructure
definition. The actor decides which alternatives to investigate and how to rank their observable costs. This
separation allows one reusable causal map without moving strategy into simulation authority.

Actor diagnostics should name the strength of a planning claim. An authored edge/path is not an ordinary-play
claim; ordinary reachability is not proof that prerequisites are present now; a current opportunity is not an
authorization; a prior authorization is not valid after its bound state becomes stale. This vocabulary prevents
catalog discovery, controlled setup, current-state reasoning, and committed evidence from being merged into one
ambiguous notion of "available".

A good frame contains enough stable identity and typed consequence data that diagnostics, replay, and
counterfactual comparison can explain the choice without rereading hidden state or diffing the entire world.
When the same missing production projection forces several actors/probes to reconstruct the same meaning, treat
that as control-surface debt rather than standard harness infrastructure.

Freeze investment intent before evaluating comparison branches. Later lifecycle costs and terminal inventory assess
the decision afterward; they do not reselect it. A pre-action estimate retains its assumptions and uncertainty.
During execution, refresh observable resource and condition checks before each service or fallback.

For chained production/mining work, consume the exact contribution plus destination-owned landing identity from
`ProcessCompletion::landings()` / `ProcessParcelLanding` or `MiningClaimReceipt`. A landing may name a
pre-existing lot when inventory coalesces compatible matter; the paired `MaterialLotSpec` preserves what this
operation contributed even though the surviving lot may now contain more matter. Selecting "the lot that looks
like the output" by scanning a destination is therefore not equivalent evidence. Actor policy may choose among
multiple landed outputs, but it should not reconstruct custody identity already decided by inventory.

For direct eating/drinking, use the admitted outcome's `completes_at()` when scheduling the next observation or
decision rather than rereading `PlayerWorkState` solely to recover the schedule. The work owner remains the
authoritative persisted continuation; the outcome is a disposable receipt for the caller that just admitted it.

### Adaptive search and freshness

An actor may perform bounded search when search itself is policy, when alternatives are physically distinct, or
when production does not yet expose a direct feasible envelope. Keep that search reproducible and preserve the
offered request, selected request, and production blocker that caused adaptation.

When the search repeatedly varies only one monotonic quantity such as batch mass and treats a stable set of
capacity/resource/lifetime errors as "too large", that is evidence for a production planning surface. The
preferred future shape is a domain-specific feasible bound or bottleneck derived from the same resolver physics,
not a harness-maintained formula and not a generic action catalog.

Separate **domain constraint classification** from **policy response classification**. If several operations
sharing one physical profile repeatedly identify the same limiting dimensions, production may expose those
dimensions. An actor remains responsible for deciding that a finite-energy limit means recharge now, switch
provider, reduce batch, use a manual fallback, or abandon the goal. The same production blocker may rationally
produce different policy responses in different contexts.

Actor-side projections and candidate frames are disposable caches. Reuse one only when no intervening
authoritative transition can affect the facts it depended on. Otherwise reacquire the narrow production
assessment/resolution. Never use a cloned future branch, diagnostic truth, or remembered validation success as
authority for the current state.

### Temporal observation horizon

An actor may choose a bounded observation horizon from legitimate information such as a current work schedule,
policy deadline, or fixed experiment horizon. Advancing several ticks in one harness/tool call is acceptable only
when the implementation executes canonical tick semantics and preserves any ordered actor-visible outcomes that
could change policy before the requested horizon.

A known completion tick is an upper bound, not foreknowledge that the operation will complete normally. Support
loss, death, suspension, depletion, or another observable transition may require an earlier decision. Therefore
an actor-facing batch should stop on declared observable event classes or return the intervening outcomes; it
must not silently leap to the requested tick and discard decision-relevant feedback.

True semantic fast-forward is outside the current actor contract unless production itself implements and proves
an equivalent authoritative interval transition. Harness code may optimize invocation overhead, not simulation
rules.

## Evidence modes

| Mode | Surface | Supported conclusion |
| --- | --- | --- |
| Ordinary/runtime | focused `survival`, `progression`; report-only `woodworking`, `fieldwork`, `power-provider` episodes | Automated-player outcomes through ordinary acquisition under the declared observable policy. |
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
remain the repair-loop surfaces. `python ci.py report` is a separate explicit Cargo example so exploratory output
does not participate in routine test builds.

Each focused target contains exactly one executable gate/probe. Generator, topology, counterfactual, and other
cross-cutting contracts stay in the broad contract/audit targets.

| Scope | Contract |
| --- | --- |
| `survival` | Ordinary hunger/thirst pressure, diet-supported recovery, preservation investment, enclosure construction and timed dismantling with exact recovery, matched storage counterfactuals, and integrated provision/prospect/stored-work sequencing. |
| `progression` | Ordinary coarse-to-fine evidence, sampling-gated mining, primitive crafting/mining/power/processing, fallback versus mechanization, maintenance/recovery/reinvestment, and liberation through concentration. Sampling work precedes hardness classification; validation confirms legality without probing hidden hardness. |
| report `woodworking` | Ordinary adze/frame-saw board pipeline with full construction-plus-work attention, wear, maintenance, and timber/copper payback against equipment-free and all-adze baselines. |
| report `fieldwork` | Sample-then-extract mining orders with wear-aware tool choice, projected versus executed cost, partial-order and supply-stop accounting, and indexed-survey information payoff. Policy uses acquired evidence only; hidden truth is diagnostic-only. |
| report `power-provider` | Copper-free crank-vs-treadle charging comparison with full build, charge, bodily cost, wear, conservation, and trusted-load validity per arm. |
| `workshop` | Installed industrial operation under finite work, survival, wear, maintenance, structural pressure, and recovery. Capability-only. |
| `ore` | Installed crush/grind/screen/regrind/concentrate flow with exact constituent accounting and terminal tailings. Capability-only benchmark. |
| `foundry` | Installed pure-copper heating/melting/casting with finite energy, adaptive batches, remelting coverage, and sink recovery. Capability-only; ore and concentrate require reduction/smelting. |

Routine focused gates run maintained deterministic cases plus one bounded deterministic organic-variation case
per probe. An explicit variation or behavior root reseeds that bounded case when reproducing or extending a gate.
`python ci.py report` owns fresh organic sampling. Full episodes are reserved for behavior that requires executed
cross-system consequences. A world may succeed, adapt, or stop at a canonical constraint; every partial or blocked
outcome must preserve trusted-load validity and relevant conservation.

### Coverage contracts

These contracts state what gameplay evidence must prove; harness module docs own step-by-step execution.

- **Liberation cost:** the episode starts from disclosed pre-owned ore and parts, so measured cost excludes acquiring them. Batch-demand and full-buffer charging run from the same state through the same canonical chain and must show identical recovery, conserved matter, and trusted-load validity. Leftover buffered work may fund later jobs. Concentrate is distinct from usable copper; the scavenger leg is coverage, not an investment recommendation.
- **Catalog continuity:** the authored reinforcement input must reach sampling, woodworking, power, crushing, grinding, and separation equipment. The reinvestment branch executes crusher and separator upgrades, including an above-base separator batch. The same reinforcement raises flywheel capacity through the energy owner without regressing carrier, limits, loss, or recovery, and the expanded envelope funds a larger processing batch.
- **Woodworking continuity:** hewing and sawing stay physically distinct. The adze accelerates hewing without changing its recovery stream and cannot satisfy sawing. The frame saw needs its authored blade and frame, has no equipment-free fallback, and its payback includes embodied timber, blade copper, wear, and maintenance. Investment intent freezes before branches run; executed counterfactuals assess but never revise that choice. Exact masses, yields, and timings live in content/production definitions.
- **Preservation continuity:** every authored enclosure stays ordinarily producible and recoverable with a distinct capacity/preservation/material/attention tradeoff. Feasibility precedes ranking; projection, selection, and execution share one finite disclosed opportunity. The actor may decline construction when edible-horizon return does not pay attention cost. Each branch projects its food lot through the survival-owned freshness projection, then proves that forecast against canonical construction and ticks. Dismantling runs through the timed player-work path before salvage; each body exposes a same-material salvage route without creating a cheaper construction cycle.
- **Maintenance and automation:** stone scrap reknaps through a manual zero-machine path with exact conservation; contaminated or mixed-temperature scrap rejects atomically. The progression pick-vs-crank counterfactual compares only opportunities present in that decision state. Automation reports feed-buffer scheduling facts and labels economic payback not-established. Mining learns shortage only from committed claims or exhausted evidence; hidden reserve never feeds policy. Service occupies exclusive player work and restores condition only at completion.

## Counterfactual and replay discipline

Counterfactual evaluation may compute a shared observation horizon outside actor policy, then replay treatment
and baseline from the same decision state to that fixed horizon. Future controlled events and branch outcomes
never become actor inputs. When production treats internal representations as equivalent, compare their
aggregate observable contract rather than incidental internal identity.

`DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` controls physical-world variation;
`DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` controls actor-policy variation where applicable; and
`DEEP_HEARTH_GAMEPLAY_SEEDS` selects explicit focused worlds for deliberate replay. Routine gates are maintained
and deterministic. Supplying a variation or behavior root explicitly opts a gate into one bounded generated case
while maintained anchors stay fixed. Failure output must retain replay input.

`python ci.py report` is the bounded exploration surface. Its default concise view includes aggregate ordinary
survival, primitive progression, woodworking, fieldwork, and power-provider diversity, live content counts, the ordinary
acquisition frontier, and compact summaries of the controlled workshop/ore/foundry evidence. Controlled summaries stay explicitly labeled
and do not imply ordinary reachability. `DEEP_HEARTH_GAMEPLAY_VERBOSE`
restores full capability diagnostics, blockers, tradeoffs, and counterfactual detail. Goal-completion comparisons
aggregate all measured worlds and retain one seeded example in the concise view; verbose output retains every comparison. Blocked
selected continuations retain their actual elapsed time, inventory, and partial upgrades rather than rolling back
to the decision state. Woodworking feedback reports divergence between the actor's pre-action attention budget
and realized payback as policy/outcome divergence, not as estimation error; nominal timber projection mismatches
remain estimate disagreements. Fieldwork feedback aggregates wear-adjusted projection error only for completed orders.
Supply-stopped orders retain actual investment, effort, output, and shortfall separately. Physical-duration summaries use the
report's registry-derived tick duration. Missing or conflicting clock evidence
is reported as unknown.
`DEEP_HEARTH_GAMEPLAY_TRACE` adds operation-level workshop narration. Increase breadth through explicit
report/replay inputs.

Agency exploration retains its three unfiltered organic worlds, then searches at most 24 further deterministic
worlds for up to two with executed policy differences. The report separates unfiltered outcomes, qualified
worlds, unqualified attempts, and the search bound. Qualification uses diagnostic matched-branch outcomes only
for evaluator sampling, never actor choice; qualified samples do not estimate prevalence. An incomplete search
is valid bounded evidence. Routine agency gates do not perform this exploration search.
