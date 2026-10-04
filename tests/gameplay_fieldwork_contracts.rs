//! Fieldwork planning, adaptation, and prospecting-instrument contracts.
//!
//! The lived fieldwork probe has its own smaller target. Keeping these contract-only tests here
//! avoids linking dozens of unrelated test bodies during the common probe iteration loop while
//! retaining a purpose-built target for exact contract repair.

#![allow(
    dead_code,
    reason = "this contract-only crate reuses fieldwork modules whose lived-probe entrypoints are compiled in the separate focused target"
)]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

#[path = "gameplay_harness/bulk_fieldwork_workload.rs"]
mod bulk_fieldwork_workload;

macro_rules! include_fieldwork_contract_tests {
    () => {
        #[path = "fieldwork_shortfall_policy_tests.rs"]
        mod fieldwork_shortfall_policy_tests;
        #[path = "fieldwork_probe/planning_tests.rs"]
        mod planning_tests;
        #[path = "fieldwork_probe/retooling_tests.rs"]
        mod retooling_tests;
        #[path = "fieldwork_probe/supply_tests.rs"]
        mod supply_tests;
    };
}

macro_rules! include_fieldwork_campaign_contract_tests {
    () => {
        #[path = "campaign_tests.rs"]
        mod tests;
    };
}

#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/fieldwork_probe.rs"]
mod fieldwork_probe;
#[path = "gameplay_harness/focused_case.rs"]
mod focused_case;
#[path = "gameplay_harness/focused_witnesses.rs"]
mod focused_witnesses;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/manual_assembly_planning.rs"]
mod manual_assembly_planning;
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
#[path = "gameplay_harness/manual_craft_topology_planning.rs"]
mod manual_craft_topology_planning;
#[path = "gameplay_harness/manual_ore_recovery.rs"]
mod manual_ore_recovery;
#[path = "gameplay_harness/material_selection.rs"]
mod material_selection;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/persistence_timing.rs"]
mod persistence_timing;
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[path = "gameplay_harness/primitive_workload.rs"]
mod primitive_workload;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/prospecting_instrument_contract_tests.rs"]
mod prospecting_instrument_contract_tests;
#[path = "gameplay_harness/prospecting_timing.rs"]
mod prospecting_timing;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
