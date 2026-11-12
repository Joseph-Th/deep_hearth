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
fn natural_overhead_cover_tracks_high_generated_roofs_and_excavation() {
    use crate::world::set_generated_block_for_test;

    let (registries, mut state, _) = generated_grounded_player_at(0, 0);
    let position = state.logistics().player().unwrap().position();
    let high_roof = VoxelCoord::new(position.x(), WORLD_MAX_Y_EXCLUSIVE - 1, position.z());
    assert_eq!(state.world().block_at(high_roof), Some(BlockStateId::AIR));
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Open)
    );

    set_generated_block_for_test(registries.world(), &mut state, high_roof, BLOCK_STATE_STONE);
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Covered),
        "an opaque high-altitude roof still physically covers the player"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    set_generated_block_for_test(registries.world(), &mut state, high_roof, BlockStateId::AIR);
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Open),
        "clearing the only high roof must restore exposed cover status"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn a_structure_beyond_generated_sky_cannot_provide_overhead_cover() {
    let (registries, mut state, _) = generated_grounded_player_at(0, 0);
    let player_cell = state.logistics().player().unwrap().position();
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
    assert_eq!(
        crate::world::assess_generated_skylight(&registries, &state, player_cell),
        9,
        "a structure outside the generated vertical world cannot cast a shadow"
    );
}

#[test]
fn narrow_framing_does_not_become_a_solid_roof_in_skylight_or_thermal_assessment() {
    use crate::world::assess_generated_skylight;

    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let player_cell = VoxelCoord::new(0, floor_y, 0);
    let frame_cell = VoxelCoord::new(0, floor_y + 3, 0);
    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(frame_cell, VoxelCoord::new(1, frame_cell.y() + 1, 1)).unwrap(),
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(250_000),
    )
    .unwrap();
    let frame = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, frame, FORM_LOG);
    let _ = validate_activate_structural_element(&registries, &state, frame)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Open),
    );
    assert_eq!(
        assess_generated_skylight(&registries, &state, player_cell),
        9,
        "narrow framing must not turn into a full-cell opaque roof"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn indexed_overhead_cover_requires_active_local_roof_and_survives_save_load() {
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::world::assess_generated_skylight;

    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let player_cell = VoxelCoord::new(0, floor_y, 0);
    assert_eq!(
        assess_generated_skylight(&registries, &state, player_cell),
        9
    );
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
    assert_eq!(
        assess_generated_skylight(&registries, &state, player_cell),
        9,
        "an unbuilt roof plan cannot eclipse real daylight"
    );
    materialize_structural_element_for_test(&registries, &mut state, roof, FORM_LOG);
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Open),
        "embodiment alone does not make an unfinished roof active"
    );
    assert_eq!(
        assess_generated_skylight(&registries, &state, player_cell),
        0,
        "an embodied full-footprint panel physically stops daylight before activation"
    );
    let _ = validate_activate_structural_element(&registries, &state, roof)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Covered)
    );
    assert!(
        !state
            .structures()
            .has_active_overhead_cover(1, 0, floor_y + 2, WORLD_MAX_Y_EXCLUSIVE),
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
    assert_eq!(
        assess_generated_skylight(&registries, &loaded, player_cell),
        0,
        "trusted index reconstruction preserves the roof's physical skylight occlusion"
    );
}
