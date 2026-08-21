# Testing

This document owns Deep Hearth's test selection, feedback lanes, harness contract, and CI gate. Use
[`README.md`](README.md) for authority and subsystem routing and [`STATUS.md`](STATUS.md) for the current
runtime capability boundary. The suite is organized so ordinary correctness work does not compile
specialized gameplay or shader validation dependencies unless that coverage is relevant.

## Validation selection

Run the smallest artifact that proves the changed contract. Do not run a compile-only lane immediately
before an executable lane that compiles the same surface.

| Need | Command |
| --- | --- |
| Fast production compile | `cargo check-fast` |
| One ordinary subsystem family | `cargo test-unit-{foundation,resources,player,industry,render}` |
| Complete ordinary core behavior | `cargo test-fast` |
| One gameplay concern | `cargo test-gameplay-{workshop,survival,progression,ore,foundry}` |
| All maintained gameplay concerns | `cargo test-gameplay` |
| Long-horizon invariants/conservation | `cargo test-soak` |
| Shader assembly and WGSL validation | `cargo test-shaders` |
| Documentation links, routes, aliases, and Rust docs | `python ci.py gate --docs` |
| Production-library lint | `python ci.py gate --lint` |
| Broad maintained core + gameplay checkpoint | `python ci.py audit` |

`python ci.py gate` runs formatting plus the production-library compile. Add only the lane flags owned
by the change: `--unit <scope>`, `--gameplay [scope]`, `--soak`, `--shaders`, `--docs`, or `--lint`.
`--unit all` and `--core` select the complete ordinary core suite.

The unit scopes are `foundation`, `resources`, `player`, `industry`, and `render`. They compile-select
ordinary colocated `#[cfg(test)]` modules; they do not define alternate behavior. `cargo test-fast`
remains the complete default-feature unit-test inventory.

Useful compile-only diagnostics are `cargo check-tests`, `cargo check-gameplay`, focused
`cargo check-gameplay-{workshop,survival,progression,ore,foundry}`, and `cargo check-all`. Use them only
when type feedback is useful before execution.

Specialized feature shapes remain isolated from ordinary builds:

- `test-soak` contains ignored long-horizon tests;
- `test-gameplay` exposes the controlled gameplay fixture boundary;
- `test-gameplay-full` adds cross-probe code for the exploratory gameplay report;
- `test-shader-validation` adds Naga-backed WGSL validation;
- `test-unit-*` selects one ordinary unit-test shard.

`python tools/check_authority_docs.py` validates authority-page links, repository routes, Cargo aliases,
and the README/STATUS/TESTING authority graph. `cargo test-doc` validates Rust documentation.
`python ci.py gate --docs` runs both.

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
| `cargo test-gameplay-survival` | Runtime survival provisioning, preservation, eating, and drinking after controlled world bootstrap |
| `cargo test-gameplay-progression` | Runtime primitive crafting, assembly, mining, upgrades, manual power, and primitive mechanization after controlled world bootstrap |
| `cargo test-gameplay-workshop` | Bootstrapped industrial workshop capability and policy behavior |
| `cargo test-gameplay-ore` | Bootstrapped crushing, grinding, screening, and regrinding capability |
| `cargo test-gameplay-foundry` | Bootstrapped pure-copper heating, melting, and casting capability |

The industrial targets are capability evaluations, not claims of end-to-end runtime progression.
`STATUS.md` remains authoritative for acquisition and world-system availability.

`cargo test-gameplay` runs all five maintained targets. `cargo test-gameplay-report` runs the broader
`test-gameplay-full` exploratory report for cold-agent understanding of the current game; it is not a
routine completion gate.

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

- `python ci.py gate`: formatting + production compile.
- `python ci.py gate --unit <scope>`: one ordinary subsystem family.
- `python ci.py gate --gameplay [scope]`: one or all maintained gameplay targets plus static gameplay
  command-policy validation.
- `python ci.py gate --soak`: complete core behavior plus long-horizon soak coverage.
- `python ci.py gate --shaders`: shader validation.
- `python ci.py gate --docs`: authority-document and Rust-documentation validation.
- `python ci.py audit`: broad maintained core + gameplay checkpoint.

Choose the smallest completion gate that covers the changed contract. Add specialized lanes only when
their distinct surface changed. Do not rerun narrower lanes after a broader selected lane already covers
them.
