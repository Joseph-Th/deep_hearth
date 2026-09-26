//! Explicit exploratory gameplay report. Routine verification uses the focused test binaries.
#![cfg(not(test))]

use std::process::ExitCode;

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

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
#[path = "gameplay_harness/direct_consumption_timing.rs"]
mod direct_consumption_timing;
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/fieldwork_probe.rs"]
mod fieldwork_probe;
#[path = "gameplay_harness/first_foundry_probe.rs"]
mod first_foundry_probe;
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
#[path = "gameplay_harness/manual_craft_execution.rs"]
mod manual_craft_execution;
#[path = "gameplay_harness/manual_craft_planning.rs"]
mod manual_craft_planning;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[path = "gameplay_harness/manual_ore_recovery.rs"]
mod manual_ore_recovery;
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
#[path = "gameplay_harness/progression_scope.rs"]
mod progression_scope;
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
#[path = "gameplay_harness/structural_fixture.rs"]
mod structural_fixture;
#[path = "gameplay_harness/survival_probe.rs"]
mod survival_probe;
#[path = "gameplay_harness/temporal.rs"]
mod temporal;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/woodworking_probe.rs"]
mod woodworking_probe;
#[path = "gameplay_harness/workshop.rs"]
mod workshop;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReportScope {
    All,
    Workshop,
    Survival,
    Progression,
    Woodworking,
    Fieldwork,
    PowerProvider,
    Agency,
    Ore,
    Foundry,
}

impl ReportScope {
    fn from_args() -> Result<Self, String> {
        let mut arguments = std::env::args().skip(1);
        let scope = match arguments.next().as_deref() {
            None | Some("all") => Self::All,
            Some("workshop") => Self::Workshop,
            Some("survival") => Self::Survival,
            Some("progression") => Self::Progression,
            Some("woodworking") => Self::Woodworking,
            Some("fieldwork") => Self::Fieldwork,
            Some("power-provider") => Self::PowerProvider,
            Some("agency") => Self::Agency,
            Some("ore") => Self::Ore,
            Some("foundry") => Self::Foundry,
            Some(scope) => {
                return Err(format!(
                    "unknown scope {scope:?}; expected all, workshop, survival, progression, woodworking, fieldwork, power-provider, agency, ore, or foundry"
                ));
            }
        };
        if let Some(argument) = arguments.next() {
            return Err(format!("unexpected extra argument {argument:?}"));
        }
        Ok(scope)
    }

    fn includes(self, scope: Self) -> bool {
        self == Self::All || self == scope
    }
}

fn main() -> ExitCode {
    use deep_hearth::content::build_registries;

    use configuration::ScenarioPlanMode;
    use focused_runner::run_focused_probe_with_registries;
    use fresh_seed::fresh_root;
    use seed::MAINTAINED_VARIATION_ROOT;

    let scope = match ReportScope::from_args() {
        Ok(scope) => scope,
        Err(error) => {
            eprintln!("gameplay-report: {error}");
            return ExitCode::from(2);
        }
    };
    let registries = build_registries();
    std::println!(
        "SIMULATION TIME physical-tick-us={}",
        registries.core().physical_tick_duration().microseconds()
    );
    let fallback_variation_root = fresh_root(MAINTAINED_VARIATION_ROOT ^ 0x4652_4553_485F_464F);
    let fallback_behavior_root = fresh_root(MAINTAINED_VARIATION_ROOT ^ 0x4652_4553_485F_4245);
    if scope == ReportScope::All {
        std::println!(
            "PLAYER FANTASY scope=current-ordinary loop=observe->infer->prepare->extract->invest->delegate->reassess->reinvest-when-justified leverage=[knowledge,attention,scarce-copper,stored-work] lifecycle-obligations=[maintenance-when-needed,energy,survival] constraints=[matter,condition]"
        );
        std::println!(
            "EVALUATION SCOPE kind=ordinary-play evidence=runtime-actions-after-disclosed-bootstrap exact-local=[survival-provisioning,woodworking,power-provider,primitive-liberation-maintained-anchor,first-foundry] movement-abstracted=[primitive-progression,fieldwork,primitive-liberation-preassembled-variation] movement-authority=absent reachability-authority=STATUS.md"
        );
    }
    if scope.includes(ReportScope::Survival) {
        run_focused_probe_with_registries(
            &registries,
            "survival-provisioning",
            survival_probe::run_survival_provisioning_probe,
            true,
            fallback_variation_root,
            fallback_behavior_root,
        );
    }
    if scope.includes(ReportScope::Woodworking) {
        run_focused_probe_with_registries(
            &registries,
            "woodworking",
            woodworking_probe::run_woodworking_probe,
            true,
            fallback_variation_root,
            fallback_behavior_root,
        );
    }
    if scope.includes(ReportScope::Fieldwork) {
        run_focused_probe_with_registries(
            &registries,
            "fieldwork",
            fieldwork_probe::run_fieldwork_probe,
            true,
            fallback_variation_root,
            fallback_behavior_root,
        );
    }
    if scope.includes(ReportScope::PowerProvider) {
        run_focused_probe_with_registries(
            &registries,
            "power-provider",
            power_provider_probe::run_power_provider_probe,
            true,
            fallback_variation_root,
            fallback_behavior_root,
        );
    }
    if scope.includes(ReportScope::Progression) {
        run_focused_probe_with_registries(
            &registries,
            "primitive-progression",
            progression_scope::run_primitive_progression_scope,
            true,
            fallback_variation_root,
            fallback_behavior_root,
        );
    }
    if scope == ReportScope::All {
        std::println!(
            "EVALUATION SCOPE kind=controlled-capability evidence=isolated-system-behavior probes=[industrial-workshop,agency,ore-preparation,foundry] ordinary-reachability=false reachability-authority=STATUS.md"
        );
    }
    if scope.includes(ReportScope::Workshop) {
        workshop::run_gameplay_harness(ScenarioPlanMode::Explore);
    }
    #[cfg(not(test))]
    if scope.includes(ReportScope::Agency) {
        agency::run_exploratory_agency_counterfactuals();
    }
    if scope.includes(ReportScope::Ore) {
        run_focused_probe_with_registries(
            &registries,
            "ore-preparation",
            ore_probe::run_ore_preparation_capability_probe,
            true,
            fallback_variation_root,
            fallback_behavior_root,
        );
    }
    if scope.includes(ReportScope::Foundry) {
        run_focused_probe_with_registries(
            &registries,
            "foundry",
            foundry_probe::run_foundry_capability_probe,
            true,
            fallback_variation_root,
            fallback_behavior_root,
        );
    }
    ExitCode::SUCCESS
}
