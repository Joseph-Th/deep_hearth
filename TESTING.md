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
| All gameplay harness concerns | `python ci.py audit --gameplay` |
| Core + gameplay audit | `python ci.py audit --all` |
| Long-horizon soak tests | `python ci.py gate --soak` |
| Shader validation | `python ci.py gate --shaders` |
| Rust API documentation | `python ci.py gate --rustdoc` |
| Clippy with warnings denied | `python ci.py gate --lint` |
| Authority links/routes + source-module docs | `python tools/check_authority_docs.py` |
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

`tests/gameplay_harness/` is the headless player-facing evaluation surface. Controlled setup may provide
state that ordinary play cannot yet create; actor behavior after setup must use canonical runtime APIs and
observable information only.

### Actor boundary

`src/content/gameplay_fixture.rs` owns setup-only helpers. Harness actor code must:

- use production validators, resolvers, commits, and simulation ticks;
- read only observable runtime state, actor policy, and canonical projections;
- never inspect hidden geology, hidden future events, setup-only authorizations, or cloned-state previews;
- preserve ordinary ownership, persistence, conservation, capability, and survival rules;
- treat balance-sensitive measurements as observations unless a test explicitly owns the threshold.

`STATUS.md` is authoritative for runtime reachability. A bootstrapped harness proves behavior of an
installed system, not an ordinary-play acquisition path.

### Targets

| Target | Required evidence |
| --- | --- |
| `survival` | Preservation, dominant reserve pressure, matched compact-calorie versus balanced-diet provisioning, physical eating/drinking, and reserve recovery. |
| `progression` | Local prospecting, primitive crafting/assembly, mining, scarce copper sequencing, manual power, autonomous crushing, productive overlap, native-copper separation, convergence, and returned player attention. |
| `workshop` | Installed industrial workshop operation under stored-work, survival, wear, maintenance, structure, power, and hidden world-change pressure; includes matched policy counterfactuals. |
| `ore` | Installed crushing, grinding, screening, and regrinding pipeline behavior. Capability depth only. |
| `foundry` | Installed pure-copper heating, melting, casting, and finite heat-recovery behavior. Capability depth only. |

### Progression contract

The progression probe starts from visible local clue regions, raw gathered matter, storage, and hidden
geology. World-scale clue discovery is outside the runtime boundary.

The actor must:

- acquire geological evidence through timed, survival-costed prospecting;
- respond to insufficient coarse evidence by using the detailed field survey before extraction;
- resolve opaque region/material mining targets without retaining hidden deposit identity;
- build the same baseline stone tools and processing line in both matched branches;
- allocate one direct native-copper reinforcement either to the pick or the hand crank at the same
  decision state;
- demonstrate the pick-first hard-material window and crank-first stored-work/processed-output window;
- run autonomous crushing concurrently with useful player work;
- recover the second reinforcement from composition-derived processed ore because direct native copper is
  insufficient;
- converge both branches on the same final capabilities and matched material workload.

Post-convergence evaluation uses the same bounded workload in both branches. Reported automation
attention break-even covers only crank/flywheel/crusher preparation versus returned free attention. It is
not a total-value estimate because the crusher also provides immediate material-progression utility.
Separator setup and full processing-line setup are reported separately.

### Workshop contract

Workshop scenarios begin with installed industrial equipment and finite resources. The actor observes
current condition, stored work, survival reserve, structural margin, and process projections, then chooses
power, batch size, maintenance timing, manual recovery, and support policy.

Exploration varies stored work, wear, maintenance supply, survival pressure, structural state, and one
hidden preauthorized material delivery. The delivery is a stress input, not a baseline event-frequency
claim. It does not occur after the scenario has already completed or reached a terminal constraint.

Agency evaluation holds physical world variation and behavior RNG fixed while changing one policy.
Distinct agency paths require different physical outcomes; counters alone cannot create a new path.
Generated worlds supplement fixed semantic anchors. A world with no policy effect is classified by its
observed cause: objective already resolved, shared terminal constraint, or dormant policy pressure.

### Report and replay

`python ci.py report` is the experiential report. It emits:

- project capability and reachability summaries;
- representative workshop pressure -> policy -> decision -> consequence highlights;
- workshop experience and matched-policy agency summaries;
- focused survival, progression, ore-preparation, and foundry reviews.

The default report is compact. Set `DEEP_HEARTH_GAMEPLAY_VERBOSE` for every workshop scenario and detailed
focused physical traces.

Gates and audits use stable deterministic defaults. The report generates fresh physical and behavior roots
unless explicit roots are supplied, and prints all realized seeds for replay.

Replay controls:

- `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED`: physical variation root;
- `DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED`: workshop policy root;
- `DEEP_HEARTH_GAMEPLAY_SEEDS`: exact comma-separated world-seed list;
- `DEEP_HEARTH_GAMEPLAY_VERBOSE`: expanded scenario and focused-probe traces.

Malformed explicit seeds fail configuration. Gameplay tests combine fixed semantic anchors with small,
independently salted deterministic variation samples. Hard assertions cover balance-independent contracts;
report output carries balance observations.

Broad gameplay audit links `gameplay_workshop` and `gameplay_audit`. The consolidated target contains the
four focused probes; focused gates retain separate binaries so one concern can be repaired independently.

## Completion

Use the smallest lane that completely covers the changed contract. Specialized surfaces such as soak,
shader, Rustdoc, or lint are required only when that surface changed or the task explicitly calls for
them. Do not rerun narrower checks after a broader selected lane already covers them.

Verification is local. The repository does not use GitHub Actions or hosted CI.
