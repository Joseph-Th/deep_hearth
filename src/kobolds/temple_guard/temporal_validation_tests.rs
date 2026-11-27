//! Impossible temple guardian displacements must not become trusted persistent actors.

use super::*;

use crate::content::build_registries;
use crate::core::state::{StateValidationError, apply_clock_advance, validate_loaded_state};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::world::{WorldGenerationSeed, validate_generate_chunk};

fn quiet_temple() -> (Registries, AppState, ChunkCoord) {
    let registries = build_registries();
    let mut state = AppState::new();
    let chunk = ChunkCoord::new(0, 0);
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x0054_454D_504C_4553),
        chunk,
        crate::content::BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let origin = VoxelCoord::new(
        chunk.world_min_x().unwrap() + 8,
        -19,
        chunk.world_min_z().unwrap() + 8,
    );
    let route = temple_warden_route_at(&registries, &state, origin).unwrap();
    validate_admit_player(&registries, &state, route[2])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    (registries, state, chunk)
}

#[test]
fn implicit_temple_resident_bodies_cannot_be_buried_in_trusted_saves() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::persistence::LoadError;
    use crate::world::{set_generated_block_for_test, temple_residents_at};

    let (registries, state, chunk) = quiet_temple();
    let sample = VoxelCoord::new(
        chunk.world_min_x().unwrap() + 8,
        -19,
        chunk.world_min_z().unwrap() + 8,
    );
    let [warden, archivist] = temple_residents_at(&registries, &state, sample).unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    for (cell, expected) in [
        (
            warden,
            "kobold temple warden occupies obstructed or absent world geometry",
        ),
        (
            VoxelCoord::new(warden.x(), warden.y() + 1, warden.z()),
            "kobold temple warden occupies obstructed or absent world geometry",
        ),
        (
            archivist,
            "temple archivist is embedded in missing or solid world",
        ),
        (
            VoxelCoord::new(archivist.x() + 1, archivist.y() + 2, archivist.z() - 1),
            "temple archivist is embedded in missing or solid world",
        ),
    ] {
        let mut forged = state.clone();
        assert_eq!(forged.world().block_at(cell), Some(BlockStateId::AIR));
        set_generated_block_for_test(registries.world(), &mut forged, cell, BLOCK_STATE_STONE);
        assert!(forged.kobold_state().temple_wardens.is_empty());
        assert!(forged.kobold_state().temple_archivists.is_empty());
        let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &forged)).unwrap();
        let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            decoded.into_state(&registries).err(),
            Some(LoadError::InvalidState(StateValidationError::Kobolds(
                expected
            ))),
            "implicit resident body cell {cell:?} cannot intersect solid terrain"
        );
    }

    // Deliberately removing only the supporting floor is a different state:
    // the resident remains in its air-valued body cells but is stranded.
    let mut stranded = state.clone();
    for anchor in [warden, archivist] {
        let floor = VoxelCoord::new(anchor.x(), anchor.y() - 1, anchor.z());
        set_generated_block_for_test(registries.world(), &mut stranded, floor, BlockStateId::AIR);
    }
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &stranded)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
            .unwrap()
            .into_state(&registries),
        Ok(stranded)
    );
}

#[test]
fn saved_warden_displacement_requires_real_elapsed_patrol_beats() {
    let (registries, state, chunk) = quiet_temple();
    for (tick, step) in [(0, 1), (0, 2), (SCOUT_STEP_TICKS, 2)] {
        let mut forged = state.clone();
        if tick != 0 {
            apply_clock_advance(&mut forged, SimulationTick::new(tick));
        }
        forged
            .kobold_state_mut()
            .temple_wardens
            .push(TempleWardenRecord { chunk, step });
        assert_eq!(
            validate_warden_cursors(&registries, &forged),
            Err("kobold temple warden precedes its first physical hall step"),
            "cursor {step} at tick {tick} cannot have walked the required hall edges"
        );
        let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &forged)).unwrap();
        assert!(
            serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
                .unwrap()
                .into_state(&registries)
                .is_err(),
            "trusted load must reject an impossible hall displacement"
        );
    }
}
