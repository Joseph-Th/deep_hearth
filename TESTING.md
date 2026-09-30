# Testing

**Role:** Local verification and test-organization authority.

Use [`README.md`](README.md) for routing, [`STATUS.md`](STATUS.md) for scope, and [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) for automated-player evidence. Use the smallest lane that proves the changed contract.

## Fast path

| Need | Command |
| --- | --- |
| Documentation/contracts | `python tools/check_authority_docs.py` |
| Build-free edit loop | `python ci.py quick` |
| CI/test tooling contracts | `python -m unittest tools.test_ci -q` |
| Type-check one unstable test/harness target | `python tools/run_test.py --check <qualified-name-or-unique-substring>` |
| One exact test | `python tools/run_test.py <qualified-name-or-unique-substring>` |
| One owner/subsystem group | `python tools/run_test.py --suite <qualified-prefix-or-substring>` |
| Shared gameplay contracts | `python ci.py gate --gameplay contracts` |
| Focused gameplay | `python ci.py gate --gameplay <scope>` |
| Compile-only proof when no executable test fits | `python ci.py gate` |

Routine iteration is `quick`, then **one** build proof. While code is unstable, `run_test.py --check <selector>` resolves the eventual test target without codegen/linking; once runnable, the test itself is compile proof. Use default `gate` only when no executable test fits. Do not stack equivalent proofs or append an audit.

Use `run_test.py --list <substring>` for build-free discovery. Unit selectors still compile the full `cfg(test)` library, so prefer a smaller focused gameplay target when equally authoritative. `.cargo/config.toml` owns the shared `target/local-ci` build shape; repository Python entrypoints remove ambient Rust/profile overrides while preserving an explicit `CARGO_TARGET_DIR`. The normal test profile keeps the smaller focused targets at 128 CGUs; exact/suite library tests and broad core/soak lanes use a measured 512-CGU override for faster post-edit codegen and linking. Run build-producing Cargo lanes serially: concurrent rustc/linker processes contend for CPU and the shared incremental cache. Only the build-free `quick` checks run in parallel.

## Escalation lanes

| Need | Command |
| --- | --- |
| Broad core/gameplay checkpoint | `python ci.py audit --all` |
| Production Clippy | `python ci.py gate --lint` |
| Gameplay exploration | `python ci.py report [--scope <scope>]` |
| Changed-source complexity review | `python ci.py bca [--path <scope>] [--since <revision>]` |

`quick` is build-free; `gate` runs one build lane; `audit` is an explicit checkpoint. Specialized gates are `--shaders`, `--rustdoc`, and `--soak`. Git-Wizard `quick` and `standard` stay build-free because generic finalization cannot select the changed behavioral contract; `full` maps to `audit --all` and is opt-in.

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

Split owner contract targets only when they materially shrink the compile graph. Progression, settlement, and woodworking qualify; fieldwork, workshop, survival, ore, and foundry keep nearby contracts in the focused target. Gameplay targets share one `test-gameplay` feature shape. Scoped reports reuse focused artifacts where possible; only the cross-system report compiles the complete report graph.

`gate --gameplay contracts` is only the small cross-scope contract target. Owner-specific contract tests are discovered and run through `run_test.py`; exact names automatically select their purpose-built contract target, so routine work does not need a broad multi-target contract gate.

Routine gameplay verification runs maintained witnesses plus one fresh replayable organic case. Explicit replay roots replace that fresh case for diagnosis; exploratory reports generate four organic cases plus broader agency search. Successful routine gates print only counts/timing, explicit replays echo their roots, failures print a narrow reproduction command, and reports retain replay inputs as evidence. [`GAMEPLAY_EVALUATION.md`](GAMEPLAY_EVALUATION.md) owns actor/evidence rules.

## Completion

Run only the lane required by the changed contract. CI/test-tooling changes run `python -m unittest tools.test_ci -q`; every additional build must prove a distinct contract.
