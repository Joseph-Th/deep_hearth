//! Ore-preparation generator contracts kept off the frequent lived probe.

#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    reason = "contract-only crate reuses the ore evaluator without compiling the lived-probe runner"
)]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

#[path = "gameplay_harness/capability_boundary.rs"]
mod capability_boundary;
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/focused_case.rs"]
mod focused_case;
#[path = "gameplay_harness/focused_witnesses.rs"]
mod focused_witnesses;
#[path = "gameplay_harness/industrial_support.rs"]
mod industrial_support;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/material_selection.rs"]
mod material_selection;
#[path = "gameplay_harness/ore_contract_tests.rs"]
mod ore_contract_tests;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/ore_probe.rs"]
mod ore_probe;
#[path = "gameplay_harness/ore_setup.rs"]
mod ore_setup;
#[path = "gameplay_harness/production_support.rs"]
mod production_support;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/structural_fixture.rs"]
mod structural_fixture;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
