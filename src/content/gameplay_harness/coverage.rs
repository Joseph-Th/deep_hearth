//! Gameplay-harness scenario contract and qualitative coverage checks.

use super::{PowerPreference, ScenarioReport};

pub(super) fn coverage_gaps(reports: &[ScenarioReport]) -> Vec<&'static str> {
    let requirements = [
        (
            "structural consequence",
            reports
                .iter()
                .any(|report| report.structure.structural_consequence),
        ),
        (
            "stable structure",
            reports
                .iter()
                .any(|report| !report.structure.structural_consequence),
        ),
        (
            "persistent structural damage",
            reports
                .iter()
                .any(|report| report.structure.structural_damage_debt),
        ),
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
        (
            "announced-load planning changes siting",
            reports
                .iter()
                .any(|report| report.choices.briefing_changed_siting),
        ),
        (
            "structural stop",
            reports
                .iter()
                .any(|report| report.structure.structural_stop),
        ),
        (
            "production suspension",
            reports
                .iter()
                .any(|report| report.structure.production_suspension),
        ),
        (
            "stranded work in process",
            reports
                .iter()
                .any(|report| report.structure.stranded_work_in_process),
        ),
        (
            "recovered suspended work",
            reports.iter().any(|report| {
                report.structure.production_suspension && !report.structure.stranded_work_in_process
            }),
        ),
        (
            "production blocked by support failure",
            reports
                .iter()
                .any(|report| report.structure.support_failure_blocked_production),
        ),
        (
            "support relocation",
            reports
                .iter()
                .any(|report| report.structure.support_relocation),
        ),
        (
            "proactive relocation",
            reports.iter().any(|report| {
                report.structure.support_relocation
                    && !report.structure.support_failure_blocked_production
            }),
        ),
        (
            "load-event deadline changes power choice",
            reports
                .iter()
                .any(|report| report.choices.deadline_power_choice),
        ),
        (
            "energy bottleneck",
            reports.iter().any(|report| report.limits.energy_bottleneck),
        ),
        (
            "throughput bottleneck",
            reports
                .iter()
                .any(|report| report.limits.throughput_bottleneck),
        ),
        (
            "maintenance warning",
            reports
                .iter()
                .any(|report| report.limits.maintenance_warning),
        ),
        (
            "maintenance service",
            reports.iter().any(|report| report.maintenance.services > 0),
        ),
        (
            "scenario not requiring maintenance",
            reports
                .iter()
                .any(|report| report.maintenance.services == 0),
        ),
        (
            "maintenance service restores productive capacity",
            reports.iter().any(|report| {
                report.maintenance.services > 0
                    && report.progress.completed_batches == report.progress.target_batches
            }),
        ),
        (
            "completed work order",
            reports
                .iter()
                .any(|report| report.progress.completed_batches == report.progress.target_batches),
        ),
        (
            "incomplete work order",
            reports
                .iter()
                .any(|report| report.progress.completed_batches < report.progress.target_batches),
        ),
        (
            "mixed-ore processing frontier",
            reports
                .iter()
                .any(|report| report.progress.ore_frontier_visible),
        ),
    ];

    requirements
        .into_iter()
        .filter_map(|(name, observed)| (!observed).then_some(name))
        .collect()
}

pub(super) fn scenario_contract_gaps(reports: &[ScenarioReport]) -> Vec<String> {
    let mut gaps = Vec::new();
    for report in reports {
        if !report.progress.stimulus_applied {
            gaps.push(format!(
                "seed 0x{:016X}: never reached its announced external load stimulus",
                report.seed
            ));
        }
    }
    gaps
}
