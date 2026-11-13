//! Crusher maintenance decisions before powered workshop batches.
//!
//! One fixed rule replaces a player persona: never start a batch that would drive the crusher below
//! its authored critical floor. Service when work is actually blocked by remaining condition, and
//! stop when replacement stock or body reserves cannot fund the service. The reviewer's own play
//! decides whether to service earlier for convenience.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PreBatchTransition {
    Proceed,
    Retry,
    Stop,
}

pub(super) fn handle_pre_batch_maintenance(
    registries: &Registries,
    context: &mut BatchSelectionContext<'_>,
) -> PreBatchTransition {
    let current_condition = context
        .state
        .equipment()
        .get_equipment(context.ids.crusher)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("crusher disappeared during gameplay harness"));
    let band = context.thresholds.classify(current_condition);
    if band != MaintenanceBand::Normal && !context.report.limits.maintenance_warning {
        context.report.limits.maintenance_warning = true;
        println!(
            "  maintenance transition: condition={}ppm band={band:?}",
            current_condition.parts_per_million()
        );
    }
    if band != MaintenanceBand::Critical {
        return PreBatchTransition::Proceed;
    }

    println!("  decision: service crusher before more work because current condition is critical");
    match service_crusher(
        registries,
        &mut *context.state,
        context.ids,
        &mut context.report.maintenance,
    ) {
        MaintenanceAttempt::Serviced => PreBatchTransition::Retry,
        MaintenanceAttempt::SupplyExhausted => {
            context.report.limits.maintenance_stop = true;
            println!(
                "  decision: stop crushing; replacement stock is exhausted and the crusher remains critical"
            );
            PreBatchTransition::Stop
        }
        MaintenanceAttempt::LaborUnavailable => {
            context.report.limits.maintenance_stop = true;
            println!(
                "  decision: stop crushing; required maintenance labor exceeds current body reserves and the crusher remains critical"
            );
            PreBatchTransition::Stop
        }
    }
}

pub(super) fn handle_maintenance_blocked_plan(
    registries: &Registries,
    context: &mut BatchSelectionContext<'_>,
) -> PreBatchTransition {
    println!(
        "  decision: service crusher because no positive powered batch is legal within the remaining condition lifetime and maintenance safety margin"
    );
    match service_crusher(
        registries,
        &mut *context.state,
        context.ids,
        &mut context.report.maintenance,
    ) {
        MaintenanceAttempt::Serviced => PreBatchTransition::Retry,
        MaintenanceAttempt::SupplyExhausted => {
            context.report.limits.maintenance_stop = true;
            println!(
                "  decision: stop crushing; replacement stock is exhausted and even the smallest powered batch is outside the crusher's remaining safe working envelope"
            );
            PreBatchTransition::Stop
        }
        MaintenanceAttempt::LaborUnavailable => {
            context.report.limits.maintenance_stop = true;
            println!(
                "  decision: stop crushing; maintenance is required for any safe powered batch but current body reserves cannot sustain the service labor"
            );
            PreBatchTransition::Stop
        }
    }
}
