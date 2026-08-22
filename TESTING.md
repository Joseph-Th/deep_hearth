# Testing

This document owns test organization, gameplay-harness rules, and local verification. Use
[`README.md`](README.md) for repository routing and [`STATUS.md`](STATUS.md) for the current capability
boundary. Choose the smallest command that proves the changed contract; build-producing checks are
checkpoints, not edit hooks.

## Commands

| Need | Command |
| --- | --- |
| Build-free edit-loop checks | `python ci.py quick` |
| Production compile | `cargo check-fast` |
| Standard production checkpoint | `python ci.py gate` |
| One exact unit test | `python tools/run_test.py <qualified-name-or-unique-substring>` |
| List unit tests without building | `python tools/run_test.py --list [substring]` |
| One gameplay concern | `python ci.py gate --gameplay {workshop,survival,progression,ore,foundry}` |
| All core tests | `python ci.py audit --core` |
| All maintained gameplay concerns | `python ci.py audit --gameplay` |
| Core + gameplay audit | `python ci.py audit --all` |
| Long-horizon soak tests | `python ci.py gate --soak` |
| Shader validation | `python ci.py gate --shaders` |
| Rust API documentation | `python ci.py gate --rustdoc` |
| Clippy with warnings denied | `python ci.py gate --lint` |
| Maintained Markdown links/routes + source-module docs | `python tools/check_authority_docs.py` |
| Human-readable gameplay report | `python ci.py report` |

`python ci.py quick` is build-free. It checks formatting, repository/documentation contracts, and the
local CI plan. Use it freely while editing.

`python ci.py gate` adds one production-library compile. Specialized gate flags replace that compile
with one focused build-producing lane. Complete core and all-gameplay suites are audit-only so an
ordinary repair loop cannot accidentally become a broad rebuild.

Do not run a compile-only command immediately before or after a test that compiles the same changed
surface. After a broad audit exposes one failure, repair with `quick`, `cargo check-fast`, or the exact
failed test; rerun the broad audit only after the repair batch is complete.

## Unit tests

Unit-test bodies live beside their owning production module in `*_tests.rs` or `mod_tests.rs` and are
loaded through `#[cfg(test)] #[path = "..."] mod tests;`. Keep test bodies out of production source files
so test-only edits do not invalidate production-only Cargo artifacts.

`python tools/run_test.py` resolves an exact or uniquely matching source test name before invoking
Cargo. Ambiguous or missing selectors fail without building. Integration-test targets infer their
Cargo-declared required features.

Assertions should prove durable contracts:

- rejection tests assert the typed error and unchanged authoritative state when atomicity matters;
- success tests assert the resulting identity, quantity, lifecycle, relationship, ownership, or other
  durable outcome;
- conservation-sensitive tests compare authoritative totals across represented owners;
- do not assert error prose, wall-clock duration, incidental ordering, transient implementation counts,
  or balance outcomes that the test does not explicitly own;
- query registries for authored values instead of copying balance constants into tests.

Long-horizon tests include `soak` in the qualified name and use `#[ignore = "long-horizon soak"]`.
Add a soak only when repeated ownership, persistence, conservation, or numerical accumulation provides
evidence a narrow test cannot.

## Gameplay harness

`tests/gameplay_harness/` is the headless player-facing evaluation surface. After controlled setup, it
uses canonical runtime validators, resolutions, commits, and simulation ticks.

`src/content/gameplay_fixture.rs` may bootstrap state that normal play cannot yet create. Setup happens
before actor policy begins and setup-only authorizations are single-use. Actor-facing code may read only
observable state, policy, and canonical resolver projections. It must not inspect hidden geological
truth, hidden future events, setup-only state, or cloned `AppState` previews.

### Evidence by target

| Target | Contract |
| --- | --- |
| `survival` | Runtime preservation, eating, drinking, and reserve recovery after controlled bootstrap |
| `progression` | Runtime primitive crafting, assembly, mining, upgrades, manual power, and mechanization |
| `workshop` | Industrial workshop capability plus matched-world player-policy consequences |
| `ore` | Bootstrapped installed crushing, grinding, screening, and regrinding capability |
| `foundry` | Bootstrapped installed pure-copper heating, melting, and casting capability |

`ore` and `foundry` are capability tests, not end-to-end acquisition claims. `STATUS.md` is authoritative
for runtime reachability. Workshop agency comparisons hold world variation and behavior RNG fixed except
for the policy being compared.

Broad gameplay audit links `gameplay_workshop` and `gameplay_audit`; the latter contains the four focused
probe modules. Focused gates keep their own binaries so repairing one concern does not require relinking
the others. Broad-audit failures map back to the corresponding focused target and exact test.

### Variation and replay

Maintained gameplay tests combine fixed semantic anchors with a small deterministic variation sample.
Hard assertions cover legality, ownership, conservation, persistence, authored capability agreement,
information boundaries, and other balance-independent contracts. Balance-sensitive results are report
observations unless an anchor explicitly owns them.

Replay controls:

- `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED`: physical variation root;
- `DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED`: workshop policy root;
- `DEEP_HEARTH_GAMEPLAY_SEEDS`: exact comma-separated world-seed list;
- `DEEP_HEARTH_GAMEPLAY_VERBOSE`: detailed exploratory output.

Malformed explicit seeds fail configuration.

## Completion

Use the smallest lane that completely covers the changed contract. Specialized surfaces such as soak,
shader, Rustdoc, or lint are required only when that surface changed or the task explicitly calls for
them. Do not rerun narrower checks after a broader selected lane already covers them.

Verification is local. The repository does not use GitHub Actions or hosted CI.
