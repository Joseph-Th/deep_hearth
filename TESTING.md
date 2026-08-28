# Testing

This page owns test organization, gameplay-harness contracts, and local verification. Use
[`README.md`](README.md) for routing and [`STATUS.md`](STATUS.md) for runtime reachability.

Choose the smallest command that completely proves the changed contract.

## Verification commands

| Need | Command |
| --- | --- |
| Documentation/contracts only | `python tools/check_authority_docs.py` |
| Build-free edit loop | `python ci.py quick` |
| Complexity and change-risk report | `python tools/check_bca.py report` |
| Changed-code complexity diff | `python tools/check_bca.py diff --since HEAD` |
| Production compile | `cargo check-fast` |
| Standard production gate | `python ci.py gate` |
| One unit/integration test | `python tools/run_test.py <qualified-name-or-unique-substring>` |
| List tests without building | `python tools/run_test.py --list [substring]` |
| Survival gameplay | `python ci.py gate --gameplay survival` |
| Primitive progression gameplay | `python ci.py gate --gameplay progression` |
| Workshop gameplay | `python ci.py gate --gameplay workshop` |
| Ore-preparation gameplay | `python ci.py gate --gameplay ore` |
| Foundry gameplay | `python ci.py gate --gameplay foundry` |
| All core tests | `python ci.py audit --core` |
| All gameplay concerns | `python ci.py audit --gameplay` |
| Core + gameplay audit | `python ci.py audit --all` |
| Long-horizon soak | `python ci.py gate --soak` |
| Shader validation | `python ci.py gate --shaders` |
| Rust API documentation | `python ci.py gate --rustdoc` |
| Clippy with warnings denied | `python ci.py gate --lint` |
| Human-readable gameplay report | `python ci.py report` |

`python ci.py quick` checks formatting, the BCA cognitive-complexity ratchet, documentation/repository
contracts, and the local CI plan without building Rust. All repository-owned BCA commands go through
`tools/check_bca.py` so both the mandatory ratchet and advisory review use the exact CLI version owned by
the project; install the current pin with
`cargo install big-code-analysis-cli --version 2.1.0 --locked`.
[`bca.toml`](bca.toml) owns the analyzed source scope and threshold, while
[`.bca-baseline.toml`](.bca-baseline.toml) pins current offenders so only new or worsened cognitive
complexity fails the routine gate. The gate ignores BCA suppression comments, and baseline entries carry
body hashes so an unchanged over-threshold function can be renamed without creating artificial debt. A BCA
failure should normally be repaired by simplifying the changed code; regenerating the baseline is reserved
for a deliberate threshold-policy change or explicitly accepted debt, not routine failure repair. Cyclomatic
complexity, file size, Halstead metrics, and other BCA signals remain advisory because they can over-penalize
exhaustive Rust matches, explicit constructors, or cohesive owner modules. `python tools/check_bca.py report`
combines those advisory metrics with repository churn and fix history to prioritize review. Use repeated
`--path` arguments to focus that report when a subsystem is already known. `python tools/check_bca.py diff`
compares the working tree to a chosen revision and accepts repeated `--path` and `--metric` arguments when a
review needs a narrower signal. For a nontrivial refactor, use the history-aware report before choosing the
target, then use a focused diff after the change to verify that the intended complexity moved rather than
merely being redistributed. For example, mining work can be reviewed with
`python tools/check_bca.py report --path src/mining/execution.rs`, followed by
`python tools/check_bca.py diff --since HEAD --path src/mining/execution.rs --metric cognitive --metric cyclomatic --metric sloc`.
Do not split cohesive code, add forwarding helpers, or refresh the baseline merely to improve these numbers.
These reports are diagnostic evidence, not completion gates. `python ci.py gate` adds the normal production
compile. Specialized gate flags replace that compile with the selected focused lane. Audit lanes are the
maintained broad runtime checks.

Do not run a compile-only command next to an executable lane that already compiles the same changed surface.

## Unit tests

Unit-test bodies live beside their owner in `*_tests.rs` or `mod_tests.rs` and are included with
`#[cfg(test)] #[path = "..."] mod tests;`.

`python tools/run_test.py` resolves an exact or unique source test before invoking Cargo. Missing or
ambiguous selectors fail closed. Integration targets receive their Cargo-declared required features.

Assertions should prove durable contracts:

- rejection: typed error and unchanged authoritative state when atomicity matters;
- success: resulting identity, quantity, lifecycle, relationship, ownership, or other durable state;
- conservation: totals across authoritative owners;
- persistence: serialized continuation and trusted-load admission where the changed state survives load;
- authored values: read them from registries rather than copying balance constants into tests.

Do not assert error prose, wall-clock time, incidental ordering, transient implementation counts, or
balance values outside the test's owned contract.

Soak tests are ignored tests whose qualified name includes `soak`. Use them only when repeated ownership,
persistence, conservation, or numerical accumulation provides evidence a focused test cannot.

## Gameplay harness

`tests/gameplay_harness/` evaluates player-facing behavior through production APIs. Controlled setup may
create state that [`STATUS.md`](STATUS.md) marks capability-only; setup does not make that state ordinarily
acquirable.

### Actor boundary

`src/content/gameplay_fixture.rs` owns setup-only helpers. After setup, actor code must:

- use production validators, resolvers, commits, and simulation ticks;
- read only observable runtime state, actor policy, and canonical projections;
- never inspect hidden geology, hidden future events, setup-only authorization, or cloned-state previews;
- preserve normal ownership, persistence, conservation, capability, and survival rules;
- keep balance-sensitive measurements observational unless a test explicitly owns the threshold.

### Gameplay targets

| Target | Contract proved |
| --- | --- |
| `survival` | Immediate hunger/thirst response, activity-dependent reserve pressure, preservation, diet tradeoffs, provisioning, and reserve recovery through canonical eating/drinking/work. |
| `progression` | Local evidence acquisition, primitive crafting/mining/power/processing, a materially consequential scarce-copper choice, autonomous work, second reinforcement, convergence, returned attention, and finite machine lifecycle. |
| `workshop` | Installed industrial operation under finite work, survival, wear, maintenance, structure, hidden world pressure, and recovery. |
| `ore` | Installed crush/grind/screen/regrind/concentrate flow over variable gangue with selective recovery, full-batch industrial separation, exact constituent accounting, and physical tailings. Capability-only. |
| `foundry` | Installed pure-copper heating, melting, casting, finite energy, finite heat recovery, and passive sink cooling. Capability-only. |

### Progression probe requirements

The progression actor starts with visible local clue regions, gathered matter, storage, and hidden geology.
World-scale clue discovery is outside scope. The probe must show that the actor:

- acquires and reasons from persisted evidence rather than hidden deposit truth;
- refines unresolved evidence only when an observed constraint makes that refinement useful;
- resolves opaque mining targets and learns extracted form/composition from owned output;
- encounters direct-source insufficiency through a canonical rejected action;
- makes the same-state scarce-copper choice between extraction capability and stored-work rate;
- obtains reciprocal physical benefit before both branches converge on the same final capabilities;
- delegates crushing while performing other useful work and recovers the second reinforcement from processed ore;
- reaches attention payback while primitive equipment still has useful physical life remaining.

Matched branches use the same post-convergence workload. Reported choice effects must be downstream material,
energy, labor, capability, or timing consequences of different physical actions, not counters alone.

### Workshop probe requirements

Workshop scenarios start with installed equipment and finite resources. The actor may observe condition,
stored work, survival reserve, structural margin, and process projections, then choose power source, batch
size, maintenance timing, manual recovery, and support policy.

A controlled hidden delivery may change the world during a live scenario. Actor logic cannot inspect it
before it occurs. The focused workshop gate proves the operational scenario without paying to compile the
broader agency experiment. `python ci.py audit --gameplay` adds matched-policy counterfactuals that hold the
physical world and behavior RNG fixed while changing one policy. Distinct agency paths require distinct
physical outcomes; no-effect comparisons are classified by observed cause rather than forced into a pass/fail
agency claim.

## Report and replay

`python ci.py report` prints capability/reachability summaries, representative workshop decisions and
consequences, matched-policy agency results, and focused survival/progression/ore/foundry reviews.

| Variable | Meaning |
| --- | --- |
| `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` | physical variation root |
| `DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` | workshop policy root |
| `DEEP_HEARTH_GAMEPLAY_SEEDS` | exact comma-separated world seeds |
| `DEEP_HEARTH_GAMEPLAY_VERBOSE` | expanded scenario and focused-probe traces |

Gates and audits use stable deterministic defaults. Reports generate fresh roots unless explicit replay
values are supplied and always print realized seeds. Malformed explicit seeds fail configuration. Hard
assertions own balance-independent contracts; report output carries balance observations.

## Completion

Use the smallest lane that covers the changed contract. Add soak, shader, Rustdoc, or lint lanes only when
the changed surface requires them. Do not rerun narrower checks after a broader selected lane already covers
them. Verification is local; hosted CI is not part of the project contract.
