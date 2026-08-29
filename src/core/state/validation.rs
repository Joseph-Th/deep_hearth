//! Performs exhaustive trusted-load validation across the root runtime graph.

use crate::energy::validate_loaded_energy;
use crate::equipment::validate_loaded_equipment;
use crate::fluid::validate_loaded_fluid;
use crate::geology::{validate_loaded_geological_knowledge, validate_loaded_geology};
use crate::inventory::validate_loaded_inventory;
use crate::labor::validate_loaded_player_work;
use crate::mining::{validate_loaded_mining, validate_loaded_mining_jobs};
use crate::production::validate_loaded_production;
use crate::registry::Registries;
use crate::structural::validate_loaded_structure;
use crate::survival::validate_loaded_survival;

use super::AppState;

mod error;
mod inventory;
mod production;
mod structural;

pub use error::StateValidationError;
use inventory::validate_inventory_references;
use production::validate_production_references;
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

    validate_loaded_energy(
        registries.energy(),
        registries.materials(),
        &state.systems.energy,
        state.tick(),
    )
    .map_err(StateValidationError::Energy)?;
    validate_loaded_fluid(registries.fluid(), &state.systems.fluid, state.tick())
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

    validate_structural_integrations(registries, state)?;
    validate_loaded_geology(registries.materials(), &state.systems.geology, state.tick())
        .map_err(StateValidationError::Geology)?;
    validate_loaded_geological_knowledge(
        registries.materials(),
        &state.systems.geological_knowledge,
        state.tick(),
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
        &state.systems.survival,
    )
    .map_err(StateValidationError::Survival)?;

    validate_inventory_references(registries, state)?;
    validate_production_references(registries, state)?;
    validate_loaded_mining_jobs(registries, state).map_err(StateValidationError::MiningJob)?;
    validate_loaded_player_work(registries, state, &state.systems.player_work)
        .map_err(StateValidationError::PlayerWork)?;

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
        "Runtime Invariant 8 (No Lost Runtime State): mining job ID cursor must remain nonzero"
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
        "Runtime Invariant 8 (No Lost Runtime State): geological deposit ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.geological_knowledge.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): geological observation ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.inventory.has_valid_id_cursors(),
        "Runtime Invariant 8 (No Lost Runtime State): inventory ID cursors must remain nonzero"
    );
    debug_assert!(
        state.systems.production.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): production ID cursor must remain nonzero"
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
