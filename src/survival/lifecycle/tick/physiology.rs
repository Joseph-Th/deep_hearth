//! Live-player physiological resolution for one authoritative survival tick.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;
use crate::survival::assessment::accumulate_diet_supported_vitality_recovery;
use crate::survival::consumption::{DirectConsumptionInstallment, direct_consumption_installment};
use crate::survival::state::{PlayerSurvivalRecord, player_record};
use crate::survival::{
    FoodCategory, PendingDirectConsumption, SurvivalExertion, SurvivalTickResourceCostError,
    Vitality, resolve_survival_tick_resource_cost,
};

use super::SurvivalTickError;

pub(super) struct LivePlayerTickResolution {
    pub(super) after: PlayerSurvivalRecord,
    pub(super) pending_after: Option<PendingDirectConsumption>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TickResourceResolution {
    after_intake: PlayerSurvivalRecord,
    energy_deficit: bool,
    hydration_deficit: bool,
}

fn apply_direct_consumption_installment(
    physiology: crate::survival::PhysiologyDefinition,
    before: PlayerSurvivalRecord,
    installment: DirectConsumptionInstallment,
    energy_shortfall: Energy,
    hydration_shortfall: Volume,
) -> PlayerSurvivalRecord {
    let residual_energy = installment
        .energy()
        .checked_sub(energy_shortfall)
        .unwrap_or(Energy::ZERO);
    let available_energy = physiology
        .maximum_metabolic_energy()
        .checked_sub(before.metabolic_energy())
        .unwrap_or_else(|| panic!("trusted survival energy exceeded authored maximum"));
    let metabolic_energy = before
        .metabolic_energy()
        .checked_add(residual_energy.min(available_energy))
        .unwrap_or_else(|| unreachable!("capped direct-consumption energy fits reserve"));

    let available_hydration = physiology
        .maximum_hydration()
        .checked_sub(before.hydration())
        .unwrap_or_else(|| panic!("trusted survival hydration exceeded authored maximum"));
    let residual_hydration = installment
        .hydration()
        .microliters()
        .saturating_sub(u128::from(hydration_shortfall.microliters()));
    let hydration_gained = residual_hydration.min(u128::from(available_hydration.microliters()));
    let hydration_gained = Volume::from_microliters(
        u64::try_from(hydration_gained)
            .unwrap_or_else(|_| unreachable!("capped hydration installment fits u64")),
    );
    let hydration = before
        .hydration()
        .checked_add(hydration_gained)
        .unwrap_or_else(|| unreachable!("capped direct-consumption hydration fits reserve"));

    let mut nutrition = before.nutrition();
    for category in [
        FoodCategory::Grain,
        FoodCategory::Fruit,
        FoodCategory::Protein,
    ] {
        nutrition = nutrition
            .add(category, installment.nutrition().get(category))
            .0;
    }
    player_record(
        metabolic_energy,
        hydration,
        before.vitality(),
        nutrition,
        before.vitality_recovery_remainder(),
    )
}

fn resolve_pending_installment(
    registries: &Registries,
    state: &AppState,
    next_tick: SimulationTick,
) -> (
    DirectConsumptionInstallment,
    Option<PendingDirectConsumption>,
) {
    let Some(pending) = state.survival().pending_direct_consumption().cloned() else {
        return (DirectConsumptionInstallment::default(), None);
    };
    let installment = direct_consumption_installment(registries, &pending, state.tick(), next_tick);
    let pending_after = (!installment.completes()).then_some(pending);
    (installment, pending_after)
}

fn resolve_tick_resources(
    physiology: crate::survival::PhysiologyDefinition,
    before: PlayerSurvivalRecord,
    exertion: SurvivalExertion,
    installment: DirectConsumptionInstallment,
) -> Result<TickResourceResolution, SurvivalTickError> {
    let resource_cost =
        resolve_survival_tick_resource_cost(physiology, exertion).map_err(|error| match error {
            SurvivalTickResourceCostError::EnergyOverflow => SurvivalTickError::EnergyCostOverflow,
            SurvivalTickResourceCostError::HydrationOverflow => {
                SurvivalTickError::HydrationCostOverflow
            }
        })?;
    let energy_cost = resource_cost.metabolic_energy();
    let hydration_loss = resource_cost.hydration();
    let energy_shortfall = energy_cost
        .checked_sub(before.metabolic_energy())
        .unwrap_or(Energy::ZERO);
    let hydration_shortfall = hydration_loss
        .checked_sub(before.hydration())
        .unwrap_or(Volume::ZERO);
    let energy_after_cost = before
        .metabolic_energy()
        .checked_sub(energy_cost)
        .unwrap_or(Energy::ZERO);
    let hydration_after_cost = before
        .hydration()
        .checked_sub(hydration_loss)
        .unwrap_or(Volume::ZERO);
    let after_cost = player_record(
        energy_after_cost,
        hydration_after_cost,
        before.vitality(),
        before.nutrition(),
        before.vitality_recovery_remainder(),
    );
    Ok(TickResourceResolution {
        after_intake: apply_direct_consumption_installment(
            physiology,
            after_cost,
            installment,
            energy_shortfall,
            hydration_shortfall,
        ),
        energy_deficit: {
            // Strict less-than is intentional: an installment covering the basal plus exertion
            // cost exactly leaves reserves at zero with no vitality penalty, a deterministic
            // one-tick grace at the exact-cover boundary.
            installment.energy() < energy_shortfall
        },
        hydration_deficit: installment.hydration().microliters()
            < u128::from(hydration_shortfall.microliters()),
    })
}

fn resolve_vitality(
    physiology: crate::survival::PhysiologyDefinition,
    before: PlayerSurvivalRecord,
    resources: TickResourceResolution,
) -> (Vitality, u32) {
    let mut vitality_loss = 0_u32;
    if resources.energy_deficit {
        vitality_loss =
            vitality_loss.saturating_add(physiology.starvation_vitality_loss_ppm_per_tick());
    }
    if resources.hydration_deficit {
        vitality_loss =
            vitality_loss.saturating_add(physiology.dehydration_vitality_loss_ppm_per_tick());
    }
    let mut recovery_remainder = before.vitality_recovery_remainder();
    let vitality_ppm = if vitality_loss > 0 {
        before
            .vitality()
            .parts_per_million()
            .saturating_sub(vitality_loss)
    } else if resources.after_intake.metabolic_energy() >= physiology.hungry_below()
        && resources.after_intake.hydration() >= physiology.thirsty_below()
        && before.vitality() < Vitality::MAXIMUM
    {
        let (recovery, next_remainder) = accumulate_diet_supported_vitality_recovery(
            physiology,
            resources.after_intake.nutrition(),
            recovery_remainder,
        );
        recovery_remainder = next_remainder;
        let recovered = before
            .vitality()
            .parts_per_million()
            .saturating_add(recovery)
            .min(Vitality::MAXIMUM.parts_per_million());
        if recovered == Vitality::MAXIMUM.parts_per_million() {
            recovery_remainder = 0;
        }
        recovered
    } else {
        if before.vitality() == Vitality::MAXIMUM {
            recovery_remainder = 0;
        }
        before.vitality().parts_per_million()
    };
    (
        Vitality::from_parts_per_million_unchecked(vitality_ppm),
        recovery_remainder,
    )
}

pub(super) fn resolve_live_player_tick(
    registries: &Registries,
    state: &AppState,
    before: PlayerSurvivalRecord,
    exertion: SurvivalExertion,
    next_tick: SimulationTick,
) -> Result<LivePlayerTickResolution, SurvivalTickError> {
    let physiology = registries.survival().physiology();
    let (installment, pending_after) = resolve_pending_installment(registries, state, next_tick);
    let resources = resolve_tick_resources(physiology, before, exertion, installment)?;
    let (vitality_after, recovery_remainder) = resolve_vitality(physiology, before, resources);
    let pending_after = if vitality_after == Vitality::ZERO {
        None
    } else {
        pending_after
    };
    let nutrition_after = resources
        .after_intake
        .nutrition()
        .decay(physiology.nutrition().decay_ppm_per_tick());
    let after = player_record(
        resources.after_intake.metabolic_energy(),
        resources.after_intake.hydration(),
        vitality_after,
        nutrition_after,
        recovery_remainder,
    );
    Ok(LivePlayerTickResolution {
        after,
        pending_after,
    })
}
