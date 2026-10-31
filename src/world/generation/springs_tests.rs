//! Actual nearby water, not a label, infinite refill, or remote hydration oracle.

use super::*;

use crate::content::{FLUID_WATER, build_registries};
use crate::core::state::{AppState, validate_loaded_state};
use crate::fluid::calculate_fluid_volume_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick;
use crate::survival::{available_local_drink_sources, validate_drink};
use crate::world::{
    ChunkCoord, available_visible_generated_water, validate_generate_chunk,
    validate_generate_player_neighborhood,
};

fn generated_water_neighborhood() -> (crate::registry::Registries, AppState, VoxelCoord) {
    let registries = build_registries();
    for number in 0..80_u64 {
        let seed = super::super::WorldGenerationSeed::new(number);
        let mut state = AppState::new();
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        let spawn = VoxelCoord::new(0, terrain_height(seed, 0, 0) + 1, 0);
        validate_admit_player(&registries, &state, spawn)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        let expansion = validate_generate_player_neighborhood(&registries, &state)
            .unwrap()
            .expect("unseen adjacent columns must be generated");
        let duplicate = expansion.clone();
        expansion.commit(&mut state).unwrap();
        let source = state
            .logistics()
            .fluid_store_locations()
            .find_map(|(store, position)| {
                state.fluid().get_store(store).and_then(|record| {
                    record
                        .contents()
                        .is_some_and(|water| water.fluid() == FLUID_WATER)
                        .then_some(position)
                })
            });
        if let Some(position) = source {
            let before = state.clone();
            assert!(matches!(
                duplicate.commit(&mut state),
                Err(super::super::ChunkGenerationCommitError::StaleWorldRevision { .. })
            ));
            assert_eq!(
                state, before,
                "a repeated expansion cannot refill a wet district"
            );
            return (registries, state, position);
        }
    }
    panic!("a new generated wilderness must have distinct finite water sources");
}

#[test]
fn regional_spring_is_real_finite_water_and_drinking_preserves_total_volume() {
    let (registries, mut state, position) = generated_water_neighborhood();
    let store = state
        .logistics()
        .fluid_stores_at(position)
        .next()
        .expect("natural water is located in logistics");
    let original = state.fluid().get_store(store).unwrap().stored_volume();
    assert!(!original.is_zero());
    assert!(state.world().has_generated_terrain_at(position));
    assert_eq!(state.world().block_at(position), Some(BlockStateId::AIR));
    assert_eq!(
        state.world().block_at(VoxelCoord::new(
            position.x(),
            position.y() - 1,
            position.z()
        )),
        Some(BLOCK_STATE_STONE)
    );
    assert!(
        matches!(
            habitat_at(
                state.world().generation_profile().unwrap().seed(),
                position.x(),
                position.z()
            ),
            HabitatKind::DampGrove | HabitatKind::MixedWoodland
        ),
        "dry scrub must not spontaneously provide surface water"
    );
    crate::logistics::relocate_player_for_test(&mut state, position);
    assert_eq!(
        available_visible_generated_water(&registries, &state)
            .into_iter()
            .map(|source| (source.store(), source.position()))
            .collect::<Vec<_>>(),
        vec![(store, position)]
    );
    assert!(
        available_local_drink_sources(&registries, &state).any(|source| source.store() == store)
    );
    let initial = calculate_fluid_volume_accounting(&state).unwrap();
    let amount = registries
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let finished = validate_drink(&registries, &state, store, amount)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..(72 * 20) {
        if state.player_work().active().is_none() {
            break;
        }
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert!(
        state.player_work().active().is_none(),
        "a real drink must finish"
    );
    assert_eq!(
        state.fluid().get_store(store).unwrap().stored_volume(),
        original.checked_sub(finished.volume()).unwrap()
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        initial,
        "every consumed microliter transfers to survival custody"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    // The spring can lie on the edge of the generated neighborhood. Ask
    // for the original footprint, not previously unexplored outer chunks.
    let seed = state.world().generation_profile().unwrap().seed();
    crate::logistics::relocate_player_for_test(
        &mut state,
        VoxelCoord::new(0, terrain_height(seed, 0, 0) + 1, 0),
    );
    assert_eq!(
        validate_generate_player_neighborhood(&registries, &state)
            .unwrap()
            .is_none(),
        true,
        "revisiting generated land cannot refill its water"
    );
}

#[test]
fn water_is_not_a_remote_resource_or_visible_through_rock() {
    let (registries, mut state, position) = generated_water_neighborhood();
    let store = state.logistics().fluid_stores_at(position).next().unwrap();
    crate::logistics::relocate_player_for_test(&mut state, position);
    assert!(
        available_visible_generated_water(&registries, &state)
            .iter()
            .any(|source| source.store() == store)
    );
    let delta = if position.x().rem_euclid(16) <= 11 {
        1
    } else {
        -1
    };
    let observer = VoxelCoord::new(position.x() + 2 * delta, position.y(), position.z());
    let wall = VoxelCoord::new(position.x() + delta, position.y(), position.z());
    for voxel in [
        observer,
        wall,
        VoxelCoord::new(observer.x(), observer.y() + 1, observer.z()),
    ] {
        crate::world::set_generated_block_for_test(
            registries.world(),
            &mut state,
            voxel,
            BlockStateId::AIR,
        );
    }
    crate::logistics::relocate_player_for_test(&mut state, observer);
    assert!(
        available_visible_generated_water(&registries, &state)
            .iter()
            .any(|source| source.store() == store)
    );
    crate::world::set_generated_block_for_test(
        registries.world(),
        &mut state,
        wall,
        BLOCK_STATE_STONE,
    );
    assert!(
        available_visible_generated_water(&registries, &state)
            .iter()
            .all(|source| source.store() != store),
        "a solid intervening rock blocks discovery without deleting water"
    );
}

#[test]
fn competing_fluid_allocation_cannot_partially_commit_a_spring_neighborhood() {
    let (registries, grown, _) = generated_water_neighborhood();
    let seed = grown.world().generation_profile().unwrap().seed();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    validate_admit_player(
        &registries,
        &state,
        VoxelCoord::new(0, terrain_height(seed, 0, 0) + 1, 0),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let pending = validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .unwrap();
    crate::fluid::validate_generate_fluid_store(
        &registries,
        &state,
        GeneratedFluidStoreSpec::new(
            Volume::from_microliters(200_000),
            FLUID_WATER,
            Volume::from_microliters(200_000),
            Temperature::from_millikelvin(293_150),
        ),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let before = state.clone();
    assert!(matches!(
        pending.commit(&mut state),
        Err(super::super::ChunkGenerationCommitError::StaleFluidRevision { .. })
    ));
    assert_eq!(
        state, before,
        "stale fluid custody must block every planned chunk and source"
    );
    assert_eq!(state.world().chunks().len(), 1);
}
