//! Broad gameplay verification and exploratory report target.
//!
//! Focused developer gates use smaller scope-specific binaries. This aggregate target remains the
//! checkpoint for cross-harness contracts, agency checks, seed contracts, and exploratory reporting.

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
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[cfg(test)]
#[path = "gameplay_harness/fixture_boundary_tests.rs"]
mod fixture_boundary_tests;
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
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/material_selection.rs"]
mod material_selection;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/ore_probe.rs"]
mod ore_probe;
#[path = "gameplay_harness/ore_setup.rs"]
mod ore_setup;
#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;
#[path = "gameplay_harness/preservation_route.rs"]
mod preservation_route;
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
#[path = "gameplay_harness/survival_probe.rs"]
mod survival_probe;

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
    use fresh_seed::fresh_root;
    use seed::MAINTAINED_VARIATION_ROOT;

    workshop::run_gameplay_harness(ScenarioPlanMode::Explore);
    agency::run_exploratory_agency_counterfactuals();

    let registries = build_registries();
    let focused_variation_root = fresh_root(MAINTAINED_VARIATION_ROOT ^ 0x4652_4553_485F_464F);
    let focused_behavior_root = fresh_root(MAINTAINED_VARIATION_ROOT ^ 0x4652_4553_485F_4245);
    std::println!(
        "FOCUSED REPORT INPUT variation_root=0x{focused_variation_root:016X} actor_behavior_root=0x{focused_behavior_root:016X}"
    );
    run_focused_probe_with_registries(
        &registries,
        "survival-provisioning",
        survival_probe::run_survival_provisioning_probe,
        true,
        focused_variation_root,
        focused_behavior_root,
    );
    run_focused_probe_with_registries(
        &registries,
        "primitive-progression",
        progression_probe::run_primitive_progression_probe,
        true,
        focused_variation_root,
        focused_behavior_root,
    );
    run_focused_probe_with_registries(
        &registries,
        "ore-preparation",
        ore_probe::run_ore_preparation_capability_probe,
        true,
        focused_variation_root,
        focused_behavior_root,
    );
    run_focused_probe_with_registries(
        &registries,
        "foundry",
        foundry_probe::run_foundry_capability_probe,
        true,
        focused_variation_root,
        focused_behavior_root,
    );
}
