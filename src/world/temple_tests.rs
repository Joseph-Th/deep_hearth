//! Generated deep sanctuaries are real excavatable geometry and hold visible heavy castes.

use super::*;

use crate::content::{
    BLOCK_STATE_STONE, BLOCK_STATE_TEMPLE_ARCHIVE, BLOCK_STATE_TEMPLE_DRIVE, FORM_SCRAP,
    MATERIAL_COPPER, build_registries,
};
use crate::core::state::{AppState, validate_loaded_state};
use crate::kobolds::{KoboldRole, KoboldStature, available_nearby_kobold_scouts};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;

#[test]
fn seeded_temple_is_buried_carved_and_matter_backed_with_two_ancient_residents() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5445_4D50_4C45_53);
    let chunk = ChunkCoord::new(0, 0);
    let mut state = AppState::new();
    let site = generation::temple::TempleSite::for_chunk(seed, chunk).unwrap();
    let guardian = site.warden();
    let archivist = site.archivist();
    let center = VoxelCoord::new(guardian.x(), guardian.y(), guardian.z() - 2);

    validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(state.world().block_at(guardian), Some(BlockStateId::AIR));
    assert_eq!(state.world().block_at(archivist), Some(BlockStateId::AIR));
    assert_eq!(
        state
            .world()
            .block_at(VoxelCoord::new(center.x() - 2, center.y(), center.z())),
        Some(BLOCK_STATE_TEMPLE_ARCHIVE)
    );
    assert_eq!(
        state
            .world()
            .block_at(VoxelCoord::new(center.x() + 2, center.y(), center.z())),
        Some(BLOCK_STATE_TEMPLE_DRIVE)
    );
    assert_eq!(
        state
            .world()
            .block_at(VoxelCoord::new(center.x(), center.y() - 1, center.z())),
        Some(BLOCK_STATE_STONE),
        "the temple retains a supporting floor"
    );
    assert_eq!(
        state
            .world()
            .block_at(VoxelCoord::new(center.x(), center.y() + 3, center.z())),
        Some(BLOCK_STATE_STONE),
        "the temple starts sealed under native rock"
    );
    let scrap = CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP);
    for (block, fill) in [
        (BLOCK_STATE_TEMPLE_ARCHIVE, 1_000_u32),
        (BLOCK_STATE_TEMPLE_DRIVE, 2_000_u32),
    ] {
        let definition = registries.world().block_for_state(block).unwrap();
        assert_eq!(
            definition.excavation().unwrap().fragmented_commodity(),
            scrap
        );
        assert_eq!(definition.material_fill_fraction_ppm(), fill);
        assert_eq!(
            registries
                .world()
                .represented_mass_for_state(block)
                .unwrap()
                .milligrams(),
            u64::from(fill) * 8_960
        );
    }
    assert_eq!(
        temple_residents_at(&registries, &state, VoxelCoord::new(200, center.y(), 200)),
        None,
        "unvisited regions do not disclose a temple or its inhabitants"
    );
    validate_admit_player(&registries, &state, center)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let sightings = available_nearby_kobold_scouts(&registries, &state).collect::<Vec<_>>();
    assert_eq!(sightings.len(), 2);
    assert!(sightings.iter().any(|seen| {
        seen.role() == KoboldRole::TempleWarden
            && seen.position() == guardian
            && seen.stature() == KoboldStature::Heavy
    }));
    assert!(sightings.iter().any(|seen| {
        seen.role() == KoboldRole::TempleArchivist
            && seen.position() == archivist
            && seen.stature() == KoboldStature::Massive
    }));

    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
    assert_eq!(
        calculate_matter_accounting(&loaded).unwrap().total(),
        conserved
    );
}

#[test]
fn temple_sites_are_district_bounded_and_seed_dependent() {
    let a = generation::temple::TempleSite::for_chunk(
        WorldGenerationSeed::new(1),
        ChunkCoord::new(0, 0),
    )
    .unwrap();
    let b = generation::temple::TempleSite::for_chunk(
        WorldGenerationSeed::new(2),
        ChunkCoord::new(0, 0),
    )
    .unwrap();
    assert_ne!(a.warden(), b.warden());
    assert!(
        generation::temple::TempleSite::for_chunk(
            WorldGenerationSeed::new(1),
            ChunkCoord::new(1, 0),
        )
        .is_none()
    );
}
