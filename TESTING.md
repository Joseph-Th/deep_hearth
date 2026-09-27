# Testing

**Role:** Local verification and test-organization authority.

Use [`README.md`](README.md) for routing,
[`STATUS.md`](STATUS.md) for scope, and [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) for automated-player
evidence semantics.

Use the smallest lane that completely proves the changed contract.

## Fast path

| Need | Command |
| --- | --- |
| Documentation/contracts | `python tools/check_authority_docs.py` |
| Build-free edit loop | `python ci.py quick` |
| CI/test tooling contracts | `python -m unittest tools.test_ci -q` |
| Compile-only production check | `python ci.py gate` |
| One exact test | `python tools/run_test.py <qualified-name-or-unique-substring>` |
| One owner/subsystem group | `python tools/run_test.py --suite <qualified-prefix-or-substring>` |
| Gameplay contracts | `python ci.py gate --gameplay contracts` |
| Focused gameplay | `python ci.py gate --gameplay <scope>` |
| Broad core/gameplay checkpoint | `python ci.py audit --all` |
| Production Clippy | `python ci.py gate --lint` |
| Gameplay exploration | `python ci.py report [--scope <scope>]` |
| Changed-source complexity review | `python ci.py bca [--path <scope>] [--since <revision>]` |

`quick` is build-free. `gate` runs one build-producing lane. `audit` is an explicit broad checkpoint and does
not belong in the ordinary edit loop.

Use `python tools/run_test.py --list` for build-free discovery. Exact selectors resolve to the smallest suitable
Cargo target, including owner-specific gameplay contract targets that are kept separate from play-like probe
binaries. Use `--check` for test-target type checking, `--lint` for focused test-target Clippy, and `--verbose`
only when captured output is needed. Compile-only checking and executable testing are alternative proof lanes:
do not run `gate` or `cargo check-fast` merely as a prerequisite to a test, because the extra build does not
replace that test target's own compilation/linking.

Specialized gates are `python ci.py gate --shaders`, `python ci.py gate --rustdoc`, and
`python ci.py gate --soak`. Scoped audits remain available as `python ci.py audit --core` and
`python ci.py audit --gameplay`.

## Evidence ladder

Stop at the first level that completely proves the changed claim:

1. **Owner:** exact local success/rejection, arithmetic, lifecycle, or invariant.
2. **Boundary:** one crossed ownership/custody edge, including atomicity or stale-state rejection where relevant.
3. **Continuation:** save/load replay or scheduled continuation for future-affecting state.
4. **System/gameplay:** focused interaction proof when behavior depends on several owners.
5. **Audit/exploration:** broader deterministic checkpoint or bounded sampling for cross-system uncertainty.

A local test does not establish a cross-owner or player-level claim. A projection that replaces caller-side
reconstruction should cover representative feasible, limiting, infeasible, and stale cases against canonical
production semantics.

### Failure triage map

| Failure | Start with |
| --- | --- |
| Unexpected rejection or stale commit | Resolver/validator, typed error, and bound revisions/preconditions. |
| Rejection changed state | Commit boundary plus owned IDs, indexes, reservations, and schedules. |
| Conservation mismatch | The first crossed custody edge and the corresponding accounting projection. |
| Trusted-load failure/divergence | `LoadedSaveEnvelope::into_state`, named validator, then the first differing continuation outcome. |
| Tick/job lifecycle mismatch | Relevant durable work record, schedule, owner state, and `TickOutcome`. |
| Gameplay choice/no-action changed | Deterministic focused scope first; use report replay roots only when investigating sampled organic behavior. |
| Capability appears unreachable | [`STATUS.md`](STATUS.md) plus the acquisition path; separate owner capability from ordinary reachability. |

Widen only when the evidence crosses another owner or runtime boundary.

## Complexity review

`bca.toml` and `.bca-baseline.toml` own the cognitive-complexity ratchet used by `python ci.py quick`.
Use `python ci.py bca` for changed code and `python ci.py bca --hotspots` only when reviewing existing
concentration. Refactor for ownership and control-flow clarity, not for a score.

Rust diagnostics answer named uncertainties only. [`tools/README.md`](tools/README.md) owns their commands and
limits.

## Unit tests

Keep unit tests with or adjacent to the owner in `*_tests.rs` or `mod_tests.rs`. Prefer the smallest
deterministic fixture and the production operation being proved.

Assertions should establish durable semantics:

- exact typed rejection and unchanged authoritative state where atomicity matters;
- resulting identity, quantity, lifecycle, ownership, or relationship on success;
- outcome/receipt agreement with durable state when continuation identity is created or selected;
- exact conservation across authoritative owners;
- persistence and deterministic continuation for state that survives load;
- authored values read from registries instead of copied balance constants.

Avoid assertions on human-readable error prose, wall-clock timing, incidental collection order/count, or copied
tuning values. Generated tests prove their declared bounded variation, not more. Soaks stay explicit and ignored
until the soak lane is requested.

## Gameplay evaluation

Focused gameplay probe targets are edit-loop compile surfaces. Larger owner contract suites use separate narrow
targets so an exact contract test does not compile the play-like probe or the consolidated gameplay audit.
All gameplay test targets keep one `test-gameplay` Cargo feature shape so moving between probe, contract, and
audit lanes reuses the same library artifact instead of fragmenting the incremental cache.
Repository-owned gameplay gates and the broad gameplay audit retain maintained witnesses and add one bounded
organic case per play-like probe. `ci.py` generates fresh replay roots for those cases and prints them with the
result; pass the printed roots back through `--variation-seed` and, where applicable, `--behavior-seed` to
reproduce a run. Reports use a broader bounded organic sample. Direct Cargo test invocation keeps deterministic
fallback roots. Actor/evidence rules are owned by [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md).

## Completion

Run only the lane required by the changed contract plus specialized evidence whose contract changed.
Documentation-only work runs `python tools/check_authority_docs.py`; CI/test-tooling changes also run
`python -m unittest tools.test_ci -q`. Do not staircase compile-only checks, exact tests, focused gameplay, and
broad audits after every edit; each additional build must prove a distinct changed contract. Broad audits are
deliberate checkpoints.
