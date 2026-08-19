//! Gameplay-harness execution contracts and intentionally small maintained-seed diversity checks.
//!
//! Physical outcomes such as support failure, relocation, maintenance pressure, and work-order
//! completion are observations, not frozen balance requirements. Hard failures stay focused on
//! canonical execution and stable maintained-input contracts.

use super::report::{MaintenancePreference, PowerPreference, ScenarioReport, StructuralPreference};
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
}
