//! Performs exhaustive trusted-load validation across the root runtime graph.

use crate::energy::validate_loaded_energy;
use crate::equipment::validate_loaded_equipment;
use crate::fluid::validate_loaded_fluid;
use crate::geology::{validate_loaded_geological_knowledge, validate_loaded_geology};
use crate::inventory::validate_loaded_inventory;
use crate::labor::validate_loaded_player_work;
use crate::mining::{validate_loaded_mining, validate_loaded_mining_jobs};
use crate::production::{validate_loaded_production, validate_loaded_production_schedule_history};
use crate::registry::Registries;
use crate::structural::validate_loaded_structure;
use crate::survival::validate_loaded_survival;

use super::AppState;

mod error;
mod inventory;
mod production;
mod reservations;
mod structural;

pub use error::StateValidationError;
use inventory::validate_inventory_references;
use production::validate_production_references;
use reservations::validate_reserved_inbound;
use structural::validate_structural_integrations;

/// Validates decoded persistent state before it can re-enter the runtime.
pub fn validate_loaded_state(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StateValidationError> {
    state
        .random
        .validate()
        .map_err(StateValidationError::Random)?;
    if state.random.root_seed() != state.world_seed {
        return Err(StateValidationError::RandomWorldSeedMismatch {
            world_seed: state.world_seed,
            random_seed: state.random.root_seed(),
        });
    }
    state
        .random
        .validate_current_app_state_reachability()
        .map_err(StateValidationError::Random)?;

    validate_loaded_energy(
        registries.energy(),
        registries.materials(),
        &state.systems.energy,
        state.tick(),
    )
    .map_err(StateValidationError::Energy)?;
    validate_loaded_fluid(
        registries.fluid(),
        registries.materials(),
        &state.systems.fluid,
        state.tick(),
    )
    .map_err(StateValidationError::Fluid)?;
    validate_loaded_equipment(
        registries.equipment(),
        registries.materials(),
        &state.systems.equipment,
        state.tick(),
    )
    .map_err(StateValidationError::Equipment)?;
    validate_loaded_structure(
        registries.structural(),
        registries.materials(),
        &state.systems.structures,
        state.tick(),
        registries.core().gravity(),
    )
    .map_err(StateValidationError::Structure)?;
    validate_loaded_inventory(
        registries.materials(),
        &state.systems.inventory,
        state.tick(),
    )
    .map_err(StateValidationError::Inventory)?;

    // Validate trace-backed enclosure matter before cross-owner structural accounting derives its
    // mass. This keeps malformed decoded trace sums on the validation-error path rather than the
    // runtime-invariant panic path used by already trusted state.
    validate_inventory_references(registries, state)?;
    validate_structural_integrations(registries, state)?;
    validate_loaded_geology(registries.materials(), &state.systems.geology, state.tick())
        .map_err(StateValidationError::Geology)?;
    validate_loaded_geological_knowledge(
        registries.materials(),
        &state.systems.geological_knowledge,
        state.tick(),
    )
    .map_err(StateValidationError::GeologicalKnowledge)?;
    crate::geology::validate_loaded_hardness_against_live_geology(
        &state.systems.geology,
        &state.systems.geological_knowledge,
    )
    .map_err(StateValidationError::GeologicalKnowledge)?;
    validate_loaded_production(&state.systems.production, state.tick())
        .map_err(StateValidationError::Production)?;
    validate_loaded_mining(&state.systems.mining, state.tick())
        .map_err(StateValidationError::Mining)?;
    validate_loaded_survival(
        registries.survival(),
        registries.materials(),
        registries.fluid(),
        &state.systems.inventory,
        &state.systems.survival,
        state.tick(),
    )
    .map_err(StateValidationError::Survival)?;

    let expected_reservations = validate_production_references(registries, state)?;
    validate_loaded_production_schedule_history(&state.systems.production, state.tick())
        .map_err(StateValidationError::Production)?;
    validate_loaded_mining_jobs(registries, state).map_err(StateValidationError::MiningJob)?;
    validate_reserved_inbound(state, expected_reservations)?;
    validate_loaded_player_work(registries, state, &state.systems.player_work)
        .map_err(StateValidationError::PlayerWork)?;
    validate_shared_future_capacity(state)?;

    Ok(())
}

fn validate_shared_future_capacity(state: &AppState) -> Result<(), StateValidationError> {
    let material_lot_ids_required = state
        .checked_future_material_lot_id_demand()
        .ok_or(StateValidationError::FutureMaterialLotIdDemandOverflow)?;
    let next_material_lot_id = state.inventory().next_lot_id();
    if next_material_lot_id
        .checked_add(material_lot_ids_required)
        .is_none()
    {
        return Err(StateValidationError::FutureMaterialLotIdCapacityExhausted {
            next_lot_id: next_material_lot_id,
            required: material_lot_ids_required,
        });
    }
    let inventory_required = state
        .checked_future_inventory_revision_demand()
        .ok_or(StateValidationError::FutureInventoryRevisionDemandOverflow)?;
    let inventory_revision = state.inventory().revision();
    if inventory_revision.checked_add(inventory_required).is_none() {
        return Err(
            StateValidationError::FutureInventoryRevisionCapacityExhausted {
                revision: inventory_revision,
                required: inventory_required,
            },
        );
    }
    let energy_required = state
        .checked_future_energy_revision_demand()
        .ok_or(StateValidationError::FutureEnergyRevisionDemandOverflow)?;
    let energy_revision = state.energy().revision();
    if energy_revision.checked_add(energy_required).is_none() {
        return Err(
            StateValidationError::FutureEnergyRevisionCapacityExhausted {
                revision: energy_revision,
                required: energy_required,
            },
        );
    }
    let equipment_required = state
        .checked_future_equipment_revision_demand()
        .ok_or(StateValidationError::FutureEquipmentRevisionDemandOverflow)?;
    let equipment_revision = state.equipment().revision();
    if equipment_revision.checked_add(equipment_required).is_none() {
        return Err(
            StateValidationError::FutureEquipmentRevisionCapacityExhausted {
                revision: equipment_revision,
                required: equipment_required,
            },
        );
    }
    let mining_required = state
        .checked_future_mining_revision_demand()
        .ok_or(StateValidationError::FutureMiningRevisionDemandOverflow)?;
    let mining_revision = state.mining().revision();
    if mining_revision.checked_add(mining_required).is_none() {
        return Err(
            StateValidationError::FutureMiningRevisionCapacityExhausted {
                revision: mining_revision,
                required: mining_required,
            },
        );
    }
    let structure_required = state
        .checked_future_structure_revision_demand()
        .ok_or(StateValidationError::FutureStructureRevisionDemandOverflow)?;
    let structure_revision = state.structures().revision();
    if structure_revision.checked_add(structure_required).is_none() {
        return Err(
            StateValidationError::FutureStructureRevisionCapacityExhausted {
                revision: structure_revision,
                required: structure_required,
            },
        );
    }
    Ok(())
}

/// Asserts every cheap runtime invariant in debug builds.
pub fn validate_invariants(registries: &Registries, state: &AppState) {
    debug_assert!(
        state.random.has_valid_core_stream(),
        "Runtime Invariant 11 (Serialization Completeness): core RNG stream must remain valid"
    );
    debug_assert!(
        state.systems.mining.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): mining job ID cursor must remain above every allocated job"
    );
    debug_assert!(
        state.systems.energy.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): energy store ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.fluid.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): fluid store ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.equipment.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): equipment ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.structures.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): structural ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.geology.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): geological deposit ID cursor must remain above every allocated deposit"
    );
    debug_assert!(
        state.systems.geological_knowledge.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): geological observation ID cursor must remain above every allocated observation"
    );
    debug_assert!(
        state.systems.inventory.has_valid_id_cursors(),
        "Runtime Invariant 8 (No Lost Runtime State): inventory ID cursors must remain above every allocated stockpile and lot"
    );
    debug_assert!(
        state.systems.production.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): production job ID cursor must remain above every allocated job"
    );
    debug_assert!(
        state
            .systems
            .production
            .earliest_due_tick()
            .is_none_or(|due| due > state.tick()),
        "Runtime Invariant 6 (Lifecycle Validity): no active production job may remain due"
    );
    debug_assert!(
        state
            .systems
            .mining
            .earliest_due_tick()
            .is_none_or(|due| due > state.tick()),
        "Runtime Invariant 6 (Lifecycle Validity): no working mining job may remain due"
    );
    debug_assert!(
        state
            .systems
            .player_work
            .has_valid_inline_schedule(state.tick()),
        "Runtime Invariant 6 (Lifecycle Validity): active inline player work must have a current unfinished schedule"
    );
    debug_assert!(
        state
            .systems
            .survival
            .has_valid_player_bounds(registries.survival().physiology()),
        "Runtime Invariant 6 (Lifecycle Validity): player survival quantities must remain within authored bounds"
    );
}
