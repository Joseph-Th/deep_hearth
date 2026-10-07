# Architecture

**Role:** Project-wide implementation rules.

Use [`README.md`](README.md) for routing,
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

Ordinary builds expose no whole-state cloning or equality for `AppState`. Unit tests and the explicit
`test-gameplay` evaluation feature may derive those traits for atomicity, determinism, replay, and
matched-counterfactual evidence only; they are not actor observation surfaces.

A value type may expose mutation when it is independently ownable and mutation is its own complete contract.
That does not authorize bypassing `AppState` ownership for generated simulation state.

## Agent-legible control grammar

Consequential subsystems use a common semantic vocabulary. Simple owners may collapse adjacent stages, but the
distinction between observation, prediction, authorization, and mutation remains explicit.

| Role | Contract |
| --- | --- |
| Definition | Immutable authored identity, limits, and references. |
| State/record | Generated authoritative fact required for continuation. |
| Projection/assessment | Read-only interpretation of definitions and current state; never another owner. |
| Resolution/decision | Concrete predicted consequence for one request without mutation. |
| Validation/authorization | State-bound proof that one consequential transition is currently legal. |
| Commit/apply | The canonical authoritative mutation. |
| Durable work | Persisted custody, reservation, provider trace, lifecycle, or schedule after admission. |
| Outcome/receipt | Stable identity and consequential result needed for continuation, presentation, or proof. |

The normal lifecycle is:

```text
request -> resolve/decide -> validate/authorize -> commit/apply
        -> durable work/tick when delayed -> outcome/claim/assessment
```

Do not manufacture types only for symmetry. A direct single-owner operation may combine stages when authority,
failure, and continuation remain unambiguous. Do not add a generic command bus around typed domain operations.

### Read-side contracts

Read surfaces expose domain meaning at the narrowest owner that knows it:

- **Exact read:** use a known record or definition by stable identity when its field is already the authoritative
  answer.
- **Semantic projection:** expose a typed assessment or derived index when legitimate callers would otherwise
  reproduce the same inclusion rule, physical formula, blocker, or relationship.
- **Concrete resolution:** bind selected runtime identities and current condition/custody/support/energy/
  knowledge facts before validation.
- **Player access:** player-facing authorization for a known runtime asset proves locality/custody before using
  that asset's condition, support, contents, occupancy, or capability state to select an error or outcome. Authored
  registry facts and already-authorized local or knowledge-scoped observations may be resolved first; unknown
  runtime identities still belong to their canonical owner error.
- **Freshness:** retained plans or projections are usable only while the authoritative dependencies that produced
  them remain unchanged. Use narrow revision/dependency stamps only when retention is useful.
- **Feasibility:** when production already computes a monotonic bound such as capacity, batch size, duration, or
  rate, expose that bound rather than forcing callers to discover it by repeated failure.
- **Receipt:** propagate owner-selected landing identity, schedule, or lifecycle result when continuation would
  otherwise require a destination rescan or whole-state diff.

A query from which callers may infer absence must state its semantics: exhaustive for a declared scope, bounded
partial with an explicit bound/continuation, or sampled with replayable inputs. Never silently truncate a
semantic query.

### Planning claim strength

Use the weakest term actually established:

| Term | Meaning |
| --- | --- |
| Direct authored edge | One definition declares one immediate typed relationship. |
| Authored path | A declared traversal connects authored edges; current world state is not implied. |
| Ordinary reachability | Normal play can acquire and execute the route; [`STATUS.md`](STATUS.md) owns this claim. |
| Current opportunity | Legitimate observable state plus canonical read-side semantics identify a concrete candidate. |
| Authorized action | Validation has bound the mutable preconditions for one commit. |
| Committed consequence | Canonical mutation/tick occurred and durable state or a typed receipt identifies the result. |

Do not use `reachable`, `available`, or `can` as synonyms across these levels.

### Temporal control

`advance_tick` is the only authoritative simulation-time mutation. Batching may call canonical tick semantics
repeatedly and stop on declared observable events; it may not skip hidden intermediate semantics. A true
fast-forward path would require a separately proved equivalent authoritative transition. The concrete stepping
contract lives in [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md#temporal-stepping-contract).

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

When a feature does not fit this direction, resolve whether the missing concept is a lower-level primitive,
owner operation, or projection. Avoid generic coordination layers that hide unresolved ownership.

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
inputs, and explicitly modeled external snapshots.

Deep Hearth claims semantic deterministic continuation for authoritative simulation state and `TickOutcome`
values when the validated registries, serialized `AppState`, and ordered external commands are identical.
Authoritative physics and state transitions use checked integer arithmetic, so supported
platforms do not acquire a separate floating-point simulation ruleset. The claim does not cover byte-identical
adapter encodings, renderer/frame output, wall-clock execution time, or continuation across different save or
registry schemas.

- No runtime stochastic owner is currently implemented. If result-affecting randomness is introduced, it must
  become explicit authoritative state or an explicit state-owned input before it can affect outcomes.
- Order-sensitive work uses stable collections or explicit sorting with complete tie-breakers.
- Wall-clock time, filesystem enumeration, hash iteration, UI timing, thread scheduling, and ambient entropy
  do not decide simulation results.
- Parallel work must restore deterministic aggregation and commit order.
- Top-level simulation order remains visible in one orchestration surface.

## Persistence and adapters

- Save/load preserves every value required for supported continuation.
- Complete `AppState` persistence is serialized only through `SaveEnvelope`; the runtime root itself is not a
  public serialization or deserialization target. This keeps hidden geology and complete world-resource truth
  behind the persistence boundary rather than ordinary read access; surface resources expose only exact-local
  observation publicly. Untrusted bytes decode only through
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
- when delayed work retains an endpoint assumption that is not recomputed into its completion transaction,
  block canonical mutations that would invalidate that assumption for the work interval; otherwise compose the
  changed endpoint into one authoritative completion projection rather than relying on admission-time state;
- expose enough outcome information to identify what moved or changed;
- when the destination owner resolves persistent identity during ingress, propagate that landing identity far
  enough for legitimate continuation rather than forcing callers to rediscover it by scanning the destination;
- validate the edge from both owners at trusted load when continuation depends on it.

Analyze cross-system behavior by tracing affected edges first, then inspect the endpoint owners. This avoids
repository-wide reading for a local ownership problem.

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

Keep rules close to the fact that makes them authoritative. Cross-owner calls correspond to ownership handoffs
rather than duplicated derivation. [`DIRECTION.md`](DIRECTION.md#control-surface-program) owns the
priority and refinement strategy for missing projections, blockers, receipts, and other control-surface debt.

## Naming

Use the workspace naming roles from `STANDARDS.md`. Project-specific read-side prefixes are:

- `assess_*` for canonical observable assessments or feasibility envelopes;
- `calculate_*` for exact read-only reconciliation/physical totals;
- `analyze_*` for structural or diagnostic derivation;
- `project_*` for disposable future consequences without mutation.

Consequential checked commands use `validate_*` and a consuming `commit` when state-bound authorization is
material. Crate-owned tick orchestration may use `decide_*` / `apply_*`. Reserve `destroy_*` for modeled
destruction and `delete_*` for literal external deletion.

## Source and comment contracts

Every maintained Rust module under `src/` or `tests/` starts with a concise `//!` purpose or ownership statement.
Use role-oriented modules and established suffixes such as `_execution`, `_integration`, `_loader`, `_ui`, and
`_adapter`. Entry modules expose the owner/control surface and major internal roles without becoming dense
implementation files. Split by ownership concern, operation stage, durable lifecycle, or independent physical
derivation, not arbitrary size. Avoid catch-all `utils`, `helpers`, `common`, and generic `manager` modules.

Keep a comment only when it preserves information the code does not state clearly on its own, such as:

- ownership or authorization constraints;
- result-sensitive ordering, precision, or arithmetic rationale;
- safety assumptions and invariant dependencies;
- durable model boundaries or tradeoffs that explain why a simpler implementation would be wrong.

Write comments and authority prose as present-tense contracts. Do not restate syntax, record chronology or
debugging sessions, preserve superseded approaches, leave TODO-style planning notes, or keep commented-out code.
Version control owns history; [`DIRECTION.md`](DIRECTION.md) owns future integration strategy.

Do not duplicate tunable authored values in comments or authority prose unless the exact value is itself part of
the documented contract. Prefer naming the owning definition, projection, or validation rule so content tuning
cannot silently stale documentation.

Prefer direct data structures and static dispatch in core systems. Add dynamic dispatch, generic registries,
background machinery, or other coordination layers only when the behavior is genuinely open or dynamic and
the ownership, failure, and verification contracts justify the complexity.
