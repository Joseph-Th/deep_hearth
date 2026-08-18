//! Gameplay-harness execution contracts and intentionally small maintained-seed diversity checks.
//!
//! Physical outcomes such as support failure, relocation, maintenance pressure, and work-order
//! completion are observations, not frozen balance requirements. Hard failures stay focused on
//! canonical execution and stable maintained-input contracts.

use super::report::{PowerPreference, ScenarioReport};

pub(super) fn assert_scenario_contracts(reports: &[ScenarioReport]) {
    for report in reports {
        assert!(
            report.progress.delivery_applied,
            "gameplay seed 0x{:016X} never executed its scheduled supported-stockpile delivery",
            report.seed
        );
    }
}

pub(super) fn assert_anchor_diversity(reports: &[ScenarioReport]) {
    for (name, preference) in [
        ("reserve-conserving", PowerPreference::PreserveReserve),
        ("condition-protecting", PowerPreference::ProtectCondition),
        ("completion-time", PowerPreference::FinishSooner),
    ] {
        assert!(
            reports
                .iter()
                .any(|report| report.policy.power_preference == preference),
            "maintained gameplay anchors are missing the {name} player priority"
        );
    }
}
