//! Moving physical support assignments requires the same free player attention as hand custody.

use super::*;
use crate::combat::{validate_cancel_primitive_preparation, validate_prepare_primitive_strike};
use crate::content::{
    EQUIPMENT_WOODEN_CLUB, FORM_LOG, FORM_LUMP, MATERIAL_STONE, MATERIAL_WOOD,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, SURFACE_GATHERING_HAND_COLLECT_STONE, build_registries,
};
use crate::core::quantity::{Area, Length, Mass, Temperature, Volume};
use crate::equipment::{
    EquipmentSupportCommitError, EquipmentSupportError, validate_assemble_equipment,
    validate_mount_equipment, validate_unmount_equipment,
};
use crate::fluid::{
    FluidSupportCommitError, FluidSupportError, add_fluid_store, validate_mount_fluid_store,
    validate_unmount_fluid_store,
};
use crate::inventory::{
    StockpileSupportCommitError, StockpileSupportError, deposit_lot_for_test,
    validate_mount_stockpile, validate_unmount_stockpile,
};
use crate::logistics::{
    PlayerActiveSelection, validate_allocate_player_ground_stockpile, validate_pickup_equipment,
    validate_place_fluid_store, validate_set_active_selection,
};
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::spatial::VoxelBounds;
use crate::structural::{
    add_structural_element, make_test_structural_geometry, materialize_structural_element_for_test,
    validate_activate_structural_element,
};
use crate::surface::{
    SurfaceResourceKind, SurfaceResourceSpec, insert_surface_resource_for_test,
    validate_start_player_surface_gathering,
};

#[test]
fn prepared_strike_excludes_all_three_physical_support_assignments_and_stale_tokens() {
    let registries = build_registries();
    let mut state = AppState::new();
    let at = VoxelCoord::new(0, 0, 0);
    let support = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        make_test_structural_geometry(
            VoxelBounds::new(at, VoxelCoord::new(1, 1, 1)).unwrap(),
            Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, support, FORM_LOG);
    let _ = validate_activate_structural_element(&registries, &state, support)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let carried = validate_admit_player(&registries, &state, at)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let ingredients = registries
        .equipment()
        .get_equipment(EQUIPMENT_WOODEN_CLUB)
        .unwrap()
        .assembly_profile()
        .unwrap();
    for input in ingredients.inputs() {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            Mass::from_milligrams(input.mass().milligrams() * 2),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let club = validate_assemble_equipment(&registries, &state, EQUIPMENT_WOODEN_CLUB, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let spare = validate_assemble_equipment(&registries, &state, EQUIPMENT_WOODEN_CLUB, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_pickup_equipment(&registries, &state, club)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::equipment(club)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let bay = validate_allocate_player_ground_stockpile(
        &registries,
        &state,
        Mass::from_milligrams(200_000),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let vessel = add_fluid_store(&mut state, Volume::from_microliters(500_000)).unwrap();
    validate_place_fluid_store(&state, vessel, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let resource = insert_surface_resource_for_test(
        &registries,
        &mut state,
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            at,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(100_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap(),
    )
    .unwrap();
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let old_bay = validate_mount_stockpile(&registries, &state, bay, support).unwrap();
    let old_equipment = validate_mount_equipment(&registries, &state, spare, support).unwrap();
    let old_vessel = validate_mount_fluid_store(&registries, &state, vessel, support).unwrap();
    let _ = validate_prepare_primitive_strike(&registries, &state, east())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let prepared = state.clone();
    assert_eq!(
        validate_mount_stockpile(&registries, &state, bay, support).err(),
        Some(StockpileSupportError::PreparedStrikeActive)
    );
    assert_eq!(
        validate_mount_equipment(&registries, &state, spare, support).err(),
        Some(EquipmentSupportError::PreparedStrikeActive)
    );
    assert_eq!(
        validate_mount_fluid_store(&registries, &state, vessel, support).err(),
        Some(FluidSupportError::PreparedStrikeActive)
    );
    assert!(matches!(
        old_bay.commit(&mut state),
        Err(StockpileSupportCommitError::StaleCombatRevision { .. })
    ));
    assert!(matches!(
        old_equipment.commit(&mut state),
        Err(EquipmentSupportCommitError::StaleCombatRevision { .. })
    ));
    assert!(matches!(
        old_vessel.commit(&mut state),
        Err(FluidSupportCommitError::StaleCombatRevision { .. })
    ));
    assert_eq!(state, prepared);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for game in [&mut state, &mut replay] {
        let _ = validate_cancel_primitive_preparation(game)
            .unwrap()
            .commit(game)
            .unwrap();
        let _ = validate_mount_stockpile(&registries, game, bay, support)
            .unwrap()
            .commit(game)
            .unwrap();
        let _ = validate_mount_equipment(&registries, game, spare, support)
            .unwrap()
            .commit(game)
            .unwrap();
        let _ = validate_mount_fluid_store(&registries, game, vessel, support)
            .unwrap()
            .commit(game)
            .unwrap();
        let old_bay = validate_unmount_stockpile(&registries, game, bay).unwrap();
        let old_equipment = validate_unmount_equipment(&registries, game, spare).unwrap();
        let old_vessel = validate_unmount_fluid_store(&registries, game, vessel).unwrap();
        let _ = validate_prepare_primitive_strike(&registries, game, east())
            .unwrap()
            .commit(game)
            .unwrap();
        assert_eq!(
            validate_unmount_stockpile(&registries, game, bay).err(),
            Some(StockpileSupportError::PreparedStrikeActive)
        );
        assert_eq!(
            validate_unmount_equipment(&registries, game, spare).err(),
            Some(EquipmentSupportError::PreparedStrikeActive)
        );
        assert_eq!(
            validate_unmount_fluid_store(&registries, game, vessel).err(),
            Some(FluidSupportError::PreparedStrikeActive)
        );
        assert!(matches!(
            old_bay.commit(game),
            Err(StockpileSupportCommitError::StaleCombatRevision { .. })
        ));
        assert!(matches!(
            old_equipment.commit(game),
            Err(EquipmentSupportCommitError::StaleCombatRevision { .. })
        ));
        assert!(matches!(
            old_vessel.commit(game),
            Err(FluidSupportCommitError::StaleCombatRevision { .. })
        ));
        let _ = validate_cancel_primitive_preparation(game)
            .unwrap()
            .commit(game)
            .unwrap();
        let old_bay = validate_unmount_stockpile(&registries, game, bay).unwrap();
        let old_equipment = validate_unmount_equipment(&registries, game, spare).unwrap();
        let old_vessel = validate_unmount_fluid_store(&registries, game, vessel).unwrap();
        validate_start_player_surface_gathering(
            &registries,
            game,
            SURFACE_GATHERING_HAND_COLLECT_STONE,
            resource,
            Mass::from_milligrams(25_000),
        )
        .unwrap()
        .commit(game)
        .unwrap();
        assert!(game.player_work().active().is_some());
        // Existing worksite recovery semantics allow unrelated support
        // changes while a scheduled gathering job is in progress.
        assert!(validate_unmount_stockpile(&registries, game, bay).is_ok());
        assert!(validate_unmount_equipment(&registries, game, spare).is_ok());
        assert!(validate_unmount_fluid_store(&registries, game, vessel).is_ok());
        assert!(matches!(
            old_bay.commit(game),
            Err(StockpileSupportCommitError::StalePlayerWorkRevision { .. })
        ));
        assert!(matches!(
            old_equipment.commit(game),
            Err(EquipmentSupportCommitError::StalePlayerWorkRevision { .. })
        ));
        assert!(matches!(
            old_vessel.commit(game),
            Err(FluidSupportCommitError::StalePlayerWorkRevision { .. })
        ));
        let _ = validate_unmount_stockpile(&registries, game, bay)
            .unwrap()
            .commit(game)
            .unwrap();
        let _ = validate_unmount_equipment(&registries, game, spare)
            .unwrap()
            .commit(game)
            .unwrap();
        let _ = validate_unmount_fluid_store(&registries, game, vessel)
            .unwrap()
            .commit(game)
            .unwrap();
        assert_eq!(validate_loaded_state(&registries, game), Ok(()));
        assert_eq!(
            calculate_matter_accounting(game).unwrap().total(),
            matter_before
        );
    }
    assert_eq!(state, replay);
}
