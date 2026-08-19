# Testing

This document owns Deep Hearth's test selection, feedback lanes, harness contract, and CI gate. The
suite is organized so ordinary correctness work does not compile specialized gameplay or shader
validation dependencies unless that coverage is relevant.

## Daily workflow

Run the narrowest qualified test while changing one behavior. Do not reflexively run the whole fast
lane after every edit: use it as a checkpoint when a coherent slice is complete, and let the local CI
gate be the final pre-commit repetition. Long-horizon soak bodies are compiled only with the
`test-soak` feature, so ordinary behavior edits do not type-check or codegen the large soak fixtures:

```text
cargo check-fast                                  # mechanical/type feedback
cargo check-tests                                 # mechanical feedback while editing unit tests
cargo test-fast <qualified-test-name> -- --exact  # behavior feedback
cargo test-fast                                   # coherent checkpoint
python ci.py gate                                 # once before commit
```

`cargo check-fast` is intentionally library-only. `cargo check-tests` type-checks the default-feature
test inventory without codegen/linking, and `cargo check-gameplay` does the same for the specialized
gameplay integration target. Use the narrowest of these while the edit is still mechanical. An exact
executable test is still required for changed behavior; compile-only feedback exists specifically to
avoid paying the dominant codegen/link cost on every intermediate edit.

`python ci.py gate` is the concise local pre-commit wrapper. It runs format and the fast core tests,
captures successful Cargo noise, prints one timed line per stage, reports the slowest successful stage,
and shows native command output only when a stage fails. Rust compiler warnings are denied by the package lint configuration, so this path
does not need a second Clippy compile merely to catch ordinary warning regressions. Add `--lint` when a
Clippy checkpoint is useful. Scope is explicit rather than inferred from git diffs: add `--soak`,
`--gameplay`, `--shaders`, or `--docs` when those contracts changed.

When a change touches a specialized boundary, add its maintained lane:

```text
cargo test-soak
cargo test-gameplay
cargo test-shaders
```

Routine successful lanes use quiet Cargo output. Failure output remains Cargo-native and includes the
failing qualified test. The test profile and dedicated validation-binary profile omit debug symbols to
reduce codegen/link time; normal dev builds retain their debugging behavior. Set Cargo's profile debug
override when an interactive test/validator debugging session needs symbols. Do not add timing
assertions or sleeps; performance of the verification workflow is an engineering property, not a
simulation contract.

## Maintained lanes

| Command | Purpose | Compile scope |
| --- | --- | --- |
| `cargo check-fast` | Fast compile/type feedback during mechanical edits | Default-feature production library only; no test harness link |
| `cargo check-tests` | Fast compile/type feedback while editing ordinary tests | Default-feature test targets, type-check only; no test codegen/link |
| `cargo check-gameplay` | Fast compile/type feedback while editing the gameplay harness | `gameplay_harness` plus `test-gameplay`, type-check only; no test link/run |
| `cargo test-fast` | Ordinary deterministic behavior, errors, persistence, and integrations | Default-feature unit-test artifact; soak-only bodies are not compiled |
| `cargo test-soak` | Long-horizon deterministic conservation/invariant scenarios | Adds `test-soak` and runs only ignored tests |
| `cargo test-gameplay` | Deterministic replay contracts, maintained workshop cases, survival provisioning, progression, and capability probes | Dedicated integration target with `test-gameplay`; library unit-test bodies are not compiled |
| `cargo test-gameplay-scenarios` | Five maintained workshop anchors plus two fresh bounded replayable variations | Reuses the dedicated gameplay artifact |
| `cargo test-gameplay-survival` | Food freshness/preservation, varied meal, and finite-water provisioning probe | Reuses the dedicated gameplay artifact; one maintained plus one fresh bounded replayable variation by default |
| `cargo test-gameplay-progression` | Primitive survival/craft/mine/manual-power/mechanization progression probe only | Reuses the dedicated gameplay artifact; one maintained plus one fresh bounded replayable variation by default |
| `cargo test-gameplay-ore` | Ore-preparation capability probe only | Reuses the dedicated gameplay artifact; one maintained plus one fresh bounded replayable variation by default |
| `cargo test-gameplay-foundry` | Foundry capability probe only | Reuses the dedicated gameplay artifact; one maintained plus one fresh bounded replayable variation by default |
| `cargo test-gameplay-report` | Exploratory maintained-plus-fresh workshop report with concise human-readable summary | Same feature-gated integration target, ignored by the gate and run with captured output disabled |
| `cargo test-shaders` | Naga parse/semantic validation of assembled WGSL without compiling the crate unit-test harness | Adds `test-shader-validation` |
| `cargo check-all` | All-target compile-only diagnostic | Default features |
| `cargo test-lint` | Optional production-library Clippy checkpoint | Default-feature library only; not part of the routine gate |
| `cargo test-lint-all` | Cross-cutting/release Clippy audit | All test features |
| `cargo test-all` | Ordinary plus ignored core/soak tests in one invocation | Adds `test-soak` once and runs the combined core inventory |
| `cargo test-all-features` | Manual combined-feature test diagnostic | All test features in one artifact; intentionally outside maintained CI presets because specialized artifacts are faster |
| `cargo test-release` | Complete optimized test inventory | All test features |
| `cargo test-doc` | Documentation build without dependencies | Default features |

`test-gameplay` exists only to expose the controlled bootstrap adapter required by the integration
harness; that adapter remains absent from ordinary production builds and delegates to canonical runtime
transactions. `test-shader-validation` likewise exists only to expose Naga-backed WGSL validation.
Neither specialized boundary enters the ordinary edit-loop build. `test-soak` likewise keeps
long-horizon fixture code out of the fast artifact; the explicit soak lane pays that compile cost only
when repeated-state evidence is actually needed.

`cargo test-lint` deliberately does not lint-compile test targets and is optional in the ordinary loop.
The package denies Rust compiler warnings, while `cargo test-fast` compiles and executes the complete
default-feature unit-test target. Running Clippy before every test checkpoint therefore pays a second
front-end pass for limited incremental signal. Use `python ci.py gate --lint` for a deliberate
production-library lint checkpoint and `cargo test-lint-all` for test/harness lint coverage or release
hardening.

When both ordinary and soak coverage are needed, prefer one `cargo test-all` invocation (or `python
ci.py gate --soak`) instead of running `test-fast` and `test-soak` separately. That invocation builds
the `test-soak` variant once and runs ordinary plus ignored tests together, without making the daily
default-feature artifact carry soak-only code.

## Test organization and assertions

Tests remain colocated with the owning source module under `#[cfg(test)]`. Prefer the smallest fixed
fixture and shortest canonical execution that prove the named rule. A rejected operation should
assert the exact typed error and unchanged authoritative state when mutation atomicity is part of the
contract. A successful operation should assert the exact identity, quantity, lifecycle, relationship,
or durable result that defines success.

Avoid assertions on human-readable error prose, incidental ordering, transient implementation counts,
or arbitrary wall-clock duration. Aggregate `any`/`all` assertions are appropriate only for stable
input/diversity contracts, not for balance-dependent outcomes. Those failures must name the missing
contract rather than returning an anonymous boolean failure.

Long-horizon tests use `soak` in the qualified test name and carry
`#[ignore = "long-horizon soak"]`. The ignore marker, not a name filter, owns lane membership. Keep one
mixed-system thousands-of-ticks soak as the broad invariant proof; subsystem soaks should exist only
where repeated ownership, conservation, persistence, or numerical accumulation adds evidence that a
narrow test cannot provide. Do not ignore ordinary behavioral tests.

## Gameplay harness

The gameplay harness is a replayable automated behavior evaluation under
`tests/gameplay_harness/`. It exercises real runtime validators, resolutions, commits, and simulation
ticks. It is evidence about system behavior under controlled legal scenarios, not a model of typical
human play.

### Boundary

The feature-gated setup bridge in `src/content/gameplay_fixture.rs` may create starting matter, fluid,
energy, equipment, geological occurrences, planned structures, and opaque authorization for a
controlled external event where the corresponding world owner is not yet implemented. Setup-only
authorizations are created before the acting policy begins and are single-use. The acting policy cannot
call setup helpers or inspect a controlled event's future tick/target. Experienced inventory movement,
maintenance, support changes, production, mining, survival, and time progression use canonical runtime
transactions.

The policy reads observable state and resolver projections. Hidden scenario state is limited to event
injection, diagnostics, and postconditions and never feeds actor decisions. It does not clone
`AppState` to preview compound future mutations.

### Maintained coverage

`cargo test-gameplay` covers:

- workshop operation as a total-mass work order with uneven finite stored work, adaptive legal batch
  sizing, replacement-stock scarcity, wear, structural loading, a hidden controlled delivery event,
  suspension/recovery, and survival cost;
- canonical manual-power recovery when stored work is insufficient, including equipment wear and the
  exact projected metabolic/hydration budget used to decide whether preserving survival reserve is
  worth leaving part of an order unfinished;
- survival provisioning and preservation;
- primitive crafting, mining, native-copper reinforcement, material-backed work storage, manual power,
  and autonomous crushing alongside concurrent player labor;
- ore preparation through crushing, grinding, screening, and selective oversize regrinding;
- pure-copper melt/cast as a separate downstream capability probe;
- seed parsing, replay, and separation of world physics from automated-player policy.

Focused aliases rerun one concern without repeating the rest:

```text
cargo test-gameplay-scenarios
cargo test-gameplay-survival
cargo test-gameplay-progression
cargo test-gameplay-ore
cargo test-gameplay-foundry
```

### Determinism and exploration

The required gate uses five maintained workshop anchors plus two fresh bounded generated variations.
Focused probes use one maintained case plus one fresh bounded generated variation. Fresh cases change
between invocations, while every run prints exact replay roots. Stable anchors provide regression
sentinels; generated cases exercise legal state combinations that fixed anchors would not cover.
World/scenario and behavior seeds are independent.

- `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` fixes/replays the generated physical variation root.
- `DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` fixes/replays the generated policy root.
- `DEEP_HEARTH_GAMEPLAY_SEEDS` supplies an exact comma-separated world-seed sweep.

Every run prints replay inputs. Malformed explicit seeds fail configuration.

`cargo test-gameplay-report` is the ignored exploratory lane. It uses fresh generated cases, prints
replay roots, the current authored equipment/energy/process catalog, compact scenario/system summaries,
the runtime-reachable versus bootstrapped workshop boundary, and explicit observed/unobserved behavior.
Its matched-world agency panel compares one stable anchor plus the most stored-work-constrained fresh
world selected from scenario inputs, so recovery policies are compared on useful physical pressure
without selecting a world by its outcome. It may be made verbose with
`DEEP_HEARTH_GAMEPLAY_VERBOSE`.

### Assertion policy

Hard failures represent stable contracts: canonical execution errors, conservation, persistence,
ownership, exact routing, authored capability agreement, deterministic seed separation, and maintained
input diversity. Balance-dependent outcomes such as number of completed batches, structural damage,
maintenance pressure, suspension, relocation, or bottleneck mix are observations unless a specific
scenario explicitly owns that contract.

The workshop variation generator may intentionally provide insufficient or fractional stored work and
replacement stock. The actor asks the canonical process resolver for the largest legal operation it can
actually power instead of treating a partial store as empty. When stored work is exhausted it may use
the runtime manual-power path; a survival-preserving policy rejects projected work that would cross the
authored hunger or thirst warning reserve. A partial order caused by those real constraints or that
policy choice is a valid observation, not a harness failure. A hidden controlled event may occur while
the actor is committed to manual work, in which case world state changes immediately but player-driven
recovery waits until that labor commitment completes.
Likewise, the primitive mechanization probe proves that player labor and autonomous machine work can
overlap; it does not require one authored duration to remain shorter than the other.

Probe inputs derive legal masses, durations, process routes, capability limits, and material
requirements from current registries where possible. Do not freeze copied balance constants into
assertions when the authoritative definition can be queried.

The harness remains one integration target to minimize build/link artifacts. Configuration, scenario
generation, contracts, probe setup, execution, seed mixing, and reporting are separate modules within
that target.

## Local CI and completion gates

Verification runs in the developer workspace. GitHub Actions and hosted runners are prohibited. Use the
repository-owned `ci.py` runner or the Cargo aliases directly; no pull-request job, scheduled workflow,
or remote runner owns the validation contract. The runner deliberately does not inspect changed files
or guess scope: explicit flags are faster to understand and cannot silently omit a relevant lane.

1. **Routine**: `python ci.py gate`.
2. **Lint checkpoint**: add `--lint` when Clippy-specific feedback is useful.
3. **Core + Soak**: `python ci.py gate --soak` (builds the `test-soak` core artifact once).
4. **Gameplay**: add `--gameplay` when workshop behavior or content changed.
5. **Shaders**: add `--shaders` when WGSL or shader assembly changed.
6. **Cross-cutting local completion**: `python ci.py full` runs fast core plus gameplay without pulling
   long-horizon soak, shader/parser, or documentation builds into unrelated work. Add `--soak`,
   `--shaders`, or `--docs` only when those contracts changed.
7. **Explicit hardening**: `python ci.py hardening` runs maintained behavior lanes first and pays for
   all-target/all-feature Clippy last. This fails sooner on behavioral regressions and avoids spending
   the lint pass when an earlier executable gate is already broken.

Local Cargo incremental state may be reused naturally between these commands. Fast core deliberately
does not compile soak-only fixtures; gameplay deliberately does not compile the crate unit-test harness.
Release hardening and documentation remain explicit local commands rather than background or scheduled
work.

Whenever the gameplay lane is selected, `ci.py` runs `tools/check_gameplay_aliases.py` after the
gameplay executable has passed. The checker therefore reuses the already-built harness artifact instead
of becoming the stage that triggers its expensive first build. It lists the real `gameplay_harness`
test inventory and its ignored subset, verifies every
filtered Cargo alias selects exactly one expected test with the correct active/ignored status, and
fails on uncontracted filtered aliases. The checker includes cheap synthetic self-tests for a valid
inventory, a missing selector, and ignored-status drift. This prevents Cargo's otherwise-successful
zero-match filtering behavior from making a stale focused alias look like a passing verification lane.

Before committing, run:

```text
python ci.py gate
```

If `cargo test-fast` was just run as the final checkpoint, there is no value in immediately running it
a second time before continuing to code. Finish the change first, then run the gate once. Cargo's
incremental cache still makes a recent checkpoint useful to the later gate without turning verification
into a ritual after every edit.

`cargo check-all` remains available as an all-target compile-only diagnostic, but it is not part of
the normal pre-commit sequence. `cargo check-fast` and `cargo test-fast` both compile production code
with Rust warnings denied; `cargo test-fast` additionally compiles and executes default-feature unit
tests. Clippy is an explicit lint checkpoint, not a prerequisite for every behavior build.

Also add `--soak`, `--gameplay`, `--shaders`, or `--docs` when the changed contract is owned by that
lane. Use `python ci.py full` for the common cross-cutting core-plus-gameplay checkpoint, and add the
specialized flags only when their contracts changed. A combined `cargo test-all-features` build remains
available as a manual diagnostic, but it is not part of the maintained presets because all-feature
Clippy already checks feature compatibility and the specialized test artifacts avoid redundant
codegen/linking.
Release hardening is deliberately separate:

```text
python ci.py hardening
cargo test-release     # only when optimized-build behavior matters
```
