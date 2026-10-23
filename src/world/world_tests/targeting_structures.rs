//! Pointed interaction respects physically built panels without hiding the nearest panel itself.

use super::*;

use crate::content::{FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION};
use crate::core::quantity::{Area, Length};
use crate::spatial::VoxelBounds;
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
    validate_activate_structural_element,
};

#[test]
fn built_panel_occludes_terrain_and_distant_structural_air_targets() {
    let (registries, mut state, floor_y) = generated_targeting_player_at(0, 0);
    let y = floor_y + 1;
    let barrier = VoxelCoord::new(2, y, 0);
    let beyond = VoxelCoord::new(3, y, 0);
    let terrain = VoxelCoord::new(4, y, 0);
    for x in 1..=4 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, y, 0),
            BlockStateId::AIR,
        );
    }
    set_generated_block_for_test(registries.world(), &mut state, terrain, BLOCK_STATE_STONE);
    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(barrier, VoxelCoord::new(3, y + 1, 1)).unwrap(),
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(1_000_000),
    )
    .unwrap();
    let member = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    let select_far = |voxel, _face| (voxel == beyond).then_some(voxel);

    // Configuration without embodied matter must not block sight or interaction.
    assert_eq!(
        raycast_player_block(&registries, &state, positive_x_view())
            .map(|target| target.map(PlayerBlockTarget::voxel)),
        Ok(Some(terrain))
    );
    assert_eq!(
        raycast_player_air_voxel(&registries, &state, positive_x_view(), select_far),
        Ok(Some(beyond))
    );

    materialize_structural_element_for_test(&registries, &mut state, member, FORM_LOG);
    assert_eq!(
        raycast_player_block(&registries, &state, positive_x_view()),
        Ok(None),
        "terrain cannot be mined through a materialized full-footprint panel"
    );
    assert_eq!(
        raycast_player_air_voxel(&registries, &state, positive_x_view(), select_far),
        Ok(None),
        "another interaction cannot target air past a materialized panel"
    );
    assert_eq!(
        raycast_player_air_voxel(&registries, &state, positive_x_view(), |voxel, _face| {
            (voxel == barrier).then_some(member)
        }),
        Ok(Some(member)),
        "the panel itself remains a valid pointed structural target"
    );
    let _ = validate_activate_structural_element(&registries, &state, member)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let reloaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(reloaded, state);
    assert_eq!(
        raycast_player_block(&registries, &reloaded, positive_x_view()),
        Ok(None)
    );
}
