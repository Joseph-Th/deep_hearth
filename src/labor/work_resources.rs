//! Shared physiological budgeting for exclusive player-owned work.

use crate::core::quantity::{Energy, Volume};
use crate::core::time::TickSpan;
use crate::survival::{PhysiologyDefinition, SurvivalExertion};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerWorkResourceBudgetError {
    EnergyOverflow,
    HydrationOverflow,
}

/// Authoritative physiological cost projected for one player-owned work order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerWorkResourceBudget {
    metabolic_energy: Energy,
    hydration: Volume,
}

impl PlayerWorkResourceBudget {
    #[must_use]
    pub const fn metabolic_energy(self) -> Energy {
        self.metabolic_energy
    }

    #[must_use]
    pub const fn hydration(self) -> Volume {
        self.hydration
    }
}

pub(crate) fn calculate_player_work_resource_budget(
    physiology: PhysiologyDefinition,
    exertion: SurvivalExertion,
    duration: TickSpan,
) -> Result<PlayerWorkResourceBudget, PlayerWorkResourceBudgetError> {
    let energy_per_tick = physiology
        .basal_energy_cost_per_tick()
        .checked_add(exertion.energy_cost_per_tick())
        .ok_or(PlayerWorkResourceBudgetError::EnergyOverflow)?;
    let metabolic_energy = energy_per_tick
        .nanojoules()
        .checked_mul(u128::from(duration.value()))
        .map(Energy::from_nanojoules)
        .ok_or(PlayerWorkResourceBudgetError::EnergyOverflow)?;
    let hydration_per_tick = physiology
        .hydration_loss_per_tick()
        .checked_add(exertion.hydration_loss_per_tick())
        .ok_or(PlayerWorkResourceBudgetError::HydrationOverflow)?;
    let hydration = hydration_per_tick
        .microliters()
        .checked_mul(duration.value())
        .map(Volume::from_microliters)
        .ok_or(PlayerWorkResourceBudgetError::HydrationOverflow)?;
    Ok(PlayerWorkResourceBudget {
        metabolic_energy,
        hydration,
    })
}

#[cfg(test)]
#[path = "work_resources_tests.rs"]
mod tests;
