//! Industrial-workshop contracts isolated from the scenario gate binary for fast exact reruns.

#[path = "gameplay_harness/capability_boundary.rs"]
mod capability_boundary;
#[allow(
    dead_code,
    reason = "contract target reuses scenario configuration without the report/gate entrypoints"
)]
#[path = "gameplay_harness/configuration.rs"]
mod configuration;
#[allow(
    dead_code,
    reason = "contract target exercises only the workshop contract subset of shared assertions"
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
#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;
#[allow(
    dead_code,
    reason = "contract target needs report data types but does not render gameplay reports"
)]
#[path = "gameplay_harness/report.rs"]
mod report;
#[allow(
    dead_code,
    reason = "contract target constructs selected scenario variants without the full runner"
)]
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
    reason = "contract tests call selected workshop internals without the public gate entrypoint"
)]
#[path = "gameplay_harness/workshop.rs"]
mod workshop;

#[path = "gameplay_harness/workshop_contract_tests.rs"]
mod workshop_contract_tests;
