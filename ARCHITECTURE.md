# Architecture

This document owns repository-level engineering law. [`TECHNICAL_DESIGN.md`](TECHNICAL_DESIGN.md) owns
Deep Hearth-specific subsystem contracts, [`STATUS.md`](STATUS.md) owns capability presence, and
[`TESTING.md`](TESTING.md) owns verification.

## Program model

Deep Hearth uses an explicit Registry / AppState / Record / System model.

- **Registries** contain immutable validated definitions and lookup tables.
- **AppState** contains generated mutable state that must survive persistence boundaries.
- **Records** contain typed identity, ownership, lifecycle, references, local values, and version state.
- **Systems** resolve, validate, decide, and mutate through canonical paths.
- **Indexes/caches/projections** are derived data with one synchronization or reconstruction owner.
- **Adapters** own external resources and side effects.

Definitions describe what may exist. Runtime records describe what does exist. Do not place mutable
progress in definitions or treat derived data as independent truth.

## Ownership

- Consequential fields remain private to the smallest owner that can keep them coherent.
- Collections and reverse indexes that must agree update through one owner operation.
- Cross-owner work coordinates owner APIs; one owner does not patch another owner's storage.
- Generated IDs, ownership relationships, schedules, and other future-affecting facts are authoritative
  state and persist when continuation requires them.
- Tests and tools use production ownership rules rather than privileged mutation paths.

## Canonical mutations

Use validate/commit for fallible multi-resource work:

```text
validate_*(&state, ...) -> Validated*
Validated*::commit(self, &mut state)
```

Validation resolves mutable preconditions before consequential mutation. The consumed token binds the
state it checked and rejects stale commits where required.

Use decide/apply when a decision reads more state than it mutates:

```text
decide_*(&state, ...) -> Plan / Outcome / Delta
apply_*(&mut state, plan)
```

Single-owner work may mutate directly when every return path preserves that owner's invariants and
synchronized indexes.

## Determinism

Authoritative results depend only on immutable definitions, serialized runtime state, ordered explicit
inputs, state-owned randomness, and explicitly modeled external snapshots.

- Result-affecting randomness is state-owned or explicitly injected from a state-owned stream.
- Order-sensitive work uses stable collections or explicit sorting with complete tie-breakers.
- Wall-clock time, filesystem enumeration, hash iteration, UI timing, thread scheduling, and ambient
  entropy do not decide simulation results.
- Parallelism may improve throughput but must restore deterministic aggregation and commit order.
- Top-level simulation order remains visible in one orchestration surface.

## Persistence and adapters

- Save/load preserves every value required for supported continuation.
- Derived indexes may be omitted only when they rebuild deterministically and are validated before use.
- Required references validate before trusted runtime use; optional references are explicit.
- Core systems do not perform implicit filesystem, network, process, renderer, or platform IO.
- External side effects occur behind adapters after internal state is valid, or through an explicit
  durable work record when retry/recovery semantics require one.

## Invariants

Maintain explicit validation for authoritative relationships, including as applicable:

- registry and record references;
- forward/reverse index agreement;
- exclusive ownership and custody;
- lifecycle, schedule, occupancy, and reservation agreement;
- transaction atomicity and unchanged state on rejection;
- generated identity ownership and monotonic cursors;
- deterministic selection/order;
- definition/runtime separation;
- serialization completeness and derived-data consistency;
- explicit external-effect boundaries.

Cheap invariants belong in ordinary runtime boundaries. Exhaustive graph/physics validation belongs at
trusted-load and explicit audit boundaries.

## APIs and representation

- Prefer concrete structs and exhaustive project-owned enums for closed vocabularies.
- Match project-owned enums explicitly; wildcard handling is for genuinely open external vocabularies.
- Map closed records explicitly enough that a new field cannot silently disappear in another
  representation.
- Group wide records by ownership concern when it clarifies invariants.
- Fallible multi-step operations return dedicated typed errors with useful precondition context.
- Pass the narrowest state access a phase needs.
- Public APIs are intentional. Do not expose production helpers only for tests.

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

Reserve `destroy_*` for consequential destruction and `delete_*` for literal external deletion. Do not
add `execute_*`, `perform_*`, or `attempt_*` names for roles already covered above.

## Source organization

Multi-file subsystems use established role suffixes such as `_execution`, `_integration`, `_loader`,
`_ui`, and `_adapter`. Each `src/` file begins with a concise `//!` purpose statement. Unit-test bodies
live in adjacent test files as defined by [`TESTING.md`](TESTING.md).

Comments explain hidden constraints, ordering, invariants, or non-obvious intent. Remove commented-out
code, obsolete production paths, and stale documentation.

Prefer direct data structures and static dispatch in core systems. Add dynamic dispatch, generic
registries, background machinery, or other coordination layers only when the behavior is genuinely open
or dynamic and the ownership, failure, and test contracts justify the complexity.
