//! Impossible temple guardian displacements must not become trusted persistent actors.

use super::*;

use crate::content::build_registries;
use crate::core::state::apply_clock_advance;
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
            validate_temple_wardens(&registries, &forged),
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
