# Testing

This document owns Deep Hearth's test selection, feedback lanes, harness contract, and local CI. Use
[`README.md`](README.md) for authority and subsystem routing and [`STATUS.md`](STATUS.md) for the current
runtime capability boundary. Ordinary iteration keeps one default-feature unit-test artifact and
compiles specialized gameplay, soak, shader, or Rustdoc shapes only when their distinct contract changed.

## Validation selection

Run the smallest artifact that proves the changed contract. Build-producing verification is a checkpoint,
not an edit hook: batch related edits until the implementation is coherent, then compile or execute the
smallest relevant surface once. Do not run a compile-only lane immediately before an executable lane that
compiles the same surface.

| Need | Command |
| --- | --- |
| Build-free edit-loop sanity | `python ci.py quick` |
| Fast production compile | `cargo check-fast` |
| Coherent production checkpoint | `python ci.py gate` |
| Complete ordinary core behavior | `cargo test-fast` |
| One exact ordinary test | `python tools/run_test.py <qualified-name>` |
| Discover ordinary test names | `python tools/run_test.py --list [substring]` |
| One gameplay concern | `python ci.py gate --gameplay {workshop,survival,progression,ore,foundry}` |
| All maintained gameplay concerns | `python ci.py gate --gameplay` |
| Long-horizon invariants/conservation | `cargo test-soak` |
| Shader assembly and WGSL validation | `cargo test-shaders` |
| Markdown authority, links, routes, and source-doc policy | `python tools/check_authority_docs.py` |
| Rust API documentation build | `python ci.py gate --rustdoc` |
| Production-library lint | `python ci.py gate --lint` |
| Broad maintained core + gameplay checkpoint | `python ci.py audit` |

`python ci.py quick` is the default and intentionally performs no Cargo build. It runs formatting, the
compile-free repository contract checker, and millisecond-scale Python tests that keep the CI plan itself
from regressing into redundant builds. `python ci.py gate` is the standard coherent checkpoint: with no
flags it adds the production-library compile. Add only the build-producing lane owned by the change:
`--core`, `--gameplay [scope]`, `--soak`, `--shaders`, `--rustdoc`, or `--lint`.

Git Wizard follows the same policy through Cargo metadata: `quick` is build-free, `standard` maps to the
production compile gate, and `full` maps to the broad maintained audit. Do not request `standard` or
`full` after each mutation. Use them after a coherent batch or once at task completion as appropriate.

Clippy is an explicit quality audit, not part of `quick`, `standard`, or `full`. It uses a distinct
compiler wrapper and may require a cold dependency build, so run `python ci.py gate --lint` when lint
policy, broadly shared production code, or a deliberate quality sweep warrants it—not as an automatic
follow-up to every successful runtime audit.

Ordinary unit tests deliberately do not use compile-time shard features. Test execution is cheap relative
to building another crate feature shape, so `cargo test-fast` keeps one reusable default-feature test
binary. For diagnosis after that artifact exists, `python tools/run_test.py <qualified-name>` runs one
fully qualified test and fails when the selector is stale or empty. `python tools/run_test.py --list`
exposes libtest's live catalog; optional substring filtering happens after discovery and never creates
another test build configuration.

`cargo check-all` is the explicit all-target compile diagnostic. For a gameplay target, use
`cargo check --locked --features test-gameplay --test <target>` only when type feedback is useful before
execution; do not add another permanent alias for every target.

On the maintained Windows workstation Cargo uses LLVM `lld-link` for Rust test and gameplay binaries.
This is a local iteration optimization; the project does not require hosted CI portability.

Specialized feature shapes remain isolated from ordinary builds:

- `test-soak` contains ignored long-horizon tests;
- `test-gameplay` exposes the controlled gameplay fixture boundary;
- `test-shader-validation` adds Naga-backed WGSL validation.

`python tools/check_authority_docs.py` validates authority-page links, repository routes, Cargo aliases,
the README/STATUS/TESTING authority graph, and required `//!` module-purpose headers across `src/`.
This checker is part of `python ci.py quick` because it is compile-free and normally completes in a
fraction of a second. `cargo test-doc` validates Rust API documentation and is owned separately by
`python ci.py gate --rustdoc` so prose-only edits do not trigger a Rustdoc build.

## Test structure and assertions

Tests stay colocated with the owning source module under `#[cfg(test)]`. Use the smallest fixture and
shortest canonical execution that prove the named rule.

- Rejections assert the exact typed error and unchanged authoritative state when atomicity matters.
- Successes assert the identity, quantity, lifecycle, relationship, or durable result that defines the
  contract.
- Do not assert human-readable error prose, wall-clock timing, incidental ordering, or transient
  implementation counts.
- Balance-dependent outcomes are observations unless a maintained test explicitly owns that outcome.
- Query authoritative registries instead of copying balance constants into assertions.

Long-horizon tests include `soak` in the qualified name and use
`#[ignore = "long-horizon soak"]`. Add a subsystem soak only when repeated ownership, conservation,
persistence, or numerical accumulation provides evidence a narrow test cannot.

## Gameplay harness

`tests/gameplay_harness/` is the headless player-facing evaluation surface. It executes canonical runtime
validators, resolutions, commits, and simulation ticks while making fixture-only setup explicit.

### Information boundary

`src/content/gameplay_fixture.rs` may bootstrap state that the runtime cannot yet create through play.
Setup happens before actor policy begins. Setup-only authorizations are single-use.

Actor-facing code may read only observable state, player policy, and canonical resolver projections. It
must not call setup helpers, enumerate hidden geological truth, inspect hidden future event state, or
clone `AppState` to preview compound mutations. Hidden setup/controller state is separate from actor
context and may be used only for event injection, diagnostics, and postconditions.

After setup, inventory movement, support changes, maintenance, production, mining, survival, manual
power, and time progression use canonical runtime transactions.

### Evidence classes

| Target | Evidence |
| --- | --- |
| `--gameplay survival` | Runtime survival provisioning, preservation, eating, and drinking after controlled world bootstrap |
| `--gameplay progression` | Runtime primitive crafting, assembly, mining, upgrades, manual power, and primitive mechanization after controlled world bootstrap |
| `--gameplay workshop` | Bootstrapped industrial workshop capability and policy behavior |
| `--gameplay ore` | Bootstrapped crushing, grinding, screening, and regrinding capability |
| `--gameplay foundry` | Bootstrapped pure-copper heating, melting, and casting capability |

The industrial targets are capability evaluations, not claims of end-to-end runtime progression.
`STATUS.md` remains authoritative for acquisition and world-system availability.

`python ci.py gate --gameplay` runs all five maintained targets. `python ci.py report` reuses those same
`test-gameplay` artifacts for an exploratory workshop sample, maintained agency counterfactuals, and
verbose survival/progression/ore/foundry evidence. It is not a routine completion gate and deliberately
does not introduce a second monolithic gameplay feature shape. Gameplay target commands are composed in
`ci.py` directly rather than duplicated across a family of Cargo aliases.

### Variation and replay

Maintained gameplay tests combine fixed anchors with small deterministic bounded variations. Generated
cases print replay inputs. Hard assertions cover legality, ownership, conservation, persistence,
authored capability agreement, catalog/reachability boundaries, and other balance-independent contracts.
Balance-sensitive outcomes remain observations unless an anchor explicitly owns them.

Replay controls:

- `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED`: physical variation root;
- `DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED`: workshop-policy root;
- `DEEP_HEARTH_GAMEPLAY_SEEDS`: exact comma-separated world-seed sweep;
- `DEEP_HEARTH_GAMEPLAY_VERBOSE`: detailed exploratory path output.

Malformed explicit seeds fail configuration.

## Completion gates

Verification runs locally through `ci.py` and Cargo aliases. GitHub Actions and hosted runners are not
part of the repository contract.

- `python ci.py quick`: build-free formatting + repository-contract check; use during editing.
- `python ci.py gate`: standard coherent checkpoint; formatting/contracts + production compile.
- `python ci.py gate --core`: complete ordinary core behavior.
- `python ci.py gate --gameplay [scope]`: one or all maintained gameplay targets.
- `python ci.py gate --soak`: complete core behavior plus long-horizon soak coverage.
- `python ci.py gate --shaders`: shader validation.
- `python ci.py gate --rustdoc`: Rust API documentation build.
- `python ci.py audit`: broad maintained core + gameplay checkpoint.

Choose the smallest completion gate that covers the changed contract. Add specialized lanes only when
their distinct surface changed. Do not rerun narrower lanes after a broader selected lane already covers
them, and do not run build-producing lanes merely because another file was edited.
