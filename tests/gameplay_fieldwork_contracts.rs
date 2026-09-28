//! Fieldwork contracts isolated from the play-like probe binary for fast exact reruns.

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

macro_rules! include_fieldwork_contract_tests {
    () => {
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
macro_rules! include_fieldwork_recovery_contract_tests {
    () => {
        #[path = "recovery_tests.rs"]
        mod tests;
    };
}

#[path = "gameplay_harness/environment.rs"]
mod environment;
#[allow(
    dead_code,
    reason = "contract target intentionally omits unrelated consumers of shared equipment helpers"
)]
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[allow(
    dead_code,
    reason = "contract target exercises fieldwork internals without invoking the play-like probe entrypoint"
)]
#[path = "gameplay_harness/fieldwork_probe.rs"]
mod fieldwork_probe;
#[allow(
    dead_code,
    reason = "fieldwork contracts construct explicit replay cases and do not exercise every focused sampling role"
)]
#[path = "gameplay_harness/focused_case.rs"]
mod focused_seeds;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/manual_assembly_planning.rs"]
mod manual_assembly_planning;
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
#[allow(
    dead_code,
    reason = "contract target uses only the fieldwork execution subset of shared ore-recovery support"
)]
#[path = "gameplay_harness/manual_ore_recovery.rs"]
mod manual_ore_recovery;
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
#[path = "gameplay_harness/prospecting_timing.rs"]
mod prospecting_timing;
#[path = "gameplay_harness/seed_mix.rs"]
mod seed;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;

#[path = "gameplay_harness/prospecting_instrument_contract_tests.rs"]
mod prospecting_instrument_contract_tests;
