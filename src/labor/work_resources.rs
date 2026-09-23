//! Shared physiological budgeting for exclusive player-owned work.

use crate::core::quantity::{Energy, Volume};
use crate::core::time::TickSpan;
use crate::survival::{
    PhysiologyDefinition, SurvivalExertion, SurvivalResourceProjectionError,
    project_survival_resource_budget,
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
    let projected =
        project_survival_resource_budget(physiology, exertion, duration).map_err(|error| {
            match error {
                SurvivalResourceProjectionError::EnergyOverflow => {
                    PlayerWorkResourceBudgetError::EnergyOverflow
                }
                SurvivalResourceProjectionError::HydrationOverflow => {
                    PlayerWorkResourceBudgetError::HydrationOverflow
                }
            }
        })?;
    Ok(PlayerWorkResourceBudget {
        metabolic_energy: projected.metabolic_energy(),
        hydration: projected.hydration(),
    })
}

#[cfg(test)]
#[path = "work_resources_tests.rs"]
mod tests;
