# Architecture

This document owns Deep Hearth's repository-level engineering architecture and coding law. `TECHNICAL_DESIGN.md` owns project-specific physical and technical design; `STATUS.md` owns implemented capability; `TESTING.md` owns test organization and validation commands; `GAME_DESIGN.md` owns product intent.

## Program model

Deep Hearth uses an explicit Registry / AppState / Record / System model.

- **Registries** hold immutable definitions and validated lookup tables loaded before runtime use.
- **AppState** holds generated mutable state required across execution and persistence boundaries.
- **Records** hold typed identity, ownership, lifecycle, references, local values, timestamps, and version state.
- **Systems** validate requests, derive outcomes, mutate through canonical paths, and enforce invariants.
- **Indexes/caches/projections** are derived structures with one synchronization or reconstruction owner; they are not independent truth.
- **External resources** such as files, processes, handles, or service clients remain behind adapter boundaries.

Static definitions describe what can exist. Runtime state records what does exist. Mutable progress does not live in registry definitions, and future-affecting generated state must be serializable when continuation crosses persistence.

## State ownership

- Consequential fields remain private to the smallest owner that can keep them coherent.
- Collections that must agree are private fields of one owner and update through atomic owner operations.
- Cross-owner work is coordinated by a system or higher orchestration boundary rather than one owner patching another owner's storage.
- A derived value is recomputed on demand or maintained by one explicit owner. Stale derived state must not silently become decision authority.
- Tests, importers, migrations, and administrative tooling use the same owner operations as production code.

## Canonical operations

Every consequential operation class has one semantic production path.

### Decide then apply

Use when a decision reads broader state than it mutates:

```text
decide_*(&state, ...) -> Plan / Outcome / Delta
apply_*(&mut state, plan)
```

The decision phase is read-only except for explicitly supplied deterministic randomness. Apply consumes the decision in the same pipeline; it is not an implicit queue.

### Validate then commit

Use for fallible multi-resource operations:

```text
validate_*(&state, ...) -> Validated*
Validated*::commit(self, &mut state)
```

Validation resolves references, permission, lifecycle, range, ownership, capacity, arithmetic, and other mutable preconditions before the first consequential mutation. Commit consumes the authorization and rechecks stale dependencies where required.

Single-owner work may mutate directly when every return path preserves the owner and all synchronized indexes.

## Determinism

Authoritative results are a function of immutable registry definitions, serialized runtime state, ordered explicit inputs, state-owned random streams, and explicitly modeled external snapshots.

- Result-affecting randomness is state-owned or explicitly injected from a state-owned stream.
- Order-sensitive work uses stable collections or explicit sorting and complete tie-breakers.
- Wall-clock time, filesystem enumeration, hash iteration, UI timing, thread scheduling, and ambient entropy do not influence deterministic decisions.
- Parallel implementation may change throughput but not authoritative semantics; aggregation restores deterministic order.
- Top-level execution order remains visible in one orchestration surface and load-bearing ordering is documented there.

## Identity and references

- Persistent references use typed IDs with the narrowest suitable representation.
- Required references validate at construction, load/import, or operation-validation boundaries before trusted use.
- Optional references are explicit rather than sentinel IDs or magic strings.
- Display names and user-facing strings are not a recovery mechanism for authoritative identity.
- Generated ID cursors and synchronized ownership/index relationships are invariant-bearing state.

## Persistence and external effects

Save/load preserves every value needed for supported continuation. Derived indexes may be omitted only when they reconstruct deterministically from trusted durable records and are validated after reconstruction.

- Core systems do not perform implicit filesystem/network/process IO.
- External side effects occur after internal state is valid or through an explicit durable work record when recovery/retry semantics require it.
- User-facing persistence failures remain distinguishable from domain validation failures.
- Do not add silent compatibility defaults for missing future-affecting state merely to make older data appear loadable.

## Runtime invariants

At minimum, maintain invariants for:

1. registry/reference validity;
2. record-reference validity;
3. index completeness and uniqueness;
4. exclusive ownership/custody;
5. lifecycle agreement with active/scheduled/indexed membership;
6. transaction atomicity;
7. generated identity/location ownership;
8. deterministic selection and ordering;
9. definition/runtime separation;
10. serialization completeness;
11. derived-data consistency;
12. explicit external-effect boundaries;
13. unchanged authoritative state on rejected operations except explicit diagnostics/audit.

Maintain `validate_invariants(state)` for cheap structural checks at the top-level pipeline boundary. Add the relevant invariant assertion with any new invariant. `TESTING.md` owns the deterministic soak that exercises them over long horizons.

## Representation and APIs

- Prefer concrete structs and exhaustive project-owned enums over string-keyed dynamic behavior for closed vocabularies.
- Match project-owned enums explicitly; wildcard handling belongs only to genuinely open/third-party vocabularies.
- Map project-owned closed records explicitly enough that a new field cannot silently disappear in another representation.
- Wide records should be grouped by ownership concern when that makes invariants and mappings clearer; do not hide unhandled fields behind broad update syntax.
- New fallible multi-step operations return dedicated typed errors with relevant precondition context.
- Pass the narrowest state context a phase needs: immutable access for decisions, owner-specific mutable access for mutation.
- Public APIs are intentional. Do not make production helpers public only to satisfy tests.

## Naming conventions

Use one project vocabulary:

| Purpose | Form |
|---|---|
| keyed lookup | `get_*` |
| conditional scan | `find_*` |
| final derivation | `resolve_*` |
| plain accessor | noun form such as `status()` |
| plain constructor | `new()` |
| aggregate assembly | `build_*` |
| runtime insertion/removal | `insert_*`, `remove_*` |
| authored registration | `register_*` |
| predicate | `is_*`, `has_*`, `can_*` |
| read-only decision | `decide_*` returning `*Plan`/`*Outcome`/`*Delta` |
| decided mutation | `apply_*` |
| checked command | `validate_*` returning `Validated*` when appropriate |
| resolved checked mutation | consuming `commit` |

Reserve `destroy_*` for destruction with consequential effects and `delete_*` for literal file/external-API deletion. Do not add new `execute_*`, `perform_*`, or `attempt_*` names for roles already covered above.

Multi-file subsystem suffixes use established roles such as `_execution`, `_integration`, `_loader`, `_ui`, and `_adapter`. Each `src/` file begins with a concise `//!` purpose statement and explains sibling relationships when the split is not obvious.

## Comments, warnings, and replacement

Comments explain hidden constraints, ordering, safety reasoning, invariants, or non-obvious intent. They do not narrate implementation history, add decorative banners, or retain commented-out code.

Classify dead code rather than suppressing it:

- intended production behavior must be wired into the canonical path;
- test-only fixtures/helpers belong under test configuration;
- obsolete behavior and its obsolete tests/docs are deleted.

Do not add fake production call sites, broad `allow(dead_code)`/lint suppression, public test shims, or historical compatibility layers merely to silence tooling. One implementation owns each concern unless an active external contract explicitly requires otherwise.

## Dependency and complexity boundaries

Prefer direct data structures and static dispatch in core systems. Add dynamic dispatch, generic registries, background machinery, or other architectural layers only when the behavior is genuinely open/dynamic and the ownership/failure/test contract justifies the added coordination cost.

Architecture rules are domain-neutral. Do not encode one feature's assumptions as global design law, and do not turn a specialized test/harness requirement into ordinary runtime architecture.
