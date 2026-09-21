//! Demand-aware crusher maintenance decisions before powered workshop batches.

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
    defer_warning: bool,
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
    if band == MaintenanceBand::Warning
        && context.variation.policy.maintenance_preference
            == MaintenancePreference::ServiceAtWarning
        && !context.report.maintenance.supply_exhausted
        && !defer_warning
    {
        println!(
            "  decision: preventive service at warning; no cheap safe next-batch candidate under current policy"
        );
        match service_crusher(
            registries,
            &mut *context.state,
            context.ids,
            &mut context.report.maintenance,
        ) {
            MaintenanceAttempt::Serviced => return PreBatchTransition::Retry,
            MaintenanceAttempt::SupplyExhausted => {
                println!(
                    "  maintenance policy: preventive service is unavailable; continue legal work until condition or another constraint forces a stop"
                );
            }
            MaintenanceAttempt::LaborUnavailable => {
                println!(
                    "  maintenance policy: preventive service exceeds current body reserves; continue legal autonomous work until maintenance becomes mandatory or another constraint stops the episode"
                );
            }
        }
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

pub(super) enum WarningDemandPlan {
    EvaluateMaintenance,
    ExecuteBatch(Box<SelectedBatch>),
    RecoverEnergy,
}

/// Decide whether warning-band work can finish before the crusher reaches the critical floor.
///
/// The horizon deliberately spans repeated legal batches and assumes the currently selected class
/// of energy supply can be replenished. Current finite charge and per-batch capacity therefore do
/// not manufacture a maintenance need. If even the optimistic current-rate horizon is shorter than
/// the remaining order, preventive service is justified; otherwise execute one safe batch and
/// reassess from the resulting observable condition. An energy blocker remains a recharge decision,
/// not evidence for mechanical service.
pub(super) fn warning_demand_plan(
    registries: &Registries,
    context: &BatchSelectionContext<'_>,
) -> WarningDemandPlan {
    let condition = context
        .state
        .equipment()
        .get_equipment(context.ids.crusher)
        .unwrap_or_else(|| panic!("workshop crusher disappeared"))
        .condition();
    if context.variation.policy.maintenance_preference != MaintenancePreference::ServiceAtWarning
        || context.thresholds.classify(condition) != MaintenanceBand::Warning
    {
        return WarningDemandPlan::EvaluateMaintenance;
    }
    let remaining = context
        .report
        .progress
        .target_mass
        .checked_sub(context.report.progress.processed_mass)
        .unwrap_or_else(|| panic!("workshop processed mass exceeded its work order"));
    let condition_horizon = [context.ids.small_drive, context.ids.large_drive]
        .into_iter()
        .map(|store| {
            assess_powered_ore_mass_envelope(
                registries,
                context.state,
                PROCESS_CRUSH_ORE,
                context.ids.crusher,
                store,
            )
            .unwrap_or_else(|error| {
                panic!("workshop warning condition-horizon projection failed: {error}")
            })
            .cumulative_mass_preserving_condition_above_with_replenished_energy(
                context.thresholds.critical_below(),
            )
        })
        .max()
        .unwrap_or_else(|| unreachable!("workshop has two authored mechanical supplies"));
    if remaining > condition_horizon {
        println!(
            "  maintenance frame: crusher={} condition={}ppm remaining={}mg scope=remaining-order condition-horizon={}mg choice=preventive-service; even optimistic replenished current-rate work reaches the critical floor before order completion",
            context.ids.crusher.value(),
            condition.parts_per_million(),
            remaining.milligrams(),
            condition_horizon.milligrams(),
        );
        return WarningDemandPlan::EvaluateMaintenance;
    }

    let planned_mass = std::cmp::min(remaining, context.variation.ore.nominal_batch_mass);
    let plan = match largest_safe_powered_crush_batch(
        registries,
        context.state,
        context.ids,
        planned_mass,
        context.thresholds,
    ) {
        CrushBatchSearch::Available(plan) => plan,
        CrushBatchSearch::EnergyUnavailable => {
            println!(
                "  maintenance frame: crusher={} condition={}ppm remaining={}mg scope=next-batch planned={}mg blocker=EnergyUnavailable choice=recover-energy-before-warning-service; service does not replenish stored work",
                context.ids.crusher.value(),
                condition.parts_per_million(),
                remaining.milligrams(),
                planned_mass.milligrams(),
            );
            return WarningDemandPlan::RecoverEnergy;
        }
        CrushBatchSearch::MaintenanceBlocked => return WarningDemandPlan::EvaluateMaintenance,
    };
    let (option, reason, choice_basis) = choose_crush_option(
        plan.small,
        plan.large,
        CrushChoiceContext {
            thresholds: context.thresholds,
            preference: context.variation.policy.power_preference,
        },
    );
    let duration = option.resolved.process_resolution().duration();
    println!(
        "  maintenance frame: remaining={}mg scope=remaining-order condition-horizon={}mg next-batch=[planned:{}mg executable:{}mg duration:{}t] choice=defer-warning-order-fits-current-condition-horizon",
        remaining.milligrams(),
        condition_horizon.milligrams(),
        planned_mass.milligrams(),
        plan.mass.milligrams(),
        duration.value(),
    );
    WarningDemandPlan::ExecuteBatch(Box::new(SelectedBatch {
        mass: plan.mass,
        option,
        reason,
        choice_basis,
        adaptive: plan.mass < planned_mass,
        condition_adaptive: plan.equipment_capacity_limited
            || plan.condition_lifetime_limited
            || plan.maintenance_limited,
        energy_adaptive: plan.energy_limited,
    }))
}
