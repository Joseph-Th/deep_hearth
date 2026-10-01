//! End-of-tick cross-owner cleanup when survival makes active player work impossible.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::{
    EquipmentMaintenanceInterruptionPlan, apply_equipment_maintenance_interruption,
    decide_equipment_maintenance_interruption,
};
use crate::geology::{
    FieldProspectingInterruptionPlan, apply_field_prospecting_interruption,
    decide_field_prospecting_interruption,
};
use crate::inventory::{
    StorageEnclosureDismantlingCancellationPlan, apply_storage_enclosure_dismantling_cancellation,
    decide_storage_enclosure_dismantling_cancellation,
};
use crate::labor::{
    ManualPowerInterruptionPlan, PlayerWork, apply_manual_power_interruption,
    decide_manual_power_interruption,
};
use crate::mining::{
    MiningCancellationError, MiningCancellationPlan, apply_mining_cancellation,
    decide_mining_cancellation,
};
use crate::production::{CompletionPlan, plan_player_death_suspension};
use crate::registry::Registries;

use super::TickError;

pub(super) enum PlayerDeathEffectPlan {
    Mining(MiningCancellationPlan),
    StorageDismantling(StorageEnclosureDismantlingCancellationPlan),
    ManualPower(ManualPowerInterruptionPlan),
    Prospecting(FieldProspectingInterruptionPlan),
    EquipmentMaintenance(EquipmentMaintenanceInterruptionPlan),
}

impl PlayerDeathEffectPlan {
    pub(super) const fn equipment_revision_steps(&self) -> u64 {
        match self {
            Self::Mining(plan) => plan.equipment_revision_steps(),
            Self::ManualPower(plan) => plan.equipment_revision_steps(),
            Self::Prospecting(plan) => plan.equipment_revision_steps(),
            Self::EquipmentMaintenance(plan) => plan.equipment_revision_steps(),
            Self::StorageDismantling(_) => 0,
        }
    }
}

/// Plans all durable consequences of survival ending unfinished direct player work.
///
/// Completion due on the fatal tick remains authoritative. Otherwise mining and dismantling are
/// canceled through their owners, instrumented direct work settles wear from elapsed active ticks,
/// interrupted maintenance retains proportional completed recovery, and manual production preserves
/// WIP by suspending it.
pub(super) fn decide_player_death_effects(
    registries: &Registries,
    state: &AppState,
    next_tick: SimulationTick,
    player_dead_after_tick: bool,
    completion_plan: &mut CompletionPlan,
) -> Result<Option<PlayerDeathEffectPlan>, TickError> {
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
            decide_mining_cancellation(
                registries,
                state,
                projected_inventory.as_ref(),
                job,
                next_tick,
            )
            .map(PlayerDeathEffectPlan::Mining)
            .map(Some)
            .map_err(|error| match error {
                MiningCancellationError::InventoryRevision => TickError::InventoryRevisionExhausted,
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
                .map(PlayerDeathEffectPlan::StorageDismantling)
                .map(Some)
                .map_err(Into::into)
        }
        PlayerWork::ManualPower { .. } => Ok(decide_manual_power_interruption(
            registries, state, next_tick,
        )
        .map(PlayerDeathEffectPlan::ManualPower)),
        PlayerWork::Prospecting { .. } => Ok(decide_field_prospecting_interruption(
            registries, state, next_tick,
        )
        .map(PlayerDeathEffectPlan::Prospecting)),
        PlayerWork::EquipmentMaintenance { .. } => {
            Ok(decide_equipment_maintenance_interruption(state, next_tick)
                .map(PlayerDeathEffectPlan::EquipmentMaintenance))
        }
        PlayerWork::ManualProduction { .. }
        | PlayerWork::Eating { .. }
        | PlayerWork::Drinking { .. } => Ok(None),
    }
}

pub(super) fn apply_player_death_effects(
    state: &mut AppState,
    plan: Option<PlayerDeathEffectPlan>,
) {
    match plan {
        Some(PlayerDeathEffectPlan::Mining(plan)) => apply_mining_cancellation(state, plan),
        Some(PlayerDeathEffectPlan::StorageDismantling(plan)) => {
            apply_storage_enclosure_dismantling_cancellation(state, plan);
        }
        Some(PlayerDeathEffectPlan::ManualPower(plan)) => {
            apply_manual_power_interruption(state, plan);
        }
        Some(PlayerDeathEffectPlan::Prospecting(plan)) => {
            apply_field_prospecting_interruption(state, plan);
        }
        Some(PlayerDeathEffectPlan::EquipmentMaintenance(plan)) => {
            apply_equipment_maintenance_interruption(state, plan);
        }
        None => {}
    }
}
