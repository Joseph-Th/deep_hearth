//! Primitive-progression topology and episode contracts kept off the frequent lived probe.

#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    reason = "contract-only crate reuses progression evaluator paths whose reporting entrypoints belong to the focused target"
)]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

macro_rules! include_progression_probe_contract_tests {
    () => {
        #[path = "progression_probe/steady_state_tests.rs"]
        mod steady_state_tests;
    };
}

#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/focused_case.rs"]
mod focused_case;
#[path = "gameplay_harness/focused_witnesses.rs"]
mod focused_witnesses;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/maintenance_timing.rs"]
mod maintenance_timing;
#[path = "gameplay_harness/manual_assembly_planning.rs"]
mod manual_assembly_planning;
#[path = "gameplay_harness/manual_craft_planning.rs"]
mod manual_craft_planning;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[path = "gameplay_harness/manual_craft_topology_planning.rs"]
mod manual_craft_topology_planning;
#[path = "gameplay_harness/manual_ore_recovery.rs"]
mod manual_ore_recovery;
#[path = "gameplay_harness/manual_power_timing.rs"]
mod manual_power_timing;
#[path = "gameplay_harness/material_selection.rs"]
mod material_selection;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/primitive_workload.rs"]
mod primitive_workload;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/progression_contract_tests.rs"]
mod progression_contract_tests;
#[path = "gameplay_harness/progression_episode_contract_tests.rs"]
mod progression_episode_contract_tests;
#[path = "gameplay_harness/progression_probe.rs"]
mod progression_probe;
#[path = "gameplay_harness/prospecting_timing.rs"]
mod prospecting_timing;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
