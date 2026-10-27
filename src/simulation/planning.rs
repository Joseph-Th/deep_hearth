//! Cross-owner planning helpers for the canonical simulation tick.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::inventory::{
    StorageEnclosureDismantlingTickPlan, decide_storage_enclosure_dismantling_tick,
};
use crate::mining::{
    MiningSettlementError, MiningTickPlan, ValidatedMiningSettlement,
    validate_awaiting_mining_output_settlement, validate_due_mining_output_settlement,
};
use crate::production::CompletionPlan;
use crate::registry::Registries;
use crate::structural::{
    StructuralConstructionTickPlan, StructuralDismantlingTickPlan,
    decide_structural_construction_tick, decide_structural_dismantling_tick,
};
use crate::surface::{SurfaceGatheringTickPlan, decide_surface_gathering_tick};

use super::TickError;

pub(super) fn decide_structural_construction_after_completions(
    registries: &Registries,
    state: &AppState,
    completion_plan: &CompletionPlan,
    next_tick: SimulationTick,
) -> Option<StructuralConstructionTickPlan> {
    state
        .player_work()
        .structural_construction_due_at(next_tick)?;
    let projected_structures = completion_plan.project_structures_after_load(state.structures());
    decide_structural_construction_tick(registries, state, projected_structures.as_ref(), next_tick)
}

pub(super) fn decide_structural_dismantling_after_completions(
    registries: &Registries,
    state: &AppState,
    completion_plan: &CompletionPlan,
    next_tick: SimulationTick,
) -> Result<Option<StructuralDismantlingTickPlan>, TickError> {
    if state
        .player_work()
        .structural_dismantling_due_at(next_tick)
        .is_none()
    {
        return Ok(None);
    }
    let projected_inventory = completion_plan.project_inventory_after_deposits(state.inventory());
    let projected_structures = completion_plan.project_structures_after_load(state.structures());
    decide_structural_dismantling_tick(
        registries,
        state,
        projected_inventory.as_ref(),
        projected_structures.as_ref(),
        next_tick,
    )
    .map_err(Into::into)
}

pub(super) fn decide_surface_gathering_after_completions(
    registries: &Registries,
    state: &AppState,
    completion_plan: &CompletionPlan,
    next_tick: SimulationTick,
) -> Result<Option<SurfaceGatheringTickPlan>, TickError> {
    if state
        .player_work()
        .surface_gathering_due_at(next_tick)
        .is_none()
    {
        return Ok(None);
    }
    let projected_inventory = completion_plan.project_inventory_after_deposits(state.inventory());
    decide_surface_gathering_tick(registries, state, projected_inventory.as_ref(), next_tick)
        .map_err(Into::into)
}

fn checked_revision_steps(
    steps: impl IntoIterator<Item = u64>,
    exhausted: TickError,
) -> Result<u64, TickError> {
    steps
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
        .ok_or(exhausted)
}

pub(super) fn require_revision_capacity(
    current: u64,
    steps: impl IntoIterator<Item = u64>,
    exhausted: TickError,
) -> Result<(), TickError> {
    let steps = checked_revision_steps(steps, exhausted.clone())?;
    if current.checked_add(steps).is_none() {
        return Err(exhausted);
    }
    Ok(())
}

pub(super) fn decide_automatic_mining_output(
    registries: &Registries,
    state: &AppState,
    next_tick: SimulationTick,
    mining_plan: Option<MiningTickPlan>,
    inventory_owner_busy: bool,
    structure_owner_busy: bool,
) -> Result<Option<ValidatedMiningSettlement>, TickError> {
    // Mining recovery is incidental to the current player action. If another scheduled phase owns
    // an inventory mutation this tick, preserve durable awaiting-settlement custody and retry next
    // tick rather than invalidating that phase's revision-bound plan or inventing a combined
    // transaction.
    if inventory_owner_busy {
        return Ok(None);
    }

    // A due completion has a distinct mining-revision binding: its output must be planned
    // against the revision after extraction. Awaiting jobs instead bind the current revision.
    if let Some(plan) = mining_plan {
        return available_mining_settlement(
            validate_due_mining_output_settlement(registries, state, plan, next_tick),
            structure_owner_busy,
        );
    }

    // Failed support can retain one output indefinitely. Its job identity must not prevent
    // independent, already-extracted outputs from landing in physically available storage.
    for job in state
        .mining()
        .jobs()
        .filter(|job| job.is_awaiting_settlement())
    {
        if let Some(settlement) = available_mining_settlement(
            validate_awaiting_mining_output_settlement(registries, state, job.id(), next_tick),
            structure_owner_busy,
        )? {
            return Ok(Some(settlement));
        }
    }
    Ok(None)
}

fn available_mining_settlement(
    planned: Result<ValidatedMiningSettlement, MiningSettlementError>,
    structure_owner_busy: bool,
) -> Result<Option<ValidatedMiningSettlement>, TickError> {
    match planned {
        Ok(settlement) if structure_owner_busy && settlement.mutates_structures() => Ok(None),
        Ok(settlement) => Ok(Some(settlement)),
        // A support can fail after mining admission. Extraction remains authoritative and the
        // durable awaiting-settlement state retries automatically after the physical blocker is
        // repaired.
        Err(MiningSettlementError::StructuralLoad(_)) => Ok(None),
        Err(MiningSettlementError::LotIdExhausted) => Err(TickError::MaterialLotIdExhausted),
        Err(MiningSettlementError::InventoryRevisionExhausted) => {
            Err(TickError::InventoryRevisionExhausted)
        }
        Err(MiningSettlementError::MiningRevisionExhausted) => {
            Err(TickError::MiningRevisionExhausted)
        }
        Err(
            MiningSettlementError::UnknownJob { .. }
            | MiningSettlementError::NotAwaitingSettlement { .. },
        ) => {
            unreachable!("automatic mining settlement must target current due or awaiting work")
        }
    }
}

pub(super) fn decide_storage_dismantling_after_completions(
    registries: &Registries,
    state: &AppState,
    completion_plan: &CompletionPlan,
    next_tick: SimulationTick,
) -> Result<Option<StorageEnclosureDismantlingTickPlan>, TickError> {
    if state
        .player_work()
        .storage_dismantling_due_at(next_tick)
        .is_none()
    {
        return Ok(None);
    }
    let projected_inventory = completion_plan.project_inventory_after_deposits(state.inventory());
    decide_storage_enclosure_dismantling_tick(
        registries,
        state,
        projected_inventory.as_ref(),
        next_tick,
    )
    .map_err(Into::into)
}

#[cfg(test)]
#[path = "planning_tests.rs"]
mod tests;
