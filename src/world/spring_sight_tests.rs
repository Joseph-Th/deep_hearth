//! Physical water sightings do not imply safe drinking or remote fluid access.

use super::*;
use crate::content::{BLOCK_STATE_STONE, FLUID_WATER, build_registries};
use crate::core::quantity::{Temperature, Volume};
use crate::core::state::{AppState, apply_clock_advance, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::fluid::{GeneratedFluidStoreSpec, validate_generate_fluid_store};
use crate::logistics::{relocate_player_for_test, validate_place_fluid_store};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::survival::{DrinkError, Vitality, player_record_with_thermal_comfort, validate_drink};
use crate::world::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WorldGenerationSeed,
    set_generated_block_for_test, validate_generate_chunk, validate_generate_player_neighborhood,
};

#[test]
fn dry_natural_basin_remains_known_only_at_the_players_feet_after_water_is_spent() {
    use crate::fluid::{add_fluid_store, validate_transfer_fluid_between_local_stores};
    use crate::world::WORLD_MIN_Y;

    let registries = build_registries();
    let (mut state, basin, position, spawn) = (0..80_u64)
        .find_map(|number| {
            let mut state = AppState::new();
            validate_generate_chunk(
                &registries,
                &state,
                WorldGenerationSeed::new(number),
                ChunkCoord::new(0, 0),
                BLOCK_STATE_STONE,
            )
            .ok()?
            .commit(&mut state)
            .ok()?;
            let y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE - 1).find(|&y| {
                state.world().block_at(VoxelCoord::new(0, y, 0)) == Some(BlockStateId::AIR)
                    && state.world().block_at(VoxelCoord::new(0, y - 1, 0))
                        == Some(BLOCK_STATE_STONE)
            })?;
            let spawn = VoxelCoord::new(0, y, 0);
            validate_admit_player(&registries, &state, spawn)
                .ok()?
                .commit(&mut state)
                .ok()?;
            validate_generate_player_neighborhood(&registries, &state)
                .ok()??
                .commit(&mut state)
                .ok()?;
            let (basin, position) = state
                .fluid()
                .spring_soil()
                .next()
                .map(|(id, soil)| (id, soil.position))?;
            Some((state, basin, position, spawn))
        })
        .expect("some local generated seed must have a finite natural spring");

    assert_eq!(assess_player_local_natural_spring(&state), None);
    relocate_player_for_test(&mut state, position);
    let initial = state.fluid().get_store(basin).unwrap().stored_volume();
    assert!(!initial.is_zero());
    assert_eq!(assess_player_local_natural_spring(&state), Some(initial));

    let receiver = add_fluid_store(&mut state, initial).unwrap();
    validate_place_fluid_store(&state, receiver, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ =
        validate_transfer_fluid_between_local_stores(&registries, &state, basin, receiver, initial)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    assert_eq!(
        assess_player_local_natural_spring(&state),
        Some(Volume::ZERO)
    );
    assert!(
        !available_visible_generated_water(&registries, &state)
            .iter()
            .any(|source| source.store() == basin),
        "no drinkable water may be implied by the recognizable dry basin"
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(
        assess_player_local_natural_spring(&replay),
        Some(Volume::ZERO)
    );
    assert_eq!(validate_loaded_state(&registries, &replay), Ok(()));
    relocate_player_for_test(&mut state, spawn);
    assert_eq!(assess_player_local_natural_spring(&state), None);
}

#[test]
fn water_landmark_across_a_chunk_is_only_a_lit_sight_not_remote_drinking() {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x5741_5445_5253_4947);
    for chunk in [ChunkCoord::new(0, 0), ChunkCoord::new(1, 0)] {
        validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    let y = WORLD_MAX_Y_EXCLUSIVE - 4;
    let observer = VoxelCoord::new(2, y, 2);
    let visible = VoxelCoord::new(21, y, 2);
    let too_far = VoxelCoord::new(30, y, 2);
    for position in [observer, visible, too_far] {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(position.x(), y - 1, position.z()),
            BLOCK_STATE_STONE,
        );
        set_generated_block_for_test(registries.world(), &mut state, position, BlockStateId::AIR);
    }
    let create = |state: &mut AppState, position| {
        let quantity = Volume::from_microliters(500_000);
        let id = validate_generate_fluid_store(
            &registries,
            state,
            GeneratedFluidStoreSpec::new(
                quantity,
                FLUID_WATER,
                quantity,
                Temperature::from_millikelvin(293_150),
            ),
        )
        .unwrap()
        .commit(state)
        .unwrap();
        validate_place_fluid_store(state, id, position)
            .unwrap()
            .commit(state)
            .unwrap();
        id
    };
    let seen = create(&mut state, visible);
    let hidden = create(&mut state, too_far);
    validate_admit_player(&registries, &state, observer)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let observed = available_visible_water_landmarks(&registries, &state);
    assert_eq!(
        observed,
        vec![VisibleWaterSource {
            store: seen,
            position: visible
        }]
    );
    assert!(!observed.iter().any(|water| water.store() == hidden));
    assert!(matches!(
        validate_drink(&registries, &state, seen, Volume::from_microliters(250_000)),
        Err(DrinkError::Access(_))
    ));
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(10, y + 1, 2),
        BLOCK_STATE_STONE,
    );
    assert!(available_visible_water_landmarks(&registries, &state).is_empty());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn generated_water_sight_is_physical_even_when_the_water_is_not_drinkable() {
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
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(position.x(), y - 1, position.z()),
            BLOCK_STATE_STONE,
        );
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
    let remote_id = create(&mut state, remote, cool);
    let hot_id = create(&mut state, scalding, Temperature::from_millikelvin(345_150));
    let observed = |state: &AppState| {
        available_visible_generated_water(&registries, state)
            .into_iter()
            .map(|source| (source.store(), source.position()))
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
        crate::survival::available_adjacent_drink_sources(&registries, &state).count(),
        0,
        "the opening graph is not generated-world sight"
    );
    let expected = vec![(first_id, first), (second_id, second), (hot_id, scalding)];
    assert_eq!(
        observed(&state),
        expected,
        "hot water is still visible, but rock, distance, and missing graph edges do not disclose other sources"
    );
    let landmarks = |state: &AppState| {
        available_visible_water_landmarks(&registries, state)
            .into_iter()
            .map(|source| (source.store(), source.position()))
            .collect::<Vec<_>>()
    };
    assert_eq!(landmarks(&state), vec![(remote_id, remote)]);
    let mut obstructed = state.clone();
    for wall_y in [y, y + 1] {
        set_generated_block_for_test(
            registries.world(),
            &mut obstructed,
            VoxelCoord::new(5, wall_y, 2),
            BLOCK_STATE_STONE,
        );
    }
    assert!(landmarks(&obstructed).is_empty());
    assert!(matches!(
        validate_drink(
            &registries,
            &state,
            remote_id,
            Volume::from_microliters(250_000)
        ),
        Err(DrinkError::Access(_))
    ));
    // A waist-high view from the player's feet would incorrectly disclose
    // water behind an opaque overhang. The physical eye and the low passage
    // must both have an open line to the source.
    let mut overhung = state.clone();
    let lintel = VoxelCoord::new(second.x(), y + 1, second.z());
    set_generated_block_for_test(registries.world(), &mut overhung, lintel, BLOCK_STATE_STONE);
    assert!(
        crate::world::has_clear_two_step_sightline(&registries, &overhung, player, first),
        "ground-level geometry alone still has a clear passage"
    );
    assert!(
        !observed(&overhung).contains(&(first_id, first)),
        "the eye-height barrier hides water beyond the low passage"
    );
    assert!(
        observed(&overhung).contains(&(hot_id, scalding)),
        "unrelated visible water must remain disclosed"
    );
    set_generated_block_for_test(registries.world(), &mut overhung, lintel, BlockStateId::AIR);
    assert_eq!(observed(&overhung), expected);

    let before = state.clone();
    assert!(matches!(
        validate_drink(
            &registries,
            &state,
            first_id,
            Volume::from_microliters(250_000)
        ),
        Err(DrinkError::Access(_))
    ));
    assert_eq!(state, before, "sight cannot bypass exact-local custody");
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let mut local_hot = state.clone();
    relocate_player_for_test(&mut local_hot, scalding);
    assert!(matches!(
        validate_drink(
            &registries,
            &local_hot,
            hot_id,
            Volume::from_microliters(250_000)
        ),
        Err(DrinkError::TemperatureOutsideConsumptionRange { store, .. }) if store == hot_id
    ));
    assert_eq!(validate_loaded_state(&registries, &local_hot), Ok(()));

    let mut dead = state.clone();
    let living = dead.survival().player().copied().unwrap();
    let revision = dead.survival().revision();
    dead.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            living.metabolic_energy(),
            living.hydration(),
            Vitality::ZERO,
            living.nutrition(),
            living.vitality_recovery_remainder(),
            living.thermal_comfort_ppm(),
        ),
    );
    assert!(observed(&dead).is_empty());
    assert!(landmarks(&dead).is_empty());
    assert_eq!(validate_loaded_state(&registries, &dead), Ok(()));

    let day_ticks = registries.core().calendar().ticks_per_day();
    apply_clock_advance(&mut state, SimulationTick::new(day_ticks * 3 / 4));
    assert!(
        observed(&state).is_empty(),
        "unlit water cannot be seen at night"
    );
    assert!(landmarks(&state).is_empty());
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);
    assert!(observed(&restored).is_empty());
    apply_clock_advance(&mut state, SimulationTick::new(day_ticks));
    assert_eq!(observed(&state), expected);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
