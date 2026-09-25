//! Focused industrial-workshop gameplay target for the fast edit/test loop.

#[cfg(not(test))]
#[path = "gameplay_harness/agency.rs"]
mod agency;
#[path = "gameplay_harness/capability_boundary.rs"]
mod capability_boundary;
#[cfg(not(test))]
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
#[cfg(not(test))]
#[path = "gameplay_harness/fresh_seed.rs"]
mod fresh_seed;
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
#[cfg(not(test))]
pub(super) use crate::output;
#[cfg(test)]
#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;
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

#[path = "gameplay_harness/workshop.rs"]
mod workshop;

#[cfg(test)]
#[test]
fn gameplay_harness_gate() {
    workshop::run_gameplay_harness(configuration::ScenarioPlanMode::Gate);
}

#[cfg(not(test))]
pub(super) fn run_report() {
    let mut arguments = std::env::args().skip(1);
    let mode = arguments.next();
    if let Some(extra) = arguments.next() {
        eprintln!("gameplay-workshop-report: unexpected extra argument {extra:?}");
        std::process::exit(2);
    }
    match mode.as_deref() {
        None | Some("workshop") => {
            workshop::run_gameplay_harness(configuration::ScenarioPlanMode::Explore);
        }
        Some("agency") => agency::run_exploratory_agency_counterfactuals(),
        Some(mode) => {
            eprintln!(
                "gameplay-workshop-report: unknown mode {mode:?}; expected workshop or agency"
            );
            std::process::exit(2);
        }
    }
}
