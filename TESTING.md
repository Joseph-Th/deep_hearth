# Testing

This document owns Deep Hearth's test selection, feedback lanes, harness contract, and CI gate. The
suite is organized so ordinary correctness work does not compile specialized gameplay or shader
validation dependencies unless that coverage is relevant.

## Daily workflow

Run the narrowest qualified test while changing one behavior. Do not reflexively run the whole fast
lane after every edit: use it as a checkpoint when a coherent slice is complete, and let the local CI
gate be the final pre-commit repetition. Long-horizon soak bodies are compiled only with the
`test-soak` feature, so ordinary behavior edits do not type-check or codegen the large soak fixtures:

```text
cargo check-fast                                  # mechanical/type feedback
cargo check-tests                                 # mechanical feedback while editing unit tests
cargo test-fast <qualified-test-name> -- --exact  # behavior feedback
cargo test-fast                                   # coherent checkpoint
python ci.py gate                                 # once before commit
```

`cargo check-fast` is intentionally library-only. `cargo check-tests` type-checks the default-feature
test inventory without codegen/linking. `cargo check-gameplay` type-checks the complete maintained
gameplay target set in one Cargo invocation, while `cargo check-gameplay-workshop`,
`cargo check-gameplay-survival`, `cargo check-gameplay-progression`, `cargo check-gameplay-ore`, and
`cargo check-gameplay-foundry` type-check only one focused gameplay target. Use the narrowest
artifact while an edit is mechanical. An executable test is still required for changed behavior;
compile-only feedback exists specifically to avoid paying the dominant codegen/link cost on every
intermediate edit.

Do not routinely run a compile-only command immediately before the executable command for the same
artifact. Once an assertion is ready to execute, run the focused test directly; its compilation already
provides the relevant type/warning feedback. On this project the first feature-selected gameplay
codegen/link is materially more expensive than the probe runtime, so `check -> test` as a ritual nearly
doubles the dominant wait without adding behavior evidence.

`python ci.py gate` is the concise local pre-commit wrapper. It runs format and the fast core tests,
captures successful Cargo noise, prints one timed line per stage, reports the slowest successful stage,
and shows the exact reproduction command plus native output only when a stage fails. Rust compiler
warnings are denied by the package lint configuration, so this path
does not need a second Clippy compile merely to catch ordinary warning regressions. Add `--lint` when a
Clippy checkpoint is useful. Scope is explicit rather than inferred from git diffs. With no lane flags,
the gate runs core behavior. Supplying `--gameplay`, `--shaders`, `--docs`, or `--lint` selects only those
specialized lanes instead of paying for an unrelated core libtest link. Add `--core` when a specialized
change also needs the ordinary core suite. `--soak` already includes ordinary core tests in the same
artifact.

Gameplay selection can be narrowed at the same entry point. `python ci.py gate --gameplay` runs the
five maintained focused targets in one Cargo invocation so they share the same `test-gameplay` library
artifact and Cargo can build independent test executables in parallel. For a change confined to one
concern, use `--gameplay workshop`, `--gameplay survival`, `--gameplay progression`, `--gameplay ore`,
or `--gameplay foundry`. Every gameplay gate adds only the static command/manifest policy check. The
heavyweight full harness is reserved for the explicit exploratory report, not routine verification.

`python ci.py audit` is the broad maintained runtime checkpoint: format, ordinary core behavior, all
maintained gameplay targets, and static gameplay command-policy validation. It deliberately does not add
the expensive soak feature shape, all-target/all-feature Clippy, documentation, or standalone shader
validation. Long-horizon conservation/accumulation changes use `python ci.py gate --soak`; test/harness
Clippy hardening uses `cargo test-lint-all`. Do not turn either optional lane into a prerequisite for a
focused fix.

When a change touches a specialized boundary, add its maintained lane:

```text
cargo test-soak
cargo test-gameplay
cargo test-shaders
```

Routine successful lanes use quiet Cargo output. Failure output remains Cargo-native and includes the
failing qualified test. The test profile and dedicated validation-binary profile omit debug symbols to
reduce codegen/link time; normal dev builds retain their debugging behavior. Set Cargo's profile debug
override when an interactive test/validator debugging session needs symbols. Do not add timing
assertions or sleeps; performance of the verification workflow is an engineering property, not a
simulation contract.

## Maintained lanes

| Command | Purpose | Compile scope |
| --- | --- | --- |
| `cargo check-fast` | Fast compile/type feedback during mechanical edits | Default-feature production library only; no test harness link |
| `cargo check-tests` | Fast compile/type feedback while editing ordinary tests | Default-feature test targets, type-check only; no test codegen/link |
| `cargo check-gameplay` | Fast compile/type feedback for all maintained gameplay concerns | Five focused integration targets plus one shared `test-gameplay` library artifact; no test link/run |
| `cargo check-gameplay-{workshop,survival,progression,ore,foundry}` | Fast compile/type feedback for one gameplay concern | One focused integration target plus `test-gameplay`; no unrelated gameplay probe codegen/link |
| `cargo test-fast` | Ordinary deterministic behavior, errors, persistence, and integrations | Default-feature unit-test artifact; soak-only bodies are not compiled |
| `cargo test-soak` | Long-horizon deterministic conservation/invariant scenarios | Adds `test-soak` and runs only ignored tests |
| `cargo test-gameplay` | Complete maintained gameplay verification | Five focused integration targets in one Cargo invocation with one shared `test-gameplay` feature shape |
| `cargo test-gameplay-workshop` | Workshop policy, seven maintained anchors, bounded variations, agency counterfactuals, and machine-process catalog coverage | Focused workshop target; excludes survival/progression/ore/foundry probe modules |
| `cargo test-gameplay-survival` | Authored food/drink discovery, freshness/preservation, varied meal choice, and finite drinking | Small dedicated target; one maintained case plus two fresh bounded replayable variations by default |
| `cargo test-gameplay-progression` | Primitive manual craft/mine/upgrade/manual-power/mechanization progression | Small dedicated target; one maintained case plus two fresh bounded replayable variations by default |
| `cargo test-gameplay-ore` | Bootstrapped industrial ore-preparation capability probe | Small dedicated target; one maintained case plus two fresh bounded replayable variations by default |
| `cargo test-gameplay-foundry` | Bootstrapped pure-copper foundry capability probe | Small dedicated target; one maintained case plus two fresh bounded replayable variations by default |
| `cargo test-gameplay-report` | Broader fresh replayable play/capability report for understanding the current game | Heavyweight `test-gameplay-full` target, ignored by maintained gates and run with captured output disabled |
| `cargo test-shaders` | Naga parse/semantic validation of assembled WGSL without compiling the crate unit-test harness | Adds `test-shader-validation` |
| `cargo check-all` | All-target compile-only diagnostic | Default features |
| `cargo test-lint` | Optional production-library Clippy checkpoint | Default-feature library only; not part of the routine gate |
| `cargo test-lint-all` | Explicit cross-cutting/release Clippy hardening | All test features |
| `cargo test-all` | Ordinary plus ignored core/soak tests in one invocation | Adds `test-soak` once and runs the combined core inventory |
| `cargo test-release` | Complete optimized test inventory | All test features |
| `cargo test-doc` | Documentation build without dependencies | Default features |

`test-gameplay` exists only to expose the controlled bootstrap adapter required by the integration
harness; that adapter remains absent from ordinary production builds and delegates to canonical runtime
transactions. `test-gameplay-full` adds only the cross-probe modules required by the explicit exploratory
report; maintained gates deliberately stay on `test-gameplay` so all focused targets reuse one library
artifact. `test-shader-validation` likewise exists only to expose Naga-backed WGSL validation.
Neither specialized boundary enters the ordinary edit-loop build. `test-soak` likewise keeps
long-horizon fixture code out of the fast artifact; the explicit soak lane pays that compile cost only
when repeated-state evidence is actually needed.

`cargo test-lint` deliberately does not lint-compile test targets and is optional in the ordinary loop.
The package denies Rust compiler warnings, while `cargo test-fast` compiles and executes the complete
default-feature unit-test target. Running Clippy before every test checkpoint therefore pays a second
front-end pass for limited incremental signal. Use `python ci.py gate --lint` for a deliberate
production-library lint checkpoint and `cargo test-lint-all` for test/harness lint coverage or release
hardening.

When both ordinary and soak coverage are needed, prefer one `cargo test-all` invocation (or `python
ci.py gate --soak`) instead of running `test-fast` and `test-soak` separately. That invocation builds
the `test-soak` variant once and runs ordinary plus ignored tests together, without making the daily
default-feature artifact carry soak-only code.

## Test organization and assertions

Tests remain colocated with the owning source module under `#[cfg(test)]`. Prefer the smallest fixed
fixture and shortest canonical execution that prove the named rule. A rejected operation should
assert the exact typed error and unchanged authoritative state when mutation atomicity is part of the
contract. A successful operation should assert the exact identity, quantity, lifecycle, relationship,
or durable result that defines success.

Avoid assertions on human-readable error prose, incidental ordering, transient implementation counts,
or arbitrary wall-clock duration. Aggregate `any`/`all` assertions are appropriate only for stable
input/diversity contracts, not for balance-dependent outcomes. Those failures must name the missing
contract rather than returning an anonymous boolean failure.

Long-horizon tests use `soak` in the qualified test name and carry
`#[ignore = "long-horizon soak"]`. The ignore marker, not a name filter, owns lane membership. Keep one
mixed-system thousands-of-ticks soak as the broad invariant proof; subsystem soaks should exist only
where repeated ownership, conservation, persistence, or numerical accumulation adds evidence that a
narrow test cannot provide. Do not ignore ordinary behavioral tests.

## Gameplay harness

The gameplay harness under `tests/gameplay_harness/` is the closest headless surrogate for actually
playing the current game. It exercises real runtime validators, resolutions, commits, and simulation
ticks and must expose where a scenario is genuinely reachable versus where missing world or progression
systems require fixture bootstrap. Correctness gates combine stable maintained anchors with a very small
fresh bounded sample so they do not repeatedly walk only memorized paths. Every generated root is printed
before execution and can be supplied explicitly to replay a surprising failure exactly.

### Boundary

The feature-gated setup bridge in `src/content/gameplay_fixture.rs` may create starting matter, fluid,
energy, equipment, geological occurrences, planned structures, and opaque authorization for a
controlled external event where the corresponding world owner is not yet implemented. Setup-only
authorizations are created before the acting policy begins and are single-use. The acting policy cannot
call setup helpers, inspect hidden geological truth, or inspect a controlled event's future tick/target.
Experienced inventory movement, maintenance, support changes, production, mining, survival, and time
progression use canonical runtime transactions.

That information boundary is structural, not conventional. Actor-facing harness context must contain
only current observable state, player policy, and canonical resolver projections. Hidden controller
state such as a future event tick/target or undiscovered geological truth belongs in a separate setup or
event-controller object and must not be reachable through actor decision APIs.

Runtime-play probes bootstrap only world state that the runtime cannot yet create through play, such as
starting authored food/drink/storage profiles, raw gathered matter, or a preauthorized mining-site
identity. Geological site discovery is not implemented, so possession of the deposit ID is explicitly a
setup assumption rather than a claimed player discovery action. Exact hidden deposit truth is not used
to choose actor actions. After bootstrap, survival and primitive progression use current runtime
transactions for eating, drinking, manual crafting, equipment assembly/upgrades, mining, energy-store
assembly, manual power, and primitive autonomous crushing.

Industrial workshop, ore-preparation, and foundry probes are capability evaluations, not current
end-to-end progression. Their industrial machines and stores have no runtime acquisition/generation
path today. Structural material transfer/validation exists, but the physical construction resolver is
not implemented, so workshop bays remain setup state rather than a claimed player construction action.
Direct industrial machine/store injection is guarded against current registry acquisition metadata: if
an injected machine or store gains a runtime acquisition route, the capability fixture fails and must
be updated to acquire it through gameplay instead of continuing to bootstrap it silently.

The acting policy reads observable state and resolver projections. Hidden scenario state is limited to
world setup/event injection, diagnostics, and postconditions and never feeds actor decisions. It does
not clone `AppState` to preview compound future mutations.

### Maintained coverage

`cargo test-gameplay` covers two explicitly different evidence classes across the maintained focused
targets. Survival, progression, and industrial capability probes are independent controlled episodes,
not one continuous save.

Runtime actions after controlled world bootstrap cover survival provisioning/preservation and primitive
progression through manual shaping, equipment assembly, hand mining, native-copper reinforcement,
material-backed work storage, survival-costed manual power, and autonomous primitive crushing alongside
concurrent player labor. The survival probe discovers edible and drinkable content from the live
`SurvivalRegistry` instead of naming a fixed food list, then varies dietary-category selection,
preservation witness, storage multiplier, depletion, drink quantity, and eat/drink ordering. The player
exists before any preservation-aging ticks occur, so food age, metabolic depletion, and hydration loss
advance on the same authoritative world clock. The actor is given an older ambient meal and a preserved
duplicate of one selected food, consumes the older exposed stock first, and retains the measurably fresher
preserved reserve; preservation therefore changes a provisioning decision rather than existing only as a
storage-physics assertion. The progression probe exercises
every currently authored manual crafting action, including the otherwise optional clay-vessel side craft,
and fails closed if runtime-acquirable equipment, runtime-assemblable energy stores, manual craft actions,
or mining methods appear without corresponding cold-agent evidence. Fresh progression cases vary legal
mining mass, raw-material and unmined geological surplus,
player-chosen stored-work reserve, and which of two scarce native-copper reinforcements is prioritized
first: faster extraction or earlier mechanization. Banked flywheel work is converted into an exact
smaller follow-up crusher batch instead of being left as decorative residual energy. Ore grade still
varies as preserved composition state, but is reported as composition-only because
concentration/smelting does not yet make grade a playable decision. The acting path receives a
preauthorized site identity because physical prospecting is absent, but it never reads hidden deposit
mass/composition to choose an action. Each progression case also replays the opposite upgrade priority on
the same world. Extraction-first spends its early pick improvement on useful ore before pursuing the
second reinforcement; mechanization-first starts the autonomous crusher earlier and continues legal player
work while that production job remains active. The comparison reports useful-ore and processed-output
milestones, direct tool/crank attention savings, useful machine overlap, machine-only idle wait, persistent
pick condition, survival cost, and total elapsed time. This isolates strategy from seed variation and
measures the attention payoff of automation without adding another build artifact.

Bootstrapped capability evaluation covers industrial workshop operation as a total-mass work order with
uneven finite stored work, adaptive legal batch sizing, replacement-stock scarcity, wear, structural
loading, a hidden controlled delivery event, suspension/recovery, and survival-costed manual-power
fallback. Separate capability probes cover industrial crushing/grinding/screening/regrinding and
pure-copper melt/cast. Fresh ore-preparation cases vary legal mass, grade, healthy-but-worn machine
condition, and finite stored work derived from current process energetics. Fresh foundry cases vary legal
mass, sub-melting copper temperature, healthy-but-worn furnace/mold condition, and finite electrical work
derived from current thermal physics. These probes describe implemented processing behavior, not a
currently reachable industrial progression chain.

Focused aliases rerun one concern without compiling or executing the unrelated gameplay probes:

```text
cargo test-gameplay-workshop
cargo test-gameplay-survival
cargo test-gameplay-progression
cargo test-gameplay-ore
cargo test-gameplay-foundry
```

For completion of a focused harness-only edit, prefer the corresponding `python ci.py gate --gameplay
<scope>` form. It adds formatting and static command-policy validation without forcing unrelated
gameplay targets to link. The Cargo aliases remain the fastest executable loop while coding.

### Anchors, bounded variation, and replay

Gameplay verification deliberately combines explicit anchors with small fresh bounded variation cases.
The required workshop capability gate keeps seven maintained anchors and adds two freshly generated world/
behavior variations. The maintained workshop set deliberately includes normal/warning/critical
maintenance starts, one threshold-relative condition-pressure case that must shorten a batch to avoid
crossing Critical condition, one fractional-energy case that requires adaptive batching, one whole-batch
stored-work shortfall that requires manual-power recovery, and one finite-work survival-pressure case
where protecting the hunger/thirst warning reserve sacrifices useful output while spending reserve can
finish the same order. Each focused survival,
progression, ore-preparation, or foundry command keeps one maintained anchor and two fresh bounded
variations. The extra cases execute in the already-built focused target, so they broaden organic input
coverage without creating a new Cargo artifact or meaningful compile/link cost. The broader report uses
a larger fresh sample for understanding rather than gating.
Generated Critical-condition workshop starts always include at least one replacement service because the
maintained Critical anchor already owns the unrecoverable/preparation boundary; fresh samples should spend
their limited budget exercising interacting decisions rather than duplicating a dead-on-arrival setup.

Generated cases remain deterministic once their seeds are known, and every command prints its replay
inputs. Assertions on fresh bounded cases are limited to canonical legality,
ownership/conservation, persistence, authored capability agreement, and other balance-independent
contracts. Outcomes such as which workshop bottleneck appears, whether a particular workshop finishes
its order, or how often relocation/maintenance occurs remain observations rather than pass/fail
requirements unless a maintained anchor explicitly owns that outcome.

- `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` reproduces the generated physical variation root.
- `DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` reproduces the generated workshop-policy root.
- `DEEP_HEARTH_GAMEPLAY_SEEDS` supplies an exact comma-separated world-seed sweep.

Malformed explicit seeds fail configuration. Use the printed roots/seeds from any surprising run to
replay it before changing an assertion or production rule.

`cargo test-gameplay-report` is the broader cold-agent play report. It uses a larger fresh workshop
sample, runs runtime-action survival/progression episodes plus industrial capability probes, prints the
current acquisition-declared versus no-runtime-path content boundary, and reports observed/unobserved
workshop behavior. Run it when the goal is understanding what the current game actually feels capable of, not as
a prerequisite after every edit. Its player-experience assessment is derived from typed review values
returned by the episodes and workshop matrix, not from a hard-coded list of supposedly demonstrated
features. It classifies survival provisioning, material-upgrade attention savings, scarce-upgrade
tradeoffs, automation attention recovery, workshop policy agency, and cross-system coupling as working or
weak and prints the raw measurements immediately below that classification. Cross-system coupling is
claimed only when the sampled workshop actually observes adaptive operation, condition-driven adaptation,
manual energy recovery, maintenance, relocation, production suspension/recovery pressure, a
survival-preserving stop, and consequential one-factor policy choices. Its matched-world agency panel uses one-factor counterfactuals: a
conservative baseline is compared separately against finish-sooner power use, spending survival reserve
for manual recovery, delayed maintenance, and move-only-on-failure structural behavior. Agency evidence
uses three maintained choice-pressure worlds rather than searching generated outcomes: the normal
baseline must expose power and structural choices, the survival-recovery anchor must expose bodily
reserve versus useful output, and the warning-maintenance anchor must expose preventive versus delayed
service. The normal gameplay test inventory reruns these maintained counterfactuals so agency cannot
silently become decorative between report runs. Adaptive-energy and manual-recovery anchors remain
system-stress evidence instead of being mislabeled as agency when their one-factor policy variants are
inert. Default output reports compact ranges for processed work, condition- versus stored-work-driven
batch adaptation, final condition, relocation/suspension, elapsed time, and survival cost; set
`DEEP_HEARTH_GAMEPLAY_VERBOSE` for exact per-policy path signatures and detailed traces.

### Assertion policy

Hard failures represent stable contracts: canonical execution errors, conservation, persistence,
ownership, exact routing, authored capability agreement, seed-channel separation, maintained-anchor
coverage, player-facing catalog coverage, and fixture/reachability boundaries. Fresh variation must not
turn balance-dependent outcomes into brittle assertions. Number of completed batches, structural damage, maintenance pressure,
suspension, relocation, bottleneck mix, meal composition, or optional stored-work reserve are
observations unless a specific maintained anchor explicitly owns that contract.

The workshop variation generator may intentionally provide insufficient or fractional stored work and
replacement stock. The actor asks the canonical process resolver for the largest legal operation it can
actually power, then reduces that candidate when necessary to preserve a non-critical projected machine
condition instead of abandoning all useful work because the nominal batch is too large. Only when no
positive powered batch can remain outside Critical does maintenance become mandatory. When stored work is exhausted it may use
the runtime manual-power path; a survival-preserving policy rejects projected work that would cross the
authored hunger or thirst warning reserve. A partial order caused by those real constraints or that
policy choice is a valid observation, not a harness failure. A hidden controlled event may occur while
the actor is committed to manual work, in which case world state changes immediately but player-driven
recovery waits until that labor commitment completes.
Likewise, the primitive mechanization probe proves that player labor and autonomous machine work can
overlap; it does not require one authored duration to remain shorter than the other.

Probe inputs derive legal masses, durations, process routes, capability limits, and material
requirements from current registries where possible. Do not freeze copied balance constants into
assertions when the authoritative definition can be queried.

The maintained gameplay gate is the focused target set itself: workshop, survival, progression, ore
preparation, and foundry. `cargo test-gameplay` builds those targets in one Cargo invocation so they
share one `test-gameplay` library artifact and can be scheduled together. The heavyweight
`gameplay_harness` target exists only for the exploratory cross-probe report. Probe modules are shared
rather than copied, and the fresh bounded variations add negligible runtime compared with test linking.

## Local CI and completion gates

Verification runs in the developer workspace. GitHub Actions and hosted runners are prohibited. Use the
repository-owned `ci.py` runner or the Cargo aliases directly; no pull-request job, scheduled workflow,
or remote runner owns the validation contract. The runner deliberately does not inspect changed files
or guess scope: explicit flags are faster to understand and cannot silently omit a relevant lane.

1. **Routine core**: `python ci.py gate` runs only format plus fast core tests.
2. **Relevant specialized coverage**: `python ci.py gate --gameplay [scope]`, `--shaders`, `--docs`, or
   `--lint` runs only format plus the selected specialized lane(s). Add `--core` only when the change
   also needs the ordinary core behavior artifact. `--soak` uses one combined core+soak artifact and
   therefore already includes core behavior.
3. **Lint checkpoint**: select `--lint` when Clippy-specific production-library feedback is useful; it
   does not implicitly trigger the core test binary.
4. **Broad maintained checkpoint**: `python ci.py audit` runs ordinary core behavior, all maintained
   gameplay targets, and static gameplay command-policy validation. Soak is intentionally separate via
   `gate --soak`, and all-target/all-feature Clippy is the separate `cargo test-lint-all` hardening lane.
   Documentation and standalone shader validation stay change-scoped through `gate --docs` and
   `gate --shaders`. The audit is intentionally broader than a focused pre-commit gate without becoming
   a run-every-expensive-shape command.

Local Cargo incremental state may be reused naturally between these commands. Fast core deliberately
does not compile soak-only fixtures; gameplay deliberately does not compile the crate unit-test harness.
Release hardening and documentation remain explicit local commands rather than background or scheduled
work.

Gameplay gates run `tools/check_gameplay_aliases.py --static` after behavior passes. The checker verifies
that maintained `test-gameplay` and `check-gameplay` aliases contain every focused target exactly once,
that each focused test/check alias selects only its complete small target, and that target paths/features
still match `Cargo.toml`. Synthetic self-tests cover malformed commands and selector drift. The checker's
live mode is reserved for explicitly validating the ignored full-report selector because doing so
requires building the heavyweight `test-gameplay-full` artifact.

Before committing, run the gate that owns the changed contract. For ordinary core behavior:

```text
python ci.py gate
```

For a shared/cross-gameplay harness change use `python ci.py gate --gameplay`. For one focused probe use its
scope, such as `python ci.py gate --gameplay progression`; add `--core` only when production/core
behavior changed too. The same rule applies to shader, documentation, and lint-only work. A pre-commit
checkpoint should prove the relevant behavior, not rebuild unrelated artifacts by ritual.

If `cargo test-fast` was just run as the final checkpoint, there is no value in immediately running it
a second time before continuing to code. Finish the change first, then run the gate once. Cargo's
incremental cache still makes a recent checkpoint useful to the later gate without turning verification
into a ritual after every edit.

`cargo check-all` remains available as an all-target compile-only diagnostic, but it is not part of
the normal pre-commit sequence. `cargo check-fast` and `cargo test-fast` both compile production code
with Rust warnings denied; `cargo test-fast` additionally compiles and executes default-feature unit
tests. Clippy is an explicit lint checkpoint, not a prerequisite for every behavior build.

Add only the lanes owned by the change. For example, a gameplay-harness-only change uses
`python ci.py gate --gameplay`; a production change that affects both ordinary unit contracts and
gameplay uses `python ci.py gate --core --gameplay`. The all-gameplay lane already contains the five
focused targets, so do not rerun their individual aliases afterward. Use `python ci.py audit` when a
broad maintained core+gameplay checkpoint is warranted; add `gate --soak` only when long-horizon evidence
is relevant. `cargo test-release` remains an explicit optimized-build diagnostic when release-mode
behavior itself matters.
