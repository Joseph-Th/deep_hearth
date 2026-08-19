//! Gameplay-harness execution contracts and intentionally small maintained-seed diversity checks.
//!
//! Physical outcomes such as support failure, relocation, maintenance pressure, and work-order
//! completion are observations, not frozen balance requirements. Hard failures stay focused on
//! canonical execution and stable maintained-input contracts.

use super::report::{
    EnergyRecoveryPreference, MaintenancePreference, PowerPreference, ScenarioReport,
    StructuralPreference,
};
use deep_hearth::maintenance::MaintenanceBand;

pub(super) fn assert_scenario_contracts(reports: &[ScenarioReport]) {
    for report in reports {
        assert!(
            report.progress.delivery_applied,
            "gameplay world 0x{:016X} / behavior 0x{:016X} never executed its controlled supported-stockpile delivery",
            report.world_seed, report.behavior_seed,
        );
    }
}

pub(super) fn assert_anchor_diversity(reports: &[ScenarioReport]) {
    for (name, preference) in [
        ("reserve-conserving", PowerPreference::PreserveReserve),
        ("completion-time", PowerPreference::FinishSooner),
    ] {
        assert!(
            reports
                .iter()
                .any(|report| report.policy.power_preference == preference),
            "maintained gameplay anchors are missing the {name} player priority"
        );
    }
    for preference in [
        EnergyRecoveryPreference::ProtectSurvival,
        EnergyRecoveryPreference::SpendSurvivalReserve,
    ] {
        assert!(
            reports
                .iter()
                .any(|report| report.policy.energy_recovery_preference == preference),
            "maintained gameplay anchors are missing the {} manual-energy recovery policy",
            preference.label(),
        );
    }
    for preference in [
        MaintenancePreference::ServiceAtWarning,
        MaintenancePreference::ServiceAtCritical,
    ] {
        assert!(
            reports
                .iter()
                .any(|report| report.policy.maintenance_preference == preference),
            "maintained gameplay anchors are missing the {} maintenance policy",
            preference.label(),
        );
    }
    for preference in [
        StructuralPreference::PreserveMargin,
        StructuralPreference::MoveOnlyForFailure,
    ] {
        assert!(
            reports
                .iter()
                .any(|report| report.policy.structural_preference == preference),
            "maintained gameplay anchors are missing the {} structural policy",
            preference.label(),
        );
    }
    for band in [
        MaintenanceBand::Normal,
        MaintenanceBand::Warning,
        MaintenanceBand::Critical,
    ] {
        assert!(
            reports
                .iter()
                .any(|report| report.inputs.initial_maintenance_band == band),
            "maintained gameplay anchors are missing the {band:?} initial maintenance band"
        );
    }

    let adaptive_matches = reports
        .iter()
        .filter(|report| {
            report.inputs.small_drive_partial_batch_ppm > 0
                && report.inputs.large_drive_batch_budget == 0
                && report.inputs.large_drive_partial_batch_ppm == 0
        })
        .collect::<Vec<_>>();
    assert_eq!(
        adaptive_matches.len(),
        1,
        "maintained workshop anchors must contain exactly one fractional stored-work pressure case"
    );
    let adaptive = &adaptive_matches[0].inputs;
    let adaptive_order_batches = adaptive
        .order_mass
        .milligrams()
        .div_ceil(adaptive.nominal_batch_mass.milligrams());
    assert!(
        adaptive.small_drive_partial_batch_ppm > 0
            && adaptive.large_drive_batch_budget == 0
            && adaptive.large_drive_partial_batch_ppm == 0
            && u64::from(adaptive.small_drive_batch_budget) < adaptive_order_batches,
        "maintained adaptive-energy anchor must retain fractional, insufficient stored work"
    );

    let recovery_matches = reports
        .iter()
        .filter(|report| {
            report.inputs.small_drive_partial_batch_ppm == 0
                && report.inputs.large_drive_batch_budget == 0
                && report.inputs.large_drive_partial_batch_ppm == 0
        })
        .filter(|report| {
            let order_batches = report
                .inputs
                .order_mass
                .milligrams()
                .div_ceil(report.inputs.nominal_batch_mass.milligrams());
            u64::from(report.inputs.small_drive_batch_budget) < order_batches
        })
        .collect::<Vec<_>>();
    assert_eq!(
        recovery_matches.len(),
        1,
        "maintained workshop anchors must contain exactly one whole-batch stored-work shortfall"
    );
    let recovery = &recovery_matches[0].inputs;
    let recovery_order_batches = recovery
        .order_mass
        .milligrams()
        .div_ceil(recovery.nominal_batch_mass.milligrams());
    assert!(
        recovery.small_drive_partial_batch_ppm == 0
            && recovery.large_drive_batch_budget == 0
            && recovery.large_drive_partial_batch_ppm == 0
            && u64::from(recovery.small_drive_batch_budget) < recovery_order_batches,
        "maintained manual-recovery anchor must retain a whole-batch stored-work shortfall"
    );

    let survival_pressure_matches = reports
        .iter()
        .filter(|report| {
            report.inputs.small_drive_partial_batch_ppm > 0
                && report.inputs.large_drive_partial_batch_ppm > 0
                && report.inputs.maintenance_replacement_units == 0
        })
        .collect::<Vec<_>>();
    assert_eq!(
        survival_pressure_matches.len(),
        1,
        "maintained workshop anchors must contain exactly one finite-work survival-pressure case"
    );
    let survival_pressure = &survival_pressure_matches[0].inputs;
    let survival_order_batches = survival_pressure
        .order_mass
        .milligrams()
        .div_ceil(survival_pressure.nominal_batch_mass.milligrams());
    assert!(
        u64::from(survival_pressure.small_drive_batch_budget)
            + u64::from(survival_pressure.large_drive_batch_budget)
            < survival_order_batches,
        "maintained survival-recovery anchor must require direct human work beyond its initial stored-work reserve"
    );
}
