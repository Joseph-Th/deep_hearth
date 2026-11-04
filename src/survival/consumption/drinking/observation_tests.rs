//! Generated-world drink-source sightings respect physical visibility, not opening graph edges.

use super::*;
use crate::content::{BLOCK_STATE_STONE, FLUID_WATER, build_registries};
use crate::core::state::{AppState, apply_clock_advance, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::fluid::{GeneratedFluidStoreSpec, validate_generate_fluid_store};
use crate::logistics::validate_place_fluid_store;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::world::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WorldGenerationSeed,
    set_generated_block_for_test, validate_generate_chunk,
};

#[test]
fn generated_water_sight_requires_a_living_lit_unobstructed_local_observer() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x5741_5445_5253_4947),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();

    let y = WORLD_MAX_Y_EXCLUSIVE - 4;
    let player = VoxelCoord::new(2, y, 2);
    let first = VoxelCoord::new(4, y, 2);
    let second = VoxelCoord::new(3, y, 2);
    let occluded = VoxelCoord::new(2, y, 4);
    let remote = VoxelCoord::new(6, y, 2);
    let scalding = VoxelCoord::new(2, y, 1);
    let barrier = VoxelCoord::new(2, y, 3);
    for position in [player, first, second, occluded, remote, scalding] {
        let below = VoxelCoord::new(position.x(), y - 1, position.z());
        set_generated_block_for_test(registries.world(), &mut state, below, BLOCK_STATE_STONE);
        set_generated_block_for_test(registries.world(), &mut state, position, BlockStateId::AIR);
    }
    set_generated_block_for_test(registries.world(), &mut state, barrier, BLOCK_STATE_STONE);
    let create = |state: &mut AppState, position, temperature| {
        let serving = Volume::from_microliters(750_000);
        let store = validate_generate_fluid_store(
            &registries,
            state,
            GeneratedFluidStoreSpec::new(serving, FLUID_WATER, serving, temperature),
        )
        .unwrap()
        .commit(state)
        .unwrap();
        validate_place_fluid_store(state, store, position)
            .unwrap()
            .commit(state)
            .unwrap();
        store
    };
    let cool = Temperature::from_millikelvin(293_150);
    let first_id = create(&mut state, first, cool);
    let _occluded_id = create(&mut state, occluded, cool);
    let second_id = create(&mut state, second, cool);
    let _remote_id = create(&mut state, remote, cool);
    let _hot_id = create(&mut state, scalding, Temperature::from_millikelvin(345_150));
    let observed = |state: &AppState| {
        AdjacentDrinkSource::visible_in_generated_world(&registries, state)
            .map(|source| (source.store(), source.fluid(), source.position()))
            .collect::<Vec<_>>()
    };
    assert!(
        observed(&state).is_empty(),
        "unadmitted observer has no sightings"
    );
    validate_admit_player(&registries, &state, player)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        available_adjacent_drink_sources(&registries, &state).count(),
        0,
        "generated terrain need not have an opening-graph edge"
    );
    assert_eq!(
        observed(&state),
        vec![
            (first_id, FLUID_WATER, first),
            (second_id, FLUID_WATER, second)
        ],
        "real local water must be visible by source identity despite index order, while hot, blocked and distant sources stay hidden"
    );
    let before_remote_drink = state.clone();
    assert!(matches!(
        super::super::validate_drink(
            &registries,
            &state,
            first_id,
            Volume::from_microliters(250_000),
        ),
        Err(super::super::DrinkError::Access(_))
    ));
    assert_eq!(
        state, before_remote_drink,
        "seeing remote water cannot bypass exact-local fluid custody"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    // The corpse retains physical custody, but has no observational agency.
    let mut dead = state.clone();
    let living = dead.survival().player().copied().unwrap();
    let revision = dead.survival().revision();
    dead.survival_state_mut().apply_player(
        revision,
        revision + 1,
        crate::survival::player_record_with_thermal_comfort(
            living.metabolic_energy(),
            living.hydration(),
            Vitality::ZERO,
            living.nutrition(),
            living.vitality_recovery_remainder(),
            living.thermal_comfort_ppm(),
        ),
    );
    assert!(observed(&dead).is_empty());
    assert_eq!(validate_loaded_state(&registries, &dead), Ok(()));

    let day_ticks = registries.core().calendar().ticks_per_day();
    apply_clock_advance(&mut state, SimulationTick::new(day_ticks * 3 / 4));
    assert!(
        observed(&state).is_empty(),
        "unlit water cannot be seen at night"
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    assert!(observed(&loaded).is_empty());
    apply_clock_advance(&mut state, SimulationTick::new(day_ticks));
    assert_eq!(
        observed(&state),
        vec![
            (first_id, FLUID_WATER, first),
            (second_id, FLUID_WATER, second)
        ]
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
