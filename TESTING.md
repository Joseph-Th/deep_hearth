# Testing

**Role:** Local verification and test-organization authority.

Use [`README.md`](README.md) for routing, [`STATUS.md`](STATUS.md) for scope, and [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) for automated-player evidence. Use the smallest lane that proves the changed contract.

## Rust agent diagnostics

Use only for a named uncertainty left by source reading and focused tests:

- `cargo modules structure --lib --no-fns --no-traits --no-types --max-depth 4`; narrow with `--focus-on`.
- `cargo mutants --list --file src/lib.rs`; execute only narrow selected mutants, never `--in-place` on a dirty/shared tree.
- `cargo expand --lib simulation` for relevant derive/proc-macro expansion.

Diagnostics do not extend CI lanes, and mutation counts are not quality metrics.

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
| Production-library type-check when no executable test fits | `python ci.py gate --production` |

Iteration is `quick`, then **one** distinct proof. Prefer an executable proof; use `gate --production` only when none fits, and `--build` only when execution is intentionally deferred. Do not stack equivalent build lanes, all targets, or routine audits.

Use `run_test.py --list <substring>` for build-free discovery and choose the smallest authoritative target. Exact tests are deterministic; focused gameplay adds one replayable case and prints roots. Core stays feature-minimal; gameplay fixtures use `test-gameplay`. Keep Cargo serial and parallelize only build-free `quick`.

## Escalation lanes

| Need | Command |
| --- | --- |
| Broad core/gameplay checkpoint | `python ci.py audit --all` |
| Production Clippy | `python ci.py gate --lint` |
| Gameplay exploration | `python ci.py report [--scope <scope>]` |
| Changed-source complexity review | `python ci.py bca [--path <scope>] [--since <revision>]` |

`quick` is build-free, `gate` is one explicit build lane, and `audit` is a checkpoint. Focused gameplay adds one replayable case; broad audits rotate that budget through one scope, while `report` owns exploration. `test-gameplay` belongs only to gameplay targets; core stays feature-minimal and negative feature gating is forbidden. Specialized gates are `--shaders`, `--rustdoc`, and `--soak`. Git-Wizard `full` maps to opt-in `audit --all`.

Use `python ci.py report --scope fieldwork --verbose --focus-seed 0x1` to inspect one world seed. Wilderness seeds can contain multiple goals; add `--focus-episode maintained/rested/stay-mobile` under `--scope wilderness` for one actor episode. All samples execute; untagged events are omitted rather than misattributed.

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
| Rust compile error in a broad lane | Follow the emitted build-only repair target; once it compiles, rerun the original semantic proof. |
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

Each gameplay scope owns a focused probe; generator/policy/owner tests live in `<scope>_contracts` targets. `run_test.py` resolves exact/suite selectors build-free through `tools/gameplay_targets.py`. Reports stay out of gates. `gate --gameplay contracts` is cross-scope; `audit --gameplay` already includes it and owner contracts, so do not stack them.

Focused gates add one fresh organic case; broad audits rotate one through one scope. Reports own wider exploration and failures print replay roots. [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) owns actor/evidence rules.

## Completion

Run only the lane required by the changed contract. `quick` already proves the local CI/test-tooling contracts; every additional build must prove a distinct contract.
