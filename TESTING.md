# Testing

This document owns Deep Hearth's test selection, feedback lanes, harness contract, and CI gate. The
suite is organized so ordinary correctness work does not compile specialized gameplay or shader
validation dependencies unless that coverage is relevant.

## Daily workflow

Run the narrowest qualified test while changing one behavior, then use the fast lane before moving
on. The fast lane does not compile long-horizon soak bodies:

```text
cargo test <qualified-test-name> -- --exact
cargo test-fast
```

When a change touches a specialized boundary, add its maintained lane:

```text
cargo test-soak
cargo test-gameplay
cargo test-shaders
```

Routine successful lanes use quiet Cargo output. Failure output remains Cargo-native and includes the
failing qualified test. The test profile omits debug symbols to reduce codegen/link time; set Cargo's
`CARGO_PROFILE_TEST_DEBUG` override when an interactive debugging session needs them. Do not add timing
assertions or sleeps; performance of the verification workflow is an engineering property, not a
simulation contract.

## Maintained lanes

| Command | Purpose | Compile scope |
| --- | --- | --- |
| `cargo test-fast` | Ordinary deterministic behavior, errors, persistence, and integrations | Default features; soak bodies are not compiled |
| `cargo test-soak` | Long-horizon deterministic conservation/invariant scenarios | Adds `test-soak` and runs only `soak` tests |
| `cargo test-gameplay` | Harness configuration contracts plus workshop exercise | Dedicated integration target; library unit-test bodies are not compiled |
| `cargo test-gameplay-report` | One workshop matrix with concise human-readable summary | Same dedicated integration target with captured output disabled |
| `cargo test-shaders` | Naga parse/semantic validation of assembled WGSL without compiling the crate unit-test harness | Adds `test-shader-validation` |
| `cargo test-check` | Silent all-target compilation of the default feature set | Default features |
| `cargo test-lint` | Clippy with warnings denied | Default features |
| `cargo test-lint-all` | Cross-cutting/release Clippy audit | All test features |
| `cargo test-all` | Complete debug test inventory including dedicated integration targets | All test features |
| `cargo test-release` | Complete optimized test inventory | All test features |
| `cargo test-doc` | Documentation build without dependencies | Default features |

The `test-soak`, `test-gameplay`, and `test-shader-validation` features exist only to control test
compilation. They do not change default runtime behavior. Naga remains absent from the default
dependency graph and ordinary core test build.

`cargo test-ci-core` is an internal CI composition rather than an edit-loop command. It compiles the
crate unit-test binary once with `test-soak` enabled and runs both ordinary and soak tests from that
single artifact. This avoids paying the dominant unit-test codegen/link cost twice in CI while keeping
`cargo test-fast` and `cargo test-soak` independently targetable during development.

## Test organization and assertions

Tests remain colocated with the owning source module under `#[cfg(test)]`. Prefer the smallest fixed
fixture and shortest canonical execution that prove the named rule. A rejected operation should
assert the exact typed error and unchanged authoritative state when mutation atomicity is part of the
contract. A successful operation should assert the exact identity, quantity, lifecycle, relationship,
or durable result that defines success.

Avoid assertions on human-readable error prose, incidental ordering, transient implementation counts,
or arbitrary wall-clock duration. Aggregate `any`/`all` assertions are appropriate only when the
contract is explicitly coverage across a maintained scenario matrix; those failures must name the
missing behavior rather than returning an anonymous boolean failure.

Long-horizon tests use `soak` in the qualified test name and are compiled only with the `test-soak`
feature. Keep one mixed-system thousands-of-ticks soak as the broad invariant proof; subsystem soaks
should exist only where repeated ownership, conservation, persistence, or numerical accumulation adds
evidence that a narrow test cannot provide.

## Gameplay harness

The workshop harness is an **exercise-mode** automated behavior evaluation. It deliberately chooses
legal situations that cover important physical consequences; it is not evidence that an ordinary
human player will make the same choices. Setup may arrange matter, equipment, finite energy, and
structural bays because acquisition/construction authorizers are not implemented. After setup, the
harness uses the same production validators, resolutions, commits, and simulation ticks as normal
runtime behavior.

The acting policy uses observable state and resolver projections. Hidden authoritative state may be
used only for diagnostics and postcondition checks. Scenario variation is deterministic and does not
consume unrelated simulation randomness. A legal scenario may complete zero batches when an in-flight
job is suspended before its first output; that is gameplay evidence, not a harness failure. The
maintained matrix owns aggregate experience claims such as completed/incomplete orders, the mixed-ore
frontier, relocation/recovery, and policy diversity rather than requiring every seed to exhibit every
behavior.

Direct fixture-only starting-state injection is deliberately isolated in
`content/gameplay_harness/bootstrap.rs`. That module may seed loose matter and stored energy or
materialize already-planned structures because the corresponding acquisition, generation, and
construction authorizers do not exist yet. The acting policy cannot call those shortcuts. Ordinary
stockpile allocation, structural geometry, equipment
allocation, process resolution, maintenance, support changes, production, and ticks use the normal
runtime APIs.

The gameplay exercise is an integration-test target rather than a crate unit test. This keeps its
large policy/scenario implementation out of the monolithic unit-test binary and lets gameplay work
rebuild against the normal library without compiling hundreds of unrelated test bodies. The same
integration target runs lightweight seed/configuration contracts first, so those checks remain in the
ordinary gameplay CI lane without forcing a feature-enabled crate unit-test build.

The maintained workshop loop covers announced-load-informed structural siting, finite power choice,
active-tick wear, exact replacement-stock maintenance, external structural disruption, persistent
structural damage, production suspension, WIP recovery/stranding, and the current mixed-ore processing
frontier. Deep Hearth does not yet own weather or forecasts, so the harness labels its timed snow load
as an external stimulus rather than presenting it as implemented gameplay. Player priorities vary by
seed within bounded deterministic choices: reserve conservation, projected machine condition, or
completion time. Safety and maintenance gates remain canonical regardless of personality. The acting
policy never sees the actual future load before the stimulus is committed.

Scenario physics follow current authored content. Crusher batch mass is chosen inside the current
equipment limit; initial condition is derived from its current maintenance bands; support geometry and
external load scale from current equipment weight, material strength, and structural thresholds; and
the announced event tick is selected inside a work horizon derived from an actually resolved batch
duration. These are bounded variations around real game quantities rather than copied balance values.

Capability probes derive legal batch sizes, output partitions, and equipment limits from the current
authored registries instead of duplicating recipe constants. Their assertions focus on conservation,
resolver/authoring agreement, legal routing, condition changes, and state validity. They may report
currently unavailable direct routes as observations, but do not freeze those absences into gameplay
requirements. The ore-preparation and foundry probes remain explicitly labeled capability checks until
concentration/smelting provides a truthful bridge between those stages.

`cargo test-gameplay` keeps successful harness output captured. `cargo test-gameplay-report` emits a
replay-input line, compact outcome and systems summaries, and one scope line distinguishing exercised
runtime behavior from bootstrap/external/deferred systems. Set `DEEP_HEARTH_GAMEPLAY_VERBOSE` to any
value before that report lane to emit the detailed decision trace. Seed controls are:

- `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED`: reproduces the normal organic scenario set from one exact
  decimal or hexadecimal root seed;
- `DEEP_HEARTH_GAMEPLAY_SEEDS`: replaces the maintained matrix with an exact comma-separated seed
  list for reproduction or deliberate sweeps.

Seed lists fail on an empty or malformed entry rather than silently dropping it. When the maintained
matrix is used, five curated scenarios own the aggregate regression coverage and three additional
organic scenarios are generated from a fresh variation root. Organic scenarios exercise universal
per-scenario contracts but cannot mask a lost maintained behavior. The input line prints the root and
all exact scenario seeds so any failure is replayable. Explicit custom seed lists prove only the
universal per-scenario contracts and do not inherit the maintained matrix's aggregate coverage claim.

## CI and completion gates

Pull requests and pushes to `main` run independent jobs so unrelated compilation does not serially
extend the feedback path, while lanes that require the same expensive unit-test artifact are combined:

1. **Quality**: format and default-feature Clippy.
2. **Core + Soak**: ordinary and long-horizon tests from one `test-soak` unit-test build.
3. **Gameplay**: the dedicated feature-gated gameplay integration target.
4. **Shaders**: the feature-gated Naga WGSL validation lane.

Jobs use locked dependencies, a pinned Rust toolchain, source-aware per-lane build caches, incremental
compilation, bounded timeouts, and concurrency cancellation for superseded runs. Cache keys retain the
dependency/toolchain prefix while advancing with source changes, so a lane can reuse its previous
incremental build instead of freezing the target cache at the first commit after a lockfile change.
Core and soak deliberately share one test artifact, and gameplay deliberately does not compile the
crate unit-test harness. The ordinary CI gate intentionally does not rebuild the entire project in
release mode or generate documentation on every change.

Before committing, run:

```text
cargo fmt --check
cargo test-lint
cargo test-fast
```

`cargo test-check` remains available as a compile-only diagnostic, but it is not part of the normal
pre-commit sequence because `cargo test-lint` already type-checks all default targets.

Also run `cargo test-soak`, `cargo test-gameplay`, or `cargo test-shaders` when the changed contract is
owned by that lane. Use `cargo test-all` when a cross-cutting change needs the complete debug inventory.
Release hardening is deliberately separate:

```text
cargo test-lint-all
cargo test-release
cargo test-doc
```
