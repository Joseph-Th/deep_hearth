//! Indexed support lookup keeps direct construction local and excludes inert plans.

use super::{
    DirectPlacementTargetError, local_active_face_supports, single_voxel_bounds,
    validate_direct_placement_position,
};
use crate::content::{
    FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Length};
use crate::core::state::{AppState, validate_loaded_state};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementGeometry, StructuralElementId, add_structural_element,
    materialize_structural_element_for_test, validate_activate_structural_element,
};
use crate::world::{BlockStateId, ChunkCoord, WorldGenerationSeed, validate_generate_chunk};

#[test]
fn pointed_placement_rejects_both_physical_tree_trunk_cells() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(41),
        ChunkCoord::new(0, 0),
        crate::content::BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let root = state.world().trees().next().unwrap().root();
    for voxel in [root, VoxelCoord::new(root.x(), root.y() + 1, root.z())] {
        assert_eq!(state.world().block_at(voxel), Some(BlockStateId::AIR));
        assert_eq!(
            validate_direct_placement_position(&registries, &state, voxel),
            Err(DirectPlacementTargetError::LocationOccupied { position: voxel }),
        );
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn supported_timber_uses_only_live_face_neighbors_after_trusted_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let target = VoxelCoord::new(3, 2, 0);
    let positions = [
        (VoxelCoord::new(2, 2, 0), true),
        (VoxelCoord::new(3, 2, 1), true),
        (VoxelCoord::new(3, 2, -1), false),
        (VoxelCoord::new(30, 2, 0), true),
    ];
    let mut elements = Vec::<StructuralElementId>::new();
    for (position, active) in positions {
        let bounds = single_voxel_bounds(position).unwrap();
        let geometry = StructuralElementGeometry::new(
            bounds,
            Length::from_micrometers(1_000_000),
            Area::from_square_millimeters(5_000),
        )
        .unwrap();
        let element = add_structural_element(
            &registries,
            &mut state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            geometry,
            true,
        )
        .unwrap();
        if active {
            materialize_structural_element_for_test(&registries, &mut state, element, FORM_LOG);
            let _ = validate_activate_structural_element(&registries, &state, element)
                .unwrap()
                .commit(&mut state)
                .unwrap();
        }
        elements.push(element);
    }
    let target_bounds = VoxelBounds::new(target, VoxelCoord::new(4, 3, 1)).unwrap();
    assert_eq!(
        local_active_face_supports(&state, target, target_bounds),
        vec![elements[0], elements[1]],
        "only two distinct active face contacts can support the placement"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    assert_eq!(
        local_active_face_supports(&loaded, target, target_bounds),
        vec![elements[0], elements[1]],
        "the rebuilt spatial index must preserve support selection"
    );
}
