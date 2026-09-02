//! Explicit exploratory gameplay report. Routine verification uses the focused test binaries.

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
#[path = "gameplay_harness/ore_probe.rs"]
mod ore_probe;
#[path = "gameplay_harness/ore_setup.rs"]
mod ore_setup;
#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[path = "gameplay_harness/preservation_route.rs"]
mod preservation_route;
#[path = "gameplay_harness/production_support.rs"]
mod production_support;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/progression_probe.rs"]
mod progression_probe;
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
#[path = "gameplay_harness/survival_probe.rs"]
mod survival_probe;
#[path = "gameplay_harness/temporal.rs"]
mod temporal;
#[path = "gameplay_harness/workshop.rs"]
mod workshop;

fn main() {
    use deep_hearth::content::build_registries;

    use configuration::ScenarioPlanMode;
    use focused_runner::run_focused_probe_with_registries;
    use fresh_seed::fresh_root;
    use seed::MAINTAINED_VARIATION_ROOT;

    let registries = build_registries();
    let fallback_variation_root = fresh_root(MAINTAINED_VARIATION_ROOT ^ 0x4652_4553_485F_464F);
    let fallback_behavior_root = fresh_root(MAINTAINED_VARIATION_ROOT ^ 0x4652_4553_485F_4245);
    std::println!(
        "PLAYER FANTASY scope=current-ordinary loop=observe->infer->prepare->extract->invest->delegate->maintain->reassess->reinvest-when-justified leverage=[knowledge,attention,scarce-copper,stored-work] constraints=[matter,energy,condition,survival]"
    );
    std::println!(
        "EVALUATION SCOPE kind=ordinary-play evidence=runtime-actions-after-disclosed-bootstrap probes=[survival-provisioning,primitive-progression] reachability-authority=STATUS.md"
    );
    run_focused_probe_with_registries(
        &registries,
        "survival-provisioning",
        survival_probe::run_survival_provisioning_probe,
        true,
        fallback_variation_root,
        fallback_behavior_root,
    );
    run_focused_probe_with_registries(
        &registries,
        "primitive-progression",
        progression_probe::run_primitive_progression_probe,
        true,
        fallback_variation_root,
        fallback_behavior_root,
    );
    std::println!(
        "EVALUATION SCOPE kind=controlled-capability evidence=isolated-system-behavior probes=[industrial-workshop,agency,ore-preparation,foundry] ordinary-reachability=false reachability-authority=STATUS.md"
    );
    workshop::run_gameplay_harness(ScenarioPlanMode::Explore);
    agency::run_exploratory_agency_counterfactuals();
    run_focused_probe_with_registries(
        &registries,
        "ore-preparation",
        ore_probe::run_ore_preparation_capability_probe,
        true,
        fallback_variation_root,
        fallback_behavior_root,
    );
    run_focused_probe_with_registries(
        &registries,
        "foundry",
        foundry_probe::run_foundry_capability_probe,
        true,
        fallback_variation_root,
        fallback_behavior_root,
    );
}
