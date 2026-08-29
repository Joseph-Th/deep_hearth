//! Enforces gameplay-harness execution invariants and actor-policy coverage.
//!
//! Physical outcomes are observations unless an explicit regression contract fixes them. Hard
//! failures target canonical execution and stable required-input contracts.

use super::configuration::MaintainedAnchor;
use super::report::{
    EnergyRecoveryPreference, MaintenancePreference, PowerPreference, ScenarioReport,
    StructuralPreference,
};
use deep_hearth::maintenance::MaintenanceBand;

pub(super) fn assert_scenario_contracts(reports: &[ScenarioReport]) {
    for report in reports {
        if !report.progress.delivery_applied {
            assert!(
                report.progress.processed_mass == report.progress.target_mass
                    || report.structure.structural_stop
                    || report.limits.maintenance_stop
                    || report.limits.energy_stop,
                "gameplay world 0x{:016X} / behavior 0x{:016X} ended before its controlled event without completing or reaching a terminal gameplay constraint",
                report.world_seed,
                report.behavior_seed,
            );
            assert!(
                report.resources.elapsed_ticks <= report.inputs.delivery_at_tick,
                "gameplay world 0x{:016X} / behavior 0x{:016X} passed its scheduled controlled-event tick without applying the event",
                report.world_seed,
                report.behavior_seed,
            );
        }
    }
}

fn anchor_report(
    reports: &[(MaintainedAnchor, ScenarioReport)],
    anchor: MaintainedAnchor,
) -> &ScenarioReport {
    let mut matches = reports
        .iter()
        .filter(|(candidate, _)| *candidate == anchor)
        .map(|(_, report)| report);
    let report = matches.next().unwrap_or_else(|| {
        panic!(
            "maintained gameplay anchors are missing the {} scenario",
            anchor.label()
        )
    });
    assert!(
        matches.next().is_none(),
        "maintained gameplay anchors contain duplicate {} scenarios",
        anchor.label()
    );
    report
}

pub(super) fn assert_anchor_diversity(reports: &[(MaintainedAnchor, ScenarioReport)]) {
    for anchor in MaintainedAnchor::ALL {
        let _ = anchor_report(reports, anchor);
    }
    assert!(
        reports
            .iter()
            .any(|(_, report)| report.progress.delivery_applied),
        "maintained gameplay anchors must include at least one workshop episode that reaches the hidden controlled world-change event"
    );

    for (name, preference) in [
        ("reserve-conserving", PowerPreference::PreserveReserve),
        ("completion-time", PowerPreference::FinishSooner),
    ] {
        assert!(
            reports
                .iter()
                .any(|(_, report)| report.policy.power_preference == preference),
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
                .any(|(_, report)| report.policy.energy_recovery_preference == preference),
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
                .any(|(_, report)| report.policy.maintenance_preference == preference),
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
                .any(|(_, report)| report.policy.structural_preference == preference),
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
                .any(|(_, report)| report.inputs.initial_maintenance_band == band),
            "maintained gameplay anchors are missing the {band:?} initial maintenance band"
        );
    }

    let adaptive = &anchor_report(reports, MaintainedAnchor::AdaptiveEnergy).inputs;
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

    let condition_pressure = anchor_report(reports, MaintainedAnchor::ConditionPressure);
    assert_eq!(
        condition_pressure.inputs.initial_maintenance_band,
        MaintenanceBand::Warning,
        "maintained condition-pressure anchor must start just above the authored Critical boundary"
    );
    assert_eq!(
        condition_pressure.policy.maintenance_preference,
        MaintenancePreference::ServiceAtCritical,
        "condition-pressure anchor must allow condition-safe batch adaptation before preventive service"
    );
    assert!(
        condition_pressure
            .progress
            .condition_adaptive_batch_operations
            > 0,
        "maintained condition-pressure anchor must exercise condition-driven adaptive batching"
    );

    let recovery = &anchor_report(reports, MaintainedAnchor::ManualRecovery).inputs;
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

    let survival_pressure = &anchor_report(reports, MaintainedAnchor::SurvivalRecovery).inputs;
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
