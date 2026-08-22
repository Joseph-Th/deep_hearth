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
| Complete ordinary core behavior | `python ci.py audit --core` |
| One exact ordinary test | `python tools/run_test.py <qualified-name-or-unique-substring>` |
| Discover ordinary test names | `python tools/run_test.py --list [substring]` |
| One gameplay concern | `python ci.py gate --gameplay {workshop,survival,progression,ore,foundry}` |
| All maintained gameplay concerns | `python ci.py audit --gameplay` |
| Long-horizon invariants/conservation | `cargo test-soak` |
| Shader assembly and WGSL validation | `cargo test-shaders` |
| Markdown authority, links, routes, and source-doc policy | `python tools/check_authority_docs.py` |
| Rust API documentation build | `python ci.py gate --rustdoc` |
| Production-library lint | `python ci.py gate --lint` |
| Broad maintained core + gameplay checkpoint | `python ci.py audit --all` |

`python ci.py quick` is the default and intentionally performs no Cargo build. It runs formatting, the
compile-free repository contract checker, and millisecond-scale Python tests that keep the CI plan itself
from regressing into redundant builds. Those independent read-only checks run concurrently and are
reported in stable stage order, so adding cheap policy coverage does not linearly lengthen the edit loop.
`python ci.py gate` is the standard coherent checkpoint: with no
flags it adds only the production-library compile. Focused gameplay gates require an explicit scope.
Complete core behavior and all-gameplay verification are deliberately audit-only, so `gate --core` and
an unscoped/all `gate --gameplay` are rejected instead of turning a repair loop into a broad relink.

Git Wizard follows the same policy through Cargo metadata: `quick` is build-free and `standard` maps to
the production compile gate. There is intentionally no automatic `full` validation level because a
generic full request has no information about which expensive runtime surface changed. Broad audits are
invoked explicitly as `audit --core`, `audit --gameplay`, or `audit --all` only when that surface is
actually required.

Clippy is an explicit quality audit, not part of `quick` or `standard`. It uses a distinct compiler
wrapper and may require a cold dependency build, so run `python ci.py gate --lint` when lint
policy, broadly shared production code, or a deliberate quality sweep warrants it—not as an automatic
follow-up to every successful runtime audit.

Ordinary unit tests deliberately do not use compile-time shard features. `cargo test-fast` therefore
owns one reusable default-feature audit artifact, not the normal edit loop. An exact library test still
has to link that whole test binary when it is stale, so use `cargo check-fast` while implementation is
moving and run the exact executable proof once the behavior is coherent. `python tools/run_test.py
<qualified-name-or-unique-substring>` preflights the selector against a source-derived catalog before
Cargo. A unique partial selector resolves to one full test name; ambiguous selectors fail before
compilation instead of broadening execution. `--list` reads that catalog directly and invokes neither
Cargo, rustc, nor the linker. Integration targets infer their Cargo-declared `required-features`;
`--features` is only needed for additional feature-gated library tests or extra target features.

Unit-test bodies live in sibling test files referenced by the owning production module with
`#[cfg(test)] #[path = "..."] mod tests;`. The module identity and private access are unchanged, but the
test body is no longer an input to production-only Cargo builds. Editing an assertion, fixture, or test
diagnostic therefore does not invalidate a warm `cargo check-fast` artifact. Keep new unit-test bodies in
the sibling `*_tests.rs`/`mod_tests.rs` file instead of reintroducing inline `mod tests { ... }` blocks.

Broad audit runs are terminal checkpoints, not diagnostic loops, and the audit preset requires an
explicit scope instead of defaulting to every maintained artifact. When a broad audit exposes one defect,
repair that defect with `quick`, `cargo check-fast`, or the single failed exact/focused target. Do not
rerun the broad audit after every repair; rerun it once after the repair batch is complete and only when
that broad surface is actually required for completion.

For a gameplay target, use `cargo check --locked --features test-gameplay --test <target>` only when
type feedback is useful before execution; do not add permanent aliases or broad all-target checks for
diagnostics that are cheaper to scope directly.

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

Tests stay adjacent to the owning source module in sibling test files loaded only under `#[cfg(test)]`.
Use the smallest fixture and shortest canonical execution that prove the named rule.

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
| `--gameplay workshop` | Bootstrapped industrial workshop capability and consequential player-policy behavior |
| `--gameplay ore` | Bootstrapped, structurally installed crushing/grinding/screening/regrinding pipeline capability; not an agency claim |
| `--gameplay foundry` | Bootstrapped, structurally installed pure-copper heating/melting/casting pipeline capability; not an agency claim |

The industrial targets are capability evaluations, not claims of end-to-end runtime progression.
The ore/foundry targets deliberately report `agency=pipeline-evidence`; they prove physical integration
and installation obligations, while workshop counterfactuals own evidence that player policies actually
change outcomes. `STATUS.md` remains authoritative for acquisition and world-system availability.
Workshop agency counterfactuals hold both world variation and behavior RNG seed fixed across compared
policies. A maintained agency assertion therefore varies the named player policy only, not hidden random
input alongside it.

`python ci.py audit --gameplay` runs all five maintained concerns while linking only two broad-checkpoint
binaries: `gameplay_workshop` plus `gameplay_audit`, which compiles the four focused probe modules once.
`python ci.py report` reuses those same `test-gameplay` artifacts for an exploratory workshop sample,
maintained agency counterfactuals, and verbose survival/progression/ore/foundry evidence. It is not a
routine completion gate and deliberately does not introduce a second gameplay feature shape. Gameplay
target commands are composed in `ci.py` directly rather than duplicated across a family of Cargo aliases.
The focused concerns intentionally remain separate test binaries for targeted gates: an edit to survival
provisioning does not relink the ore/foundry harness merely to prove the survival contract. The explicit
broad audit trades that isolation for one consolidated focused-probe link, avoiding four redundant
checkpoint links. Failures from the consolidated target point back to the corresponding narrow focused
target whenever Cargo reports the exact failed test.
Cargo target auto-discovery is disabled for binaries, examples, integration tests, and benches. Every
independently linked local tool or gameplay binary is listed explicitly in `Cargo.toml`; shared harness
modules stay below `tests/gameplay_harness/` so adding a helper cannot silently create another executable
and expand the build graph.

### Variation and replay

Maintained gameplay tests combine fixed semantic anchors with a minimal deterministic variation sample.
Each focused concern runs its maintained anchor plus one generated case; the workshop gate retains all
of its distinct semantic anchors plus one generated case. Explicit seed lists remain the mechanism for
deliberate multi-seed sweeps. Generated cases print replay inputs. Hard assertions cover legality,
ownership, conservation, persistence, authored capability agreement, catalog/reachability boundaries,
and other balance-independent contracts. Balance-sensitive outcomes remain observations unless an anchor
explicitly owns them.

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
- `python ci.py gate --gameplay {workshop,survival,progression,ore,foundry}`: one maintained gameplay target.
- `python ci.py audit --core`: complete ordinary core behavior.
- `python ci.py audit --gameplay`: all maintained gameplay targets.
- `python ci.py gate --soak`: ignored long-horizon soak coverage only; ordinary core behavior remains a separate audit.
- `python ci.py gate --shaders`: shader validation.
- `python ci.py gate --rustdoc`: Rust API documentation build.
- `python ci.py audit --all`: broad maintained core + gameplay checkpoint; use only when both surfaces are required.

Choose the smallest completion gate that covers the changed contract. Add specialized lanes only when
their distinct surface changed. `gate` deliberately cannot launch the complete core suite or all five
gameplay binaries, and unscoped `audit` is rejected rather than silently selecting both. Do not rerun
narrower lanes after a broader selected lane already covers them, and do not run build-producing lanes
merely because another file was edited.
