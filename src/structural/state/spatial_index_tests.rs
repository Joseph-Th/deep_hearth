//! Local structural-index membership, overlap, and bounded broad-geometry coverage.

use super::*;

#[test]
fn small_overlapping_and_oversized_bounds_have_exact_local_candidates() {
    let small = VoxelBounds::new(VoxelCoord::new(-2, -1, 0), VoxelCoord::new(1, 2, 2)).unwrap();
    let overlapping = VoxelBounds::new(VoxelCoord::new(0, 0, 1), VoxelCoord::new(1, 1, 2)).unwrap();
    let broad = VoxelBounds::new(
        VoxelCoord::new(i64::MIN, 0, 1),
        VoxelCoord::new(i64::MAX, 1, 2),
    )
    .unwrap();
    let mut index = ElementSpatialIndex::new();
    index.insert(StructuralElementId::new(1), small);
    index.insert(StructuralElementId::new(2), overlapping);
    index.insert(StructuralElementId::new(3), broad);

    assert_eq!(
        index
            .candidates(VoxelCoord::new(0, 0, 1))
            .collect::<Vec<_>>(),
        [1, 2, 3].map(StructuralElementId::new)
    );
    assert_eq!(
        index
            .candidates(VoxelCoord::new(-2, -1, 0))
            .collect::<Vec<_>>(),
        [StructuralElementId::new(1)]
    );
    assert_eq!(
        index
            .candidates(VoxelCoord::new(5, 0, 1))
            .collect::<Vec<_>>(),
        [StructuralElementId::new(3)]
    );
    assert!(index.candidates(VoxelCoord::new(5, 4, 6)).next().is_none());

    index.remove(StructuralElementId::new(1), small);
    index.remove(StructuralElementId::new(2), overlapping);
    index.remove(StructuralElementId::new(3), broad);
    assert!(index.by_voxel.is_empty());
    assert!(index.broad_elements.is_empty());
}

#[test]
fn structural_creation_removal_and_trusted_load_keep_local_occupancy_in_sync() {
    use crate::content::{MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries};
    use crate::core::quantity::{Area, Length};
    use crate::core::state::AppState;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::structural::{
        StructuralElementGeometry, StructuralElementRecord, add_structural_element,
        validate_remove_structural_element,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let near = VoxelCoord::new(3, 0, -2);
    let remote = VoxelCoord::new(4, 0, -2);
    let geometry = |at: VoxelCoord| {
        StructuralElementGeometry::new(
            VoxelBounds::new(at, VoxelCoord::new(at.x() + 1, at.y() + 1, at.z() + 1)).unwrap(),
            Length::from_micrometers(1_000_000),
            Area::from_square_millimeters(5_000),
        )
        .unwrap()
    };
    let first = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry(near),
        true,
    )
    .unwrap();
    let second = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry(remote),
        true,
    )
    .unwrap();
    assert_eq!(
        state
            .structures()
            .elements_covering(near)
            .map(StructuralElementRecord::id)
            .collect::<Vec<_>>(),
        vec![first]
    );
    assert_eq!(
        state
            .structures()
            .elements_covering(remote)
            .map(StructuralElementRecord::id)
            .collect::<Vec<_>>(),
        vec![second]
    );
    let serialized = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(
        loaded, state,
        "derived structure indexes rebuild identically"
    );
    let _ = validate_remove_structural_element(&registries, &state, first)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(state.structures().elements_covering(near).next().is_none());
    assert_eq!(
        state
            .structures()
            .elements_covering(remote)
            .map(StructuralElementRecord::id)
            .collect::<Vec<_>>(),
        vec![second],
        "removing one member cannot discard a different voxel's occupancy"
    );
}
