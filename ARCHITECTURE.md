# Architecture

This page owns project-wide engineering rules. [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) owns subsystem
contracts, [`STATUS.md`](STATUS.md) owns capability presence, and [`TESTING.md`](TESTING.md) owns
verification.

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
checked and rejects stale commits where required.

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

- Result-affecting randomness comes from persisted state-owned streams or an explicit state-owned input.
- Order-sensitive work uses stable collections or explicit sorting with complete tie-breakers.
- Wall-clock time, filesystem enumeration, hash iteration, UI timing, thread scheduling, and ambient entropy
  do not decide simulation results.
- Parallel work must restore deterministic aggregation and commit order.
- Top-level simulation order remains visible in one orchestration surface.

## Persistence and adapters

- Save/load preserves every value required for supported continuation.
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

Cheap invariants belong in ordinary runtime boundaries. Exhaustive graph and physics validation belongs at
trusted load and explicit audit boundaries.

## API and representation rules

- Prefer concrete structs and exhaustive project-owned enums for closed vocabularies.
- Match project-owned enums explicitly. Wildcards are for genuinely open external vocabularies.
- Map closed records explicitly enough that a new field cannot silently disappear in another
  representation.
- Group wide records by ownership concern when it clarifies invariants.
- Fallible multi-step operations return dedicated typed errors with useful precondition context.
- Pass the narrowest state access each phase requires.
- Keep public APIs intentional. Do not expose production helpers only for tests.

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

## Source organization

Use established role suffixes such as `_execution`, `_integration`, `_loader`, `_ui`, and `_adapter` when a
subsystem spans files. Each `src/` file begins with a concise `//!` purpose statement. Unit-test bodies live
in adjacent test files as defined by [`TESTING.md`](TESTING.md).

Comments explain hidden constraints, ordering, invariants, or non-obvious intent. Remove commented-out code,
obsolete paths, and stale documentation.

Prefer direct data structures and static dispatch in core systems. Add dynamic dispatch, generic registries,
background machinery, or other coordination layers only when the behavior is genuinely open or dynamic and
the ownership, failure, and verification contracts justify the complexity.
