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
| `survival` | Runtime preservation, dominant reserve pressure, matched-world compact-calorie versus balanced-diet provisioning, eating, drinking, and reserve recovery after controlled bootstrap |
| `progression` | Runtime primitive crafting, assembly, mining, scarce-upgrade sequencing, manual power, mechanization, productive overlap, composition-derived native-copper separation, convergence, and autonomous player-free time |
| `workshop` | Pressure-rich industrial workshop capability, multi-system adaptation, recoverable disruption, and matched-world player-policy consequences |
| `ore` | Bootstrapped installed crushing, grinding, screening, and regrinding capability; pipeline-depth evidence only |
| `foundry` | Bootstrapped installed pure-copper heating, melting, casting, and finite heat recovery capability; pipeline-depth evidence only |

`ore` and `foundry` are capability tests, not end-to-end acquisition claims. `STATUS.md` is authoritative
for runtime reachability. Workshop exploration intentionally samples constrained stored work, wear, and a
scheduled hidden controlled delivery to expose adaptation paths; the event is not forced after an order
has already completed or reached a terminal stop. Its event density is stress evidence, not a claim about
baseline world-event frequency. Workshop agency comparisons hold world variation and behavior RNG
fixed except for the policy being compared. Agency path signatures contain physical outcomes only;
decision counters and other activity bookkeeping cannot create a distinct path. The agency surface also
includes three bounded generated worlds so the same policy family is observed outside the maintained
edge-case fixtures without requiring those worlds to be actionable. Non-actionable worlds are classified
by observed cause: completed objective, shared terminal world constraint, or genuinely dormant policy
pressure. This prevents successful completion or an unavoidable physical stop from being mislabeled as
missing agency. The survival probe makes every authored dietary category in its controlled world available
to both matched policies, reconstructs each branch independently rather than cloning actor state, and reports
the material/water cost of a compact-calorie meal against the recovery resilience of a balanced meal.
Primitive progression provides only one direct native-copper reinforcement parcel. The matched-world
counterfactual therefore measures sequencing without making the first choice permanent. Setup supplies only
bounded visible clue regions and hidden geological truth. Acting code must perform the canonical timed field-
inspection action for each initial clue, pay its survival cost, persist the resulting uncertain geological
evidence, and only then resolve opaque region/material mining targets without retaining hidden deposit IDs.
World-scale discovery of clue locations remains a bootstrap boundary because terrain/world representation is
not implemented; evidence acquisition itself is no longer bootstrapped.
Both branches then naturally construct the same baseline stone pick and stone processing line and mine the
same copper parcel so the pick and crank are both real existing upgrade targets at the matched
decision point. Reinforce the pick first for an exclusive hard-material access window and faster extraction,
or reinforce the crank first for an exclusive processed-output/stored-work window and faster charging. The
probe requires both choices to occur at the same simulation tick and reports the duration of both exclusive
affordance windows rather than treating construction-order delay as agency. While the first autonomous
crusher batch runs, the actor uses returned attention to mine additional ore. After crushing completes, both
branches must route an exact
composition-derived portion through the authored primitive separator, recover the missing native-copper
parcel, and only then forge the second reinforcement. The probe requires the direct native seam to remain
insufficient for that second upgrade, proves the recovered copper came from processed ore, preserves crushed
particle state in the stone residue, and requires both branches to converge on the same final capabilities
and extracted hard-ore total.

The progression probe then runs the same bounded 64-cycle post-convergence workload in both branches.
Attention payback measures the crank/flywheel/crusher automation investment only; separator preparation is
reported separately because it has an immediate material-progression return rather than only a delegated-
attention return. Full processing-line setup is reported as a third figure so neither cost is hidden. Payback
remains balance evidence rather than a hard legality gate. Final material/workload parity remains matched
between the counterfactuals. The probe also projects a full accumulator charge through the canonical manual-
power validator on each branch's actual pre-charge state, so equipment, store, physiology, condition, and
whole-tick limits all remain authoritative while random partial-fill quantization cannot hide a real work-
rate improvement. Reports distinguish useful player work overlapping autonomous machine time from genuinely
returned player-free time and prove whether processed output actually enabled the next acquisition rather
than inferring utility from registry declarations. `python ci.py report` emits explicit
survival, progression, workshop-experience, agency, and capability-role review lines so the
experiential conclusions do not have to be reconstructed from raw per-scenario counters. Its default
output stays concise: aggregate workshop evidence, representative disruption/recovery/constraint
highlights, focused experience reviews, and compact industrial capability summaries. Set
`DEEP_HEARTH_GAMEPLAY_VERBOSE` when every workshop scenario and detailed focused physical trace is
needed for exact diagnosis.

Broad gameplay audit links `gameplay_workshop` and `gameplay_audit`; the latter contains the four focused
probe modules. Focused gates keep their own binaries so repairing one concern does not require relinking
the others. Broad-audit failures map back to the corresponding focused target and exact test.

### Variation and replay

Maintained gameplay tests combine fixed semantic anchors with a small deterministic variation sample.
The workshop gate retains seven semantic anchors and adds two generated physical worlds. Each focused
probe retains one semantic anchor and adds two independently salted generated worlds, so a shared replay
root does not collapse different gameplay concerns onto the same scenario seed. Hard assertions cover
legality, ownership, conservation, persistence, authored capability agreement, information boundaries,
and other balance-independent contracts. Balance-sensitive results are report observations unless an
anchor explicitly owns them.

`python ci.py report` is deliberately experiential rather than a deterministic gate. Unless replay roots
are already present in the environment, it creates fresh physical and behavior roots once for the whole
report, then passes them through workshop exploration, agency evaluation, and the independently salted
focused probes. The roots and realized seeds are printed, so any surprising run remains exactly
replayable. Ordinary gates and audits keep stable defaults for repeatable verification.

Replay controls:

- `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED`: physical variation root;
- `DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED`: workshop policy root;
- `DEEP_HEARTH_GAMEPLAY_SEEDS`: exact comma-separated world-seed list;
- `DEEP_HEARTH_GAMEPLAY_VERBOSE`: every workshop scenario plus detailed exploratory and focused-probe traces.

Malformed explicit seeds fail configuration.

## Completion

Use the smallest lane that completely covers the changed contract. Specialized surfaces such as soak,
shader, Rustdoc, or lint are required only when that surface changed or the task explicitly calls for
them. Do not rerun narrower checks after a broader selected lane already covers them.

Verification is local. The repository does not use GitHub Actions or hosted CI.
