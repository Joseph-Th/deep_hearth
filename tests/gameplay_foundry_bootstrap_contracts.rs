//! First-foundry generator and planning contracts kept off the frequent lived probe.

#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    reason = "contract-only crate reuses the first-foundry evaluator without compiling the lived-probe runner"
)]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

macro_rules! include_first_foundry_generation_contract_tests {
    () => {
        #[path = "first_foundry_probe/generation_tests.rs"]
        mod generation_tests;
    };
}

#[path = "gameplay_harness/copper_progression_world.rs"]
mod copper_progression_world;
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/exact_local_runtime.rs"]
mod exact_local_runtime;
#[path = "gameplay_harness/first_foundry_probe.rs"]
mod first_foundry_probe;
#[path = "gameplay_harness/focused_case.rs"]
mod focused_case;
#[path = "gameplay_harness/focused_witnesses.rs"]
mod focused_witnesses;
#[path = "gameplay_harness/foundry_bootstrap_contract_tests.rs"]
mod foundry_bootstrap_contract_tests;
#[path = "gameplay_harness/inherited_condition.rs"]
mod inherited_condition;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/manual_craft_execution.rs"]
mod manual_craft_execution;
#[path = "gameplay_harness/manual_power_timing.rs"]
mod manual_power_timing;
#[path = "gameplay_harness/material_selection.rs"]
mod material_selection;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[path = "gameplay_harness/primitive_workload.rs"]
mod primitive_workload;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/workshop_craft_planning.rs"]
mod workshop_craft_planning;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;
