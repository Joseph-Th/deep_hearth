//! New storage placement shares the physical floor rule of existing-stockpile enclosure.

use super::*;
use crate::content::{
    BASIC_TIMBER_COVER_PANEL_AREA, BASIC_TIMBER_COVER_PANEL_THICKNESS, BLOCK_STATE_STONE,
    FORM_BOARD, MATERIAL_WOOD, STORAGE_ROUGH_TIMBER_FIELD_BOX,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::Temperature;
use crate::core::state::validate_loaded_state;
use crate::inventory::deposit_lot_for_test;
use crate::logistics::{player_body_is_grounded, validate_allocate_player_ground_stockpile};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick;
use crate::spatial::VoxelBounds;
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
    validate_activate_structural_element,
};
use crate::world::{
    ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed, validate_generate_chunk,
};

#[test]
fn placement_cannot_assemble_a_hanging_box_without_ground_support() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xB0AF_F00D),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let floor = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(5, y, 5)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let floating = VoxelCoord::new(5, floor + 3, 5);
    let carried = validate_admit_player(&registries, &state, floating)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let definition = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap();
    for input in definition.assembly_profile().inputs() {
        let _ = deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    assert!(!player_body_is_grounded(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        validate_place_player_storage(&registries, &state, STORAGE_ROUGH_TIMBER_FIELD_BOX).err(),
        Some(PlayerStoragePlacementError::Construction(
            StorageEnclosureConstructionError::UnsupportedFloor { position: floating },
        )),
    );
    assert_eq!(state, before);

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..100 {
        if player_body_is_grounded(&registries, &state) {
            break;
        }
        let live = advance_control_tick(&registries, &mut state).unwrap();
        let loaded = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(live, loaded);
        assert_eq!(state, replay);
    }
    assert!(player_body_is_grounded(&registries, &state));
    for actor in [&mut state, &mut replay] {
        let box_stockpile =
            validate_place_player_storage(&registries, actor, STORAGE_ROUGH_TIMBER_FIELD_BOX)
                .unwrap()
                .commit(actor)
                .unwrap();
        assert_eq!(
            actor
                .logistics()
                .stationary_stockpile_position(box_stockpile),
            Some(VoxelCoord::new(5, floor + 1, 5))
        );
        assert!(
            actor
                .inventory()
                .get_stockpile(box_stockpile)
                .unwrap()
                .enclosure()
                .is_some()
        );
        assert_eq!(
            calculate_matter_accounting(actor).unwrap().total(),
            conserved
        );
        assert_eq!(validate_loaded_state(&registries, actor), Ok(()));
    }
    assert_eq!(state, replay);
}

#[test]
fn both_enclosure_construction_paths_accept_fixed_built_footing_and_bind_structure_revision() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xB0AF_F00D),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(5, y, 5)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let floor = VoxelCoord::new(5, surface_y + 1, 5);
    let at = VoxelCoord::new(5, surface_y + 2, 5);
    assert_eq!(
        state.world().block_at(floor),
        Some(crate::world::BlockStateId::AIR)
    );
    let panel = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            VoxelBounds::new(floor, VoxelCoord::new(6, floor.y() + 1, 6)).unwrap(),
            BASIC_TIMBER_COVER_PANEL_THICKNESS,
            BASIC_TIMBER_COVER_PANEL_AREA,
        )
        .unwrap(),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, panel, FORM_BOARD);
    let carried = validate_admit_player(&registries, &state, at)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    assert!(player_body_is_grounded(&registries, &state));
    let definition = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap();
    for input in definition.assembly_profile().inputs() {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let target = validate_allocate_player_ground_stockpile(
        &registries,
        &state,
        definition.maximum_stockpile_capacity(),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    // A real jump leaves the same discrete custody voxel before it leaves
    // the floor's vertical meter. Destination support alone is insufficient.
    let mut airborne = state.clone();
    let _ = crate::simulation::advance_control_tick_with_input(
        &registries,
        &mut airborne,
        crate::logistics::PlayerControlInput::NEUTRAL.with_jump(true),
    )
    .unwrap();
    assert_eq!(airborne.logistics().player().unwrap().position(), at);
    assert!(!player_body_is_grounded(&registries, &airborne));
    let before = airborne.clone();
    assert_eq!(
        validate_place_player_storage(&registries, &airborne, STORAGE_ROUGH_TIMBER_FIELD_BOX).err(),
        Some(PlayerStoragePlacementError::Construction(
            StorageEnclosureConstructionError::PlayerAirborne,
        )),
    );
    assert_eq!(
        validate_build_storage_enclosure(
            &registries,
            &airborne,
            STORAGE_ROUGH_TIMBER_FIELD_BOX,
            target,
            carried,
        )
        .err(),
        Some(StorageEnclosureConstructionError::PlayerAirborne),
    );
    assert_eq!(airborne, before);
    assert_eq!(validate_loaded_state(&registries, &airborne), Ok(()));

    let place =
        validate_place_player_storage(&registries, &state, STORAGE_ROUGH_TIMBER_FIELD_BOX).unwrap();
    let build = validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        target,
        carried,
    )
    .unwrap();
    let expected = state.structures().revision();
    let _ = validate_activate_structural_element(&registries, &state, panel)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let actual = state.structures().revision();
    assert!(actual > expected);
    let before = state.clone();
    assert_eq!(
        place.commit(&mut state),
        Err(PlayerStoragePlacementCommitError::StructureRevisionChanged { expected, actual })
    );
    assert_eq!(
        build.commit(&mut state),
        Err(StorageEnclosureCommitError::StaleStructureRevision { expected, actual })
    );
    assert_eq!(state, before);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let mut new_box = state.clone();
    let new_id =
        validate_place_player_storage(&registries, &new_box, STORAGE_ROUGH_TIMBER_FIELD_BOX)
            .unwrap()
            .commit(&mut new_box)
            .unwrap();
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        target,
        carried,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    for (world, id) in [(&state, target), (&new_box, new_id)] {
        assert_eq!(
            world.logistics().stationary_stockpile_position(id),
            Some(at)
        );
        assert_eq!(
            world
                .inventory()
                .get_stockpile(id)
                .unwrap()
                .enclosure()
                .unwrap()
                .embodied_mass(),
            definition.assembly_profile().input_mass()
        );
        assert_eq!(
            calculate_matter_accounting(world).unwrap().total(),
            conserved
        );
        assert_eq!(validate_loaded_state(&registries, world), Ok(()));
    }
}
