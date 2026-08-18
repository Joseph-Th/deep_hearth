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
cargo test-fast <qualified-test-name> -- --exact  # behavior feedback
cargo test-fast                                   # coherent checkpoint
python ci.py gate                                 # once before commit
```

`cargo check-fast` is intentionally library-only and does not build the monolithic unit-test harness.
Use it for compile/type feedback when an edit has not yet reached a behavior checkpoint. An exact test
is still required for changed behavior; the check alias exists to avoid paying test codegen/link cost
for intermediate mechanical edits.

`python ci.py gate` is the concise local pre-commit wrapper. It runs format and the fast core tests,
captures successful Cargo noise, prints one timed line per stage, and shows native command output only
when a stage fails. Rust compiler warnings are denied by the package lint configuration, so this path
does not need a second Clippy compile merely to catch ordinary warning regressions. Add `--lint` when a
Clippy checkpoint is useful. Scope is explicit rather than inferred from git diffs: add `--soak`,
`--gameplay`, `--shaders`, or `--docs` when those contracts changed.

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
| `cargo test-fast` | Ordinary deterministic behavior, errors, persistence, and integrations | Default-feature unit-test artifact; soak-only bodies are not compiled |
| `cargo test-soak` | Long-horizon deterministic conservation/invariant scenarios | Adds `test-soak` and runs only ignored tests |
| `cargo test-gameplay` | Replay contracts, maintained anchors, bounded fresh workshop sampling, survival provisioning, progression, and capability probes | Dedicated integration target with `test-gameplay`; library unit-test bodies are not compiled |
| `cargo test-gameplay-scenarios` | Five maintained workshop anchors plus two fresh replayable organic cases | Reuses the dedicated gameplay artifact |
| `cargo test-gameplay-survival` | Food freshness/preservation, varied meal, and finite-water provisioning probe | Reuses the dedicated gameplay artifact; one maintained plus one organic sample by default |
| `cargo test-gameplay-progression` | Primitive survival/craft/mine/manual-power/mechanization progression probe only | Reuses the dedicated gameplay artifact; one maintained plus one organic sample by default |
| `cargo test-gameplay-ore` | Ore-preparation capability probe only | Reuses the dedicated gameplay artifact; one maintained plus one organic sample by default |
| `cargo test-gameplay-foundry` | Foundry capability probe only | Reuses the dedicated gameplay artifact; one maintained plus one organic sample by default |
| `cargo test-gameplay-report` | Exploratory anchor-plus-organic workshop report with concise human-readable summary | Same feature-gated integration target, ignored by the gate and run with captured output disabled |
| `cargo test-shaders` | Naga parse/semantic validation of assembled WGSL without compiling the crate unit-test harness | Adds `test-shader-validation` |
| `cargo test-check` | Silent all-target compilation of the default feature set | Default features |
| `cargo test-lint` | Optional production-library Clippy checkpoint | Default-feature library only; not part of the routine gate |
| `cargo test-lint-all` | Cross-cutting/release Clippy audit | All test features |
| `cargo test-all` | Ordinary plus ignored core/soak tests in one invocation | Adds `test-soak` once and runs the combined core inventory |
| `cargo test-all-features` | Manual combined-feature test diagnostic | All test features in one artifact; intentionally outside maintained CI presets because specialized artifacts are faster |
| `cargo test-release` | Complete optimized test inventory | All test features |
| `cargo test-doc` | Documentation build without dependencies | Default features |

`test-gameplay` exists only to expose the controlled bootstrap adapter required by the integration
harness; that adapter remains absent from ordinary production builds and delegates to canonical runtime
transactions. `test-shader-validation` likewise exists only to expose Naga-backed WGSL validation.
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

The gameplay harness is an **exercise-mode** automated behavior evaluation. It deliberately chooses
legal situations that cover important physical consequences; it is not evidence that an ordinary
human player will make the same choices. Setup may arrange matter, potable fluid, equipment, finite
energy, and structural bays because acquisition/construction authorizers are not implemented. After setup, the
harness uses the same production validators, resolutions, commits, and simulation ticks as normal
runtime behavior. A compact survival-provisioning probe exercises implemented food freshness,
preservation, a three-category meal, finite drinking water, biological ownership, and matter/fluid
conservation. Its meal quantities are derived from current physiology and authored food energy rather
than copied balance constants, while its preservation factor and bounded storage/depletion horizons are
scenario inputs. The workshop matrix evaluates constrained operation and recovery, while the
primitive progression probe separately follows the early-game fantasy through survival-costed manual
crafting, finite mining of both mineralized ore and a distinct native-metal occurrence,
native-copper cold-working, in-place equipment reinforcement, repeated component shaping,
material-backed power-storage construction, primitive-machine assembly, manual
power generation, stored work, and mechanized comminution. The probe compares equal-mass extraction
before/after pick reinforcement and the same exact energy charge before/after crank reinforcement. It
then starts another canonical mining job while the primitive crusher runs and proves the shorter machine
job completes while the player's longer mining work remains active, making freed attention an observed
mechanization result rather than an inferred timer improvement. Focused unit coverage separately proves
ordinary ore form cannot enter that route and contaminated native-metal composition is rejected as well.

The acting policy uses observable state and resolver projections. Hidden authoritative state may be
used only for diagnostics and postcondition checks. The workshop initializes normal player survival, so
its elapsed simulation time also consumes authored metabolic-energy and hydration reserves; the compact
report and matched-world agency panel expose that cost rather than treating time as free. The policy does not clone `AppState` to simulate
compound future mutations that normal callers cannot preview. It chooses from current canonical
projections, knows scheduled-event timing supplied by the scenario, and reacts to the actual resulting
state after that event. Physical scenario randomness and automated-player behavior randomness are
separate channels: changing the behavior seed cannot change ore, support, condition, delivery, or work
reserves, while changing the world seed cannot change the selected player policy. Each resulting case
is deterministic from its printed world/behavior pair. The required scenario gate keeps five maintained
anchor pairs for stable regression comparison and adds two fresh bounded organic cases on every run;
the explicit report lane adds four. Fresh roots are printed before execution, so a failure remains
exactly replayable without turning the ordinary harness into one memorized script. A legal scenario may
complete zero batches when an in-flight job is suspended before its first output; that is gameplay
evidence, not a harness failure. Maintained anchors guarantee power-policy diversity, both maintenance
styles, both structural-risk styles, all three initial maintenance bands, and one case where delivery
timing makes the faster finite-power source strategically relevant. Aggregate balance outcomes remain observations.
Balance-dependent outcomes such as completion, maintenance pressure, structural damage, suspension,
and relocation are reported as observations rather than frozen into aggregate pass/fail requirements.

Direct fixture-only starting-state injection is deliberately isolated in
`src/content/gameplay_fixture.rs`. That feature-gated bridge may seed loose matter and stored energy or
potable fluid, or materialize already-planned structures because the corresponding acquisition,
generation, and construction authorizers do not exist yet. The acting policy cannot call those shortcuts. Ordinary
stockpile allocation, structural geometry, equipment
allocation, process resolution, maintenance, support changes, production, and ticks use the normal
runtime APIs.

The gameplay exercise source lives under `tests/gameplay_harness/` and is an integration-test target
rather than library code or a crate unit test. This keeps its large policy/scenario implementation out
of both library codegen and the monolithic unit-test binary. A harness-only edit therefore rebuilds the
dedicated test target against the cached library instead of invalidating the feature-enabled core
crate. Configuration, execution contracts, probe setup, reporting, and seed mixing are separate modules
inside that one target; deterministic scenario-input generation is also isolated from workshop
execution so balance/input-policy edits do not accumulate in the controller. The lane still builds one
specialized artifact rather than multiplying integration targets. Seed/configuration contracts are
ordinary named tests with direct typed assertions instead of one aggregated boolean-gap test, so
failures point at the exact contract without adding another Cargo target.

The maintained workshop loop covers current-state structural siting, finite power choice,
processing-duty wear, exact replacement-stock maintenance, inventory-owned stored-matter loading,
persistent structural damage, production suspension, WIP recovery/stranding, survival-time cost, and
current mixed-ore processing frontier. The timed disruption is no longer a synthetic weather/load write: a real bulk
material transfer moves seeded starting matter into a mounted stockpile, and the inventory subsystem
updates the support's `StoredMatter` load through its canonical transaction. The harness chooses when
to attempt that transfer; this is not presented as an implemented logistics scheduler. Player
priorities vary independently of world physics within bounded legal choices: conserving scarce
high-power reserve versus minimizing completion time; preventive service at warning versus service only
when critical; and preserving structural margin versus relocating only when failure forces the issue. Safety,
ownership, support, energy, and critical-condition gates remain canonical regardless of personality.
Both generated bays must be legal crusher locations before play begins. Preserve-margin policy uses the
known scheduled delivery target to keep the crusher off that support; failure-only policy chooses by
current structural margin and reacts only if later load creates a real problem. The policy never
pre-applies the future delivery in a private cloned world. The known delivery time can still influence a
power choice when one real resolver projection finishes before the event and another does not. After
delivery, relocation decisions use the game's atomic equipment-relocation validator, including its
structural projection and commit, rather than a harness-only unmount/remount preview.

Scenario physics follow current authored content. Crusher batch mass is chosen inside the current
equipment limit; initial crusher condition varies across the full non-failed authored condition range,
so normal, warning, and critical starts can occur without hard-coding their frequencies. Each support is
a materialized 2 m wood member, and its cross-section is sized from the load it actually carries so
embodied mass and self-weight remain part of the same structural physics while both bays begin legal.
Background stored cargo scales from current equipment/material quantities; delivery mass spans roughly
20-160% of crusher mass, broad enough to create warnings, persistent damage, occasional collapse, and
recovery without targeting structural thresholds. The delivery tick is selected inside a work horizon
derived from an actually resolved batch duration. The support generator deliberately spans a broad
ordinary utilization range rather than targeting authored failure thresholds, so structural outcomes
can change naturally as game balance evolves. Workshop ore
uses copper plus stone host material, matching the current mineralized-ore representation used by the
mining path rather than substituting downstream slag as natural gangue.

The primitive progression probe derives its current legal path rather than maintaining a copied recipe
script. Equipment/store assembly profiles and upgrade additions determine which components must be
made; the crafting registry determines the producing manual processes, output quantities, required raw
matter, and active durations; the mining method plus current stone-pick capability determines a
seed-varied legal extraction batch; and ore grade varies within a bounded mixed copper/stone range. The
before/after mining comparison still uses the same mass, and crank comparison still requests the same
stored work, so those assertions test the gameplay benefit instead of a frozen balance ratio. The final
primitive stage overlaps autonomous crushing with player mining and verifies both owners progress on the
same simulation timeline. Raw starting matter and geological occurrences remain controlled bootstrap
because world generation and resource-acquisition ownership are still deferred.

Focused survival, progression, ore-preparation, and foundry probes do not run one permanently frozen
case. In their normal focused aliases they execute one maintained anchor plus one fresh organic sample;
the exact seed list is printed before execution. `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` reproduces the
organic focused sample, while `DEEP_HEARTH_GAMEPLAY_SEEDS` replaces the focused plan with the exact
listed seeds for direct replay or a deliberate wider sweep. This retains a stable comparison point while
regularly exercising nearby legal gameplay without multiplying Cargo targets or materially increasing
runtime.

Capability probes derive legal batch sizes, output partitions, and equipment limits from the current
authored registries instead of duplicating recipe constants. Their assertions focus on conservation,
resolver/authoring agreement, legal routing, condition changes, and state validity. They may report
currently unavailable direct routes as observations, but do not freeze those absences into gameplay
requirements. The ore-preparation and foundry probes remain explicitly labeled capability checks until
concentration/smelting provides a truthful bridge between those stages.

Assembly/recovery tests keep reversibility honest. Additive equipment-upgrade tests require stable
runtime identity, creation time, accumulated condition, existing material traces, matter accounting,
save/load validity, and stale-token rejection. Pristine equipment disassembly must restore exact
embodied traces without ID reuse, while any wear blocks exact recovery. Material-backed energy-store
disassembly has the analogous conservation/ID contract and rejects even a minimally charged store so
container removal cannot become an energy sink.

`cargo test-gameplay` keeps successful harness output captured. It runs five maintained anchor scenarios
plus two fresh organic workshop scenarios, seed/configuration and seed-separation contracts, the
survival-provisioning probe, primitive progression probe, and two separately named capability-probe
tests. The focused probes each use a maintained-plus-organic physical sample by default. The split makes
each expensive behavior slice directly targetable without
rerunning unrelated harness execution:

```text
cargo test-gameplay-scenarios
cargo test-gameplay-survival
cargo test-gameplay-progression
cargo test-gameplay-ore
cargo test-gameplay-foundry
```

Together these are the required gameplay gate. `cargo test-gameplay-report` runs the ignored
exploratory report instead;
it adds four fresh organic scenarios, emits a replay-input line, a compact content/reachability summary,
sampled input ranges, compact outcome and systems summaries, a two-world matched-policy agency panel,
and one scope line distinguishing what that invocation actually exercised from bootstrap/deferred
systems. The scope line separates observed from unobserved balance-dependent structural/recovery
outcomes so a run with no suspension or relocation does not claim that it experienced those events.
`EXPERIENCE` lines include final condition, elapsed time, survival expenditure, remaining
mechanical/maintenance reserves, and whether each power decision was discretionary, deadline-driven, or
forced by only one remaining source. The systems summary counts those decision bases plus throughput,
energy-delivery, and balanced bottlenecks per completed batch rather than reducing them to one boolean
per scenario. The content summary distinguishes
authored equipment/energy definitions from runtime-assemblable definitions and upgrade routes so a cold
reader does not mistake controlled industrial fixtures for currently reachable progression. Set
`DEEP_HEARTH_GAMEPLAY_VERBOSE` to any value before that report lane to emit the detailed decision trace
and the registry-derived catalog in stable ID order. Seed controls are:

- `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED`: fixes the organic world/scenario root to one exact decimal or
  hexadecimal value; when omitted, a fresh root is generated for the run. Focused probes use this same
  physical root to derive their organic sample with probe-specific salts;
- `DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED`: independently fixes the organic automated-player policy root;
  maintained anchors keep their fixed behavior seeds so the stable baseline is unchanged;
- `DEEP_HEARTH_GAMEPLAY_SEEDS`: replaces the anchor-plus-organic world plan with an exact
  comma-separated world-seed list for reproduction or deliberate sweeps; the behavior root remains a
  separate input.

Seed lists fail on an empty or malformed entry rather than silently dropping it. Focused probes print
their exact seed list before execution. Hard failures remain
canonical execution/invariant failures and capability-probe conservation failures, not required
balance outcomes. The input line prints anchor/organic/custom counts, world root, behavior root, and
every exact `world@behavior` pair. Re-running with the printed roots reproduces the generated matrix;
explicit custom seed lists are labeled as custom, run the same per-scenario contracts, and do not claim
aggregate outcome coverage. Scenario-only summaries explicitly mark the survival/progression/ore/foundry
probes as not run rather than implying results from separate targets.

## Local CI and completion gates

Verification runs in the developer workspace. GitHub Actions and hosted runners are prohibited. Use the
repository-owned `ci.py` runner or the Cargo aliases directly; no pull-request job, scheduled workflow,
or remote runner owns the validation contract. The runner deliberately does not inspect changed files
or guess scope: explicit flags are faster to understand and cannot silently omit a relevant lane.

1. **Routine**: `python ci.py gate`.
2. **Lint checkpoint**: add `--lint` when Clippy-specific feedback is useful.
3. **Core + Soak**: `python ci.py gate --soak` (builds the `test-soak` core artifact once).
4. **Gameplay**: add `--gameplay` when workshop behavior or content changed.
5. **Shaders**: add `--shaders` when WGSL or shader assembly changed.
6. **Cross-cutting local completion**: `python ci.py full` runs core+soak and gameplay without pulling
   shader/parser or documentation builds into unrelated work. Add `--shaders` or `--docs` only when
   those contracts changed.
7. **Explicit hardening**: `python ci.py hardening` pays for all-target/all-feature Clippy, then reuses
   the maintained core+soak, gameplay, shader, and documentation lanes rather than building a second
   monolithic all-feature test artifact.

Local Cargo incremental state may be reused naturally between these commands. Fast core deliberately
does not compile soak-only fixtures; gameplay deliberately does not compile the crate unit-test harness.
Release hardening and documentation remain explicit local commands rather than background or scheduled
work.

Whenever the gameplay lane is selected, `ci.py` first runs `tools/check_gameplay_aliases.py`. That
contract lists the real `gameplay_harness` test inventory and its ignored subset, verifies every
filtered Cargo alias selects exactly one expected test with the correct active/ignored status, and
fails on uncontracted filtered aliases. The checker includes cheap synthetic self-tests for a valid
inventory, a missing selector, and ignored-status drift. This prevents Cargo's otherwise-successful
zero-match filtering behavior from making a stale focused alias look like a passing verification lane.

Before committing, run:

```text
python ci.py gate
```

If `cargo test-fast` was just run as the final checkpoint, there is no value in immediately running it
a second time before continuing to code. Finish the change first, then run the gate once. Cargo's
incremental cache still makes a recent checkpoint useful to the later gate without turning verification
into a ritual after every edit.

`cargo test-check` remains available as an all-target compile-only diagnostic, but it is not part of
the normal pre-commit sequence. `cargo check-fast` and `cargo test-fast` both compile production code
with Rust warnings denied; `cargo test-fast` additionally compiles and executes default-feature unit
tests. Clippy is an explicit lint checkpoint, not a prerequisite for every behavior build.

Also add `--soak`, `--gameplay`, `--shaders`, or `--docs` when the changed contract is owned by that
lane. Use `python ci.py full` when a cross-cutting change needs every maintained local lane. A combined
`cargo test-all-features` build remains available as a manual diagnostic, but it is not part of the
maintained presets because all-feature Clippy already checks feature compatibility and the specialized
test artifacts avoid redundant codegen/linking.
Release hardening is deliberately separate:

```text
python ci.py hardening
cargo test-release     # only when optimized-build behavior matters
```
