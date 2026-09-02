//! Consolidated gameplay checkpoint.
//!
//! Focused edit loops keep their smaller integration-test crates. Broad checkpoints compile the
//! shared harness module graph once here so common support is not repeatedly code-generated and
//! linked into six separate binaries.

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

#[path = "gameplay_harness/agency.rs"]
mod agency;
#[path = "gameplay_harness/capability_boundary.rs"]
mod capability_boundary;
#[path = "gameplay_harness/catalog.rs"]
mod catalog;
#[path = "gameplay_harness/configuration.rs"]
mod configuration;
#[path = "gameplay_harness/contracts.rs"]
mod contracts;
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/focused_runner.rs"]
mod focused_runner;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/foundry_setup.rs"]
mod foundry_setup;
#[path = "gameplay_harness/fresh_seed.rs"]
mod fresh_seed;
#[path = "gameplay_harness/industrial_support.rs"]
mod industrial_support;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/maintenance_timing.rs"]
mod maintenance_timing;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[path = "gameplay_harness/manual_power_timing.rs"]
mod manual_power_timing;
#[path = "gameplay_harness/material_selection.rs"]
mod material_selection;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/ore_setup.rs"]
mod ore_setup;
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[path = "gameplay_harness/preservation_route.rs"]
mod preservation_route;
#[path = "gameplay_harness/production_support.rs"]
mod production_support;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/report.rs"]
mod report;
#[path = "gameplay_harness/scenario.rs"]
mod scenario;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/structural_fixture.rs"]
mod structural_fixture;
#[path = "gameplay_harness/temporal.rs"]
mod temporal;

#[path = "gameplay_harness/configuration_tests.rs"]
mod configuration_tests;
#[path = "gameplay_harness/fixture_boundary_tests.rs"]
mod fixture_boundary_tests;
#[path = "gameplay_harness/foundry_contract_tests.rs"]
mod foundry_contract_tests;
#[path = "gameplay_harness/foundry_probe.rs"]
mod foundry_probe;
#[path = "gameplay_harness/ore_contract_tests.rs"]
mod ore_contract_tests;
#[path = "gameplay_harness/ore_probe.rs"]
mod ore_probe;
#[path = "gameplay_harness/process_catalog_contract_tests.rs"]
mod process_catalog_contract_tests;
#[path = "gameplay_harness/progression_contract_tests.rs"]
mod progression_contract_tests;
#[path = "gameplay_harness/progression_probe.rs"]
mod progression_probe;
#[path = "gameplay_harness/scenario_tests.rs"]
mod scenario_tests;
#[path = "gameplay_harness/seed_contract_tests.rs"]
mod seed_contract_tests;
#[path = "gameplay_harness/survival_contract_tests.rs"]
mod survival_contract_tests;
#[path = "gameplay_harness/survival_probe.rs"]
mod survival_probe;
#[path = "gameplay_harness/workshop.rs"]
mod workshop;
#[path = "gameplay_harness/workshop_contract_tests.rs"]
mod workshop_contract_tests;

#[test]
fn gameplay_survival_provisioning_probe() {
    focused_runner::run_focused_probe(
        "survival-provisioning",
        survival_probe::run_survival_provisioning_probe,
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
