//! Fail-closed checks for observable canonical tick outcomes.

use deep_hearth::mining::MiningJobId;
use deep_hearth::production::ProductionJobId;
use deep_hearth::simulation::TickOutcome;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TickEventAllowance<'a> {
    pub(super) production_jobs: &'a [ProductionJobId],
    pub(super) production_availability_changes: bool,
    pub(super) mining_jobs: &'a [MiningJobId],
    pub(super) manual_power: bool,
    pub(super) equipment_maintenance: bool,
    pub(super) storage_enclosure_dismantling: bool,
    pub(super) field_prospecting: bool,
}

/// Rejects every observable work event except those a stepper explicitly expects.
///
/// Keep the complete current TickOutcome work surface here so simulation changes have one harness
/// observation boundary to audit instead of many independent field lists.
pub(super) fn assert_tick_events_within(
    outcome: &TickOutcome,
    allowance: TickEventAllowance<'_>,
    context: &str,
) {
    assert_production_events_within(outcome, allowance, context);
    assert_player_work_events_within(outcome, allowance, context);
}

fn assert_production_events_within(
    outcome: &TickOutcome,
    allowance: TickEventAllowance<'_>,
    context: &str,
) {
    if allowance.production_jobs.is_empty() {
        assert!(
            outcome.production_availability_changes().is_empty(),
            "gameplay harness {context} crossed a production availability change"
        );
        assert!(
            outcome.production_completions().is_empty(),
            "gameplay harness {context} crossed a production completion"
        );
        return;
    }

    if allowance.production_availability_changes {
        assert!(
            outcome
                .production_availability_changes()
                .iter()
                .all(|change| allowance.production_jobs.contains(&change.job())),
            "gameplay harness {context} crossed an unrelated production availability change"
        );
    } else {
        assert!(
            outcome.production_availability_changes().is_empty(),
            "gameplay harness {context} crossed a production availability change"
        );
    }
    assert!(
        outcome
            .production_completions()
            .iter()
            .all(|completion| allowance.production_jobs.contains(&completion.job())),
        "gameplay harness {context} crossed an unrelated production completion"
    );
}

fn assert_player_work_events_within(
    outcome: &TickOutcome,
    allowance: TickEventAllowance<'_>,
    context: &str,
) {
    assert!(
        outcome
            .ready_mining_jobs()
            .iter()
            .all(|job| allowance.mining_jobs.contains(job)),
        "gameplay harness {context} crossed an unrelated mining completion"
    );
    assert!(
        allowance.manual_power || outcome.manual_power().is_none(),
        "gameplay harness {context} crossed unrelated manual-power work"
    );
    assert!(
        allowance.equipment_maintenance || outcome.equipment_maintenance().is_none(),
        "gameplay harness {context} crossed unrelated equipment-maintenance work"
    );
    assert!(
        allowance.storage_enclosure_dismantling
            || outcome.storage_enclosure_dismantling().is_none(),
        "gameplay harness {context} crossed unrelated storage-dismantling work"
    );
    assert!(
        allowance.field_prospecting || outcome.field_prospecting().is_none(),
        "gameplay harness {context} crossed unrelated field-prospecting work"
    );
}
