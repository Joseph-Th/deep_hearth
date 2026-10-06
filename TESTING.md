# Testing

**Role:** Local verification and test-organization authority.

Use [`README.md`](README.md) for routing, [`STATUS.md`](STATUS.md) for scope, and [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) for automated-player evidence. Use the smallest lane that proves the changed contract.

## Fast path

| Need | Command |
| --- | --- |
| Documentation/contracts only | `python tools/check_authority_docs.py` |
| Build-free edit loop: format, complexity, docs, and local CI contracts | `python ci.py quick` |
| Build/link one selected test target without running it | `python tools/run_test.py --build <qualified-name-or-unique-substring>` |
| One exact test | `python tools/run_test.py <qualified-name-or-unique-substring>` |
| One owner/subsystem group | `python tools/run_test.py --suite <qualified-prefix-or-substring>` |
| Shared gameplay contracts | `python ci.py gate --gameplay contracts` |
| Focused gameplay | `python ci.py gate --gameplay <scope>` |
| Production-library type-check when no executable test fits | `python ci.py gate` |

Iteration is `quick`, then **one** proof. Run an executable proof directly whenever one fits; a separate test-target type-check only spends compiler time on an artifact the executable cannot reuse. `gate` is an optional production-only type-check when no executable contract fits and does not cover `cfg(test)`. Use `--build` only when execution is intentionally deferred. Do not stack equivalent build lanes, run all test targets, or append an audit to routine edits.

Use `run_test.py --list <substring>` for build-free discovery and prefer the smallest authoritative target. Exact owner/unit/contract tests stay deterministic. Focused gameplay adds one fresh replayable case and prints roots; `ci.py gate --gameplay <scope>` uses the same shape. Reports reuse focused artifacts when practical and otherwise use a report-only example. Exact/core library tests share one feature-minimal persistent `unit-test` artifact; gameplay fixtures compile only in smaller `test-gameplay` integration targets. Keep Cargo invocations serial and parallelize only build-free `quick` checks.

## Escalation lanes

| Need | Command |
| --- | --- |
| Broad core/gameplay checkpoint | `python ci.py audit --all` |
| Production Clippy | `python ci.py gate --lint` |
| Gameplay exploration | `python ci.py report [--scope <scope>]` |
| Changed-source complexity review | `python ci.py bca [--path <scope>] [--since <revision>]` |

`quick` is build-free, `gate` runs one build lane, and `audit` is an explicit checkpoint. Focused gameplay runs maintained witnesses plus one fresh replayable case. Broad gameplay audits rotate that one-case budget through one scope instead of sampling every scope. `report` owns broader exploration. `test-gameplay` is additive but belongs only to gameplay targets; core/unit tests stay feature-minimal. Negative feature gating is forbidden. Specialized gates are `--shaders`, `--rustdoc`, and `--soak`. Git-Wizard `quick`/`standard` stay build-free; `full` maps to opt-in `audit --all`.

## Evidence ladder

Stop at the first level that completely proves the changed claim:

1. **Owner:** exact local success/rejection, arithmetic, lifecycle, or invariant.
2. **Boundary:** one crossed ownership/custody edge, including atomicity or stale-state rejection where relevant.
3. **Continuation:** save/load replay or scheduled continuation for future-affecting state.
4. **System/gameplay:** focused interaction proof when behavior depends on several owners.
5. **Audit/exploration:** broader deterministic checkpoint or bounded sampling for cross-system uncertainty.

A local test does not establish a cross-owner or player-level claim. Projection tests cover feasible, limiting, infeasible, and stale cases.

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

`bca.toml` and `.bca-baseline.toml` own the `quick` complexity ratchet. Use `python ci.py bca` for a concise changed-code review and `--hotspots` when the full diagnostic report is useful. Refactor for clarity, not a score. [`tools/README.md`](tools/README.md) owns optional diagnostics.

## Unit tests

Keep unit tests adjacent to the owner in `*_tests.rs` or `mod_tests.rs`. Prefer the smallest deterministic fixture and production operation that proves the claim.

Assertions establish durable semantics:

- exact typed rejection and unchanged authoritative state where atomicity matters;
- resulting identity, quantity, lifecycle, ownership, or relationship on success;
- outcome/receipt agreement with durable state when continuation identity is created or selected;
- exact conservation across authoritative owners;
- persistence and deterministic continuation for state that survives load;
- authored values read from registries instead of copied balance constants.

Avoid assertions on error prose, wall-clock timing, incidental order/count, or copied tuning values. Generated tests prove only their declared bounded variation. Soaks stay explicit and ignored until requested.

## Gameplay evaluation

Each gameplay scope owns a focused probe artifact. The workshop agency counterfactual has its own target because it
is an evaluator, not part of the ordinary workshop probe. Generator, policy, and owner tests live in purpose-built
`<scope>_contracts` targets, so assertion edits do not rebuild the lived probe. `run_test.py` resolves exact and suite
selectors to owner targets build-free; `tools/gameplay_targets.py` owns routing. All targets share `test-gameplay` and one
Cargo cache. Report-only formatting stays out of frequent gate builds. `gate --gameplay contracts` remains the small
cross-scope contract target.

Focused gates and exact focused probes add one fresh organic case; broad audits rotate one through one scope. Reports own broader exploration; failures print a narrow reproduction command with replay roots. [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) owns actor/evidence rules.

## Completion

Run only the lane required by the changed contract. `quick` already proves the local CI/test-tooling contracts; every additional build must prove a distinct contract.
