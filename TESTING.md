# Testing

This document owns test organization, gameplay-harness contracts, and local verification. Use
[`README.md`](README.md) for repository routing. Choose the smallest command that completely proves the
changed contract.

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
local CI plan.

`python ci.py gate` adds one production-library compile. Specialized gate flags replace that compile
with one focused build-producing lane. Complete core and all-gameplay suites are audit lanes.

Do not run a compile-only command immediately before or after a test that compiles the same changed
surface.

## Unit tests

Unit-test bodies live beside their owning production module in `*_tests.rs` or `mod_tests.rs` and are
loaded through `#[cfg(test)] #[path = "..."] mod tests;`. Keep test bodies out of production source files
so test-only edits do not invalidate production-only Cargo artifacts.

`python tools/run_test.py` resolves an exact or uniquely matching source test name before invoking
Cargo. Ambiguous or missing selectors fail without building. Integration-test targets infer their
Cargo-declared required features.

Assertions prove durable contracts:

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
state that ordinary play cannot yet create. Actor behavior after setup uses canonical runtime APIs and
observable information only.

### Actor boundary

`src/content/gameplay_fixture.rs` owns setup-only helpers. Harness actor code must:

- use production validators, resolvers, commits, and simulation ticks;
- read only observable runtime state, actor policy, and canonical projections;
- never inspect hidden geology, hidden future events, setup-only authorizations, or cloned-state previews;
- preserve ordinary ownership, persistence, conservation, capability, and survival rules;
- treat balance-sensitive measurements as observations unless a test explicitly owns the threshold.

[`STATUS.md`](STATUS.md) is authoritative for runtime reachability. A bootstrapped harness proves behavior
of an installed system, not an ordinary-play acquisition path.

### Targets

| Target | Required evidence |
| --- | --- |
| `survival` | Matched hunger-versus-thirst pressure response through canonical eating/drinking, matched full-reserve prospecting versus manual-power work showing activity-dependent dominant reserve pressure, preservation, dominant long-horizon reserve pressure, matched compact-calorie versus balanced-diet provisioning, and reserve recovery. |
| `progression` | Local prospecting, primitive crafting/assembly, mining, materially distinct scarce-copper sequencing windows, manual power, autonomous crushing, productive overlap, native-copper separation, convergence, returned player attention, and finite primitive-machine lifecycle/payback. |
| `workshop` | Installed industrial workshop operation under stored-work, survival, wear, maintenance, structure, power, and hidden world-change pressure; includes matched policy counterfactuals. |
| `ore` | Installed crushing, grinding, screening, regrinding, and generalized copper-concentration behavior over variable multi-constituent gangue, including one full prepared batch through a structurally installed industrial separator, exact constituent accounting, and physical tailings. Capability depth only. |
| `foundry` | Installed pure-copper heating, melting, casting, and finite heat-recovery behavior. Capability depth only. |

The survival pressure checks use matched starting reserves for two distinct questions. Warning-boundary cases verify that the useful canonical response changes with the immediate need; full-reserve work cases verify that different canonical activities can make different reserves dominant without requiring artificial meter rotation.

### Progression contract

The progression probe starts from visible local clue regions, raw gathered matter, storage, and hidden
geology. World-scale clue discovery is outside the current runtime boundary.

The actor must:

- acquire geological evidence through timed, survival-costed prospecting;
- choose among resolved clues from acquired evidence and canonical action blockers rather than fixture
  roles or hidden deposit properties;
- defer an unresolved clue while current resolved options remain useful, then pay for the detailed field
  survey when a newly observed material constraint makes that uncertainty relevant;
- resolve opaque region/material mining targets without retaining hidden deposit identity;
- learn extracted commodity form and composition from owned inventory, and size later processing from
  that observed matter rather than hidden scenario grade;
- learn direct-source insufficiency through a canonical rejected extraction request rather than hidden
  remaining-mass knowledge;
- build the same baseline stone tools and processing line in both matched branches;
- allocate one direct native-copper reinforcement either to the pick or the hand crank at the same
  decision state;
- demonstrate reciprocal scarce-copper leverage: pick-first must turn harder geology into a materially better
  processable feed, while crank-first must improve useful stored-work generation before convergence;
- run autonomous crushing concurrently with useful player work;
- recover the second reinforcement from composition-derived processed ore because direct native copper is
  insufficient;
- converge both branches on the same final capabilities and matched material workload.

Post-convergence evaluation uses the same bounded workload in both branches. Automation attention
break-even measures crank/flywheel/crusher preparation against returned free attention only, must occur
within a bounded number of repeated cycles, and must leave a meaningful number of useful cycles before
the primitive crusher or crank reaches its physical condition-limited endpoint. Separator setup and full
processing-line setup are reported separately. The scarce-copper choice must create distinct physical
consequences with reciprocal leverage, not merely nonzero milestone timing deltas. Matched counterfactuals
must report the actual downstream material, energy, labor, or capability differences that make each branch
useful before convergence.

### Workshop contract

Workshop scenarios begin with installed industrial equipment and finite resources. The actor observes
current condition, stored work, survival reserve, structural margin, and process projections, then chooses
power, batch size, maintenance timing, manual recovery, and support policy.

Exploration varies stored work, wear, maintenance supply, survival pressure, structural state, and one
hidden preauthorized material delivery. The delivery is a stress input and does not occur after the
scenario has completed or reached a terminal constraint.

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

The default report keeps replay roots, aggregate workshop/agency evidence, and one review line per focused
probe seed. Set `DEEP_HEARTH_GAMEPLAY_VERBOSE` for per-world agency rows, survival sub-probes, expanded
progression tradeoff/autonomy decomposition, and detailed scenario traces.

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

Broad gameplay audit links one `gameplay_audit` executable containing the workshop contracts and all four
focused probes. Focused gates retain separate binaries so one concern can be repaired independently without
relinking unrelated harness code. The human-readable report is also one ignored test in that consolidated
target, so a broad audit followed by a report reuses the same executable.

## Completion

Use the smallest lane that completely covers the changed contract. Specialized surfaces such as soak,
shader, Rustdoc, or lint are required only when that surface changed or the task explicitly calls for
them. Do not rerun narrower checks after a broader selected lane already covers them.

Verification is local. The repository does not use GitHub Actions or hosted CI.
