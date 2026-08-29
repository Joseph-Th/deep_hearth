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
| Run one owner/subsystem test group | `python tools/run_test.py --suite <qualified-prefix-or-substring>` |
| Focused gameplay | `python ci.py gate --gameplay {workshop,survival,progression,ore,foundry}` |
| Core audit | `python ci.py audit --core` |
| Gameplay audit | `python ci.py audit --gameplay` |
| Core + gameplay audit | `python ci.py audit --all` |
| Clippy | `python ci.py gate --lint` |
| Shader validation | `python ci.py gate --shaders` |
| Rustdoc | `python ci.py gate --rustdoc` |
| Long-horizon soak | `python ci.py gate --soak` |
| Gameplay exploration report | `python ci.py report` |
| Changed-source BCA review | `python ci.py bca [--path <scope>] [--since <revision>]` |
| Current BCA hotspot review | `python ci.py bca --hotspots [--path <scope>] [--since <revision>]` |

`python ci.py quick` is the build-free repository-policy loop. Use `cargo check-fast` for production type
checking, `run_test.py --check` for test-code type checking, and an exact test or suite for executable proof.
Do not pair a compile-only lane with an executable lane that already compiles the same surface.

Exact test selectors must resolve uniquely. Suite selectors must match at least one test. Both fail before
building when the source catalog cannot satisfy the request. Focused gameplay targets share the same
`test-gameplay` feature shape as the broad gameplay audit.

## Complexity review

`bca.toml` and `.bca-baseline.toml` own the cognitive-complexity ratchet used by `python ci.py quick`.
New or worsened over-threshold cognitive complexity fails the ratchet; other BCA metrics are advisory.

Use `python ci.py bca` to review changed code and `python ci.py bca --hotspots` to identify current high-cost
areas. Scope with `--path` or change the comparison base with `--since`. Treat metrics as diagnostic evidence:
refactor only when the result supports clearer ownership or control flow. Do not fragment cohesive code or
refresh the baseline merely to improve a score.

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

`src/content/gameplay_fixture.rs` owns controlled scenario construction before actor admission.

### Evidence modes

| Mode | Surface | Supported conclusion |
| --- | --- | --- |
| Ordinary/runtime experience | `survival` and `progression` focused episodes | Canonical mechanics and automated-player outcomes under the documented observable policy and ordinary acquisition routes. |
| Controlled capability | `workshop`, `ore`, and `foundry` | Canonical mechanics under disclosed prearranged infrastructure. This is capability evidence, not ordinary reachability evidence. |
| Counterfactual | Matched branches inside focused probes | Action-attributable differences from the same actor-visible decision state over the same comparison horizon. |
| Exploratory | `python ci.py report` and explicit replay/sweep inputs | Bounded discovery, diagnostics, and reproducible examples. Exploration does not add a pass/fail requirement to routine gates. |

Automation can establish reproducible mechanical consequences, blockers, conservation, persistence, and the
behavior of the documented actor policy for evaluated inputs. It does not establish human comprehension,
enjoyment, subjective fairness or balance, visual quality, likely human strategy, or population/world
frequencies beyond the declared sample. Absence from a bounded search is unverified unless canonical production
logic or an exhaustive check establishes unavailability.

### Gameplay scopes

Each gameplay scope has a focused target. `gameplay_audit` is the broad checkpoint and report surface. All
use the same `test-gameplay` feature contract.

| Scope | Contract |
| --- | --- |
| `survival` | Hunger/thirst pressure, exact authored food-option availability, food-category coverage, bounded quantity-scaled eating/drinking attention, ordinary raw-timber -> manual-board -> manual-chest-body -> preservation-enclosure construction, completed-profile compatibility for existing contents, non-retroactive preservation-state effects, diet tradeoffs, provisioning, varied prospecting-work cost, reserve recovery, actual diet-supported vitality recovery. |
| `progression` | Coarse-to-fine evidence acquisition and information-value decisions, primitive crafting/mining/power/processing, scarce-copper choice, direct-labor fallback versus mechanization, delegated work, finite-recovery sorting, flywheel self-discharge/recharge, second reinforcement, convergence, finite machine lifecycle, and material-backed service that preserves prior scarce upgrades. |
| `workshop` | Installed industrial operation under finite work, survival, wear, maintenance, structural pressure, hidden world change, and recovery. |
| `ore` | Installed crush/grind/screen/regrind/concentrate flow with selective recovery, exact constituent accounting, gangue-hosted prepared-feed acceptance, and terminal current-tier tailings. Capability-only. |
| `foundry` | Installed pure-copper heating/melting/casting, finite electrical and thermal capacity, adaptive batches, molten remainder, passive sink recovery. Capability-only. |

### Gameplay evidence contract

Gameplay harnesses use production owners as the rules authority. Controlled setup may establish unavailable
prerequisites, but actor legality and consequences come from registries, resolvers, validators, commits, and
simulation ticks. Harness policy must not duplicate balance values or physical formulas.

Routine focused gates combine maintained regression cases with bounded reproducible variation. Full episodes
are reserved for behavior that requires executed cross-system consequences. A world may succeed, adapt, or stop
at a canonical constraint; every partial or blocked outcome must preserve trusted-load validity and relevant
conservation.

Actor decisions may use observable consequences such as attention, material demand, survival cost, throughput,
capacity, condition, and acquired evidence. They must not use registry order, hidden geology, future controlled
events, comparison-branch outcomes, or other implementation identity as tie-breakers. Observable ties require
an explicit actor policy.

Counterfactual evaluation may compute a shared observation horizon outside actor policy, then replay matched
branches to that fixed tick. Branch outcomes and future controlled events never enter actor choice. Assertions
follow production equivalence: when production treats multiple internal representations as equivalent, the
harness checks their aggregate observable contract.

Fresh sampling is replayable. `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` controls physical-world variation;
`DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` independently controls actor policy where applicable;
`DEEP_HEARTH_GAMEPLAY_SEEDS` selects explicit focused worlds. Failure output must retain the replay input.

### Exploration and replay

`python ci.py report` is the exploration surface for bounded gameplay variation and exact replay inputs. It is
not an additional required gate. Default output summarizes registry/acquisition state and player-visible
behavior. Use verbose output for full registry-derived catalogs and focused accounting; use trace for
operation-level workshop execution.

| Variable | Meaning |
| --- | --- |
| `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` | physical variation root |
| `DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` | actor-policy root for scopes that expose behavior choices |
| `DEEP_HEARTH_GAMEPLAY_SEEDS` | exact comma-separated world seeds |
| `DEEP_HEARTH_GAMEPLAY_VERBOSE` | expanded decisions, blockers, tradeoffs, focused-probe diagnostics |
| `DEEP_HEARTH_GAMEPLAY_TRACE` | operation-level workshop narration plus verbose diagnostics |

Keep routine samples bounded. Increase breadth through the report or explicit replay/sweep inputs rather than
turning the edit loop into a multi-seed soak.

## Completion

Run only the lanes required by the changed contract. Broad audits are explicit checkpoints, not default edit
loops. Verification is local; hosted CI is outside the project contract.
