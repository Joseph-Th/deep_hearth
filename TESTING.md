# Testing

**Role:** Local verification and test-organization authority.

Use [`README.md`](README.md) for routing, [`STATUS.md`](STATUS.md) for scope, and [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) for automated-player evidence. Use the smallest lane that proves the changed contract.

## Fast path

| Need | Command |
| --- | --- |
| Documentation/contracts | `python tools/check_authority_docs.py` |
| Build-free edit loop | `python ci.py quick` |
| CI/test tooling contracts | `python -m unittest tools.test_ci -q` |
| One exact test | `python tools/run_test.py <qualified-name-or-unique-substring>` |
| One owner/subsystem group | `python tools/run_test.py --suite <qualified-prefix-or-substring>` |
| Gameplay contracts | `python ci.py gate --gameplay contracts` |
| Focused gameplay | `python ci.py gate --gameplay <scope>` |
| Compile-only proof when no executable test fits | `python ci.py gate` |

Routine iteration is `quick` while editing, then **one** build-producing proof. A passing Rust test is compile proof for its target. Use the default `gate` only when no executable test fits or a production-only cfg path changed. Do not stack an exact unit test and focused gameplay for the same claim.

Use `python tools/run_test.py --list <substring>` for build-free discovery and the printed `repair:` command after failures. `run_test.py` owner-shards exact and single-owner suites; switching owners selects another Rust artifact, so keep one repair loop on one owner. Cargo check/test artifacts also differ, so do not precheck an executable test.

## Escalation lanes

| Need | Command |
| --- | --- |
| Broad core/gameplay checkpoint | `python ci.py audit --all` |
| Production Clippy | `python ci.py gate --lint` |
| Gameplay exploration | `python ci.py report [--scope <scope>]` |
| Changed-source complexity review | `python ci.py bca [--path <scope>] [--since <revision>]` |

`quick` is build-free. `gate` runs one build lane. `audit` is a deliberate checkpoint, not an edit-loop step. Specialized gates are `--shaders`, `--rustdoc`, and `--soak`; scoped audits are `audit --core` and `audit --gameplay`.

## Evidence ladder

Stop at the first level that completely proves the changed claim:

1. **Owner:** exact local success/rejection, arithmetic, lifecycle, or invariant.
2. **Boundary:** one crossed ownership/custody edge, including atomicity or stale-state rejection where relevant.
3. **Continuation:** save/load replay or scheduled continuation for future-affecting state.
4. **System/gameplay:** focused interaction proof when behavior depends on several owners.
5. **Audit/exploration:** broader deterministic checkpoint or bounded sampling for cross-system uncertainty.

A local test does not establish a cross-owner or player-level claim. Projection APIs cover feasible, limiting, infeasible, and stale cases against canonical semantics.

### Failure triage map

| Failure | Start with |
| --- | --- |
| Unexpected rejection/stale commit | Resolver, typed error, bound revisions/preconditions. |
| Rejection changed state | Commit boundary, IDs, indexes, reservations, schedules. |
| Conservation mismatch | First crossed custody edge and accounting projection. |
| Trusted-load divergence | `LoadedSaveEnvelope::into_state`, validator, first differing continuation. |
| Tick/job mismatch | Durable work record, schedule, owner state, `TickOutcome`. |
| Gameplay choice changed | Focused scope first; replay sampled roots only when needed. |
| Capability unreachable | [`STATUS.md`](STATUS.md) and the acquisition path. |

Widen only when the evidence crosses another owner or runtime boundary.

## Complexity review

`bca.toml` and `.bca-baseline.toml` own the `quick` complexity ratchet. Use `python ci.py bca` for changed code and `--hotspots` only for existing concentration. Refactor for clarity, not a score. [`tools/README.md`](tools/README.md) owns optional diagnostics.

## Unit tests

Keep unit tests with or adjacent to the owner in `*_tests.rs` or `mod_tests.rs`. Prefer the smallest
deterministic fixture and the production operation being proved.

Assertions establish durable semantics:

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

Focused probes own nearby contracts when they compile nearly the same harness graph; use a separate contract target only when materially narrower. Gameplay targets share one `test-gameplay` feature shape. Reports use examples and keep report-only formatting out of tests when measurement justifies the split.

Routine gameplay verification combines maintained witnesses with one fresh replayable organic case. Reports use four organic cases plus broader agency search. Failures print replay roots. [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) owns actor/evidence rules.

## Completion

Run only the lane required by the changed contract. Documentation-only work uses `check_authority_docs.py`; CI/test-tooling changes also run `python -m unittest tools.test_ci -q`. Every additional build must prove a distinct contract.
