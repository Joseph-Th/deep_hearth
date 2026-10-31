//! Natural pools change without player action while retaining exact water custody.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::{AppState, validate_loaded_state};
use crate::fluid::calculate_fluid_volume_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::world::{
    ChunkCoord, WorldGenerationSeed, validate_generate_chunk, validate_generate_player_neighborhood,
};

fn actual_natural_pool() -> (Registries, AppState, super::super::FluidStoreId, VoxelCoord) {
    let registries = build_registries();
    for seed_number in 0..80_u64 {
        let seed = WorldGenerationSeed::new(seed_number);
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
        let surface = (crate::world::WORLD_MIN_Y..crate::world::WORLD_MAX_Y_EXCLUSIVE - 1)
            .find(|y| {
                state.world().block_at(VoxelCoord::new(0, *y, 0))
                    == Some(crate::world::BlockStateId::AIR)
                    && state.world().block_at(VoxelCoord::new(0, *y - 1, 0))
                        == Some(BLOCK_STATE_STONE)
            })
            .unwrap();
        let spawn = VoxelCoord::new(0, surface, 0);
        validate_admit_player(&registries, &state, spawn)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        validate_generate_player_neighborhood(&registries, &state)
            .unwrap()
            .unwrap()
            .commit(&mut state)
            .unwrap();
        let candidate = state.fluid().spring_soil().next();
        if let Some((id, soil)) = candidate {
            return (registries, state, id, soil.position);
        }
    }
    panic!("sparse local wilderness never generated a natural water pocket");
}

#[test]
fn seasonal_fronts_change_actual_pool_volume_without_changing_world_water() {
    let (registries, mut state, id, _) = actual_natural_pool();
    let baseline = calculate_fluid_volume_accounting(&state).unwrap();
    let original = state.fluid().get_store(id).unwrap().stored_volume();
    let mut saw_loss = false;
    let mut saw_return = false;
    let mut previous = original;
    for _ in 0..6_000 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let visible = state.fluid().get_store(id).unwrap().stored_volume();
        if visible < previous {
            saw_loss = true;
        } else if visible > previous {
            saw_return = true;
        }
        previous = visible;
        if saw_loss && saw_return {
            break;
        }
    }
    assert!(
        saw_loss,
        "dry fronts should move actual surface water into the catchment"
    );
    assert!(
        saw_return,
        "cool calm fronts should return previously retained water"
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        baseline,
        "surface-to-soil exchange must not create or delete water"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let serialized = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..440 {
        let _ = advance_tick(&registries, &mut loaded).unwrap();
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(loaded, state);
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), baseline);
}

#[test]
fn tampered_catchment_coordinates_and_impossible_storage_are_rejected() {
    let (registries, state, id, _) = actual_natural_pool();
    let save = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let key = id.value().to_string();
    let mut misplaced = save.clone();
    misplaced["state"]["systems"]["fluid"]["spring_soil"][&key]["position"] =
        serde_json::to_value(VoxelCoord::new(900, 4, 900)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_value(misplaced).unwrap();
    assert!(decoded.into_state(&registries).is_err());

    let mut overfilled = save;
    overfilled["state"]["systems"]["fluid"]["spring_soil"][&key]["soaked"] =
        serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(overfilled).unwrap();
    assert!(decoded.into_state(&registries).is_err());
}

#[test]
fn a_built_roof_stops_weather_exchange_without_erasing_the_water() {
    let (registries, mut state, id, position) = actual_natural_pool();
    let roof = VoxelCoord::new(position.x(), position.y() + 3, position.z());
    assert_eq!(
        state.world().block_at(roof),
        Some(crate::world::BlockStateId::AIR),
        "the generated pool needs a genuine sky-facing column"
    );
    crate::world::set_generated_block_for_test(
        registries.world(),
        &mut state,
        roof,
        BLOCK_STATE_STONE,
    );
    let standing = state.fluid().get_store(id).unwrap().stored_volume();
    let total = calculate_fluid_volume_accounting(&state).unwrap();
    for _ in 0..1_800 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.fluid().get_store(id).unwrap().stored_volume(),
        standing
    );
    assert_eq!(
        state.fluid().spring_soil.get(&id).unwrap().soaked,
        Volume::ZERO
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn natural_pool_is_ground_bound_not_an_attachable_vessel() {
    use crate::fluid::{FluidSupportError, validate_mount_fluid_store};
    use crate::structural::StructuralElementId;

    let (registries, mut state, id, position) = actual_natural_pool();
    crate::logistics::relocate_player_for_test(&mut state, position);
    let original = state.clone();
    assert_eq!(
        validate_mount_fluid_store(&registries, &state, id, StructuralElementId::new(1),).err(),
        Some(FluidSupportError::NaturalPoolIsNotAVessel { store: id }),
    );
    assert_eq!(state, original);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
