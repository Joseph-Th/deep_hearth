//! Consolidated broad gameplay audit and report target.
//!
//! Focused developer gates retain independent binaries so one concern can be repaired without
//! relinking unrelated probe code. Broad verification compiles the shared workshop and focused-probe
//! support once here and links one executable instead of paying for a workshop binary plus a second
//! aggregate binary.

use std::env;

#[path = "gameplay_harness/agency.rs"]
mod agency;
#[path = "gameplay_harness/capability_boundary.rs"]
mod capability_boundary;
#[cfg(test)]
#[path = "gameplay_harness/catalog_contract_tests.rs"]
mod catalog_contract_tests;
#[path = "gameplay_harness/configuration.rs"]
mod configuration;
#[path = "gameplay_harness/contracts.rs"]
mod contracts;
#[path = "gameplay_harness/focused_runner.rs"]
mod focused_runner;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/foundry_probe.rs"]
mod foundry_probe;
#[path = "gameplay_harness/foundry_setup.rs"]
mod foundry_setup;
#[path = "gameplay_harness/fresh_seed.rs"]
mod fresh_seed;
#[path = "gameplay_harness/industrial_support.rs"]
mod industrial_support;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/ore_probe.rs"]
mod ore_probe;
#[path = "gameplay_harness/ore_setup.rs"]
mod ore_setup;
#[path = "gameplay_harness/production_support.rs"]
mod production_support;
#[path = "gameplay_harness/progression_probe.rs"]
mod progression_probe;
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
#[path = "gameplay_harness/structural_fixture.rs"]
mod structural_fixture;
#[path = "gameplay_harness/support.rs"]
mod support;
#[path = "gameplay_harness/survival_probe.rs"]
mod survival_probe;

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

#[cfg(test)]
mod focused {
    use super::focused_runner::run_focused_probe;
    use super::foundry_probe::run_foundry_capability_probe;
    use super::ore_probe::run_ore_preparation_capability_probe;
    use super::progression_probe::run_primitive_progression_probe;
    use super::survival_probe::run_survival_provisioning_probe;

    #[test]
    fn gameplay_survival_provisioning_probe() {
        run_focused_probe("survival-provisioning", run_survival_provisioning_probe);
    }

    #[test]
    fn gameplay_primitive_progression_probe() {
        run_focused_probe("primitive-progression", run_primitive_progression_probe);
    }

    #[test]
    fn gameplay_ore_preparation_probe() {
        run_focused_probe("ore-preparation", run_ore_preparation_capability_probe);
    }

    #[test]
    fn gameplay_foundry_probe() {
        run_focused_probe("foundry", run_foundry_capability_probe);
    }
}

#[test]
#[ignore = "human-readable gameplay report"]
fn gameplay_report() {
    use deep_hearth::content::build_registries;

    use configuration::ScenarioPlanMode;
    use focused_runner::run_focused_probe_with_registries;
    use focused_seeds::MAINTAINED_VARIATION_ROOT;
    use fresh_seed::fresh_root;

    workshop::run_gameplay_harness(ScenarioPlanMode::Explore);
    agency::run_maintained_agency_counterfactuals();

    let registries = build_registries();
    let focused_variation_root = fresh_root(MAINTAINED_VARIATION_ROOT ^ 0x4652_4553_485F_464F);
    std::println!("FOCUSED REPORT INPUT variation_root=0x{focused_variation_root:016X}");
    run_focused_probe_with_registries(
        &registries,
        "survival-provisioning",
        survival_probe::run_survival_provisioning_probe,
        focused_variation_root,
    );
    run_focused_probe_with_registries(
        &registries,
        "primitive-progression",
        progression_probe::run_primitive_progression_probe,
        focused_variation_root,
    );
    run_focused_probe_with_registries(
        &registries,
        "ore-preparation",
        ore_probe::run_ore_preparation_capability_probe,
        focused_variation_root,
    );
    run_focused_probe_with_registries(
        &registries,
        "foundry",
        foundry_probe::run_foundry_capability_probe,
        focused_variation_root,
    );
}
