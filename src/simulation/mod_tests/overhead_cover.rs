//! Generated-sky boundary for physical overhead-cover accounting.

use super::*;
use crate::content::STRUCTURAL_PROFILE_AXIAL_COMPRESSION;
use crate::core::quantity::{Area, Length};
use crate::core::state::validate_loaded_state;
use crate::spatial::VoxelBounds;
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
    validate_activate_structural_element,
};

#[test]
fn a_structure_beyond_generated_sky_cannot_provide_overhead_cover() {
    let (registries, mut state, _) = generated_grounded_player_at(0, 0);
    let sky_above = VoxelCoord::new(0, WORLD_MAX_Y_EXCLUSIVE + 1, 0);
    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(sky_above, VoxelCoord::new(1, WORLD_MAX_Y_EXCLUSIVE + 2, 1)).unwrap(),
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(1_000_000),
    )
    .unwrap();
    let roof = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, roof, FORM_LOG);
    let _ = validate_activate_structural_element(&registries, &state, roof)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Open),
        "structure entirely above generated sky must not shelter the player"
    );
}

#[test]
fn indexed_overhead_cover_requires_active_local_roof_and_survives_save_load() {
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let roof_cell = VoxelCoord::new(0, floor_y + 3, 0);
    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(roof_cell, VoxelCoord::new(1, roof_cell.y() + 1, 1)).unwrap(),
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(1_000_000),
    )
    .unwrap();
    let roof = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Open),
        "an indexed plan is not shelter"
    );
    materialize_structural_element_for_test(&registries, &mut state, roof, FORM_LOG);
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Open),
        "embodiment alone does not make an unfinished roof active"
    );
    let _ = validate_activate_structural_element(&registries, &state, roof)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Covered)
    );
    assert_eq!(
        state
            .structures()
            .has_active_overhead_cover(1, 0, floor_y + 2, WORLD_MAX_Y_EXCLUSIVE),
        false,
        "adjacent columns cannot inherit a neighboring roof"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(
        loaded, state,
        "horizontal index must rebuild on trusted load"
    );
    assert_eq!(
        assess_player_overhead_cover(&registries, &loaded),
        Some(PlayerOverheadCoverAssessment::Covered)
    );
}
