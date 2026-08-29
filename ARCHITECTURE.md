# Architecture

This page owns project-wide implementation rules. Use [`README.md`](README.md) for routing,
[`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) for subsystem semantics, [`STATUS.md`](STATUS.md) for runtime
scope, and [`TESTING.md`](TESTING.md) for verification.

The default shape is one authoritative owner, one canonical consequential mutation path, deterministic
replay from persisted state, and explicit adapter boundaries.

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

## API and representation rules

- Prefer concrete structs and exhaustive project-owned enums for closed vocabularies.
- Match project-owned enums explicitly. Wildcards are for genuinely open external vocabularies.
- Map closed records explicitly enough that a new field cannot silently disappear in another
  representation.
- Group wide records by ownership concern when it clarifies invariants.
- Fallible multi-step operations return dedicated typed errors with useful precondition context.
- Pass the narrowest state access each phase requires.
- Keep public APIs intentional. Do not expose production operations only to support tests.

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
