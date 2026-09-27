//! Primitive-progression contracts isolated from the play-like probe binary for fast exact reruns.

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[allow(
    dead_code,
    unused_imports,
    reason = "progression contracts reuse selected first-foundry projections without its report path"
)]
#[path = "gameplay_harness/first_foundry_probe.rs"]
mod first_foundry_probe;
#[allow(
    dead_code,
    unused_imports,
    reason = "progression contracts reuse focused case types without invoking the probe runner"
)]
#[path = "gameplay_harness/focused_runner.rs"]
mod focused_runner;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/maintenance_timing.rs"]
mod maintenance_timing;
#[path = "gameplay_harness/manual_craft_batches.rs"]
mod manual_craft_batches;
#[path = "gameplay_harness/manual_craft_execution.rs"]
mod manual_craft_execution;
#[path = "gameplay_harness/manual_craft_planning.rs"]
mod manual_craft_planning;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[path = "gameplay_harness/manual_ore_recovery.rs"]
mod manual_ore_recovery;
#[path = "gameplay_harness/manual_power_timing.rs"]
mod manual_power_timing;
#[path = "gameplay_harness/material_selection.rs"]
mod material_selection;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[allow(
    dead_code,
    unused_imports,
    reason = "progression contracts reuse primitive-liberation planners without running its report"
)]
#[path = "gameplay_harness/primitive_liberation.rs"]
mod primitive_liberation;
#[path = "gameplay_harness/primitive_workload.rs"]
mod primitive_workload;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[allow(
    dead_code,
    unused_imports,
    reason = "progression contracts exercise selected probe helpers without the full episode"
)]
#[path = "gameplay_harness/progression_probe.rs"]
mod progression_probe;
#[path = "gameplay_harness/prospecting_timing.rs"]
mod prospecting_timing;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;

#[path = "gameplay_harness/progression_contract_tests.rs"]
mod progression_contract_tests;
