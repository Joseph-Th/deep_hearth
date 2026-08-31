# Architecture

This page owns project-wide implementation rules. Use [`README.md`](README.md) for routing,
[`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) for subsystem semantics, [`STATUS.md`](STATUS.md) for runtime
scope, and [`TESTING.md`](TESTING.md) for verification.

The default shape is one authoritative owner, one canonical consequential mutation path, deterministic
replay from persisted state, and explicit adapter boundaries.

## Contract map

| Question | Read |
| --- | --- |
| What owns generated state and who may mutate it? | [State model](#state-model); [Ownership](#ownership) |
| How should an agent observe, plan, authorize, continue, and verify? | [Agent-legible control grammar](#agent-legible-control-grammar) |
| Where does a new concept belong in the abstraction tower? | [Abstraction and dependency direction](#abstraction-and-dependency-direction) |
| How should fallible mutation and stale state behave? | [Mutation and failure](#mutation-and-failure) |
| What must replay/load preserve? | [Determinism](#determinism); [Persistence and adapters](#persistence-and-adapters) |
| How are cross-owner edges and system invariants structured? | [Invariants](#invariants); [Cross-owner flow discipline](#cross-owner-flow-discipline) |
| What API, naming, source-layout, and comment shapes are preferred? | [API and representation rules](#api-and-representation-rules); [Naming](#naming); [Source and comment contracts](#source-and-comment-contracts) |

## State model

| Role | Authority |
| --- | --- |
| Registries | immutable validated definitions and lookup tables |
| `AppState` | generated mutable state required for continuation |
| Records | typed identity, ownership, lifecycle, references, local values, revisions |
| Systems | canonical resolution, validation, decisions, and mutation |
| Indexes/caches/projections | derived data with one synchronization or reconstruction owner |
| Adapters | external resources, IO, platform effects, renderer integration |

Definitions describe what may exist. Runtime records describe what exists. Mutable progress belongs in
runtime state, never in definitions or derived projections.

### Public read, owned write

`AppState` exposes immutable root-owner accessors to legitimate callers while mutable owner access stays
crate-private. Public commands therefore inspect the same authoritative records that adapters, tests, and
automated actors can read, but consequential writes must return through owner-controlled validators/commits or
the canonical tick orchestrator. Do not add a public `*_mut` escape hatch to make an adapter, harness, or agent
integration convenient.

A value type may expose mutation when it is independently ownable and mutation is its own complete contract,
such as an explicitly owned deterministic RNG. That does not authorize bypassing `AppState` ownership for
generated simulation state.

## Agent-legible control grammar

Consequential subsystems should present the same conceptual control surface even when some roles collapse into
one implementation for a simple owner:

| Role | Purpose |
| --- | --- |
| Definition | Immutable authored possibility, identity, limits, and references. |
| State/record | Authoritative generated fact required for continuation. |
| Projection/assessment | Read-only observable state or derived consequence; never an alternate owner. |
| Resolution/decision | Pure or read-only derivation of one concrete operation from current facts. |
| Validation/authorization | Proof that a requested consequential transition is legal against a bound state snapshot. |
| Commit/apply | The one mutation path that transfers ownership, advances lifecycle, or changes an authoritative fact. |
| Outcome | Typed description of the committed consequence needed by callers, orchestration, presentation, or tests. |
| Validation/audit | Local and cross-owner checks that reconstruct truth rather than trusting cached claims. |

This grammar is a design interface, not a requirement to manufacture empty types. A small single-owner action
may combine resolution and validation, or return a simple outcome. What must remain explicit is the distinction
between observation, prediction, authorization, and mutation.

Callers should not need privileged field access or before/after whole-state diffing to understand a normal
operation. Prefer narrow canonical projections for important decision inputs and typed outcomes for important
committed consequences. Presentation, gameplay actors, tests, and future automation should consume those same
surfaces rather than reimplementing domain rules.

### Agent operability properties

Agent ergonomics is an architectural quality of the system, not a harness convenience. A well-shaped subsystem
minimizes the reasoning and tool calls required to reach a correct consequential action while preserving the
same authority and safety boundaries used by every other caller.

Optimize for these properties:

| Property | Architectural consequence |
| --- | --- |
| Bounded orientation | A cold start reaches the relevant authority, owner, operation, and proof without repository-wide enumeration. |
| Semantic addressability | A task can be located by authority layer, owner, operation stage, crossed flow, and proof level rather than by ambiguous feature vocabulary. |
| Local completeness | The controlling definition/state, canonical operation, typed rejection, durable continuation identity, and adjacent proof are discoverable from one bounded owner/edge cone. |
| Semantic compression | Production surfaces expose domain meaning directly when legitimate callers would otherwise reconstruct the same formula, blocker, or consequence from raw fields. |
| Monotonic accretion | New capability extends existing vocabulary, owners, edges, operation stages, and evidence wherever their semantics already fit instead of adding a parallel mini-architecture. |
| Cheap falsification | The smallest wrong nearby behavior has a focused proof that fails close to the controlling abstraction. |
| Explicit uncertainty | Missing reachability, absent observation, bounded search, and unsupported modeling are represented as such rather than guessed through implementation detail. |

Do not optimize call count by weakening revision checks, bounds, typing, or authority. The objective is fewer
unnecessary calls because each semantic surface is more complete, not fewer checks.

### System address and search anchors

Every consequential change should be expressible as one control coordinate:

```text
authority / owner / stage / flow / proof
```

The coordinate is intentionally orthogonal. For example, one mining task may be about current reachability,
`MiningState`, validation, information plus matter custody, and a boundary proof. Another may share the mining
feature name while the task actually concerns authored intent, `GeologicalKnowledgeState`, observation,
information flow, and gameplay evidence.

Once the coordinate is known, preferred search anchors are stable semantic nouns and symbols: authority
heading, owner type/module, request/resolution/validated token, durable job or record identity, typed error,
outcome, and adjacent test. Source layout should make those anchors cheap to follow. Repeated need to search by
incidental field names, inspect whole-state dumps, or infer success from unrelated diffs is evidence that the
control surface or locality is weak.

### Consequential operation lifecycle

Use one conceptual lifecycle for consequential work. Stages that add no value for a simple operation may
collapse, but stages must not change meaning between subsystems:

```text
request / intent
    -> resolve / decide
    -> validate / authorize
    -> commit / apply
    -> durable work or immediate authoritative state
    -> tick / scheduled continuation when needed
    -> typed outcome, completion, claim, or assessment
```

- **Request** carries caller intent and stable domain identities, not mutable authority.
- **Resolve/decide** computes physical consequences, candidate providers, bottlenecks, or plans without mutation.
- **Validate/authorize** binds all mutable preconditions needed for the promised atomic commit.
- **Commit/apply** is the ownership transition. It does not redo domain planning through a second ruleset.
- **Durable work** owns any custody, reservation, schedule, or provider trace that must survive after the command.
- **Tick/continuation** advances only persisted future-affecting state through visible orchestration.
- **Outcome/claim/assessment** exposes the consequential result at the abstraction level a legitimate caller
  needs, without requiring privileged reconstruction.

Do not add a generic command bus merely to make these stages look uniform. The shared lifecycle is semantic;
typed domain requests, resolutions, validations, and outcomes remain preferable to erased action payloads.

### Planning freshness, feasibility, and receipts

An agent should be able to retain useful reasoning without pretending that a derived answer is authoritative.
Three contracts keep that working model cheap and safe:

- **Freshness:** a retained projection, resolution, or plan is usable only while the authoritative dependencies
  that determined it remain unchanged. Validated tokens bind the mutable dependencies needed for commit. When a
  legitimate caller benefits from retaining an expensive read-side result across other actions, prefer narrow
  owner revisions, dependency stamps, or equally explicit invalidation semantics over a global world revision.
- **Feasibility:** when one requested dimension is monotonic and production already computes its limiting
  quantities, expose the useful bound or bottleneck through the narrowest domain resolver/assessment. Repeated
  binary search or validator probing merely to discover a maximum feasible batch, duration, capacity, or rate is
  control-surface debt. Candidate preferences and search order remain caller policy.
- **Receipt:** after a consequential commit or tick, return or expose the stable identity, schedule, lifecycle,
  and consequential deltas needed to continue the operation. A caller should not need a whole-state rescan just
  to learn which admitted job exists, what completed, or which owner-local facts changed.

For custody transitions, prefer an **owner landing receipt** over a coordinator-specific reconstruction. If an
inventory ingress already determines which persistent lot identity survives insertion/coalescing, that identity
is the inventory owner's semantic result. A production completion, mining claim, salvage operation, or future
logistics delivery may compose that receipt with its own job/route identity instead of inventing a separate
"produced lot" concept. The receipt should describe contribution-to-surviving-identity, not imply that a new
record was allocated when the matter merged into an existing one.

Current examples follow that rule: reserved inventory deposits return surviving lot identities; production
completion pairs each exact output contribution with its surviving identity per stream, and mining claim pairs
its exact claimed output with its surviving identity. Direct food/fluid admission similarly returns its
already-resolved completion tick so the caller can continue without rereading work state solely to rediscover
the schedule.

A receipt or dependency stamp is evidence about authoritative state, not another state owner. Callers may cache
it as disposable working memory. If a later command or tick can affect a dependency, refresh the relevant
projection before using it again; if the only safe refresh rule is "reread the entire world", the control
surface is insufficiently local.

Do not manufacture outcomes for symmetry. A commit result is sufficient when it exposes every consequential
fact the caller cannot already name and may legitimately need next. Returning `()` is appropriate when the
caller already holds the durable target identity, the change is immediate, no new schedule/custody/identity is
created, and any important non-obvious consequence is already represented by the validated token or another
canonical outcome. Conversely, a receipt is warranted when commit chooses or creates a persistent identity,
starts delayed work whose schedule is not otherwise exposed, resolves merge/routing identity, or produces a
cross-owner consequence that cannot be reconstructed from the caller's existing stable identities.

Prefer exposing useful precomputed continuation on the validated token when it exists before commit. For
example, a timed start token may expose its exact `work()`/schedule while completion later emits a distinct
typed outcome. This avoids returning the same information twice merely to standardize signatures.

Typed rejection should identify the causal domain precondition and the identities or expected/actual quantities
needed to understand it. Production errors do not prescribe strategy. An actor or UI may classify the same
typed blocker as resize, replenish, wait, repair, reroute, gather information, or stop according to its own
policy. Do not introduce one global blocker enum merely to encode those policy choices.

### Planning graphs and horizon

Long-horizon planning becomes cheaper when three different graphs remain explicit instead of being collapsed
into one notion of "reachability":

| Graph | Question | Source of truth | Lifetime |
| --- | --- | --- | --- |
| Possibility graph | What authored transformations, constructions, upgrades, recoveries, providers, and carrier relationships could participate in a route? | Immutable validated registries and their rebuildable reverse indexes | Registry lifetime |
| State graph | What actually exists now, where is it owned, and what condition/custody/support/schedule relationships currently hold? | Authoritative `AppState` owners | Persisted continuation |
| Opportunity graph | Which concrete operations are legitimately observable and feasible now, and what currently blocks nearby candidates? | Read-only production projections/resolvers over registries plus actor-visible current state | Disposable and freshness-bound |

The possibility graph is domain topology, not ordinary reachability. A process may have authored inputs,
outputs, provider requirements, and a physically valid resolver while its required infrastructure is not
ordinarily obtainable. [`STATUS.md`](STATUS.md) remains the authority for that distinction. Likewise, the state
graph may contain controlled-fixture infrastructure that an ordinary actor could not have acquired.

Use the narrowest observation shape that owns the queried meaning:

- **Exact read:** read a known authoritative record/definition by stable identity when the caller already knows
  what it wants. A semantic facade adds no value merely to report condition, mass, capacity, schedule, or another
  already-authoritative field.
- **Caller enumeration:** iterate a bounded/stable authored or observable collection when inclusion/ranking is
  genuinely policy, reporting, or exploratory choice. The owner should not absorb the caller's preference.
- **Semantic query:** add a typed projection/index when the inclusion rule itself is reusable domain meaning
  that callers otherwise copy, especially across registries/owners. Examples include authored producers of a
  commodity, nominal definitions satisfying capability requirements, or process execution-family ownership.
- **Concrete resolver:** once stable runtime identities are selected, use the canonical owner resolver to
  incorporate condition, support, custody, energy, knowledge, or other mutable facts.

This hierarchy prevents agent ergonomics from degenerating into accessor proliferation. Raw iteration is debt
only when legitimate callers repeatedly reconstruct the same semantic relationship that the domain can state
more directly.

#### Query completeness contract

Any discovery/query surface from which a caller may reason about absence must make its scope and completeness
legible. Prefer one of these shapes:

- **Exhaustive for a declared scope:** every matching item in that exact immutable/observable domain is returned
  in deterministic order. Empty means no match exists within the declared scope.
- **Bounded partial:** the caller supplies or receives an explicit item/work/horizon bound; the result states
  that more candidates may exist and provides a deterministic continuation cursor when continuation is useful.
- **Sampled/exploratory:** the sampling policy and replay input are explicit. Absence is evidence only about the
  sample, never about production availability.

Do not silently truncate semantic queries. Stable ordering needs complete tie-breakers so continuation does not
skip or duplicate candidates. A cursor identifies position in a deterministic query result, not authorization;
for mutable opportunity queries it is also subject to the query's freshness contract. If current state changes
materially between pages, restart or reject continuation rather than merging pages from different state views.

Prefer goal-directed exhaustive queries over huge global catalogs when the authored set is naturally bounded.
Pagination is useful only when the real result can grow enough to matter; do not introduce cursor ceremony for
small registry collections that can be returned completely and cheaply.

The opportunity graph should normally be generated lazily and goal-directed rather than materialized as one
universal action catalog. A query such as "what can produce this commodity?", "what can provide this typed
capability?", or "what is blocking this selected operation?" may use immutable reverse indexes and canonical
owner projections. The answer must retain typed domain identities and requirements so the caller can descend to
the real resolver/validator instead of executing an erased graph edge.

Long-horizon policy may chain possibility edges to form hypotheses. Near execution, each step must be regrounded
in actor-visible state, freshly resolved, then authorized through the canonical command. After commit, use the
receipt to advance the plan. This gives one safe planning ladder:

```text
goal
    -> authored possibilities
    -> observable current state
    -> fresh concrete opportunities / blockers
    -> policy choice
    -> validation / authorization
    -> commit / continuation
    -> receipt / feedback
    -> replan as needed
```

Reverse indexes over immutable definitions are semantic compression, not alternate rules. Prefer owner-specific
or cross-registry derived indexes that preserve typed relationships over a generic graph framework. They may be
built once with `Registries`, validated against their source definitions, and omitted from persistence. Do not
encode actor preference, hidden runtime truth, or mutable availability into them.

When topology spans several validated registries, the aggregate `Registries` assembly boundary is the natural
owner of the derived cross-registry index. Domain registries continue to own their definitions; the aggregate
may cache relationships already established by cross-validation. For example, one process-topology projection
could bind a `ProcessId` to exactly one resolver family, its typed energy role, and nominal matching definition
IDs without making any claim about runtime instances or ordinary acquisition.

### Claim-strength vocabulary

Use vocabulary that states how much has actually been established. Each row is stronger than the rows above it:

| Term | Claim |
| --- | --- |
| Direct authored edge | One definition declares an immediate production, assembly, upgrade, recovery, provider, or other typed relationship. Nothing is claimed about prerequisites beyond that edge. |
| Authored path | A transitive chain of direct authored edges connects a goal to declared roots or other hypotheses. Current world state and ordinary acquisition are not implied. |
| Ordinary reachability | The project has an ordinary-play acquisition path under the current scope contract. [`STATUS.md`](STATUS.md) owns this claim. |
| Current opportunity | Actor-visible current state plus canonical read-side semantics identify a concrete candidate worth considering now. It is not yet mutation authority. |
| Authorized action | Validation has bound the current mutable preconditions for one consequential commit. |
| Committed consequence | Canonical mutation/tick has occurred and a durable record or typed receipt identifies the result. |

Do not use `reachable`, `available`, or `can` as casual synonyms across these levels. Domain APIs may keep an
established name whose narrower semantics are already explicit, but new surfaces should name the weakest claim
they actually prove. In particular, a local definition predicate such as a declared assembly/acquisition route
must not be interpreted as transitive or ordinary reachability.

### Temporal control and batching

`advance_tick` is the authoritative temporal transition. Agent ergonomics does not justify a second clock path.
Distinguish two very different optimizations:

- **Batched stepping** executes the canonical tick repeatedly inside one bounded call and returns the ordered
  outcomes, selected matching outcomes, or a deterministic summary sufficient for the caller's declared stop
  condition. This can reduce caller loops, transport/tool calls, and repeated state inspection without changing
  simulation semantics.
- **Semantic fast-forward** computes the state after an interval without executing every canonical tick. This is
  a new simulation algorithm, not an API convenience. It is valid only when equivalence is proved for every
  affected phase, threshold crossing, random draw, passive loss, survival effect, suspension/resume transition,
  and externally observable outcome ordering.

Prefer batching before fast-forward. A batch must have an explicit maximum tick/horizon bound and must not hide
an outcome the caller needs to make an intervention. When the caller asks to stop on a domain event, evaluate
that stop condition only from legitimate ordered `TickOutcome` data or other actor-visible facts, never hidden
future state. If no such common caller exists, a local adapter/harness loop is sufficient and no production API
is needed.

Known schedules are planning aids, not permission to jump over intervening semantics. A completion receipt may
let a caller choose an upper bound such as `completes_at`, while canonical ticking still determines whether an
earlier death, support loss, suspension, passive depletion, or other observable event changes the plan.

## Abstraction and dependency direction

Build upward through a stable tower:

```text
quantities + time + identity
        -> material/spatial/capability primitives
        -> authoritative subsystem owners
        -> cross-owner transactions and durable work
        -> tick orchestration and trusted-load graph validation
        -> player/agent projections and behavior evaluation
        -> external adapters and presentation
```

Lower layers define vocabulary and invariants; higher layers coordinate them. A lower layer must not acquire a
dependency on a higher-level workflow merely to make one feature convenient. Cross-owner coordinators depend on
owner APIs, not owner internals. Derived projections depend on authoritative state, never the reverse.

When a new feature does not fit this direction, first ask whether a missing primitive, owner operation, or
projection should be added lower in the tower. Avoid generic coordination layers that merely hide unresolved
ownership.

### Accretion path

Prefer extending the tower in this order:

```text
reuse vocabulary
    -> extend one owner or definition
    -> reuse or add one explicit cross-owner edge
    -> expose the narrow observation/resolution/blocker/outcome needed to control it
    -> integrate continuation/orchestration only when future state requires it
    -> add the cheapest distinguishing proof
    -> update the single authority page whose truth changed
```

This ordering is not a mandatory implementation sequence. It is a pressure against semantic entropy. A change
that requires a new owner, new generic action shape, new cache, new status vocabulary, new harness legality
model, and new broad test lane for one local behavior is probably attached at the wrong abstraction level.

Accretive work should leave the next change cheaper to understand than an equivalent change would have been
before it. Useful signs include a reusable typed edge, a canonical projection replacing repeated reconstruction,
a more local failure diagnostic, or a proof that future work can invoke instead of rebuilding a scenario.

### Where a new concept belongs

Classify a concept by lifecycle and authority before choosing a module or type name:

| Question | If yes | Default placement |
| --- | --- | --- |
| Is it immutable authored possibility, identity, physical/capability limit, or a reference between definitions? | It describes what may exist rather than what currently exists. | Owning registry/definition layer; validate cross-references during registry construction. |
| Does generated value affect future authoritative continuation after the current call? | It is durable state, custody, lifecycle, schedule, or identity. | Smallest existing runtime owner that can maintain the invariant; create a new owner only for an independent lifecycle boundary. |
| Is it fully derivable from definitions plus current authoritative state? | Persisting it would duplicate truth. | Read-only projection/assessment, resolver, or rebuildable index/cache with one reconstruction owner. |
| Is it a concrete prediction of one requested operation before mutation? | Caller needs consequences/bottlenecks for planning. | Typed `Resolved*`, `Plan`, `Outcome`, or equivalent read-only decision object near the domain derivation. |
| Does it prove one consequential transition is legal against mutable state? | It is authorization, not durable world truth. | Revision/state-bound `Validated*` token consumed by one commit. Do not serialize it as runtime progress. |
| Must custody, reservations, occupancy, providers, or timing survive after command admission? | The operation continues beyond the call. | Durable job/work record in the owner that controls that lifecycle; endpoint owners retain their own independent facts. |
| Does it only reconcile several owners without changing them? | It is evidence or accounting. | Read-only accounting/projection layer; never another custody store. |
| Is it strategy, candidate ordering, search budget, experiment setup, or reporting policy? | It changes how an actor/evaluator chooses or measures, not simulation legality. | Gameplay-evaluation/harness layer, explicitly separate from production semantics. |
| Is it filesystem/network/process/renderer/platform state or another external effect? | Repository simulation cannot own/replay it as internal state. | Adapter boundary or explicit durable external-work record when retry semantics require one. |

If more than one row appears to own the same fact, separate the concepts before implementing them. For example,
an authored process limit, a runtime machine condition, a resolved effective throughput, and a validated start
authorization are four different facts even when one gameplay action uses all four.

## Ownership

- Keep consequential fields private to the smallest owner that can maintain their invariants.
- Update synchronized collections and reverse indexes through one owner operation.
- Cross-owner work coordinates owner APIs; it does not patch another owner's storage.
- Persist generated IDs, ownership relationships, schedules, and other facts that affect continuation.
- Tests and tools follow production ownership rules; they do not introduce alternate mutation paths.

### Accretive ownership

Prefer extending an existing owner with one new durable fact or operation over introducing a neighboring owner
for the same concept. Create a new owner only when the fact has an independent lifecycle, identity, persistence,
or invariant boundary that cannot be maintained coherently by an existing owner.

Every new persisted fact should have one obvious answer to each question: who creates it, who may change it,
who may destroy or release it, how it is reconstructed or validated on load, and which public projection is safe
for callers. If two modules can both answer those questions, ownership is not yet resolved.

## Mutation and failure

Use validate/commit for fallible multi-owner work:

```text
validate_*(&state, ...) -> Validated*
Validated*::commit(self, &mut state)
```

Validation resolves preconditions before consequential mutation. A validated token binds the state it
checked and rejects stale commits where required. Cross-owner commits must perform every recoverable
conflict check before their first authoritative mutation. After that point, remaining owner writes are
infallible prevalidated applies (with invariant assertions), not new domain-error branches.

Use decide/apply when a read-heavy decision produces a narrow write:

```text
decide_*(&state, ...) -> Plan / Outcome / Delta
apply_*(&mut state, plan)
```

Single-owner code may mutate directly only when every return path preserves that owner's invariants and
indexes.

Ordinary domain rejection returns typed errors. Failed consequential operations do not partially commit the
promised effect. Rejection preserves IDs, indexes, reservations, schedules, and other operation-owned state
unless mutation of that state is itself the explicit failure contract.

## Determinism

Authoritative results depend only on immutable definitions, serialized runtime state, ordered explicit
inputs, state-owned randomness, and explicitly modeled external snapshots.

Deep Hearth claims semantic deterministic continuation for authoritative simulation state and `TickOutcome`
values when the validated registries, serialized `AppState`, ordered external commands, and persisted random
state are identical. Authoritative physics and state transitions use checked integer arithmetic, so supported
platforms do not acquire a separate floating-point simulation ruleset. The claim does not cover byte-identical
adapter encodings, renderer/frame output, wall-clock execution time, or continuation across different save or
registry schemas.

- Result-affecting randomness comes from persisted state-owned streams or an explicit state-owned input.
- Order-sensitive work uses stable collections or explicit sorting with complete tie-breakers.
- Wall-clock time, filesystem enumeration, hash iteration, UI timing, thread scheduling, and ambient entropy
  do not decide simulation results.
- Parallel work must restore deterministic aggregation and commit order.
- Top-level simulation order remains visible in one orchestration surface.

## Persistence and adapters

- Save/load preserves every value required for supported continuation.
- `AppState` is serializable but is not a public deserialization target. Untrusted bytes decode only through
  `LoadedSaveEnvelope`; `into_state` is the promotion boundary that checks exact schemas, rebuilds derived
  indexes, and validates the complete state graph before returning runtime state.
- Derived indexes may be omitted from persistence only when they rebuild deterministically and validate
  before use.
- Required references validate at the trusted-load boundary; optional references are explicit.
- Core systems perform no implicit filesystem, network, process, renderer, or platform IO.
- External effects occur behind adapters after internal state is valid, or through an explicit durable work
  record when retry/recovery semantics require one.

## Invariants

Validate authoritative relationships where applicable:

- registry and runtime references;
- forward/reverse index agreement;
- exclusive ownership and custody;
- lifecycle, schedule, occupancy, and reservation agreement;
- transaction atomicity and unchanged state on rejection;
- generated identity ownership and monotonic cursors;
- deterministic selection and ordering;
- definition/runtime separation;
- serialization completeness and derived-data consistency;
- external-effect boundaries.

Cheap invariants run at ordinary runtime boundaries. Exhaustive graph and physics validation runs at trusted
load and explicit audit boundaries.

## Cross-owner flow discipline

Reason about cross-system behavior as transfers over explicit edges. The important edge kinds are matter,
fluid, stored/modelled energy, player attention and survival expenditure, information/authorization, structural
support/load, reservations/occupancy, identity, and schedule ownership.

For every consequential edge:

- identify the owner before and after the transition;
- make admission capacity and exclusivity explicit;
- preserve exact represented quantities or an explicit modeled sink/source;
- persist any custody that survives beyond the command;
- expose enough outcome information to identify what moved or changed;
- when the destination owner resolves persistent identity during ingress, propagate that landing identity far
  enough for legitimate continuation rather than forcing callers to rediscover it by scanning the destination;
- validate the edge from both owners at trusted load when continuation depends on it.

This is the preferred way to analyze a new mechanic or bug: trace the affected edges first, then inspect the
owners at their endpoints. It is usually cheaper and more reliable than reading every module involved in the
broader feature area.

## API and representation rules

- Prefer concrete structs and exhaustive project-owned enums for closed vocabularies.
- Match project-owned enums explicitly. Wildcards are for genuinely open external vocabularies.
- Map closed records explicitly enough that a new field cannot silently disappear in another
  representation.
- Group wide records by ownership concern when it clarifies invariants.
- Fallible multi-step operations return dedicated typed errors with useful precondition context.
- Pass the narrowest state access each phase requires.
- Keep public APIs intentional. Do not expose production operations only to support tests.
- Preserve the `AppState` public-read/crate-private-write split; expose a semantic command or narrow projection
  instead of returning mutable owner state.
- Prefer a small read surface that answers control questions directly over exposing raw collections for callers
  to reconstruct domain meaning.
- Keep prediction and mutation separable where callers benefit from planning, diagnostics, or counterfactual
  evaluation; the prediction must use the same authoritative rules as the eventual mutation.
- Return stable domain identity and consequential deltas/outcomes when callers otherwise would need to infer
  success by rescanning unrelated state.

### Semantic locality and control-surface debt

Keep rules close to the fact that makes them authoritative. A legitimate caller should normally need only the
owner, the explicit crossed edge, and their adjacent proofs to answer a control question. Cross-owner work can
span several modules, but the reason for each hop should be an ownership handoff, not duplicated derivation.

Treat the following as concrete control-surface debt signals when they recur:

- multiple callers copy the same threshold, provider selection, physical formula, or legality rule;
- callers need privileged mutable access or hidden state to answer an ordinary planning question;
- success or failure is inferred from a whole-state diff instead of a typed result;
- a durable operation lacks one stable identity for continuation, inspection, claim, cancellation, or recovery;
- a typed error identifies only a generic failure while the owner already knows the actionable blocker;
- a local contract routinely requires a broad gameplay/audit run to diagnose because no focused proof exists;
- tests or harnesses use a second mutation path because the production boundary is too awkward to reuse.

Repair the narrowest owner or edge that removes the repeated reconstruction. Do not answer these signals with a
universal reflection API or generic command bus.

## Naming

| Role | Form |
| --- | --- |
| keyed lookup | `get_*` |
| conditional scan | `find_*` |
| final derivation | `resolve_*` |
| accessor | noun form such as `status()` |
| constructor | `new()` |
| aggregate assembly | `build_*` |
| runtime insertion/removal | `insert_*`, `remove_*` |
| authored registration | `register_*` |
| predicate | `is_*`, `has_*`, `can_*` |
| read-only decision | `decide_*` returning `*Plan` / `*Outcome` / `*Delta` |
| decided mutation | `apply_*` |
| checked command | `validate_*` returning `Validated*` when appropriate |
| validated mutation | consuming `commit` |

Reserve `destroy_*` for consequential destruction and `delete_*` for literal external deletion. Do not add
`execute_*`, `perform_*`, or `attempt_*` for roles already covered above.

## Source and comment contracts

Use established role suffixes such as `_execution`, `_integration`, `_loader`, `_ui`, and `_adapter` when a
subsystem spans files. Every maintained Rust module under `src/` or `tests/` starts with a concise `//!` statement
of purpose or ownership. Unit-test bodies live in adjacent test files as defined by [`TESTING.md`](TESTING.md).

Treat an owner or overlay entry module as a local semantic index. It should make the outward control surface and
major internal roles discoverable through concise module purpose, deliberate re-exports, and role-oriented
submodules. When a file becomes unwieldy, split by ownership concern, operation stage, durable lifecycle, or
independent physical derivation rather than arbitrary line count. Keep orchestration at the entry point only
when seeing that sequence is itself part of the contract; move dense implementation behind named roles.

Avoid catch-all `utils`, `helpers`, `common`, or generic `manager` modules for domain behavior. A reusable helper
belongs with the smallest vocabulary/owner whose invariant explains it. A caller should have one obvious import
and search route for a canonical operation instead of several equivalent aliases spread across the tree.

Keep a comment only when it preserves information the code does not state clearly on its own, such as:

- ownership or authorization constraints;
- result-sensitive ordering, precision, or arithmetic rationale;
- safety assumptions and invariant dependencies;
- durable model boundaries or tradeoffs that explain why a simpler implementation would be wrong.

Do not restate syntax, narrate chronology, preserve superseded approaches, record debugging sessions, or leave
commented-out production code. State present constraints directly instead of describing how the implementation
arrived there.

Prefer direct data structures and static dispatch in core systems. Add dynamic dispatch, generic registries,
background machinery, or other coordination layers only when the behavior is genuinely open or dynamic and
the ownership, failure, and verification contracts justify the complexity.
