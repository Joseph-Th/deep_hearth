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
| Ordinary/runtime | focused `survival`, `progression`; report-only `woodworking`, `fieldwork` episodes | Automated-player outcomes through ordinary acquisition under the declared observable policy. |
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
cross-cutting contracts stay in the broad contract/audit targets even when they exercise the same helper code.
This is a build-performance boundary: filtering a test at runtime is not sufficient because Rust would still
code-generate every `#[test]` reachable from that focused crate.

| Scope | Contract |
| --- | --- |
| `survival` | Hunger/thirst pressure, exact authored food-option availability, food-category coverage, bounded quantity-scaled eating/drinking attention, world-seeded inherited reserve history independent of actor behavior, value-sensitive storage investment from one shared raw-material opportunity using projected edible-horizon return versus actor attention tolerance, survival-owned prospective freshness across a future authored enclosure transition, ordinary raw-timber -> manual-board -> manual-enclosure-body -> preservation construction, timed player-work enclosure dismantling with exact matter recovery and ambient-storage restoration, matched storage counterfactuals at one wall-clock endpoint, completed-profile compatibility for existing contents, non-retroactive preservation-state effects, diet tradeoffs, varied prospecting-work cost, reserve recovery, an integrated hydration-warning -> provision -> prospect -> stored-work sequence, and actual diet-supported vitality recovery. |
| `progression` | Coarse-to-fine evidence and information-value decisions; material-backed sampling; primitive crafting/mining/power/processing; a local scarce-copper pick-vs-crank sequencing counterfactual; direct-labor fallback versus mechanization; delegated work; finite recovery, stored-work loss/recharge, maintenance, and later reinvestment; ordinary quern/sizing-screen liberation through concentration; and finite organic opportunity. Separate contracts verify the reinforced sampling hammer and ordinary woodworking investments. The adze must improve attention without changing its canonical recovery stream. The dedicated frame saw must remain a copper-bearing investment with better timber recovery, real wear, typed capability gating, conservation, and deterministic mid-job save/load continuation; exact timing and yield values remain owned by production/content tests rather than duplicated here. |
| report `woodworking` | Replayable project size and owned-copper pressure across the ordinary adze/frame-saw decision. The episode executes the selected route and reports its attention and recovered-board consequence against a canonical adze counterfactual, so a material-efficiency trade does not masquerade as a simple upgrade. |
| report `fieldwork` | Replayable hidden channel, within-channel location, grade, gangue, hardness, and extraction mass. The actor compares only acquired transect evidence, uses cheap inspections before one detailed physical sample, then selects the least-capable ordinary extraction tool whose authored hardness limit covers the acquired 50 MPa hardness band. Mining admission remains the canonical legality proof and the hard-rock specialist may still adapt to its smaller typed batch limit. Hidden geological truth is diagnostic-only and never feeds policy. The reinforced indexed survey separately proves a real four-cell per-voxel information payoff, including hardness only on cells whose acquired abundance evidence establishes definite material presence. |
| `workshop` | Installed industrial operation under finite work, survival, wear, maintenance, structural pressure, hidden world change, and recovery. |
| `ore` | Installed crush/grind/screen/regrind/concentrate flow with selective recovery, exact constituent accounting, gangue-hosted prepared-feed acceptance, independently varied finite stored work, canonical per-stage stopping behavior, and terminal current-tier tailings. Capability-only industrial benchmark. |
| `foundry` | Installed room-temperature pure-copper heating/melting/casting with a same-furnace/same-electrical-source sensible-preheat energy-partition counterfactual, currently dominated rather than claimed as an active strategy, finite electrical and thermal capacity, adaptive melt/cast batches, ingot/reinforcement/native-copper/scrap remelting coverage, molten remainder, and passive sink recovery. Capability-only. |

Routine focused gates keep maintained regression/coverage cases and add one fresh replayable organic case.
`python ci.py report` expands that sample for broader exploration without turning the ordinary repair loop into
a soak. Full episodes are reserved for behavior that requires executed cross-system consequences. A world may
succeed, adapt, or stop at a canonical constraint; every partial or blocked outcome must preserve trusted-load
validity and relevant conservation.

### Catalog continuity contract

The catalog contract checks ordinary acquisition-graph continuity that should not depend on one actor-policy
branch. Copper reinforcement coverage derives the canonical reinforcement input from the authored pick upgrade
and requires that same input to reach the geological sampling hammer, woodworking adze, hand crank, stone crusher,
stone separator, and stone rotary quern, while allowing new compatible upgrade targets to be added. The ordinary mature reinvestment branch must execute the crusher and separator
upgrades, including a separator batch above the base 500 g envelope, rather than treating those edges as catalog
facts only. The same reinforcement must also increase the material-backed stone flywheel's stored-work capacity
through the energy owner without regressing carrier, transfer limits, passive loss, or disassembly recovery; the
expanded stored-work envelope must fund a real larger primitive-processing batch.

Woodworking continuity keeps the hewing and sawing routes physically distinct. The adze may accelerate the
ordinary hewing process without changing that process's canonical recovery stream, but it cannot satisfy the
dedicated sawing capability. The frame-saw process has no equipment-free fallback and must remain reachable only
after its authored copper blade and timber frame are assembled. Its better board recovery therefore spends scarce
copper and a replaceable wear component rather than becoming a hidden free efficiency multiplier. Exact current
input masses, yields, and timings remain content/production facts rather than gameplay-evaluation policy.

Preservation coverage requires every authored enclosure to remain ordinarily producible and recoverable while
retaining a distinct practical tradeoff in capacity, preservation, raw material, or construction attention.
Candidate ranking first respects physical feasibility: an enclosure that cannot hold the protected reserve or
cannot be produced from the disclosed raw-material opportunity is not an actor option. Inherited reserve history
is generated only from the world seed; actor policy must never rewrite which enclosure existed before admission
or how old retained food already is. The matched freshness-return experiment intentionally compares the fastest
and strongest feasible construction branches at one wall-clock endpoint. Its signed effective-age result is
intentional: extra construction delay can make stronger preservation temporarily worse. The report also compares
the exact remaining edible lifetime from the survival owner, so slower construction can still demonstrate a
longer-horizon advantage without an invented break-even approximation. Each branch must first project its selected
food lot through the planned authored storage transition using the survival-owned freshness projection, then prove
that forecast exactly against the later canonical construction and tick outcome. The projection is planning
evidence only and never substitutes for construction validation. Separate bounded contracts require medium and
bulk reserves to eliminate undersized candidates before ranking and require material-constrained opportunities to
exclude enclosure routes whose raw inputs are unavailable.
Each current preservation body must expose at least one exact same-material manual salvage route into a different
reusable form; timber bodies recover as boards plus explicit chip residue, while the carved stone crock returns
reworkable stone scrap. Additional legitimate salvage routes may coexist, and reverse routes must not create a
cheaper construction cycle.
The runtime experience must first dismantle the installed enclosure through the production player-work path: the
enclosure remains authoritative until its authored completion tick, survival reserves pay the authored exertion,
ambient storage is restored only on completion, and the complete embodied body mass returns to inventory before
any later manual salvage interpretation.

Primitive stone maintenance recovery must remain a zero-machine manual path from pure stone scrap to nonzero
reusable tool matter plus explicit chip residue with exact mass conservation and more attention than fresh lump
knapping. Unit evidence also requires contaminated or mixed-temperature scrap to reject atomically and an executed
maintenance loop to use recovered stone for a later service. Bounded survival behavior must vary preservation
willingness without choosing infrastructure independently of observed payoff. The actor projects every feasible
authored enclosure to one common future endpoint through the survival-owned freshness projection, values remaining
edible lifetime against a replayable behavior-root attention price, and may therefore select an intermediate
capacity/preservation/attention compromise rather than only the fastest or strongest endpoint. The fastest and
strongest branches remain matched reference cases for interpreting the selected frontier result. The progression
episode's matched local copper counterfactual remains balance evidence, not actor-policy variation
or a claim about the player's global copper portfolio. It compares only the pick and hand-crank opportunities
present in that decision state: current authored timing must show why pick-first is rational there while
crank-first retains a measurable early-autonomy benefit. Woodworking, sampling, quarrying, sizing, and later
machine upgrades are separate ordinary copper sinks and must not be erased by wording that calls this local
comparison "the first copper choice." If future content or tuning makes the matched pick/crank benefits genuinely
reciprocal, the harness may promote that local counterfactual back into actor policy only after the executed
comparison demonstrates the change.

Primitive automation coverage distinguishes capability from economics. Maintained progression worlds carry a
deep deterministic geological opportunity and must demonstrate setup-attention payback plus continued useful
work. Organic worlds vary between shallow and deep finite opportunities independently of the machinery's repeat
limit. A machine that performs useful canonical work but reaches known target exhaustion before setup payback is
a valid gameplay outcome, not a harness failure. The actor may discover that an investment was premature because
the available evidence does not expose hidden total deposit mass; hidden reserve truth must not become policy
input merely to make the investment look optimal in hindsight. The mature reinvestment counterfactual is
resolved only from the post-delegation, post-service state the actor actually reaches. Maintained deep worlds must
still execute that branch, while an organic branch that has canonically exhausted its known target reports
`known-target-supply` rather than receiving fixture ore or advertising an opportunity that later experience has
erased. Progression reporting records replacement-component preparation and the authored maintenance service interval
separately. Service occupies exclusive player work, consumes survival reserves through ordinary tick advancement,
and restores equipment condition only at its scheduled completion; missing world-space tool/access mechanics in
`STATUS.md` must not be described as missing maintenance labor.

Actor policy reads these values from production registries and state. The prose above owns required coverage,
not a second copy of the formulas or selection thresholds used to make actor decisions.

## Counterfactual and replay discipline

Counterfactual evaluation may compute a shared observation horizon outside actor policy, then replay treatment
and baseline from the same decision state to that fixed horizon. Future controlled events and branch outcomes
never become actor inputs. When production treats internal representations as equivalent, compare their
aggregate observable contract rather than incidental internal identity.

`DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` controls physical-world variation;
`DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` controls actor-policy variation where applicable; and
`DEEP_HEARTH_GAMEPLAY_SEEDS` selects explicit focused worlds for deliberate replay. Routine gates generate one
fresh organic case by default; supplying the roots replays that organic case exactly while maintained anchors stay
fixed. Failure output must retain replay input.

`python ci.py report` is the bounded exploration surface. Its default concise view includes ordinary survival,
primitive progression, woodworking, fieldwork, live content counts, the ordinary acquisition frontier, and
compact summaries of the controlled workshop/ore/foundry evidence. Controlled summaries stay explicitly labeled
and do not imply ordinary reachability; hiding them entirely would make a cold reader miss important interaction,
recovery, and finite-capacity behavior that already exists in the simulation. `DEEP_HEARTH_GAMEPLAY_VERBOSE`
restores full capability diagnostics, blockers, tradeoffs, and counterfactual detail;
`DEEP_HEARTH_GAMEPLAY_TRACE` adds operation-level workshop narration. Increase breadth through explicit
report/replay inputs rather than turning the edit loop into a multi-seed soak.
