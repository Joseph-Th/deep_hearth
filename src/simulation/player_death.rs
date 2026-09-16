//! End-of-tick cross-owner cleanup when survival makes active player work impossible.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::inventory::{
    StorageEnclosureDismantlingCancellationPlan, apply_storage_enclosure_dismantling_cancellation,
    decide_storage_enclosure_dismantling_cancellation,
};
use crate::labor::PlayerWork;
use crate::mining::{
    MiningCancellationError, MiningCancellationPlan, apply_mining_cancellation,
    decide_mining_cancellation,
};
use crate::production::{CompletionPlan, plan_player_death_suspension};

use super::TickError;

pub(super) enum PlayerDeathCancellationPlan {
    Mining(MiningCancellationPlan),
    StorageDismantling(StorageEnclosureDismantlingCancellationPlan),
}

/// Plans all durable consequences of survival ending unfinished direct player work.
///
/// Completion due on the fatal tick remains authoritative. Otherwise mining and dismantling are
/// canceled through their owners, while manual production preserves WIP by suspending it.
pub(super) fn decide_player_death_effects(
    state: &AppState,
    next_tick: SimulationTick,
    player_dead_after_tick: bool,
    completion_plan: &mut CompletionPlan,
) -> Result<Option<PlayerDeathCancellationPlan>, TickError> {
    if !player_dead_after_tick {
        return Ok(None);
    }
    plan_player_death_suspension(state, next_tick, completion_plan)?;
    let Some(work) = state.player_work().active() else {
        return Ok(None);
    };
    match work {
        PlayerWork::Mining { job } => {
            let record = state.mining().get_job(job).unwrap_or_else(|| {
                panic!("player mining job disappeared before death cancellation")
            });
            if record.completes_at() == next_tick {
                return Ok(None);
            }
            let projected_inventory =
                completion_plan.project_inventory_after_deposits(state.inventory());
            decide_mining_cancellation(state, projected_inventory.as_ref(), job)
                .map(PlayerDeathCancellationPlan::Mining)
                .map(Some)
                .map_err(|error| match error {
                    MiningCancellationError::InventoryRevision => {
                        TickError::InventoryRevisionExhausted
                    }
                    MiningCancellationError::MiningRevision => TickError::MiningRevisionExhausted,
                })
        }
        PlayerWork::StorageEnclosureDismantling { work } => {
            if work.completes_at() == next_tick {
                return Ok(None);
            }
            let projected_inventory =
                completion_plan.project_inventory_after_deposits(state.inventory());
            decide_storage_enclosure_dismantling_cancellation(projected_inventory.as_ref(), work)
                .map(PlayerDeathCancellationPlan::StorageDismantling)
                .map(Some)
                .map_err(Into::into)
        }
        PlayerWork::ManualProduction { .. }
        | PlayerWork::ManualPower { .. }
        | PlayerWork::Prospecting { .. }
        | PlayerWork::Eating { .. }
        | PlayerWork::Drinking { .. }
        | PlayerWork::EquipmentMaintenance { .. } => Ok(None),
    }
}

pub(super) fn apply_player_death_cancellation(
    state: &mut AppState,
    plan: Option<PlayerDeathCancellationPlan>,
) {
    match plan {
        Some(PlayerDeathCancellationPlan::Mining(plan)) => apply_mining_cancellation(state, plan),
        Some(PlayerDeathCancellationPlan::StorageDismantling(plan)) => {
            apply_storage_enclosure_dismantling_cancellation(state, plan);
        }
        None => {}
    }
}
