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
| `cargo test-gameplay` | Workshop exercise harness plus harness configuration contracts | Adds `test-gameplay` |
| `cargo test-gameplay-report` | One workshop matrix with concise human-readable summary | Adds `test-gameplay` |
| `cargo test-shaders` | Naga parse/semantic validation of assembled WGSL without compiling the crate unit-test harness | Adds `test-shader-validation` |
| `cargo test-check` | Silent all-target compilation of the default feature set | Default features |
| `cargo test-lint` | Clippy with warnings denied | Default features |
| `cargo test-lint-all` | Cross-cutting/release Clippy audit | All test features |
| `cargo test-all` | Complete debug library inventory including specialized lanes | All test features |
| `cargo test-release` | Complete optimized library inventory | All test features |
| `cargo test-doc` | Documentation build without dependencies | Default features |

The `test-soak`, `test-gameplay`, and `test-shader-validation` features exist only to control test
compilation. They do not change default runtime behavior. Naga remains absent from the default
dependency graph and ordinary core test build.

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
consume unrelated simulation randomness.

The maintained workshop loop covers forecast-aware structural siting, finite power choice, active-tick
wear, exact replacement-stock maintenance, environmental disruption, persistent structural damage,
production suspension, WIP recovery/stranding, and the current mixed-ore processing frontier. The
maintenance path uses the production resolver and conserved repair transaction, not a harness-only
condition reset. The separate ore-preparation and foundry probes remain explicitly labeled capability
checks until concentration/smelting provides a truthful bridge between those stages.

`cargo test-gameplay` keeps successful harness output captured. `cargo test-gameplay-report` emits one
compact outcome line plus a `SYSTEMS` line summarizing player control, recovery, pressure, and current
bottlenecks. Set `DEEP_HEARTH_GAMEPLAY_VERBOSE` to any value before that report lane to emit the
detailed decision trace. Seed controls are:

- `DEEP_HEARTH_GAMEPLAY_EXPLORATORY_SEED`: replaces the one fixed exploratory seed;
- `DEEP_HEARTH_GAMEPLAY_SEEDS`: replaces the maintained matrix with an exact comma-separated seed
  list for reproduction or deliberate sweeps.

Seed lists fail on an empty or malformed entry rather than silently dropping it. When the maintained
matrix is used, its assertion reports named coverage gaps. Explicit custom seed lists prove only the
universal per-scenario contracts and do not inherit the maintained matrix's aggregate coverage claim.

## CI and completion gates

Pull requests and pushes to `main` run independent jobs so unrelated compilation does not serially
extend the feedback path:

1. **Quality**: format and default-feature Clippy.
2. **Core**: ordinary fast tests only.
3. **Soak**: feature-gated long-horizon ownership/invariant tests.
4. **Gameplay**: the feature-gated gameplay harness lane.
5. **Shaders**: the feature-gated Naga WGSL validation lane.

Jobs use locked dependencies, a pinned Rust toolchain, source-aware per-lane build caches, incremental
compilation, bounded timeouts, and concurrency cancellation for superseded runs. Cache keys retain the
dependency/toolchain prefix while advancing with source changes, so a lane can reuse its previous
incremental build instead of freezing the target cache at the first commit after a lockfile change.
The ordinary CI gate intentionally does not rebuild the entire project in release mode or generate
documentation on every change.

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
