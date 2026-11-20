//! Shared physiological budgeting for exclusive player-owned work.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::registry::Registries;
use crate::survival::{
    FoodFreshness, PhysiologyDefinition, SurvivalExertion, SurvivalResourceProjectionError,
    assess_surface_food_freshness, project_survival_resource_budget,
};

use super::PlayerWork;

/// A living player can attempt one ordinary hand-foraged portion to avert starvation
/// or dehydration, even if the reserves cannot pay for the entire task. The normal
/// survival tick charges every cost and turns any deficit into vitality damage.
/// Derive this policy from actual work and source custody at both admission and load.
pub(crate) fn allows_work_reserve_deficit(
    registries: &Registries,
    state: &AppState,
    work: PlayerWork,
) -> bool {
    let PlayerWork::SurfaceGathering { work } = work else {
        return false;
    };
    let Some(method) = registries.labor().get_surface_gathering(work.method()) else {
        return false;
    };
    if work.gathered_mass() > method.direct_batch_mass()
        || !state
            .logistics()
            .player()
            .is_some_and(|player| player.carried_stockpile() == work.destination())
    {
        return false;
    }
    state
        .surface()
        .get(work.resource())
        .is_some_and(|resource| {
            matches!(
                assess_surface_food_freshness(registries, resource, work.completes_at()),
                Some(FoodFreshness::Fresh { .. })
            )
        })
}

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
