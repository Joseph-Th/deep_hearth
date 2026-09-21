//! Broad workshop contracts for the consolidated gameplay audit.

use deep_hearth::content::build_registries;
use deep_hearth::core::quantity::Mass;
use deep_hearth::maintenance::Condition;

use super::configuration::{
    MAINTAINED_BEHAVIOR_ROOT, MAINTAINED_VARIATION_ROOT, MaintainedAnchor, ScenarioPlanMode,
    scenario_seeds_from,
};
use super::{scenario, workshop};

fn condition(parts_per_million: u32) -> Condition {
    Condition::new(parts_per_million)
        .unwrap_or_else(|error| panic!("gameplay harness condition is invalid: {error}"))
}

#[test]
fn short_warning_order_defers_service_until_safe_completion() {
    use super::report::MaintenancePreference;
    use deep_hearth::content::EQUIPMENT_JAW_CRUSHER;
    use deep_hearth::maintenance::MaintenanceBand;

    let registries = build_registries();
    let definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .unwrap_or_else(|| panic!("authored crusher disappeared"));
    let mut variation = scenario::ScenarioVariation::from_seeds(
        &registries,
        4,
        1,
        Some(MaintainedAnchor::WarningMaintenance),
    );
    variation.policy.maintenance_preference = MaintenancePreference::ServiceAtWarning;
    let profile = definition
        .maintenance_profile()
        .unwrap_or_else(|| panic!("authored service disappeared"));
    let service_duration_value = profile
        .required_service_duration(variation.crusher.initial_crusher_condition)
        .value();
    assert_eq!(
        definition
            .maintenance_thresholds()
            .classify(variation.crusher.initial_crusher_condition),
        MaintenanceBand::Warning
    );

    // run_scenario uses canonical starts/ticks/service and audits total matter and trusted load.
    let report = workshop::runner::run_scenario(&registries, variation, None);
    std::println!(
        "WARNING ANCHOR order={}mg nominal={}mg processed={}mg batches={} services={} service_ticks={} elapsed={} final_condition={}ppm retained_stock={}mg deferrals={}",
        variation.ore.order_mass.milligrams(),
        variation.ore.nominal_batch_mass.milligrams(),
        report.progress.processed_mass.milligrams(),
        report.progress.operations_completed,
        report.maintenance.services,
        report.maintenance.service_ticks,
        report.resources.elapsed_ticks,
        report.resources.final_condition_ppm,
        report.resources.maintenance_stock_remaining.milligrams(),
        report.maintenance.warning_deferrals
    );
    assert_eq!(report.progress.processed_mass, variation.ore.order_mass);
    assert_eq!(report.progress.operations_completed, 6);
    assert_eq!(report.maintenance.services, 0);
    assert_eq!(report.maintenance.critical_services, 0);
    assert_eq!(report.maintenance.replacement_spent, Mass::ZERO);
    assert_eq!(report.maintenance.warning_deferrals, 6);
    assert_eq!(
        report.maintenance.service_ticks, 0,
        "deferral policy must not pay authored service time during the order"
    );
    let thresholds = definition.maintenance_thresholds();
    let critical = thresholds.critical_below().parts_per_million();
    let warning = thresholds.warning_below().parts_per_million();
    assert!(report.resources.final_condition_ppm >= critical);
    assert!(report.resources.final_condition_ppm < warning);
    assert!(report.resources.maintenance_stock_remaining.milligrams() > 0);
    assert!(report.resources.elapsed_ticks < service_duration_value);
}

#[test]
fn warning_service_prevents_condition_limited_batching_when_order_outlasts_safe_horizon() {
    use super::report::MaintenancePreference;
    use deep_hearth::maintenance::MaintenanceBand;

    let registries = build_registries();
    let mut warning = scenario::ScenarioVariation::from_seeds(
        &registries,
        29,
        1,
        Some(MaintainedAnchor::ConditionPressure),
    );
    warning.policy.maintenance_preference = MaintenancePreference::ServiceAtWarning;
    let warning_report = workshop::runner::run_scenario(&registries, warning, None);

    let mut critical_only = warning;
    critical_only.policy.maintenance_preference = MaintenancePreference::ServiceAtCritical;
    let critical_report = workshop::runner::run_scenario(&registries, critical_only, None);

    assert_eq!(
        warning_report.inputs.initial_maintenance_band,
        MaintenanceBand::Warning
    );
    assert_eq!(
        warning_report.progress.processed_mass,
        warning.ore.order_mass
    );
    assert_eq!(
        critical_report.progress.processed_mass,
        warning.ore.order_mass
    );
    assert_eq!(warning_report.maintenance.services, 1);
    assert_eq!(critical_report.maintenance.services, 1);
    assert_eq!(
        warning_report.progress.condition_adaptive_batch_operations, 0,
        "preventive warning service must avoid condition-limited batch shrinking"
    );
    assert!(
        critical_report.progress.condition_adaptive_batch_operations > 0,
        "critical-only policy must expose the near-critical condition pressure before service"
    );
    assert!(warning_report.maintenance.replacement_spent > Mass::ZERO);
}

fn warning_workshop_with_one_stored_batch(
    registries: &deep_hearth::registry::Registries,
) -> scenario::ScenarioVariation {
    use super::report::{EnergyRecoveryPreference, MaintenancePreference};

    let mut variation = scenario::ScenarioVariation::from_seeds(
        registries,
        4,
        1,
        Some(MaintainedAnchor::WarningMaintenance),
    );
    variation.policy.maintenance_preference = MaintenancePreference::ServiceAtWarning;
    variation.policy.energy_recovery_preference = EnergyRecoveryPreference::ProtectSurvival;
    variation.ore.order_mass = variation
        .ore
        .nominal_batch_mass
        .checked_add(variation.ore.nominal_batch_mass)
        .unwrap_or_else(|| panic!("depletion fixture order overflowed"));
    variation.crusher.small_drive_batch_budget = 1;
    variation.crusher.small_drive_partial_batch_ppm = 0;
    variation.crusher.large_drive_batch_budget = 0;
    variation.crusher.large_drive_partial_batch_ppm = 0;
    variation.crusher.maintenance_replacement_units = 1;
    // Keep the production delivery path without introducing a second blocking constraint.
    variation.delivery.mass = Mass::from_milligrams(1);
    variation
}

#[test]
fn depleted_warning_workshop_recharges_without_wasteful_service() {
    use deep_hearth::content::EQUIPMENT_JAW_CRUSHER;
    use deep_hearth::maintenance::MaintenanceBand;

    let registries = build_registries();
    let variation = warning_workshop_with_one_stored_batch(&registries);
    let report = workshop::runner::run_scenario(&registries, variation, None);
    let definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .unwrap_or_else(|| panic!("authored crusher disappeared"));

    assert_eq!(
        report.inputs.initial_maintenance_band,
        MaintenanceBand::Warning
    );
    assert_eq!(report.progress.processed_mass, variation.ore.order_mass);
    assert!(report.choices.manual_recharges > 0);
    assert!(report.resources.manually_generated_energy.nanojoules() > 0);
    assert_eq!(report.maintenance.services, 0);
    assert_eq!(report.maintenance.service_ticks, 0);
    assert_eq!(report.maintenance.replacement_spent, Mass::ZERO);
    assert_eq!(
        report.resources.maintenance_stock_remaining,
        definition
            .maintenance_profile()
            .unwrap_or_else(|| panic!("authored crusher profile disappeared"))
            .full_service_replacement_mass()
    );
    assert_eq!(
        definition
            .maintenance_thresholds()
            .classify(condition(report.resources.final_condition_ppm)),
        MaintenanceBand::Warning,
        "recharged work must still preserve the critical-condition floor"
    );
    assert!(!report.limits.energy_stop);
    assert!(!report.limits.maintenance_stop);
}

#[test]
fn depleted_warning_workshop_preserves_stock_when_recharge_is_declined() {
    let registries = build_registries();
    let mut variation = warning_workshop_with_one_stored_batch(&registries);
    variation.survival.start_at_hydration_warning_boundary = true;
    let report = workshop::runner::run_scenario(&registries, variation, None);

    assert_eq!(
        report.progress.processed_mass,
        variation.ore.nominal_batch_mass
    );
    assert!(report.limits.manual_recovery_declined);
    assert!(report.limits.energy_stop);
    assert!(!report.limits.maintenance_stop);
    assert_eq!(report.choices.manual_recharges, 0);
    assert_eq!(report.resources.manually_generated_energy.nanojoules(), 0);
    assert_eq!(report.maintenance.services, 0);
    assert_eq!(report.maintenance.service_ticks, 0);
    assert_eq!(report.maintenance.replacement_spent, Mass::ZERO);
    assert!(!report.resources.maintenance_stock_remaining.is_zero());
}

#[test]
fn organic_warning_energy_shortfalls_do_not_manufacture_service_costs() {
    use super::report::{
        EnergyRecoveryPreference, MaintenancePreference, PowerPreference, ScenarioPolicyVariation,
        StructuralPreference,
    };
    use deep_hearth::maintenance::MaintenanceBand;

    let registries = build_registries();
    // Fixed discoveries from the 0xE1B76C1750B8BA0A / 0x57A9B15C0A1F2C8F report.
    for world_seed in [0x2426_1F4A_8649_4594, 0x018E_36B3_6C91_CFFF] {
        let mut variation =
            scenario::ScenarioVariation::from_seeds(&registries, world_seed, 1, None);
        variation.policy = ScenarioPolicyVariation {
            power_preference: PowerPreference::PreserveReserve,
            energy_recovery_preference: EnergyRecoveryPreference::ProtectSurvival,
            maintenance_preference: MaintenancePreference::ServiceAtWarning,
            structural_preference: StructuralPreference::PreserveMargin,
        };
        let warning = workshop::runner::run_scenario(&registries, variation, None);
        variation.policy.maintenance_preference = MaintenancePreference::ServiceAtCritical;
        let critical_only = workshop::runner::run_scenario(&registries, variation, None);

        std::println!(
            "WARNING ENERGY REPLAY world=0x{world_seed:016X} warning=[ore:{}mg elapsed:{}t services:{} recharges:{}] critical-only=[ore:{}mg elapsed:{}t services:{} recharges:{}]",
            warning.progress.processed_mass.milligrams(),
            warning.resources.episode_end_tick,
            warning.maintenance.services,
            warning.choices.manual_recharges,
            critical_only.progress.processed_mass.milligrams(),
            critical_only.resources.episode_end_tick,
            critical_only.maintenance.services,
            critical_only.choices.manual_recharges,
        );
        assert_eq!(
            warning.inputs.initial_maintenance_band,
            MaintenanceBand::Warning
        );
        assert_eq!(warning.progress.processed_mass, variation.ore.order_mass);
        assert!(warning.choices.manual_recharges > 0);
        assert_eq!(warning.maintenance.services, 0);
        assert_eq!(warning.maintenance.replacement_spent, Mass::ZERO);
        assert_eq!(
            warning.progress.processed_mass,
            critical_only.progress.processed_mass
        );
        assert_eq!(
            warning.resources.episode_end_tick,
            critical_only.resources.episode_end_tick
        );
        assert_eq!(
            warning.resources.final_condition_ppm,
            critical_only.resources.final_condition_ppm
        );
        assert_eq!(
            warning.resources.maintenance_stock_remaining,
            critical_only.resources.maintenance_stock_remaining
        );
    }
}

#[test]
fn depleted_workshop_still_pays_mandatory_critical_service() {
    use deep_hearth::content::EQUIPMENT_JAW_CRUSHER;

    let registries = build_registries();
    let mut variation = warning_workshop_with_one_stored_batch(&registries);
    let thresholds = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .unwrap_or_else(|| panic!("authored crusher disappeared"))
        .maintenance_thresholds();
    variation.crusher.initial_crusher_condition =
        condition(thresholds.critical_below().parts_per_million() / 2);
    variation.survival.start_at_hydration_warning_boundary = true;
    let report = workshop::runner::run_scenario(&registries, variation, None);

    assert_eq!(report.maintenance.services, 1);
    assert_eq!(report.maintenance.critical_services, 1);
    assert!(report.maintenance.service_ticks > 0);
    assert!(report.maintenance.replacement_spent > Mass::ZERO);
    assert_eq!(
        report.progress.processed_mass,
        variation.ore.nominal_batch_mass
    );
    assert!(report.limits.energy_stop);
    assert!(report.limits.manual_recovery_declined);
    assert!(!report.limits.maintenance_stop);
    assert!(
        report.resources.final_condition_ppm >= thresholds.critical_below().parts_per_million()
    );
}

#[test]
fn gameplay_terminal_prework_stop_does_not_plan_unreachable_work_or_wait_for_hidden_event() {
    let registries = build_registries();
    let mut variation = scenario::ScenarioVariation::from_seeds(&registries, 4, 1, None);
    variation.crusher.initial_crusher_condition = condition(1);
    variation.crusher.maintenance_replacement_units = 0;
    variation.delivery.delivery_at_tick = 64;

    let report = workshop::runner::run_scenario(&registries, variation, None);

    assert!(report.limits.maintenance_stop);
    assert!(!report.progress.delivery_applied);
    assert_eq!(report.progress.operations_completed, 0);
    assert_eq!(report.resources.elapsed_ticks, 0);
    assert!(report.resources.elapsed_ticks < report.inputs.delivery_at_tick);
    assert_eq!(report.resources.metabolic_energy_spent.nanojoules(), 0);
    assert_eq!(report.resources.hydration_spent.microliters(), 0);
}

#[test]
fn critical_service_is_affordable_from_warning_hydration_reserves() {
    let registries = build_registries();
    let mut variation = scenario::ScenarioVariation::from_seeds(&registries, 4, 1, None);
    variation.crusher.initial_crusher_condition = condition(41_036);
    variation.crusher.maintenance_replacement_units = 2;
    variation.survival.start_at_hydration_warning_boundary = true;
    variation.delivery.delivery_at_tick = 64;

    let report = workshop::runner::run_scenario(&registries, variation, None);

    assert!(!report.limits.maintenance_stop);
    assert!(!report.maintenance.labor_unavailable);
    assert!(!report.maintenance.supply_exhausted);
    assert_eq!(report.maintenance.services, 1);
    assert_eq!(report.maintenance.critical_services, 1);
    assert!(report.maintenance.replacement_spent > Mass::ZERO);
    assert!(!report.resources.maintenance_stock_remaining.is_zero());
    assert_eq!(report.progress.processed_mass, report.progress.target_mass);
}

#[test]
fn initial_service_rebases_hidden_event_timing_after_elapsed_work() {
    let registries = build_registries();
    let variation = scenario::ScenarioVariation::from_seeds(
        &registries,
        9,
        0x88BD_D3FE_783B_B94D,
        Some(super::configuration::MaintainedAnchor::CriticalMaintenance),
    );

    let report = workshop::runner::run_scenario(&registries, variation, None);

    assert!(report.maintenance.service_ticks > 0);
    assert!(
        report.inputs.delivery_at_tick > report.maintenance.service_ticks,
        "controlled delivery must be scheduled after initial service has advanced authoritative time"
    );
}

#[test]
fn hidden_delivery_payload_does_not_change_pre_event_actor_choices() {
    let registries = build_registries();
    let plan = scenario_seeds_from(
        ScenarioPlanMode::Gate,
        None,
        None,
        None,
        MAINTAINED_VARIATION_ROOT,
        MAINTAINED_BEHAVIOR_ROOT,
    )
    .unwrap_or_else(|error| panic!("maintained hidden-delivery seed plan failed: {error:?}"));
    let case = plan
        .cases()
        .iter()
        .find(|case| case.anchor == Some(MaintainedAnchor::ManualRecovery))
        .copied()
        .unwrap_or_else(|| panic!("maintained world-disruption workshop case disappeared"));
    let baseline = scenario::ScenarioVariation::from_seeds(
        &registries,
        case.world_seed,
        case.behavior_seed,
        case.anchor,
    );
    let baseline_report = workshop::runner::run_scenario(&registries, baseline, None);
    assert!(
        baseline_report.progress.delivery_applied,
        "maintained world-disruption fixture must reach the controlled delivery"
    );
    let mut alternate = baseline;
    alternate.delivery.destination_is_compact = !baseline.delivery.destination_is_compact;
    alternate.delivery.mass = Mass::from_milligrams(
        baseline
            .delivery
            .mass
            .milligrams()
            .checked_add(1)
            .unwrap_or_else(|| panic!("hidden-delivery counterfactual mass overflowed")),
    );

    let alternate_report = workshop::runner::run_scenario(&registries, alternate, None);

    assert_ne!(
        baseline_report.inputs.delivery_mass,
        alternate_report.inputs.delivery_mass
    );
    assert_ne!(
        baseline_report.inputs.delivery_is_compact,
        alternate_report.inputs.delivery_is_compact
    );
    assert!(baseline_report.progress.delivery_applied);
    assert!(alternate_report.progress.delivery_applied);
    assert_eq!(
        baseline_report.inputs.delivery_at_tick, alternate_report.inputs.delivery_at_tick,
        "hidden delivery payload must not alter controller event timing"
    );
    assert_eq!(
        baseline_report.progress.operations_before_delivery,
        alternate_report.progress.operations_before_delivery,
        "actor must make the same number of pre-event workshop decisions when only hidden delivery payload changes"
    );
    assert_eq!(
        baseline_report.choices.chose_compact_support,
        alternate_report.choices.chose_compact_support,
        "initial support choice must depend only on observable structural state"
    );
}
