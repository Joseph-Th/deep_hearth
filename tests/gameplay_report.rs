//! Cross-system exploratory gameplay report. Scoped reports use focused owner surfaces.
#![cfg(not(test))]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

#[path = "gameplay_harness/agency.rs"]
mod agency;
#[path = "gameplay_harness/bulk_fieldwork_workload.rs"]
mod bulk_fieldwork_workload;
#[path = "gameplay_harness/capability_boundary.rs"]
mod capability_boundary;
#[path = "gameplay_harness/capital_investment_policy.rs"]
mod capital_investment_policy;
#[path = "gameplay_harness/catalog.rs"]
mod catalog;
#[path = "gameplay_harness/configuration.rs"]
mod configuration;
#[path = "gameplay_harness/contracts.rs"]
mod contracts;
#[path = "gameplay_harness/direct_consumption_timing.rs"]
mod direct_consumption_timing;
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/exact_local_runtime.rs"]
mod exact_local_runtime;
#[path = "gameplay_harness/fieldwork_probe.rs"]
mod fieldwork_probe;
#[path = "gameplay_harness/first_foundry_probe.rs"]
mod first_foundry_probe;
#[path = "gameplay_harness/focused_case.rs"]
mod focused_case;
#[path = "gameplay_harness/focused_runner.rs"]
mod focused_runner;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/focused_witnesses.rs"]
mod focused_witnesses;
#[path = "gameplay_harness/foundry_probe.rs"]
mod foundry_probe;
#[path = "gameplay_harness/foundry_setup.rs"]
mod foundry_setup;
#[path = "gameplay_harness/fresh_seed.rs"]
mod fresh_seed;
#[path = "gameplay_harness/industrial_support.rs"]
mod industrial_support;
#[path = "gameplay_harness/inherited_condition.rs"]
mod inherited_condition;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/maintenance_timing.rs"]
mod maintenance_timing;
#[path = "gameplay_harness/manual_assembly_planning.rs"]
mod manual_assembly_planning;
#[path = "gameplay_harness/manual_construction_package_planning.rs"]
mod manual_construction_package_planning;
#[path = "gameplay_harness/manual_construction_planning.rs"]
mod manual_construction_planning;
#[path = "gameplay_harness/manual_craft_batches.rs"]
mod manual_craft_batches;
#[path = "gameplay_harness/manual_craft_equipment_planning.rs"]
mod manual_craft_equipment_planning;
#[path = "gameplay_harness/manual_craft_execution.rs"]
mod manual_craft_execution;
#[path = "gameplay_harness/manual_craft_planning.rs"]
mod manual_craft_planning;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[path = "gameplay_harness/manual_craft_topology_planning.rs"]
mod manual_craft_topology_planning;
#[path = "gameplay_harness/manual_ore_recovery.rs"]
mod manual_ore_recovery;
#[path = "gameplay_harness/manual_ore_recovery_evaluation.rs"]
mod manual_ore_recovery_evaluation;
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
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[path = "gameplay_harness/power_provider_probe.rs"]
mod power_provider_probe;
#[path = "gameplay_harness/powered_craft_planning.rs"]
mod powered_craft_planning;
#[path = "gameplay_harness/preservation_route.rs"]
mod preservation_route;
#[path = "gameplay_harness/primitive_liberation.rs"]
mod primitive_liberation;
#[path = "gameplay_harness/primitive_workload.rs"]
mod primitive_workload;
#[path = "gameplay_harness/production_support.rs"]
mod production_support;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/progression_probe.rs"]
mod progression_probe;
#[path = "gameplay_harness/prospecting_timing.rs"]
mod prospecting_timing;
#[path = "gameplay_harness/report.rs"]
mod report;
#[path = "gameplay_harness/scenario.rs"]
mod scenario;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/settlement_drill_contract_tests.rs"]
mod settlement_drill_contract_tests;
#[path = "gameplay_harness/settlement_fixture.rs"]
mod settlement_fixture;
#[path = "gameplay_harness/settlement_generation.rs"]
mod settlement_generation;
#[path = "gameplay_harness/settlement_helve_contract_tests.rs"]
mod settlement_helve_contract_tests;
#[path = "gameplay_harness/settlement_power_planning.rs"]
mod settlement_power_planning;
#[path = "gameplay_harness/settlement_probe.rs"]
mod settlement_probe;
#[path = "gameplay_harness/settlement_wire_contract_tests.rs"]
mod settlement_wire_contract_tests;
#[path = "gameplay_harness/settlement_workshop_investment.rs"]
mod settlement_workshop_investment;
#[path = "gameplay_harness/structural_fixture.rs"]
mod structural_fixture;
#[path = "gameplay_harness/survival_probe.rs"]
mod survival_probe;
#[path = "gameplay_harness/temporal.rs"]
mod temporal;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/woodworking_policy.rs"]
mod woodworking_policy;
#[path = "gameplay_harness/woodworking_probe.rs"]
mod woodworking_probe;
#[path = "gameplay_harness/workshop.rs"]
mod workshop;
#[path = "gameplay_harness/workshop_craft_planning.rs"]
mod workshop_craft_planning;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;

fn main() {
    use deep_hearth::content::build_registries;

    use configuration::ScenarioPlanMode;
    use focused_runner::run_focused_probe_with_registries;
    use fresh_seed::fresh_root;
    use seed::MAINTAINED_VARIATION_ROOT;

    if let Some(argument) = std::env::args().nth(1) {
        eprintln!(
            "gameplay-report: unexpected argument {argument:?}; use `python ci.py report --scope <scope>` for scoped exploration"
        );
        std::process::exit(2);
    }

    let registries = build_registries();
    std::println!(
        "SIMULATION TIME physical-tick-us={}",
        registries.core().physical_tick_duration().microseconds()
    );
    let fallback_variation_root = fresh_root(MAINTAINED_VARIATION_ROOT ^ 0x4652_4553_485F_464F);
    let fallback_behavior_root = fresh_root(MAINTAINED_VARIATION_ROOT ^ 0x4652_4553_485F_4245);
    std::println!(
        "DESIGN TARGET scope=ordinary-after-disclosed-bootstrap locality=mixed-exact-local+spatial-proxy loop=observe->infer->prepare->extract->invest->delegate->reassess->reinvest-when-justified leverage=[knowledge,attention,scarce-copper,stored-work] lifecycle-obligations=[maintenance-when-needed,energy,survival] constraints=[matter,condition]"
    );
    std::println!(
        "EVALUATION SCOPE kind=ordinary-play evidence=runtime-actions-after-disclosed-bootstrap exact-local=[survival-provisioning,woodworking,power-provider,settlement,primitive-liberation,first-foundry] movement-abstracted=[primitive-progression,fieldwork] movement-authority=absent reachability-authority=STATUS.md"
    );

    run_focused_probe_with_registries(
        &registries,
        "settlement",
        settlement_probe::run_settlement_probe,
        true,
        fallback_variation_root,
        fallback_behavior_root,
    );
    settlement_drill_contract_tests::run_spindle_drill_investment_experience();
    settlement_wire_contract_tests::run_wire_drawbench_investment_experience();
    settlement_helve_contract_tests::run_helve_hammer_investment_experience();
    settlement_workshop_investment::run_lathe_investment_experience();
    settlement_workshop_investment::run_toolroom_investment_experience();
    run_focused_probe_with_registries(
        &registries,
        "foundry-bootstrap",
        first_foundry_probe::run_first_foundry_probe,
        true,
        fallback_variation_root,
        fallback_behavior_root,
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
        "woodworking",
        woodworking_probe::run_woodworking_probe,
        true,
        fallback_variation_root,
        fallback_behavior_root,
    );
    run_focused_probe_with_registries(
        &registries,
        "fieldwork",
        fieldwork_probe::run_fieldwork_probe,
        true,
        fallback_variation_root,
        fallback_behavior_root,
    );
    run_focused_probe_with_registries(
        &registries,
        "power-provider",
        power_provider_probe::run_power_provider_probe,
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
    run_focused_probe_with_registries(
        &registries,
        "primitive-liberation",
        primitive_liberation::run_primitive_liberation_probe,
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
