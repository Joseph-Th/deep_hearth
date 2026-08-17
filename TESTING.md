# Testing

This document owns Deep Hearth's test selection, feedback lanes, harness contract, and CI gate. The
suite is organized so ordinary correctness work does not compile specialized gameplay or shader
validation dependencies unless that coverage is relevant.

## Daily workflow

Run the narrowest qualified test while changing one behavior, then use the fast lane before moving
on. Long-horizon soak tests compile into the same unit-test artifact but are marked ignored, so the
fast lane does not execute them and a subsequent soak run reuses the existing build:

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
failing qualified test. The test profile and dedicated validation-binary profile omit debug symbols to
reduce codegen/link time; normal dev builds retain their debugging behavior. Set Cargo's profile debug
override when an interactive test/validator debugging session needs symbols. Do not add timing
assertions or sleeps; performance of the verification workflow is an engineering property, not a
simulation contract.

## Maintained lanes

| Command | Purpose | Compile scope |
| --- | --- | --- |
| `cargo test-fast` | Ordinary deterministic behavior, errors, persistence, and integrations | Default-feature unit-test artifact; ignored long-horizon tests do not run |
| `cargo test-soak` | Long-horizon deterministic conservation/invariant scenarios | Reuses the default-feature unit-test artifact and runs only ignored tests |
| `cargo test-gameplay` | Seed/replay configuration contracts plus maintained workshop exercise | Dedicated integration target; library unit-test bodies are not compiled |
| `cargo test-gameplay-report` | One workshop matrix with concise human-readable summary | Same dedicated integration target with captured output disabled |
| `cargo test-shaders` | Naga parse/semantic validation of assembled WGSL without compiling the crate unit-test harness | Adds `test-shader-validation` |
| `cargo test-check` | Silent all-target compilation of the default feature set | Default features |
| `cargo test-lint` | Clippy with warnings denied | Default features |
| `cargo test-lint-all` | Cross-cutting/release Clippy audit | All test features |
| `cargo test-all` | Complete debug test inventory including dedicated integration targets | All test features |
| `cargo test-release` | Complete optimized test inventory | All test features |
| `cargo test-doc` | Documentation build without dependencies | Default features |

The `test-gameplay` and `test-shader-validation` features exist only to expose specialized test
boundaries. They do not change default runtime behavior. Naga remains absent from the default
dependency graph and ordinary core test build. Soaks intentionally use no feature because feature
splitting forced a second full unit-test codegen/link step after ordinary tests.

`cargo test-ci-core` is an internal CI composition rather than an edit-loop command. It compiles the
crate unit-test binary once and runs ordinary plus ignored soak tests from that single artifact. Local
`cargo test-fast` and `cargo test-soak` select different execution subsets without changing Cargo
features, so running them back to back does not require a second unit-test binary.

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

The workshop harness is an **exercise-mode** automated behavior evaluation. It deliberately chooses
legal situations that cover important physical consequences; it is not evidence that an ordinary
human player will make the same choices. Setup may arrange matter, equipment, finite energy, and
structural bays because acquisition/construction authorizers are not implemented. After setup, the
harness uses the same production validators, resolutions, commits, and simulation ticks as normal
runtime behavior.

The acting policy uses observable state and resolver projections. Hidden authoritative state may be
used only for diagnostics and postcondition checks. Each scenario is deterministic from its printed
seed and does not consume unrelated simulation randomness, while normal runs add a small fresh organic
sample so the harness does not become one memorized script. A legal scenario may complete zero batches
when an in-flight job is suspended before its first output; that is gameplay evidence, not a harness
failure. Maintained seeds guarantee only stable input/policy diversity. Balance-dependent outcomes such
as completion, maintenance pressure, structural damage, suspension, and relocation are reported as
observations rather than frozen into aggregate pass/fail requirements.

Direct fixture-only starting-state injection is deliberately isolated in
`src/content/gameplay_fixture.rs`. That feature-gated bridge may seed loose matter and stored energy or
materialize already-planned structures because the corresponding acquisition, generation, and
construction authorizers do not exist yet. The acting policy cannot call those shortcuts. Ordinary
stockpile allocation, structural geometry, equipment
allocation, process resolution, maintenance, support changes, production, and ticks use the normal
runtime APIs.

The gameplay exercise source lives under `tests/gameplay_harness/` and is an integration-test target
rather than library code or a crate unit test. This keeps its large policy/scenario implementation out
of both library codegen and the monolithic unit-test binary. A harness-only edit therefore rebuilds the
dedicated test target against the cached library instead of invalidating the feature-enabled core
crate. Configuration contracts live in that same target so the gameplay lane has one specialized
artifact rather than multiplying Cargo targets for small checks.

The maintained workshop loop covers delivery-informed structural siting, finite power choice,
active-tick wear, exact replacement-stock maintenance, inventory-owned stored-matter loading,
persistent structural damage, production suspension, WIP recovery/stranding, and the current mixed-ore
processing frontier. The timed disruption is no longer a synthetic weather/load write: a real bulk
material transfer moves seeded starting matter into a mounted stockpile, and the inventory subsystem
updates the support's `StoredMatter` load through its canonical transaction. The harness chooses when
to attempt that transfer; this is not presented as an implemented logistics scheduler. Player
priorities vary by seed within bounded deterministic choices: reserve conservation, projected machine
condition, or
completion time. Safety, ownership, support, energy, and maintenance gates remain canonical regardless
of personality.

Scenario physics follow current authored content. Crusher batch mass is chosen inside the current
equipment limit; initial condition is derived from its current maintenance bands; support geometry,
background stored cargo, and delivery mass scale from current equipment/material quantities; and the
delivery tick is selected inside a work horizon derived from an actually resolved batch duration. The
support generator deliberately spans a broad ordinary utilization range rather than targeting authored
failure thresholds, so structural outcomes can change naturally as game balance evolves.

Capability probes derive legal batch sizes, output partitions, and equipment limits from the current
authored registries instead of duplicating recipe constants. Their assertions focus on conservation,
resolver/authoring agreement, legal routing, condition changes, and state validity. They may report
currently unavailable direct routes as observations, but do not freeze those absences into gameplay
requirements. The ore-preparation and foundry probes remain explicitly labeled capability checks until
concentration/smelting provides a truthful bridge between those stages.

`cargo test-gameplay` keeps successful harness output captured. `cargo test-gameplay-report` emits a
replay-input line, sampled input ranges, compact outcome and systems summaries, and one scope line
distinguishing exercised runtime behavior from bootstrap/deferred systems. Set
`DEEP_HEARTH_GAMEPLAY_VERBOSE` to any value
before that report lane to emit the detailed decision trace. Seed controls are:

- `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED`: reproduces the normal organic scenario set from one exact
  decimal or hexadecimal root seed;
- `DEEP_HEARTH_GAMEPLAY_SEEDS`: replaces the anchor-plus-organic plan with an exact comma-separated
  seed list for reproduction or deliberate sweeps.

Seed lists fail on an empty or malformed entry rather than silently dropping it. Normal runs combine
five fixed anchor scenarios with three organic scenarios derived from a fresh variation root. The
anchors preserve reproducible comparison and all three operating priorities; the organic sample gives
each run slightly different physical conditions without enlarging the lane enough to hurt iteration.
`DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` reproduces any organic set exactly. Hard failures remain canonical
execution/invariant failures and capability-probe conservation failures, not required balance outcomes.
The input line prints the variation root and all exact scenario seeds so every failure is replayable.
Explicit custom seed lists run the same per-scenario contracts without claiming aggregate outcome
coverage.

## CI and completion gates

Pull requests and pushes to `main` run independent jobs so unrelated compilation does not serially
extend the feedback path, while lanes that require the same expensive unit-test artifact are combined:

1. **Quality**: format and default-feature Clippy.
2. **Core + Soak**: ordinary and ignored long-horizon tests from one default-feature unit-test build.
3. **Gameplay**: the dedicated feature-gated gameplay integration target.
4. **Shaders**: the feature-gated Naga WGSL validation lane.

Jobs use locked dependencies, a pinned Rust toolchain, one shared Cargo dependency cache, source-aware
per-lane target caches, incremental compilation, bounded timeouts, and concurrency cancellation for
superseded runs. Target keys include the source trees each lane actually compiles, including gameplay
integration sources, while restore prefixes reuse the previous incremental artifact after source
changes. Splitting dependency downloads from target artifacts avoids storing the same Cargo registry
payload in every lane cache. Core and soak deliberately share one test artifact, and gameplay
deliberately does not compile the crate unit-test harness. The ordinary CI gate intentionally does not
rebuild the entire project in release mode or generate documentation on every change.

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
