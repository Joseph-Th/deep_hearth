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
| Ordinary/runtime | `survival`, `progression` | Automated-player outcomes through ordinary acquisition under the declared observable policy. |
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

All focused targets use the `test-gameplay` feature contract. `gameplay_audit` is the broad checkpoint/report
surface.

| Scope | Contract |
| --- | --- |
| `survival` | Hunger/thirst pressure, exact authored food-option availability, food-category coverage, bounded quantity-scaled eating/drinking attention, actor-selected attention-efficient versus maximum-protection storage investment from one shared raw-material opportunity, ordinary raw-timber -> manual-board -> manual-enclosure-body -> preservation construction, the selected enclosure backing the actual retained provisioning reserve, completed-profile compatibility for existing contents, non-retroactive preservation-state effects, diet tradeoffs, varied prospecting-work cost, reserve recovery, and actual diet-supported vitality recovery. |
| `progression` | Coarse-to-fine evidence acquisition and information-value decisions, primitive crafting/mining/power/processing, scarce-copper choice, direct-labor fallback versus mechanization, delegated work, finite-recovery sorting, flywheel self-discharge/recharge, second reinforcement, convergence, finite machine lifecycle, material-backed service that preserves prior scarce upgrades, and mature reinvestment that executes crusher, separator, and flywheel upgrades to expose coupled throughput, batch-capacity, and stored-work consequences. |
| `workshop` | Installed industrial operation under finite work, survival, wear, maintenance, structural pressure, hidden world change, and recovery. |
| `ore` | Installed crush/grind/screen/regrind/concentrate flow with selective recovery, exact constituent accounting, gangue-hosted prepared-feed acceptance, and terminal current-tier tailings. Capability-only. |
| `foundry` | Installed room-temperature pure-copper sensible preheating, melting/casting, finite electrical and thermal capacity, adaptive batches, ingot/reinforcement/native-copper/scrap remelting coverage, molten remainder, and passive sink recovery. Capability-only. |

Routine focused gates combine maintained regression cases with bounded reproducible variation. Full episodes
are reserved for behavior that requires executed cross-system consequences. A world may succeed, adapt, or stop
at a canonical constraint; every partial or blocked outcome must preserve trusted-load validity and relevant
conservation.

### Catalog continuity contract

The catalog contract checks ordinary acquisition-graph continuity that should not depend on one actor-policy
branch. Current copper reinforcement coverage requires the exact 20 g reinforcement commodity to reach the pick,
hand crank, stone crusher, and stone separator through additive upgrades from assemblable bases, with maintenance
and worn-recovery routes retained. The ordinary mature reinvestment branch must execute the crusher and separator
upgrades, including a separator batch above the base 500 g envelope, rather than treating those edges as catalog
facts only. The same reinforcement also upgrades the material-backed stone flywheel from 500 J to 750 J through
the energy owner while preserving carrier, transfer limits, passive loss, and exact disassembly recovery; the
expanded stored-work envelope must fund a real larger primitive-processing batch.

Preservation coverage requires both timber enclosure definitions to remain ordinarily producible and verifies
that the stronger double-wall option keeps the same usable capacity while requiring more embodied timber and
more construction attention for its higher preservation multiplier. The ordinary survival actor compares those
physical costs from the same available raw-timber opportunity, and the chosen enclosure must be the enclosure
that protects the later retained reserve rather than a parallel demonstration fixture. Every storage body must expose one manual
salvage route that exactly partitions detached body mass into reusable boards and explicit chip residue; reverse
routes must not create a cheaper construction cycle.

Primitive stone maintenance recovery must remain a zero-machine manual path from 1 kg pure stone scrap to an
exact 0.8 kg working component plus 0.2 kg chips, with more attention than fresh lump knapping. Unit evidence also
requires contaminated or mixed-temperature scrap to reject atomically and an executed maintenance loop to use
recovered stone for a later service. Bounded survival behavior must exercise both preservation investment
objectives when multiple physically distinct definitions exist. The progression episode's matched first-investment counterfactual
remains the narrower pick-versus-crank behavioral experiment.

Actor policy reads these values from production registries and state. The prose above owns required coverage,
not a second copy of the formulas or selection thresholds used to make actor decisions.

## Counterfactual and replay discipline

Counterfactual evaluation may compute a shared observation horizon outside actor policy, then replay treatment
and baseline from the same decision state to that fixed horizon. Future controlled events and branch outcomes
never become actor inputs. When production treats internal representations as equivalent, compare their
aggregate observable contract rather than incidental internal identity.

`DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` controls physical-world variation;
`DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` controls actor-policy variation where applicable; and
`DEEP_HEARTH_GAMEPLAY_SEEDS` selects explicit focused worlds. Failure output must retain replay input.

`python ci.py report` is the bounded exploration surface. `DEEP_HEARTH_GAMEPLAY_VERBOSE` expands decisions,
blockers, tradeoffs, and focused diagnostics; `DEEP_HEARTH_GAMEPLAY_TRACE` adds operation-level workshop
narration. Increase breadth through explicit report/replay inputs rather than turning the edit loop into a
multi-seed soak.
