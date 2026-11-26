//! Buffered contextual actions preserve physical identity across control samples and saves.

use super::*;
use crate::content::{CAMPFIRE_FUEL_MASS, ENERGY_STONE_RING_CAMPFIRE, FORM_LUMP, MATERIAL_STONE};
use crate::core::state::validate_loaded_state;
use crate::energy::{validate_assemble_energy_store, validate_combust_selected_fuel};
use crate::logistics::{PlayerActiveSelection, validate_set_active_selection};
use crate::matter::calculate_matter_accounting;

fn settle_secondary_replay(
    registries: &Registries,
    initial: &AppState,
    input: PlayerDirectControlInput,
) -> (AppState, SecondaryActionOutcome) {
    let saved = serde_json::to_vec(&SaveEnvelope::new(registries, initial)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(registries)
        .unwrap();
    let mut state = initial.clone();
    loop {
        let current =
            advance_control_tick_with_direct_input(registries, &mut state, input).unwrap();
        let restored =
            advance_control_tick_with_direct_input(registries, &mut replay, input).unwrap();
        assert_eq!(current, restored);
        if current.simulation().is_some() {
            assert_eq!(state, replay);
            assert!(state.logistics().pending_secondary_use().is_none());
            assert_eq!(validate_loaded_state(registries, &state), Ok(()));
            return (state, current.secondary_action().unwrap().clone());
        }
        assert!(state.logistics().pending_secondary_use().is_some());
    }
}

/// Identical held and released gestures must settle to the same outcome and state.
fn settle_held_and_released(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> (AppState, SecondaryActionOutcome) {
    let hold = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(direction));
    let released = settle_secondary_replay(registries, state, PlayerDirectControlInput::NEUTRAL);
    let held = settle_secondary_replay(registries, state, hold);
    assert_eq!(released, held);
    held
}

#[test]
fn queued_timber_construction_cannot_turn_into_refueling_after_target_changes() {
    let (registries, mut state, foot_y) = generated_grounded_player_at(0, 0);
    let ground = VoxelCoord::new(1, foot_y - 1, 0);
    let destination = VoxelCoord::new(1, foot_y, 0);
    set_generated_block_for_test(registries.world(), &mut state, ground, BLOCK_STATE_STONE);
    for y in foot_y..=foot_y + 1 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(1, y, 0),
            BlockStateId::AIR,
        );
    }
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000_000),
        ),
        (
            timber,
            Mass::from_milligrams(6_500_000)
                .checked_add(CAMPFIRE_FUEL_MASS)
                .unwrap(),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let fire =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(timber)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let direction = ViewDirection::new(600_000, -800_000, 0).unwrap();
    let original_hit = crate::world::raycast_player_block(&registries, &state, direction).unwrap();
    assert_eq!(
        original_hit.map(|hit| (hit.voxel(), hit.face())),
        Some((ground, crate::spatial::CubeFace::Top))
    );
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(direction)),
    )
    .unwrap();
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );

    // Change the physical target without moving the actor or changing the hand.
    // A solid obstruction now intercepts the original floor-directed ray.
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        destination,
        BLOCK_STATE_STONE,
    );
    assert_ne!(
        crate::world::raycast_player_block(&registries, &state, direction).unwrap(),
        original_hit
    );
    let inventory = state.inventory().clone();
    let structures = state.structures().clone();
    let matter = calculate_matter_accounting(&state).unwrap().total();
    let (state, outcome) = settle_held_and_released(&registries, &state, direction);
    assert_eq!(outcome, SecondaryActionOutcome::BufferedContextChanged);
    assert_eq!(state.inventory(), &inventory);
    assert_eq!(state.structures(), &structures);
    assert_eq!(
        state.energy().get_store(fire).unwrap().stored(),
        Energy::ZERO
    );
    assert!(state.player_work().active().is_none());
    assert!(state.logistics().pending_secondary_use().is_none());
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), matter);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn queued_refueling_cannot_redirect_to_a_newly_available_fire() {
    let (registries, mut state, foot_y) = generated_grounded_player_at(0, 0);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(1, foot_y + 1, 0),
        BlockStateId::AIR,
    );
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(2, foot_y + 1, 0),
        BLOCK_STATE_STONE,
    );
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(2_000_000),
        ),
        (
            timber,
            Mass::from_milligrams(CAMPFIRE_FUEL_MASS.milligrams() * 3),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let original =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(timber)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let direction = ViewDirection::new(1_000_000, 0, 0).unwrap();
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(direction)),
    )
    .unwrap();
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );

    // A still-valid target consumes exactly one charge, even with the same
    // input sampled on every control tick including the due boundary.
    let (settled, outcome) = settle_held_and_released(&registries, &state, direction);
    assert_eq!(
        outcome,
        SecondaryActionOutcome::CombustionFuelAdded {
            store: original,
            released: crate::content::CAMPFIRE_FUEL_THERMAL_ENERGY,
        }
    );
    assert_eq!(
        settled
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(timber),
        Mass::from_milligrams(CAMPFIRE_FUEL_MASS.milligrams() * 2)
    );
    assert_eq!(
        settled.energy().get_store(original).unwrap().stored(),
        crate::content::CAMPFIRE_FUEL_THERMAL_ENERGY
    );

    // Fill the original fire and assemble another before the queued use matures.
    // The remaining charge is admissible at the new store, not the old one.
    for _ in 0..2 {
        let _ = validate_combust_selected_fuel(&registries, &state, original, carried, timber)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    let replacement =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    assert!(
        validate_combust_selected_fuel(&registries, &state, original, carried, timber).is_err()
    );
    assert!(
        validate_combust_selected_fuel(&registries, &state, replacement, carried, timber).is_ok()
    );
    let inventory = state.inventory().clone();
    let matter = calculate_matter_accounting(&state).unwrap().total();
    let (state, outcome) = settle_held_and_released(&registries, &state, direction);
    assert_eq!(outcome, SecondaryActionOutcome::BufferedContextChanged);
    assert_eq!(state.inventory(), &inventory);
    assert_eq!(
        state.energy().get_store(replacement).unwrap().stored(),
        Energy::ZERO
    );
    assert!(state.logistics().pending_secondary_use().is_none());
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), matter);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}
