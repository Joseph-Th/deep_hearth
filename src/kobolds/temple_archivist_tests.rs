//! Archivists attend their real copper archive; breach vigils are caused by missing world matter.

use super::*;
use crate::content::{
    BASIC_TIMBER_POST_CROSS_SECTION, BASIC_TIMBER_POST_LENGTH, BLOCK_STATE_STONE,
    BLOCK_STATE_TEMPLE_ARCHIVE, FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    build_registries,
};
use crate::core::state::{AppState, validate_loaded_state};
use crate::logistics::relocate_player_for_test;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelBounds;
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
};
use crate::world::{WorldGenerationSeed, set_generated_block_for_test, validate_generate_chunk};

fn occupied_archive() -> (Registries, AppState, VoxelCoord) {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x0054_454D_504C_4553);
    let chunk = ChunkCoord::new(0, 0);
    let mut state = AppState::new();
    validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let observer = VoxelCoord::new(
        chunk.world_min_x().unwrap() + 8,
        -19,
        chunk.world_min_z().unwrap() + 8,
    );
    let [_, home] = temple_residents_at(&registries, &state, observer).unwrap();
    let witness = VoxelCoord::new(home.x() - 4, home.y(), home.z() + 2);
    assert_eq!(state.world().block_at(witness), Some(BlockStateId::AIR));
    validate_admit_player(&registries, &state, witness)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    (registries, state, home)
}

fn advance_to(registries: &Registries, state: &mut AppState, goal: u64) {
    while state.tick().value() < goal {
        let _ = advance_tick(registries, state).unwrap();
    }
    assert_eq!(state.tick().value(), goal);
}

fn visible_archivist(registries: &Registries, state: &AppState) -> Option<(VoxelCoord, bool)> {
    crate::kobolds::available_nearby_kobold_scouts(registries, state)
        .find(|seen| seen.role() == crate::kobolds::KoboldRole::TempleArchivist)
        .map(|seen| (seen.position(), seen.is_defending()))
}

fn archive_cell(home: VoxelCoord) -> VoxelCoord {
    VoxelCoord::new(home.x() - 5, home.y(), home.z() + 2)
}

#[test]
fn narrow_upper_corner_post_blocks_the_massive_archivist_but_an_empty_plan_does_not() {
    let (registries, mut planned, home) = occupied_archive();
    let next = archivist_position(home, 1).unwrap();
    // The next body's far upper corner lies outside its current home body.
    // Checking only the anchor, or treating narrow posts as non-obstacles,
    // would let the massive resident walk through this materialized timber.
    let post_at = VoxelCoord::new(next.x(), next.y() + 2, next.z() - 1);
    assert_eq!(planned.world().block_at(post_at), Some(BlockStateId::AIR));
    assert!(!crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &planned,
        post_at
    ));
    let bounds = VoxelBounds::new(
        post_at,
        VoxelCoord::new(post_at.x() + 1, post_at.y() + 1, post_at.z() + 1),
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
        &registries,
        &mut planned,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &planned), Ok(()));
    let mut built = planned.clone();
    materialize_structural_element_for_test(&registries, &mut built, post, FORM_LOG);
    assert_eq!(validate_loaded_state(&registries, &built), Ok(()));
    assert!(built.structures().has_embodied_member_intersecting(bounds));
    let just_outside = VoxelBounds::new(
        VoxelCoord::new(post_at.x() + 1, post_at.y(), post_at.z()),
        VoxelCoord::new(post_at.x() + 2, post_at.y() + 1, post_at.z() + 1),
    )
    .unwrap();
    assert!(
        !built
            .structures()
            .has_embodied_member_intersecting(just_outside),
        "touching the edge of a half-open footprint is not an intersection"
    );
    assert!(
        !planned
            .structures()
            .has_embodied_member_intersecting(bounds)
    );

    advance_to(&registries, &mut planned, 240);
    advance_to(&registries, &mut built, 240);
    let chunk = ChunkCoord::containing(home);
    assert_eq!(planned.kobold_state().temple_archivist_step(chunk), 1);
    assert_eq!(built.kobold_state().temple_archivist_step(chunk), 0);
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &built)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut built, 250);
    advance_to(&registries, &mut replay, 250);
    assert_eq!(built, replay);
    assert_eq!(built.kobold_state().temple_archivist_step(chunk), 0);
    assert_eq!(validate_loaded_state(&registries, &built), Ok(()));
}

#[test]
fn archivist_inspects_when_a_living_player_is_near_across_a_generated_chunk_border() {
    let (registries, mut state, home) = occupied_archive();
    let seed = WorldGenerationSeed::new(0x0054_454D_504C_4553);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(-1, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let observer = VoxelCoord::new(-1, home.y(), home.z());
    assert_eq!(home.x().abs_diff(observer.x()), 7);
    for position in [
        observer,
        VoxelCoord::new(observer.x(), observer.y() + 1, observer.z()),
    ] {
        set_generated_block_for_test(registries.world(), &mut state, position, BlockStateId::AIR);
    }
    relocate_player_for_test(&mut state, observer);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    advance_to(&registries, &mut state, 240);
    assert_eq!(
        state
            .kobold_state()
            .temple_archivist_step(ChunkCoord::containing(home)),
        1,
        "a nearby player across a chunk edge must trigger the existing physical inspection"
    );
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, restored);
}

#[test]
fn archivist_walks_a_real_wide_supported_approach_to_the_archive_and_back() {
    let (registries, mut state, home) = occupied_archive();
    let chunk = ChunkCoord::containing(home);
    let original_mass = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(state.kobold_state().temple_archivist_step(chunk), 0);
    assert_eq!(
        state.world().block_at(archive_cell(home)),
        Some(BLOCK_STATE_TEMPLE_ARCHIVE)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    advance_to(&registries, &mut state, 230);
    assert!(state.kobold_state().temple_archivists.is_empty());
    for (at, step) in [(240, 1), (250, 2), (260, 3), (270, 4), (280, 5)] {
        advance_to(&registries, &mut state, at);
        assert_eq!(state.kobold_state().temple_archivist_step(chunk), step);
        let position = archivist_position(home, step).unwrap();
        assert_eq!(state.world().block_at(position), Some(BlockStateId::AIR));
        assert!(crate::kobolds::temple_caste_occupies_voxel(
            &registries,
            &state,
            position
        ));
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
    let at_shell = archivist_position(home, 5).unwrap();
    assert_eq!(at_shell.x(), archive_cell(home).x() + 2);
    assert_eq!(at_shell.z(), archive_cell(home).z());
    assert_eq!(
        visible_archivist(&registries, &state),
        Some((at_shell, false))
    );
    // Only authoritative generated block matter, not a virtual archive alarm.
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        original_mass
    );

    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for (at, step) in [(290, 6), (300, 7), (310, 8), (320, 9), (330, 0)] {
        advance_to(&registries, &mut state, at);
        advance_to(&registries, &mut replay, at);
        assert_eq!(state, replay);
        assert_eq!(state.kobold_state().temple_archivist_step(chunk), step);
    }
    assert!(state.kobold_state().temple_archivists.is_empty());
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        original_mass
    );
    assert_eq!(
        state.world().block_at(archive_cell(home)),
        Some(BLOCK_STATE_TEMPLE_ARCHIVE)
    );
}

#[test]
fn breached_archive_causes_local_guarded_vigil_without_repairing_the_shell() {
    let (registries, mut state, home) = occupied_archive();
    let shell = archive_cell(home);
    set_generated_block_for_test(registries.world(), &mut state, shell, BlockStateId::AIR);
    let remaining = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 280);
    let chunk = ChunkCoord::containing(home);
    assert_eq!(state.kobold_state().temple_archivist_step(chunk), 5);
    assert_eq!(
        visible_archivist(&registries, &state),
        Some((archivist_position(home, 5).unwrap(), true))
    );
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for at in [290, 300, 310] {
        advance_to(&registries, &mut state, at);
        advance_to(&registries, &mut replay, at);
        assert_eq!(state, replay);
        assert_eq!(state.kobold_state().temple_archivist_step(chunk), 5);
        assert_eq!(state.world().block_at(shell), Some(BlockStateId::AIR));
    }
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        remaining
    );

    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(home.x() + 24, home.y(), home.z()),
    );
    relocate_player_for_test(
        &mut replay,
        VoxelCoord::new(home.x() + 24, home.y(), home.z()),
    );
    for (at, step) in [(320, 4), (330, 3), (340, 2), (350, 1), (360, 0)] {
        advance_to(&registries, &mut state, at);
        advance_to(&registries, &mut replay, at);
        assert_eq!(state, replay);
        assert_eq!(state.kobold_state().temple_archivist_step(chunk), step);
    }
    assert!(state.kobold_state().temple_archivists.is_empty());
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        remaining
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn attending_archivist_does_not_abandon_a_breached_archive_when_player_is_near_its_body() {
    let (registries, mut state, home) = occupied_archive();
    let chunk = ChunkCoord::containing(home);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        archive_cell(home),
        BlockStateId::AIR,
    );
    advance_to(&registries, &mut state, 280);
    assert_eq!(state.kobold_state().temple_archivist_step(chunk), 5);

    // A side excavation leaves the player outside the original chamber's
    // radius, but close to the archivist's displaced, massive body.
    let observer = VoxelCoord::new(home.x() - 6, home.y(), home.z() + 3);
    assert!(!within_manhattan_reach(
        home,
        observer,
        ARCHIVIST_LOCAL_REACH
    ));
    assert!(
        resident_body_cells(archivist_position(home, 5).unwrap())
            .any(|cell| within_manhattan_reach(cell, observer, ARCHIVIST_LOCAL_REACH))
    );
    for position in [
        observer,
        VoxelCoord::new(observer.x(), observer.y() + 1, observer.z()),
    ] {
        set_generated_block_for_test(registries.world(), &mut state, position, BlockStateId::AIR);
    }
    relocate_player_for_test(&mut state, observer);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for at in [290, 300, 310] {
        advance_to(&registries, &mut state, at);
        advance_to(&registries, &mut replay, at);
        assert_eq!(state, replay);
        assert_eq!(state.kobold_state().temple_archivist_step(chunk), 5);
    }
}

#[test]
fn archive_inspection_cannot_sense_through_the_actual_blocked_throat() {
    let (registries, mut state, home) = occupied_archive();
    let shell = archive_cell(home);
    set_generated_block_for_test(registries.world(), &mut state, shell, BlockStateId::AIR);
    let throat = VoxelCoord::new(home.x() - 4, home.y(), home.z() + 2);
    let witness = VoxelCoord::new(throat.x(), throat.y(), throat.z() + 1);
    relocate_player_for_test(&mut state, witness);
    set_generated_block_for_test(registries.world(), &mut state, throat, BLOCK_STATE_STONE);
    assert_eq!(state.world().block_at(throat), Some(BLOCK_STATE_STONE));
    advance_to(&registries, &mut state, 280);
    assert_eq!(
        state
            .kobold_state()
            .temple_archivist_step(ChunkCoord::containing(home)),
        5
    );
    assert!(!archive_shell_breached(&state, home, 5));
    advance_to(&registries, &mut state, 290);
    assert_eq!(
        state
            .kobold_state()
            .temple_archivist_step(ChunkCoord::containing(home)),
        6
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn archive_inspection_obeys_full_massive_body_clearance_and_returns_on_blockage() {
    let (registries, mut state, home) = occupied_archive();
    let destination = archivist_position(home, 3).unwrap();
    // Stone in an outer corner blocks the otherwise empty anchor cell.
    let outer = VoxelCoord::new(destination.x(), destination.y() + 2, destination.z() - 1);
    assert_eq!(state.world().block_at(outer), Some(BlockStateId::AIR));
    set_generated_block_for_test(registries.world(), &mut state, outer, BLOCK_STATE_STONE);
    advance_to(&registries, &mut state, 260);
    let chunk = ChunkCoord::containing(home);
    assert_eq!(state.kobold_state().temple_archivist_step(chunk), 2);
    advance_to(&registries, &mut state, 270);
    assert_eq!(state.kobold_state().temple_archivist_step(chunk), 1);
    advance_to(&registries, &mut state, 280);
    assert_eq!(state.kobold_state().temple_archivist_step(chunk), 0);
    assert!(state.kobold_state().temple_archivists.is_empty());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn full_archivist_footprint_is_visible_and_blocks_the_players_true_body() {
    let (registries, mut state, home) = occupied_archive();
    advance_to(&registries, &mut state, 280);
    let chunk = ChunkCoord::containing(home);
    assert_eq!(state.kobold_state().temple_archivist_step(chunk), 5);
    let anchor = archivist_position(home, 5).unwrap();
    // This corner is beyond two cells from the anchor, but only two cells
    // from the actual other side of the huge archivist's occupied body.
    let observer = VoxelCoord::new(home.x(), home.y(), home.z() + 2);
    relocate_player_for_test(&mut state, observer);
    assert!(anchor.x().abs_diff(observer.x()) > 2);
    assert_eq!(
        visible_archivist(&registries, &state),
        Some((anchor, false))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let mut interpenetrating = state.clone();
    relocate_player_for_test(
        &mut interpenetrating,
        VoxelCoord::new(anchor.x() + 1, anchor.y(), anchor.z()),
    );
    assert!(validate_loaded_state(&registries, &interpenetrating).is_err());
    assert!(crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        VoxelCoord::new(anchor.x() + 1, anchor.y() + 2, anchor.z() - 1),
    ));
}

#[test]
fn local_player_in_the_next_wide_archivist_cell_prevents_inspection() {
    let (registries, mut state, home) = occupied_archive();
    advance_to(&registries, &mut state, 250);
    let chunk = ChunkCoord::containing(home);
    assert_eq!(state.kobold_state().temple_archivist_step(chunk), 2);
    let destination = archivist_position(home, 3).unwrap();
    relocate_player_for_test(&mut state, destination);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    advance_to(&registries, &mut state, 260);
    assert_eq!(state.kobold_state().temple_archivist_step(chunk), 2);
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(home.x() - 4, home.y(), home.z() + 2),
    );
    advance_to(&registries, &mut state, 270);
    assert_eq!(state.kobold_state().temple_archivist_step(chunk), 1);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn archivist_cannot_step_off_a_mined_out_floor_until_support_is_restored() {
    let (registries, mut state, home) = occupied_archive();
    advance_to(&registries, &mut state, 250);
    let chunk = ChunkCoord::containing(home);
    assert_eq!(state.kobold_state().temple_archivist_step(chunk), 2);
    let anchor = archivist_position(home, 2).unwrap();
    let undermined = VoxelCoord::new(anchor.x() + 1, anchor.y() - 1, anchor.z() - 1);
    assert_eq!(state.world().block_at(undermined), Some(BLOCK_STATE_STONE));
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        undermined,
        BlockStateId::AIR,
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    advance_to(&registries, &mut state, 260);
    assert_eq!(
        state.kobold_state().temple_archivist_step(chunk),
        2,
        "a massive resident cannot depart while any part of its floor is absent"
    );
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut restored_from_save = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 270);
    advance_to(&registries, &mut restored_from_save, 270);
    assert_eq!(state, restored_from_save);
    assert_eq!(state.kobold_state().temple_archivist_step(chunk), 2);

    for game in [&mut state, &mut restored_from_save] {
        set_generated_block_for_test(registries.world(), game, undermined, BLOCK_STATE_STONE);
    }
    advance_to(&registries, &mut state, 280);
    advance_to(&registries, &mut restored_from_save, 280);
    assert_eq!(state, restored_from_save);
    assert_eq!(state.kobold_state().temple_archivist_step(chunk), 1);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn save_validation_rejects_invented_archivists_and_missing_state_field() {
    let (registries, mut state, home) = occupied_archive();
    advance_to(&registries, &mut state, 240);
    let save = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        state
            .kobold_state()
            .temple_archivist_step(ChunkCoord::containing(home)),
        1
    );
    for (field, fake) in [
        ("step", serde_json::json!(10)),
        ("step", serde_json::json!(5)),
    ] {
        let mut altered = save.clone();
        altered["state"]["systems"]["kobolds"]["temple_archivists"][0][field] = fake;
        assert!(
            serde_json::from_value::<LoadedSaveEnvelope>(altered)
                .unwrap()
                .into_state(&registries)
                .is_err()
        );
    }
    let mut duplicated = save.clone();
    let repeat = duplicated["state"]["systems"]["kobolds"]["temple_archivists"][0].clone();
    duplicated["state"]["systems"]["kobolds"]["temple_archivists"]
        .as_array_mut()
        .unwrap()
        .push(repeat);
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(duplicated)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
    let mut remote = save.clone();
    remote["state"]["systems"]["kobolds"]["temple_archivists"][0]["chunk"]["x"] =
        serde_json::json!(8);
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(remote)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
    let mut missing = save.clone();
    missing["state"]["systems"]["kobolds"]
        .as_object_mut()
        .unwrap()
        .remove("temple_archivists");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());
    // A block cannot appear inside the archived body just by forging a save.
    let position = archivist_position(home, 1).unwrap();
    let mut occupied = state.clone();
    set_generated_block_for_test(
        registries.world(),
        &mut occupied,
        position,
        BLOCK_STATE_STONE,
    );
    assert!(validate_loaded_state(&registries, &occupied).is_err());
}

#[test]
fn distant_archive_does_not_animate_a_hidden_caste() {
    let (registries, mut state, home) = occupied_archive();
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(home.x() + 24, home.y(), home.z()),
    );
    advance_to(&registries, &mut state, 300);
    assert!(state.kobold_state().temple_archivists.is_empty());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
