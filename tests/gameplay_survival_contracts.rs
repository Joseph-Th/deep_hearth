//! Survival contracts isolated from the player-level probe for fast exact reruns.

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

#[allow(
    dead_code,
    reason = "survival contracts reuse timing support through only the evaluator branches they exercise"
)]
#[path = "gameplay_harness/direct_consumption_timing.rs"]
mod direct_consumption_timing;
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[allow(
    dead_code,
    reason = "contract cases use replay roles without rendering all focused-case labels"
)]
#[path = "gameplay_harness/focused_case.rs"]
mod focused_case;
#[allow(
    dead_code,
    reason = "contract generation uses focused seed planning without the player-probe dispatch helper"
)]
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[allow(
    dead_code,
    reason = "survival contracts compile shared branches that do not all execute manual power"
)]
#[path = "gameplay_harness/manual_power_timing.rs"]
mod manual_power_timing;
#[allow(
    dead_code,
    reason = "contract target does not render player-facing physical-duration labels"
)]
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[path = "gameplay_harness/preservation_route.rs"]
mod preservation_route;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[allow(
    dead_code,
    reason = "survival contracts compile shared branches that do not all execute prospecting"
)]
#[path = "gameplay_harness/prospecting_timing.rs"]
mod prospecting_timing;
#[allow(
    dead_code,
    reason = "contract target uses deterministic seed mixing without the maintained probe root constant"
)]
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[allow(
    dead_code,
    unused_imports,
    reason = "survival contracts exercise selected evaluator internals without invoking the player-level report entrypoint"
)]
#[path = "gameplay_harness/survival_probe.rs"]
mod survival_probe;
#[path = "gameplay_harness/temporal.rs"]
mod temporal;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;

#[path = "gameplay_harness/survival_contract_tests.rs"]
mod survival_contract_tests;
