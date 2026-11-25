//! Removing explicit structural support must not suspend physical custody in air.

use super::*;
use crate::content::{FORM_LOG, STRUCTURAL_PROFILE_AXIAL_COMPRESSION};
use crate::core::quantity::{Area, Length};
use crate::equipment::{
    EquipmentSupportCommitError, EquipmentSupportError, validate_mount_equipment,
    validate_unmount_equipment,
};
use crate::inventory::{
    StockpileId, StockpileSupportCommitError, StockpileSupportError, validate_mount_stockpile,
    validate_unmount_stockpile,
};
use crate::logistics::validate_place_ground_stockpile;
use crate::spatial::VoxelBounds;
use crate::structural::{
    add_structural_element, make_test_structural_geometry, materialize_structural_element_for_test,
    validate_activate_structural_element, validate_link_support,
};

fn mounted_custody() -> (Registries, AppState, VoxelCoord, StockpileId, EquipmentId) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let floor = VoxelCoord::new(5, 20, 5);
    let at = VoxelCoord::new(5, 21, 5);
    let anchor = VoxelCoord::new(6, 21, 5);
    for voxel in [floor, VoxelCoord::new(6, 20, 5)] {
        set_generated_block_for_test(registries.world(), &mut state, voxel, BLOCK_STATE_STONE);
    }
    for voxel in [at, anchor] {
        set_generated_block_for_test(registries.world(), &mut state, voxel, BlockStateId::AIR);
    }
    let mut members = Vec::new();
    for (position, grounded) in [(anchor, true), (at, false)] {
        let member = add_structural_element(
            &registries,
            &mut state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            make_test_structural_geometry(
                VoxelBounds::new(
                    position,
                    VoxelCoord::new(position.x() + 1, position.y() + 1, position.z() + 1),
                )
                .unwrap(),
                Length::from_micrometers(1),
                Area::from_square_millimeters(10_000),
            ),
            grounded,
        )
        .unwrap();
        materialize_structural_element_for_test(&registries, &mut state, member, FORM_LOG);
        if !grounded {
            let _ = validate_link_support(&registries, &state, member, members[0])
                .unwrap()
                .commit(&mut state)
                .unwrap();
        }
        let _ = validate_activate_structural_element(&registries, &state, member)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        members.push(member);
    }
    let stockpile =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_place_ground_stockpile(&state, stockpile, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let equipment = assemble_stone_pick_at(&registries, &mut state, at);
    let _ = validate_mount_stockpile(&registries, &state, stockpile, members[1])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = validate_mount_equipment(&registries, &state, equipment, members[1])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    (registries, state, floor, stockpile, equipment)
}

#[test]
fn unmount_footing_stored_stockpile_cannot_be_left_floating() {
    let (registries, mut state, floor, stockpile, _) = mounted_custody();
    let pending = validate_unmount_stockpile(&registries, &state, stockpile).unwrap();
    let expected = state.world().revision();
    set_generated_block_for_test(registries.world(), &mut state, floor, BlockStateId::AIR);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let position = VoxelCoord::new(floor.x(), floor.y() + 1, floor.z());
    let before = state.clone();
    let actual = state.world().revision();
    assert_eq!(
        validate_unmount_stockpile(&registries, &state, stockpile).err(),
        Some(StockpileSupportError::UnmountedMatterRequiresFixedFloor {
            stockpile,
            position
        })
    );
    assert_eq!(
        pending.commit(&mut state),
        Err(StockpileSupportCommitError::StaleWorldRevision { expected, actual })
    );
    assert_eq!(state, before);
    set_generated_block_for_test(registries.world(), &mut state, floor, BLOCK_STATE_STONE);
    let matter = calculate_matter_accounting(&state).unwrap().total();
    let _ = validate_unmount_stockpile(&registries, &state, stockpile)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .supported_by(),
        None
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), matter);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn unmount_footing_embodied_equipment_cannot_be_left_floating() {
    let (registries, mut state, floor, _, equipment) = mounted_custody();
    let pending = validate_unmount_equipment(&registries, &state, equipment).unwrap();
    let expected = state.world().revision();
    set_generated_block_for_test(registries.world(), &mut state, floor, BlockStateId::AIR);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let position = VoxelCoord::new(floor.x(), floor.y() + 1, floor.z());
    let before = state.clone();
    let actual = state.world().revision();
    assert_eq!(
        validate_unmount_equipment(&registries, &state, equipment).err(),
        Some(EquipmentSupportError::UnmountedMatterRequiresFixedFloor {
            equipment,
            position
        })
    );
    assert_eq!(
        pending.commit(&mut state),
        Err(EquipmentSupportCommitError::StaleWorldRevision { expected, actual })
    );
    assert_eq!(state, before);
    set_generated_block_for_test(registries.world(), &mut state, floor, BLOCK_STATE_STONE);
    let matter = calculate_matter_accounting(&state).unwrap().total();
    let _ = validate_unmount_equipment(&registries, &state, equipment)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .unwrap()
            .supported_by(),
        None
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), matter);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
