//! Consolidated gameplay checkpoint.
//!
//! Focused edit loops use smaller integration-test crates. Broad checkpoints compile the shared
//! harness graph once here rather than rebuilding common support across focused targets.

#![allow(
    dead_code,
    unused_assignments,
    unused_imports,
    unused_variables,
    reason = "the consolidated audit assembles every gameplay helper while report-only paths are excluded from test builds; focused and owner targets remain warning-clean"
)]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

macro_rules! include_woodworking_policy_contract_tests {
    () => {
        #[path = "woodworking_policy_tests.rs"]
        mod tests;
    };
}

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

#[path = "gameplay_harness/agency.rs"]
mod agency;
#[path = "gameplay_harness/capability_boundary.rs"]
mod capability_boundary;
#[path = "gameplay_harness/capital_investment_policy.rs"]
mod capital_investment_policy;
#[path = "gameplay_harness/catalog.rs"]
mod catalog;
#[path = "gameplay_harness/configuration.rs"]
mod configuration;
#[path = "gameplay_harness/contracts.rs"]
mod contracts;
#[path = "gameplay_harness/direct_consumption_timing.rs"]
mod direct_consumption_timing;
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/fieldwork_probe.rs"]
mod fieldwork_probe;
#[path = "gameplay_harness/fieldwork_shortfall_policy.rs"]
mod fieldwork_shortfall_policy;
#[path = "gameplay_harness/fieldwork_shortfall_policy_tests.rs"]
mod fieldwork_shortfall_policy_tests;
#[path = "gameplay_harness/first_foundry_probe.rs"]
mod first_foundry_probe;
#[path = "gameplay_harness/focused_case.rs"]
mod focused_case;
#[path = "gameplay_harness/focused_runner.rs"]
mod focused_runner;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/focused_witnesses.rs"]
mod focused_witnesses;
#[path = "gameplay_harness/foundry_setup.rs"]
mod foundry_setup;
#[path = "gameplay_harness/industrial_support.rs"]
mod industrial_support;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/maintenance_timing.rs"]
mod maintenance_timing;
#[path = "gameplay_harness/manual_assembly_planning.rs"]
mod manual_assembly_planning;
#[path = "gameplay_harness/manual_craft_batches.rs"]
mod manual_craft_batches;
#[path = "gameplay_harness/manual_craft_equipment_planning.rs"]
mod manual_craft_equipment_planning;
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
#[path = "gameplay_harness/manual_ore_recovery_evaluation.rs"]
mod manual_ore_recovery_evaluation;
#[path = "gameplay_harness/manual_power_timing.rs"]
mod manual_power_timing;
#[path = "gameplay_harness/material_selection.rs"]
mod material_selection;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/ore_setup.rs"]
mod ore_setup;
#[path = "gameplay_harness/persistence_timing.rs"]
mod persistence_timing;
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[path = "gameplay_harness/power_provider_probe.rs"]
mod power_provider_probe;
#[path = "gameplay_harness/powered_craft_planning.rs"]
mod powered_craft_planning;
#[path = "gameplay_harness/preservation_route.rs"]
mod preservation_route;
#[path = "gameplay_harness/primitive_liberation.rs"]
mod primitive_liberation;
#[path = "gameplay_harness/production_support.rs"]
mod production_support;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/prospecting_timing.rs"]
mod prospecting_timing;
#[path = "gameplay_harness/report.rs"]
mod report;
#[path = "gameplay_harness/scenario.rs"]
mod scenario;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/settlement_power_planning.rs"]
mod settlement_power_planning;
#[path = "gameplay_harness/settlement_probe.rs"]
mod settlement_probe;
#[path = "gameplay_harness/structural_fixture.rs"]
mod structural_fixture;
#[path = "gameplay_harness/temporal.rs"]
mod temporal;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;

#[path = "gameplay_harness/capital_investment_policy_tests.rs"]
mod capital_investment_policy_tests;
#[path = "gameplay_harness/configuration_tests.rs"]
mod configuration_tests;
#[path = "gameplay_harness/fixture_boundary_tests.rs"]
mod fixture_boundary_tests;
#[path = "gameplay_harness/foundry_contract_tests.rs"]
mod foundry_contract_tests;
#[path = "gameplay_harness/foundry_probe.rs"]
mod foundry_probe;
#[path = "gameplay_harness/manual_craft_selection_tests.rs"]
mod manual_craft_selection_tests;
#[path = "gameplay_harness/ore_contract_tests.rs"]
mod ore_contract_tests;
#[path = "gameplay_harness/ore_probe.rs"]
mod ore_probe;
#[path = "gameplay_harness/primitive_liberation_contract_tests.rs"]
mod primitive_liberation_contract_tests;
#[path = "gameplay_harness/primitive_workload.rs"]
mod primitive_workload;
#[path = "gameplay_harness/process_catalog_contract_tests.rs"]
mod process_catalog_contract_tests;
#[path = "gameplay_harness/progression_contract_tests.rs"]
mod progression_contract_tests;
#[path = "gameplay_harness/progression_episode_contract_tests.rs"]
mod progression_episode_contract_tests;
#[path = "gameplay_harness/progression_probe.rs"]
mod progression_probe;
#[path = "gameplay_harness/prospecting_instrument_contract_tests.rs"]
mod prospecting_instrument_contract_tests;
#[path = "gameplay_harness/saw_bench_contract_tests.rs"]
mod saw_bench_contract_tests;
#[path = "gameplay_harness/scenario_tests.rs"]
mod scenario_tests;
#[path = "gameplay_harness/seed_contract_tests.rs"]
mod seed_contract_tests;
#[path = "gameplay_harness/settlement_drill_contract_tests.rs"]
mod settlement_drill_contract_tests;
#[path = "gameplay_harness/settlement_fixture.rs"]
mod settlement_fixture;
#[path = "gameplay_harness/settlement_generation.rs"]
mod settlement_generation;
#[path = "gameplay_harness/settlement_generation_contract_tests.rs"]
mod settlement_generation_contract_tests;
#[path = "gameplay_harness/settlement_helve_contract_tests.rs"]
mod settlement_helve_contract_tests;
#[path = "gameplay_harness/settlement_machine_contract_tests.rs"]
mod settlement_machine_contract_tests;
#[path = "gameplay_harness/settlement_wire_contract_tests.rs"]
mod settlement_wire_contract_tests;
#[path = "gameplay_harness/survival_contract_tests.rs"]
mod survival_contract_tests;
#[path = "gameplay_harness/survival_probe.rs"]
mod survival_probe;
#[path = "gameplay_harness/woodworking_contract_tests.rs"]
mod woodworking_contract_tests;
#[path = "gameplay_harness/woodworking_policy.rs"]
mod woodworking_policy;
#[path = "gameplay_harness/woodworking_probe.rs"]
mod woodworking_probe;
#[path = "gameplay_harness/workshop.rs"]
mod workshop;
#[path = "gameplay_harness/workshop_contract_tests.rs"]
mod workshop_contract_tests;
#[path = "gameplay_harness/workshop_craft_planning.rs"]
mod workshop_craft_planning;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;

#[test]
fn gameplay_harness_gate() {
    workshop::run_gameplay_harness(configuration::ScenarioPlanMode::Gate);
}

#[test]
fn gameplay_survival_provisioning_probe() {
    focused_runner::run_focused_probe(
        "survival-provisioning",
        survival_probe::run_survival_provisioning_probe,
    );
}

#[test]
fn gameplay_agency_counterfactuals() {
    agency::run_gameplay_agency_counterfactuals();
}

#[test]
fn gameplay_settlement_probe() {
    focused_runner::run_focused_probe("settlement", settlement_probe::run_settlement_probe);
}

#[test]
fn gameplay_foundry_bootstrap_probe() {
    focused_runner::run_focused_probe(
        "foundry-bootstrap",
        first_foundry_probe::run_first_foundry_probe,
    );
}

#[test]
fn gameplay_primitive_progression_probe() {
    focused_runner::run_focused_probe(
        "primitive-progression",
        progression_probe::run_primitive_progression_probe,
    );
}

#[test]
fn gameplay_primitive_liberation_probe() {
    focused_runner::run_focused_probe(
        "primitive-liberation",
        primitive_liberation::run_primitive_liberation_probe,
    );
}

#[test]
fn gameplay_ore_preparation_probe() {
    focused_runner::run_focused_probe(
        "ore-preparation",
        ore_probe::run_ore_preparation_capability_probe,
    );
}

#[test]
fn gameplay_foundry_probe() {
    focused_runner::run_focused_probe("foundry", foundry_probe::run_foundry_capability_probe);
}

#[test]
fn gameplay_woodworking_probe() {
    focused_runner::run_focused_probe("woodworking", woodworking_probe::run_woodworking_probe);
}

#[test]
fn gameplay_fieldwork_probe() {
    focused_runner::run_focused_probe("fieldwork", fieldwork_probe::run_fieldwork_probe);
}

#[test]
fn gameplay_power_provider_probe() {
    focused_runner::run_focused_probe(
        "power-provider",
        power_provider_probe::run_power_provider_probe,
    );
}
