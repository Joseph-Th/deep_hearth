//! Power-provider generator contracts kept off the frequent lived probe.

#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    reason = "contract-only crate reuses the power-provider evaluator without compiling the lived-probe runner"
)]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

macro_rules! include_power_provider_generation_contract_tests {
    () => {
        #[path = "power_provider_probe/generation_tests.rs"]
        mod generation_tests;
    };
}

#[path = "gameplay_harness/bulk_fieldwork_workload.rs"]
mod bulk_fieldwork_workload;
#[path = "gameplay_harness/capital_investment_policy.rs"]
mod capital_investment_policy;
#[path = "gameplay_harness/direct_consumption_timing.rs"]
mod direct_consumption_timing;
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/exact_local_runtime.rs"]
mod exact_local_runtime;
#[path = "gameplay_harness/focused_case.rs"]
mod focused_case;
#[path = "gameplay_harness/focused_witnesses.rs"]
mod focused_witnesses;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/maintenance_timing.rs"]
mod maintenance_timing;
#[path = "gameplay_harness/manual_construction_package_planning.rs"]
mod manual_construction_package_planning;
#[path = "gameplay_harness/manual_construction_planning.rs"]
mod manual_construction_planning;
#[path = "gameplay_harness/manual_craft_batches.rs"]
mod manual_craft_batches;
#[path = "gameplay_harness/manual_craft_execution.rs"]
mod manual_craft_execution;
#[path = "gameplay_harness/manual_craft_planning.rs"]
mod manual_craft_planning;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[path = "gameplay_harness/manual_power_timing.rs"]
mod manual_power_timing;
#[path = "gameplay_harness/material_selection.rs"]
mod material_selection;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[path = "gameplay_harness/power_provider_probe.rs"]
mod power_provider_probe;
#[path = "gameplay_harness/primitive_workload.rs"]
mod primitive_workload;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/settlement_demand.rs"]
mod settlement_demand;
#[path = "gameplay_harness/stationary_survival.rs"]
mod stationary_survival;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
