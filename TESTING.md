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
| `cargo test-gameplay` | Deterministic seed/replay contracts plus the maintained anchor workshop matrix | Dedicated integration target with `test-gameplay`; library unit-test bodies are not compiled |
| `cargo test-gameplay-report` | Exploratory anchor-plus-organic workshop report with concise human-readable summary | Same feature-gated integration target, ignored by the gate and run with captured output disabled |
| `cargo test-shaders` | Naga parse/semantic validation of assembled WGSL without compiling the crate unit-test harness | Adds `test-shader-validation` |
| `cargo test-check` | Silent all-target compilation of the default feature set | Default features |
| `cargo test-lint` | Production-library Clippy with warnings denied | Default-feature library only; avoids lint-compiling the large unit-test target before `test-fast` compiles it normally |
| `cargo test-lint-all` | Cross-cutting/release Clippy audit | All test features |
| `cargo test-all` | Complete debug test inventory including dedicated integration targets | All test features |
| `cargo test-release` | Complete optimized test inventory | All test features |
| `cargo test-doc` | Documentation build without dependencies | Default features |

`test-gameplay` exists only to expose the controlled bootstrap adapter required by the integration
harness; that adapter remains absent from ordinary production builds and delegates to canonical runtime
transactions. `test-shader-validation` likewise exists only to expose Naga-backed WGSL validation.
Neither specialized boundary enters the ordinary edit-loop build. Soaks intentionally use no feature
so fast and soak execution reuse one unit-test artifact.

`cargo test-lint` deliberately does not lint-compile test targets in the ordinary loop. `cargo
test-fast` immediately compiles and executes the complete default-feature unit-test target, so running
all-target Clippy first pays substantial duplicate compilation cost without adding type-check coverage.
Use `cargo test-lint-all` when test/harness lint coverage is material to the change or for release
hardening.

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
used only for diagnostics and postcondition checks. The policy does not clone `AppState` to simulate
compound future mutations that normal callers cannot preview. It chooses from current canonical
projections, knows scheduled-event timing supplied by the scenario, and reacts to the actual resulting
state after that event. Each scenario is deterministic from its printed seed and does not consume
unrelated simulation randomness. The required gate runs only maintained anchors; the explicit report
lane adds a small organic sample so exploratory review is not limited to one memorized script. A legal
scenario may complete zero batches when an in-flight job is suspended before its first output; that is
gameplay evidence, not a harness failure. Maintained seeds guarantee only stable input/policy diversity.
Balance-dependent outcomes such as completion, maintenance pressure, structural damage, suspension,
and relocation are reported as observations rather than frozen into aggregate pass/fail requirements.

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
crate. Configuration, execution contracts, probe setup, reporting, and seed mixing are separate modules
inside that one target so common edits invalidate less source while the lane still builds one specialized
artifact. Seed/configuration contracts are ordinary named tests with direct typed assertions instead of
one aggregated boolean-gap test, so failures point at the exact contract without adding another Cargo
target.

The maintained workshop loop covers current-state structural siting, finite power choice,
active-tick wear, exact replacement-stock maintenance, inventory-owned stored-matter loading,
persistent structural damage, production suspension, WIP recovery/stranding, and the current mixed-ore
processing frontier. The timed disruption is no longer a synthetic weather/load write: a real bulk
material transfer moves seeded starting matter into a mounted stockpile, and the inventory subsystem
updates the support's `StoredMatter` load through its canonical transaction. The harness chooses when
to attempt that transfer; this is not presented as an implemented logistics scheduler. Player
priorities vary by seed within bounded deterministic choices: reserve conservation, projected machine
condition, or
completion time. Safety, ownership, support, energy, and maintenance gates remain canonical regardless
of personality. Initial siting compares the canonical mount projections available in the current state;
the policy does not pre-apply the future delivery in a private cloned world. The known delivery time can
still influence a power choice when one real resolver projection finishes before the event and another
does not. After delivery, relocation decisions use the game's atomic equipment-relocation validator,
including its structural projection and commit, rather than a harness-only unmount/remount preview.

Scenario physics follow current authored content. Crusher batch mass is chosen inside the current
equipment limit; initial crusher condition varies across the full non-failed authored condition range,
so normal, warning, and critical starts can occur without hard-coding their frequencies; support geometry,
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

`cargo test-gameplay` is deterministic by default and keeps successful harness output captured. It runs
the five maintained anchor scenarios plus seed/configuration contracts and capability probes. This is
the required gameplay gate. `cargo test-gameplay-report` runs the ignored exploratory report instead;
it adds four organic scenarios, emits a replay-input line, the current authored equipment/process
catalog in stable ID order, sampled input ranges, compact outcome and systems summaries, and one scope
line distinguishing exercised runtime behavior from bootstrap/deferred systems. The catalog is
registry-derived rather than a second hand-maintained harness list, so newly authored workshop content
is visible without maintaining a duplicate list. Set
`DEEP_HEARTH_GAMEPLAY_VERBOSE` to any value
before that report lane to emit the detailed decision trace. Seed controls are:

- `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED`: extends the gate or report with four deterministic organic
  scenarios derived from one exact decimal or hexadecimal root seed; when omitted, the report uses a
  fixed maintained exploratory root so full and report lanes remain replayable;
- `DEEP_HEARTH_GAMEPLAY_SEEDS`: replaces the anchor-plus-organic plan with an exact comma-separated
  seed list for reproduction or deliberate sweeps.

Seed lists fail on an empty or malformed entry rather than silently dropping it. The anchors preserve
reproducible comparison and all three operating priorities. Organic sampling is deliberately outside
the required gate and defaults to a maintained root, so CI workload and results never depend on
wall-clock entropy. Hard failures remain canonical execution/invariant failures and capability-probe
conservation failures, not required balance outcomes. The input line prints the plan source,
anchor/organic/custom counts, variation root when applicable, and all exact scenario seeds so every
failure is replayable. Explicit custom seed lists are labeled as custom, run the same per-scenario
contracts, and do not claim aggregate outcome coverage.

## CI and completion gates

Pull requests and pushes to `main` keep expensive validation lanes independent so unrelated compilation
does not serially extend the feedback path. Pull requests use one cheap preflight job that checks out
the repository once, unit-tests the changed-path classifier, and emits the complete build plan.
Irrelevant downstream jobs are skipped before runner allocation, toolchain installation, checkout, or
cache restore. Documentation-only changes therefore avoid every Rust build. Harness-only changes run
rustfmt without production-library Clippy, and specialized gameplay/shader jobs skip known-unrelated
subsystem changes. The classifier uses only the Python standard library and treats unknown/new paths as
relevant, so the optimization fails safe rather than silently excluding new dependencies. Pushes to
`main` bypass the preflight entirely and start every lane directly as the post-merge backstop.

1. **Quality**: format and default-feature Clippy.
2. **Core + Soak**: ordinary and ignored long-horizon tests from one default-feature unit-test build.
3. **Gameplay**: the dedicated feature-gated gameplay integration target.
4. **Shaders**: the feature-gated Naga WGSL validation lane.

Jobs use locked dependencies, a pinned Rust toolchain, one shared Cargo dependency cache, source-aware
target caches, incremental compilation, bounded timeouts, and concurrency cancellation for superseded
runs. Core and gameplay caches cross-restore the common target directory so dependency artifacts and
feature-compatible incremental work can be reused even though gameplay enables its bootstrap-only
feature. Target keys still include the source trees each lane actually compiles, and restore prefixes
reuse prior artifacts after source changes. Splitting dependency downloads from target artifacts avoids
storing the same Cargo registry payload in every lane cache. Core and soak deliberately share one test
artifact, and gameplay deliberately does not compile the crate unit-test harness. The ordinary CI gate
intentionally does not rebuild the entire project in release mode or generate documentation on every
change.

Before committing, run:

```text
cargo fmt --check
cargo test-lint
cargo test-fast
```

`cargo test-check` remains available as an all-target compile-only diagnostic, but it is not part of
the normal pre-commit sequence. Production code is type-checked by `cargo test-lint`; default-feature
unit-test code is compiled and executed by `cargo test-fast`.

Also run `cargo test-soak`, `cargo test-gameplay`, or `cargo test-shaders` when the changed contract is
owned by that lane. Use `cargo test-all` when a cross-cutting change needs the complete debug inventory.
Release hardening is deliberately separate:

```text
cargo test-lint-all
cargo test-release
cargo test-doc
```
