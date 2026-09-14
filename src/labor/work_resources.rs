//! Shared physiological budgeting for exclusive player-owned work.

use crate::core::quantity::{Energy, Volume};
use crate::core::time::TickSpan;
use crate::survival::{
    PhysiologyDefinition, SurvivalExertion, SurvivalTickResourceCostError,
    resolve_survival_tick_resource_cost,
};

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
    let per_tick =
        resolve_survival_tick_resource_cost(physiology, exertion).map_err(|error| match error {
            SurvivalTickResourceCostError::EnergyOverflow => {
                PlayerWorkResourceBudgetError::EnergyOverflow
            }
            SurvivalTickResourceCostError::HydrationOverflow => {
                PlayerWorkResourceBudgetError::HydrationOverflow
            }
        })?;
    let metabolic_energy = per_tick
        .metabolic_energy()
        .nanojoules()
        .checked_mul(u128::from(duration.value()))
        .map(Energy::from_nanojoules)
        .ok_or(PlayerWorkResourceBudgetError::EnergyOverflow)?;
    let hydration = per_tick
        .hydration()
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
