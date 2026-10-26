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
    assert_eq!(
        index.column_candidates(0, 1).collect::<Vec<_>>(),
        [1, 2, 3].map(StructuralElementId::new)
    );
    assert_eq!(
        index.column_candidates(-2, 0).collect::<Vec<_>>(),
        [StructuralElementId::new(1)]
    );
    assert_eq!(
        index.column_candidates(i64::MAX - 1, 1).collect::<Vec<_>>(),
        [StructuralElementId::new(3)],
        "large prisms remain discoverable by column without expanding every voxel"
    );
    assert!(index.column_candidates(0, 5).next().is_none());

    index.remove(StructuralElementId::new(1), small);
    index.remove(StructuralElementId::new(2), overlapping);
    index.remove(StructuralElementId::new(3), broad);
    assert!(index.by_voxel.is_empty());
    assert!(index.by_column.is_empty());
    assert!(index.broad_elements.is_empty());
}

#[test]
fn horizontal_rectangle_query_deduplicates_and_preserves_member_id_priority() {
    let mut index = ElementSpatialIndex::new();
    let broad = VoxelBounds::new(
        VoxelCoord::new(i64::MIN, -1, 1),
        VoxelCoord::new(i64::MAX, 2, 2),
    )
    .unwrap();
    let local = VoxelBounds::new(VoxelCoord::new(1, 0, 1), VoxelCoord::new(3, 2, 3)).unwrap();
    let remote = VoxelBounds::new(VoxelCoord::new(30, 0, 1), VoxelCoord::new(31, 1, 2)).unwrap();
    index.insert(StructuralElementId::new(50), local);
    index.insert(StructuralElementId::new(2), broad);
    index.insert(StructuralElementId::new(1), remote);
    index.insert(
        StructuralElementId::new(4),
        VoxelBounds::new(VoxelCoord::new(2, 0, 1), VoxelCoord::new(3, 1, 2)).unwrap(),
    );
    assert_eq!(
        index
            .candidates_in_columns(0, 15, 0, 15)
            .collect::<Vec<_>>(),
        [2, 4, 50].map(StructuralElementId::new),
        "a panel spanning multiple columns must appear once, sorted with broad geometry"
    );
    assert_eq!(
        index
            .candidates_in_columns(16, 31, 0, 15)
            .collect::<Vec<_>>(),
        [1, 2].map(StructuralElementId::new)
    );
    assert!(index.candidates_in_columns(0, 15, 3, 15).next().is_none());
    assert_eq!(
        index
            .candidates_in_columns(i64::MAX - 16, i64::MAX, 1, 1)
            .collect::<Vec<_>>(),
        [StructuralElementId::new(2)],
        "the broad fallback remains exact at coordinate extremes"
    );
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
    assert!(
        !state
            .structures()
            .has_active_overhead_cover(near.x(), near.z(), -1, 2),
        "an indexed plan without physical activation cannot shelter an actor"
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
    assert!(
        state
            .structures()
            .spatial_index
            .column_candidates(near.x(), near.z())
            .next()
            .is_none(),
        "removing an element must remove its horizontal projection"
    );
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
