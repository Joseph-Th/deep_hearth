//! Gameplay-harness execution contracts and intentionally small maintained-seed diversity checks.
//!
//! Physical outcomes such as support failure, relocation, maintenance pressure, and work-order
//! completion are observations, not frozen balance requirements. The harness reports those outcomes
//! so a cold agent can understand the current game while hard failures stay focused on broken
//! execution contracts and canonical invariants.

use super::report::{PowerPreference, ScenarioReport};

pub(super) fn anchor_diversity_gaps(reports: &[ScenarioReport]) -> Vec<&'static str> {
    let requirements = [
        (
            "reserve-conserving player priority",
            reports
                .iter()
                .any(|report| report.policy.power_preference == PowerPreference::PreserveReserve),
        ),
        (
            "condition-protecting player priority",
            reports
                .iter()
                .any(|report| report.policy.power_preference == PowerPreference::ProtectCondition),
        ),
        (
            "completion-time player priority",
            reports
                .iter()
                .any(|report| report.policy.power_preference == PowerPreference::FinishSooner),
        ),
    ];

    requirements
        .into_iter()
        .filter_map(|(name, observed)| (!observed).then_some(name))
        .collect()
}

pub(super) fn scenario_contract_gaps(reports: &[ScenarioReport]) -> Vec<String> {
    reports
        .iter()
        .filter(|report| !report.progress.delivery_applied)
        .map(|report| {
            format!(
                "seed 0x{:016X}: never executed its scheduled supported-stockpile delivery",
                report.seed
            )
        })
        .collect()
}
