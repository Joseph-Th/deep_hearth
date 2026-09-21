//! Manual-energy recovery planning and execution for the workshop actor.

use super::*;
use deep_hearth::labor::PlayerWorkStartError;

struct ManualRecoveryProbe {
    option: Option<ManualRecoveryOption>,
    survival_limited: bool,
    policy_declined: bool,
    equipment_limited: bool,
    storage_limited: bool,
}

pub(super) enum ManualRecoverySearch {
    Available {
        mass: Mass,
        option: Box<ManualRecoveryOption>,
        adaptive_constraint: Option<ManualRecoveryConstraint>,
    },
    DeclinedForSurvival,
    SurvivalLimited,
    EquipmentLimited,
    StorageLimited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ManualRecoveryConstraint {
    SurvivalPolicy,
    SurvivalReserve,
    EquipmentCondition,
    StorageCapacity,
}

pub(super) struct ManualRecoveryOption {
    name: &'static str,
    store: EnergyStoreId,
    energy: Energy,
    start: ValidatedManualPowerStart,
}

fn manual_recovery_option(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    mass: Mass,
    name: &'static str,
    store: EnergyStoreId,
    envelope: PoweredOreMassEnvelope,
) -> Result<Option<ManualRecoveryOption>, ManualPowerError> {
    let Some(energy) = envelope.additional_energy_required_for(mass) else {
        return Ok(None);
    };
    if energy.is_zero() {
        return Ok(None);
    }
    validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, ids.hand_crank, store, energy),
    )
    .map(|start| {
        Some(ManualRecoveryOption {
            name,
            store,
            energy,
            start,
        })
    })
}

pub(super) fn execute_manual_recovery(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    option: ManualRecoveryOption,
    controller: &mut ControlledDeliveryRuntime<'_>,
    actor: &mut ScenarioActorRuntime<'_>,
) {
    let budget = option.start.resource_budget();
    let work = option.start.work();
    let started_at = work.started_at().value();
    let completes_at = work.completes_at().value();
    let duration = work
        .completes_at()
        .checked_duration_since(work.started_at())
        .unwrap_or_else(|| panic!("manual-recovery completion precedes its start"))
        .value();
    let stored_before = state
        .energy()
        .get_store(option.store)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("manual-recovery {} drive disappeared", option.name));
    let survival_before = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("workshop survival state disappeared before manual recovery"));
    println!(
        "  manual recovery: crank {}nJ into {} drive over {}t; projected body cost={}nJ/{}uL, reserves={}nJ/{}uL",
        option.energy.nanojoules(),
        option.name,
        duration,
        budget.metabolic_energy().nanojoules(),
        budget.hydration().microliters(),
        survival_before.metabolic_energy().nanojoules(),
        survival_before.hydration().microliters(),
    );
    option
        .start
        .commit(state)
        .unwrap_or_else(|error| panic!("manual-recovery start commit failed: {error}"));

    let event_tick = controller.delivery.delivery_at_tick;
    let mut event_assessment = None;
    if !actor.report.progress.delivery_applied
        && event_tick > started_at
        && event_tick < completes_at
    {
        assert!(!advance_manual_power_to(
            registries,
            state,
            work,
            event_tick,
            "workshop manual recovery before controlled event",
        ));
        println!(
            "  interruption: controlled world event occurs during manual charging; structural response waits until the charging work releases player attention"
        );
        event_assessment = Some(apply_delivery(registries, state, ids, controller, actor));
    }
    if state.tick().value() < completes_at {
        let _remaining_ticks = finish_manual_power_work(
            registries,
            state,
            work,
            "workshop manual recovery completion",
        );
    }
    if !actor.report.progress.delivery_applied && state.tick().value() == event_tick {
        event_assessment = Some(apply_delivery(registries, state, ids, controller, actor));
    }
    if let Some(assessment) = event_assessment {
        adapt_after_delivery(registries, state, ids, actor, assessment);
    }

    assert_eq!(state.player_work().active(), None);
    let stored_after = state
        .energy()
        .get_store(option.store)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("manual-recovery {} drive disappeared", option.name));
    assert_eq!(
        stored_after,
        stored_before
            .checked_add(option.energy)
            .unwrap_or_else(|| panic!("manual-recovery energy accounting overflowed")),
        "manual power must add exactly its validated generated work"
    );
    actor.report.choices.manual_recharges = actor
        .report
        .choices
        .manual_recharges
        .checked_add(1)
        .unwrap_or_else(|| panic!("manual-recovery count overflowed"));
    actor.report.resources.manually_generated_energy = actor
        .report
        .resources
        .manually_generated_energy
        .checked_add(option.energy)
        .unwrap_or_else(|| panic!("manual-recovery generated-energy accounting overflowed"));
    actor.report.resources.manual_power_ticks = actor
        .report
        .resources
        .manual_power_ticks
        .checked_add(duration)
        .unwrap_or_else(|| panic!("manual-recovery duration accounting overflowed"));
    actor.report.resources.manual_power_metabolic_energy = actor
        .report
        .resources
        .manual_power_metabolic_energy
        .checked_add(budget.metabolic_energy())
        .unwrap_or_else(|| panic!("manual-recovery metabolic accounting overflowed"));
    actor.report.resources.manual_power_hydration = actor
        .report
        .resources
        .manual_power_hydration
        .checked_add(budget.hydration())
        .unwrap_or_else(|| panic!("manual-recovery hydration accounting overflowed"));
}

fn probe_manual_recovery_option(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    mass: Mass,
    preference: EnergyRecoveryPreference,
) -> ManualRecoveryProbe {
    let mut options = Vec::new();
    let mut survival_limited = false;
    let mut equipment_limited = false;
    let mut storage_limited = false;
    for (name, store) in [("small", ids.small_drive), ("large", ids.large_drive)] {
        let envelope = assess_powered_ore_mass_envelope(
            registries,
            state,
            PROCESS_CRUSH_ORE,
            ids.crusher,
            store,
        )
        .unwrap_or_else(|error| {
            panic!("workshop {name} drive replenishment projection failed: {error}")
        });
        if mass > envelope.maximum_mass_with_replenished_energy() {
            equipment_limited = true;
            continue;
        }
        match manual_recovery_option(registries, state, ids, mass, name, store, envelope) {
            Ok(Some(option)) => options.push(option),
            Ok(None) => {}
            Err(ManualPowerError::Work(
                PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
                | PlayerWorkStartError::InsufficientHydration { .. },
            )) => survival_limited = true,
            Err(ManualPowerError::EnergySink(EnergySinkError::InsufficientCapacity { .. })) => {
                storage_limited = true;
            }
            Err(
                ManualPowerError::ZeroEquipmentPower { .. }
                | ManualPowerError::ConditionDuration(_),
            ) => equipment_limited = true,
            Err(error) => panic!("workshop manual-power recovery projection failed: {error}"),
        }
    }

    let survival = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("workshop survival state disappeared before manual recovery"));
    let physiology = registries.survival().physiology();
    let before_policy_filter = options.len();
    if preference == EnergyRecoveryPreference::ProtectSurvival {
        options.retain(|option| {
            let budget = option.start.resource_budget();
            let energy_after = survival
                .metabolic_energy()
                .checked_sub(budget.metabolic_energy());
            let hydration_after = survival.hydration().checked_sub(budget.hydration());
            energy_after.is_some_and(|value| value >= physiology.hungry_below())
                && hydration_after.is_some_and(|value| value >= physiology.thirsty_below())
        });
    }
    let policy_declined = before_policy_filter > 0 && options.is_empty();
    let option_key = |option: &ManualRecoveryOption| {
        let budget = option.start.resource_budget();
        let duration = option
            .start
            .work()
            .completes_at()
            .checked_duration_since(option.start.work().started_at())
            .unwrap_or_else(|| panic!("manual-recovery option completes before it starts"))
            .value();
        let output_power = state
            .energy()
            .get_store(option.store)
            .and_then(|store| registries.energy().get_store(store.definition()))
            .and_then(|definition| definition.max_output_power().whole_microwatts())
            .unwrap_or_else(|| {
                panic!(
                    "manual-recovery {} drive lost its authored whole-microwatt output-power definition",
                    option.name
                )
            });
        (
            budget.metabolic_energy(),
            budget.hydration(),
            duration,
            std::cmp::Reverse(output_power),
        )
    };
    let best_key = options.iter().map(&option_key).min();
    let option = best_key.map(|best_key| {
        let mut best = options
            .into_iter()
            .filter(|candidate| option_key(candidate) == best_key);
        let selected = best
            .next()
            .unwrap_or_else(|| unreachable!("manual-recovery best key came from an option"));
        assert!(
            best.next().is_none(),
            "manual recovery has multiple equally useful observable destinations; add an explicit player policy instead of using store identity or label"
        );
        selected
    });
    ManualRecoveryProbe {
        option,
        survival_limited,
        policy_declined,
        equipment_limited,
        storage_limited,
    }
}

pub(super) fn largest_manual_recovery(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    desired: Mass,
    preference: EnergyRecoveryPreference,
) -> ManualRecoverySearch {
    let desired_probe = probe_manual_recovery_option(registries, state, ids, desired, preference);
    if let Some(option) = desired_probe.option {
        return ManualRecoverySearch::Available {
            mass: desired,
            option: Box::new(option),
            adaptive_constraint: None,
        };
    }

    let adaptive_constraint = if desired_probe.policy_declined {
        Some(ManualRecoveryConstraint::SurvivalPolicy)
    } else if desired_probe.survival_limited {
        Some(ManualRecoveryConstraint::SurvivalReserve)
    } else if desired_probe.equipment_limited {
        Some(ManualRecoveryConstraint::EquipmentCondition)
    } else if desired_probe.storage_limited {
        Some(ManualRecoveryConstraint::StorageCapacity)
    } else {
        None
    };

    let mut low = 1_u64;
    let mut high = desired.milligrams().saturating_sub(1);
    let mut best = None;
    while low <= high {
        let midpoint = low + (high - low) / 2;
        let mass = Mass::from_milligrams(midpoint);
        let probe = probe_manual_recovery_option(registries, state, ids, mass, preference);
        if let Some(option) = probe.option {
            best = Some((mass, option));
            low = midpoint + 1;
        } else {
            high = midpoint.saturating_sub(1);
        }
    }
    if let Some((mass, option)) = best {
        return ManualRecoverySearch::Available {
            mass,
            option: Box::new(option),
            adaptive_constraint,
        };
    }

    let minimum_probe = probe_manual_recovery_option(
        registries,
        state,
        ids,
        production_minimum_batch_mass(registries, PROCESS_CRUSH_ORE),
        preference,
    );
    if minimum_probe.policy_declined || desired_probe.policy_declined {
        ManualRecoverySearch::DeclinedForSurvival
    } else if minimum_probe.survival_limited || desired_probe.survival_limited {
        ManualRecoverySearch::SurvivalLimited
    } else if minimum_probe.equipment_limited || desired_probe.equipment_limited {
        ManualRecoverySearch::EquipmentLimited
    } else if minimum_probe.storage_limited || desired_probe.storage_limited {
        ManualRecoverySearch::StorageLimited
    } else {
        panic!(
            "manual recovery search found no viable option without a classified physical or policy constraint"
        )
    }
}
