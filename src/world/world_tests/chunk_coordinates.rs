//! Trusted-load rejection of chunk identities that cannot cover any physical voxels.

use super::*;

#[test]
fn trusted_load_rejects_unrepresentable_chunk_coordinates() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    // Tree absence is a legal world state after an obstructed generation site.
    // Keep the forged chunk as the only changed physical identity.
    encoded["state"]["systems"]["world"]["trees"] = serde_json::json!([]);

    for invalid in [
        ChunkCoord::new(i64::MAX, 0),
        ChunkCoord::new(i64::MIN, 0),
        ChunkCoord::new(0, i64::MAX),
        ChunkCoord::new(0, i64::MIN),
    ] {
        let mut forged = encoded.clone();
        forged["state"]["systems"]["world"]["chunks"][0]["coord"] =
            serde_json::to_value(invalid).unwrap();
        let decoded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
        assert_eq!(
            decoded.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::World(
                WorldValidationError::ChunkCoordinateOverflow { chunk: invalid }
            ))),
            "unreachable chunk {invalid:?} must not be trusted"
        );
    }
}

#[test]
fn generated_chunk_at_extreme_representable_coordinates_survives_trusted_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let boundary = ChunkCoord::new(
        i64::MIN.div_euclid(CHUNK_SIDE as i64),
        i64::MAX.div_euclid(CHUNK_SIDE as i64),
    );
    validate_generate_chunk(&registries, &state, SEED, boundary, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded.into_state(&registries), Ok(state));
}
