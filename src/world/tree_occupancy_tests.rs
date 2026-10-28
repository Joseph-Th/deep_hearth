//! Tree trunks and built structural members cannot share physical space.

use super::*;
use crate::content::{
    BASIC_TIMBER_POST_CROSS_SECTION, BASIC_TIMBER_POST_LENGTH, BLOCK_STATE_STONE, FORM_LOG,
    MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
};

fn materialize_narrow_post(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    voxel: VoxelCoord,
) {
    let bounds = VoxelBounds::new(
        voxel,
        VoxelCoord::new(voxel.x() + 1, voxel.y() + 1, voxel.z() + 1),
    )
    .unwrap();
    let geometry = StructuralElementGeometry::new(
        bounds,
        BASIC_TIMBER_POST_LENGTH,
        BASIC_TIMBER_POST_CROSS_SECTION,
    )
    .unwrap();
    assert!(!geometry.covers_horizontal_footprint());
    let post = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(registries, state, post, FORM_LOG);
    assert!(state.structures().has_embodied_member_at(voxel));
    assert!(!state.structures().blocks_full_cell(voxel));
}

fn juvenile_with_neighbor() -> (
    crate::registry::Registries,
    AppState,
    VoxelCoord,
    VoxelCoord,
) {
    let registries = build_registries();
    let (seed, from) = (1..10_000)
        .find_map(|value| {
            let seed = WorldGenerationSeed::new(value);
            let profile = WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
            let tree = trees::seeded_tree_identity(profile, ChunkCoord::new(0, 0))?;
            let from = tree.root();
            let far_x = from.x().checked_sub(2)?;
            (tree.initial_wood().milligrams() == 8_000_000
                && generation::terrain_height(seed, from.x() - 1, from.z()) == from.y() - 1
                && generation::terrain_height(seed, far_x, from.z()) == from.y() - 1)
                .then_some((seed, from))
        })
        .expect("a juvenile with supported, adjacent open ground");
    let to = VoxelCoord::new(from.x() - 2, from.y(), from.z());
    let mut state = AppState::new();
    validate_admit_player(
        &registries,
        &state,
        VoxelCoord::new(from.x() - 1, from.y(), from.z()),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
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
    assert!(state.world().has_standing_clearance(registries.world(), to));
    (registries, state, from, to)
}

#[test]
fn generated_tree_cannot_appear_through_an_existing_narrow_post() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(7);
    let profile = WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
    let tree = trees::seeded_tree_identity(profile, ChunkCoord::new(0, 0)).unwrap();
    for voxel in [
        tree.root(),
        VoxelCoord::new(tree.root().x(), tree.root().y() + 1, tree.root().z()),
    ] {
        let mut state = AppState::new();
        materialize_narrow_post(&registries, &mut state, voxel);
        let before = state.clone();
        assert_eq!(
            validate_generate_chunk(
                &registries,
                &state,
                seed,
                ChunkCoord::new(0, 0),
                BLOCK_STATE_STONE,
            )
            .err(),
            Some(ChunkGenerationError::TreePlacementObstructed { voxel })
        );
        assert_eq!(state, before);
    }
}

#[test]
fn juvenile_transplant_cannot_overlap_built_post_in_either_trunk_cell() {
    for head in [false, true] {
        let (registries, mut state, from, to) = juvenile_with_neighbor();
        let voxel = VoxelCoord::new(to.x(), to.y() + i64::from(head), to.z());
        materialize_narrow_post(&registries, &mut state, voxel);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let before = state.clone();
        assert!(matches!(
            validate_transplant_juvenile(&registries, &state, from, to),
            Err(SaplingTransplantError::DestinationBlocked { root }) if root == to
        ));
        assert_eq!(state, before);
    }
}

#[test]
fn trusted_load_rejects_a_transplanted_tree_forged_through_a_narrow_post() {
    let (registries, mut state, from, to) = juvenile_with_neighbor();
    materialize_narrow_post(&registries, &mut state, to);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    // Forge the world history, not the structural owner. World-only validation
    // accepts the otherwise plausible seed-backed relocation; cross-owner
    // admission must additionally reject the physical overlap.
    let mut value = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let world = &mut value["state"]["systems"]["world"];
    world["trees"][0]["root"] = serde_json::to_value(to).unwrap();
    world["transplants"] = serde_json::json!([{
        "origin": from,
        "from": from,
        "to": to,
        "transplanted_at": state.tick(),
    }]);
    let loaded = serde_json::from_value::<LoadedSaveEnvelope>(value)
        .unwrap()
        .into_state(&registries);
    assert!(matches!(
        loaded,
        Err(crate::persistence::LoadError::InvalidState(
            StateValidationError::World(WorldValidationError::InvalidStandingTree { root })
        )) if root == to
    ));
}
