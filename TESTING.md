# Testing

This page owns test organization, gameplay-harness contracts, and local verification. Use
[`README.md`](README.md) for routing and [`STATUS.md`](STATUS.md) for runtime scope.

Use the smallest lane that completely proves the changed contract.

## Fast path

| Need | Command |
| --- | --- |
| Documentation/contracts | `python tools/check_authority_docs.py` |
| Build-free edit loop | `python ci.py quick` |
| Production compile | `cargo check-fast` |
| Standard production gate | `python ci.py gate` |
| List tests without building | `python tools/run_test.py --list [substring]` |
| Type-check a test target without linking | `python tools/run_test.py --check <qualified-name-or-unique-substring>` |
| Run one exact unit/integration test | `python tools/run_test.py <qualified-name-or-unique-substring>` |
| Focused gameplay | `python ci.py gate --gameplay {workshop,survival,progression,ore,foundry}` |
| Core audit | `python ci.py audit --core` |
| Gameplay audit | `python ci.py audit --gameplay` |
| Core + gameplay audit | `python ci.py audit --all` |
| Clippy | `python ci.py gate --lint` |
| Shader validation | `python ci.py gate --shaders` |
| Rustdoc | `python ci.py gate --rustdoc` |
| Long-horizon soak | `python ci.py gate --soak` |
| Gameplay exploration report | `python ci.py report` |
| Changed-source BCA review | `python tools/check_bca.py review --changed --since HEAD [--path <scope>]` |

`python ci.py quick` runs formatting, the cognitive-complexity ratchet, documentation contracts, and local CI
contracts without building Rust. Do not pair a compile-only command with an executable lane that already
compiles the same surface.

For Rust test iteration, use `--check` while code is changing, then run the exact test for behavioral proof.
The selector is resolved from source before Cargo runs, so missing or ambiguous test names fail without a
build. After a broad gameplay failure, rerun the exact failing test on `gameplay_audit` first to reuse the
already-linked target.

## Complexity review

`bca.toml` and `.bca-baseline.toml` own the cognitive-complexity ratchet used by `python ci.py quick`.
New or worsened over-threshold cognitive complexity fails the ratchet. Other BCA metrics are advisory.

Use `python tools/check_bca.py review --changed --since HEAD` for nontrivial refactors. Add repeated `--path`
filters when the task is already scoped. `report` and `diff` remain available for custom analysis. Treat BCA as
diagnostic evidence: simplify code when the result supports a clearer design; do not split cohesive code or
refresh the baseline only to improve a score.

## Unit tests

Unit-test bodies live beside their owner in `*_tests.rs` or `mod_tests.rs` and are included with
`#[cfg(test)] #[path = "..."] mod tests;`.

Assertions prove durable contracts:

- rejection: typed error and unchanged authoritative state when atomicity matters;
- success: resulting identity, quantity, lifecycle, relationship, ownership, or other durable state;
- conservation: totals across authoritative owners;
- persistence: serialized continuation and trusted-load admission for state that survives load;
- authored values: read from registries instead of duplicating balance constants.

Avoid assertions on error prose, wall-clock duration, incidental ordering, transient implementation counts,
or balance values outside the test's owned contract.

Soak tests are ignored tests whose qualified name includes `soak`. Use them only when repeated ownership,
persistence, conservation, or numerical accumulation adds evidence that focused tests cannot provide.

## Gameplay harness

`tests/gameplay_harness/` evaluates player-facing behavior through production APIs. Controlled setup may
create capability-only state; setup does not make that state ordinarily reachable.

### Actor boundary

After setup, actor code must:

- use production validators, resolvers, commits, and simulation ticks;
- read only observable runtime state, actor policy, and canonical projections;
- never inspect hidden geology, future controlled events, setup authorization, or cloned-state previews;
- preserve normal ownership, persistence, conservation, capability, and survival rules;
- keep balance-sensitive measurements observational unless a test explicitly owns the threshold.

`src/content/gameplay_fixture.rs` owns setup-only helpers.

### Targets

| Target | Contract |
| --- | --- |
| `survival` | Hunger/thirst pressure, food-category availability, preservation, diet tradeoffs, provisioning, activity cost, reserve recovery, actual diet-supported vitality recovery. |
| `progression` | Evidence acquisition and information-value decisions, primitive crafting/mining/power/processing, scarce-copper choice, delegated work, second reinforcement, convergence, finite machine lifecycle. |
| `workshop` | Installed industrial operation under finite work, survival, wear, maintenance, structural pressure, hidden world change, and recovery. |
| `ore` | Installed crush/grind/screen/regrind/concentrate flow with selective recovery, exact constituent accounting, physical tailings. Capability-only. |
| `foundry` | Installed pure-copper heating/melting/casting, finite electrical and thermal capacity, adaptive batches, molten remainder, passive sink recovery. Capability-only. |

### Gameplay evidence contract

Routine gameplay gates combine stable maintained cases with one fresh bounded organic world per evaluated
concern. Maintained anchors and named coverage cases own strict regression claims. Organic cases keep the
harness exposed to nearby legal game states and player choices; they may complete, adapt, or stop on a
recognized canonical constraint without inheriting an anchor's balance-specific success requirement.

Fresh sampling is reproducible rather than fixed: every gate prints the realized variation root and/or exact
world seeds. Set `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` to replay the same physical sample and
`DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` to replay workshop policy variation. Explicit
`DEEP_HEARTH_GAMEPLAY_SEEDS` values run exactly the requested focused worlds.

The progression probe must demonstrate player-visible evidence, an observable scarce-copper choice, physical
consequences for both branches, useful concurrent work during delegated processing, convergence on the next
capability, and bounded wear/lifecycle evidence. Maintained regression worlds retain the deferred-survey
archetype where a direct-source shortage makes better information worth acquiring. Organic/replay worlds may
instead receive enough cheap surface evidence to rule out a dominated occurrence and skip a redundant survey
and extraction sample. Both paths must make that decision from acquired evidence; the actor must not read
hidden geology or choose from counterfactual outcomes. The review reports both the pick's mining-attention
reduction and the crank's power/charge-attention effect so the scarce investment is evaluated by its physical
consequences rather than only by branch labels.

The survival probe treats food availability as part of the world rather than forcing every world to contain a
meaningful diet choice. If the available supply lacks part of the authored diet set, the review labels that
choice `supply-collapsed`. When the full diet set is available, matched compact and balanced provisioning
branches recover from a real vitality deficit through normal simulation ticks and must demonstrate the
resulting vitality difference, not merely a projected recovery-rate difference.

Workshop regression starts from installed finite infrastructure. The actor chooses from observable condition,
stored work, survival reserve, structural margin, and process projections. Controlled world events remain
hidden until they occur. The gameplay audit adds matched-policy counterfactuals with the physical world and
behavior RNG held fixed.

All recognized partial/blocked outcomes must leave trusted-load-valid state and preserve relevant
conservation invariants. Unexpected resolver, commit, ownership, or persistence failures are hard failures.
Harness logic asks production resolvers for feasible actions rather than duplicating capability, energy, wear,
timing, or yield calculations.

### Exploration and replay

`python ci.py report` expands the organic sample beyond the routine gate and prints aggregate behavioral
evidence plus exact replay inputs. It is an exploration/diagnostic surface, not an additional required gate.
The compact report retains the maintained anchor plus every bounded organic focused outcome (currently two
organic worlds per focused concern). The anchor gives the cold agent a stable reference capability while the
organic worlds show whether choices, blockers, and information paths actually vary; named coverage-only
diagnostics remain filtered. Use verbose or trace output only when needed.

| Variable | Meaning |
| --- | --- |
| `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` | physical variation root |
| `DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` | workshop policy root |
| `DEEP_HEARTH_GAMEPLAY_SEEDS` | exact comma-separated world seeds |
| `DEEP_HEARTH_GAMEPLAY_VERBOSE` | expanded decisions, blockers, tradeoffs, focused-probe diagnostics |
| `DEEP_HEARTH_GAMEPLAY_TRACE` | operation-level workshop narration plus verbose diagnostics |

Generated samples are deliberately small so routine gameplay still has one build-producing lane and fast
runtime. Increase sample breadth in the report or explicit replay/sweep inputs rather than turning the edit
loop into a multi-seed soak.

## Completion

Run only the lanes required by the changed contract. Broad audits are explicit checkpoints, not default edit
loops. Verification is local; hosted CI is outside the project contract.
