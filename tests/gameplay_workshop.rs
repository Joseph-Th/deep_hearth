//! Focused workshop gameplay target for fast operational iteration.

use std::env;

#[path = "gameplay_harness/capability_boundary.rs"]
mod capability_boundary;
#[path = "gameplay_harness/configuration.rs"]
mod configuration;
#[path = "gameplay_harness/contracts.rs"]
mod contracts;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/fresh_seed.rs"]
mod fresh_seed;
#[path = "gameplay_harness/industrial_support.rs"]
mod industrial_support;
#[path = "gameplay_harness/report.rs"]
mod report;
#[path = "gameplay_harness/scenario.rs"]
mod scenario;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[cfg(test)]
#[path = "gameplay_harness/seed_contract_tests.rs"]
mod seed_contract_tests;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/support.rs"]
mod support;

fn has_verbose_output() -> bool {
    env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some()
}

macro_rules! println {
    ($($argument:tt)*) => {{
        if has_verbose_output() {
            std::println!($($argument)*);
        }
    }};
}

#[path = "gameplay_harness/workshop.rs"]
mod workshop;

#[cfg(test)]
#[path = "gameplay_harness/workshop_contract_tests.rs"]
mod workshop_contract_tests;
