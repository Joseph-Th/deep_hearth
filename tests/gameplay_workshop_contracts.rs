//! Workshop contracts isolated from the player-level probe for fast exact reruns.

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

#[path = "gameplay_harness/capability_boundary.rs"]
mod capability_boundary;
#[allow(
    dead_code,
    reason = "contract target uses scenario construction but not every maintained/exploratory label"
)]
#[path = "gameplay_harness/configuration.rs"]
mod configuration;
#[allow(
    dead_code,
    reason = "contract target calls selected scenario checks directly instead of aggregate harness assertions"
)]
#[path = "gameplay_harness/contracts.rs"]
mod contracts;
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/industrial_support.rs"]
mod industrial_support;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/maintenance_timing.rs"]
mod maintenance_timing;
#[path = "gameplay_harness/manual_power_timing.rs"]
mod manual_power_timing;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[allow(
    dead_code,
    reason = "contract target inspects scenario results without rendering every report field"
)]
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
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[allow(
    dead_code,
    reason = "contract target calls the scenario runner directly rather than the outer gameplay harness entrypoint"
)]
#[path = "gameplay_harness/workshop.rs"]
mod workshop;

#[path = "gameplay_harness/workshop_contract_tests.rs"]
mod workshop_contract_tests;
