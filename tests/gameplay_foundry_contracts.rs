//! Foundry contracts isolated from the player-level probe for fast exact reruns.

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

#[path = "gameplay_harness/capability_boundary.rs"]
mod capability_boundary;
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[allow(
    dead_code,
    reason = "foundry contracts use setup helpers without constructing player-level focused cases"
)]
#[path = "gameplay_harness/focused_case.rs"]
mod focused_case;
#[allow(
    dead_code,
    reason = "foundry contracts need one maintained witness constant from the shared witness catalog"
)]
#[path = "gameplay_harness/focused_witnesses.rs"]
mod focused_witnesses;
#[allow(
    dead_code,
    reason = "foundry contracts exercise selected planning/setup helpers without running the full capability probe"
)]
#[path = "gameplay_harness/foundry_probe.rs"]
mod foundry_probe;
#[allow(
    dead_code,
    reason = "contract assertions inspect only the subset of setup identities relevant to each case"
)]
#[path = "gameplay_harness/foundry_setup.rs"]
mod foundry_setup;
#[path = "gameplay_harness/industrial_support.rs"]
mod industrial_support;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/manual_power_timing.rs"]
mod manual_power_timing;
#[path = "gameplay_harness/material_selection.rs"]
mod material_selection;
#[path = "gameplay_harness/production_support.rs"]
mod production_support;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[allow(
    dead_code,
    reason = "contract target uses seed mixing without the focused sampling root helpers"
)]
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/structural_fixture.rs"]
mod structural_fixture;
#[allow(
    dead_code,
    reason = "contract target does not execute the full thermal-recovery cooldown path"
)]
#[path = "gameplay_harness/temporal.rs"]
mod temporal;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;

#[path = "gameplay_harness/foundry_contract_tests.rs"]
mod foundry_contract_tests;
