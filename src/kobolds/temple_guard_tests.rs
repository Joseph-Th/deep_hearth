//! A temple defender is a persistent local actor, not a distance-triggered spawn.

use super::*;

use crate::content::{
    BASIC_TIMBER_POST_CROSS_SECTION, BASIC_TIMBER_POST_LENGTH, BLOCK_STATE_STONE, FORM_LOG,
    FORM_ROUGH_BOX_BODY, MATERIAL_WOOD, STORAGE_ROUGH_TIMBER_FIELD_BOX,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::Temperature;
use crate::core::state::validate_loaded_state;
use crate::inventory::{deposit_lot_for_test, validate_place_player_storage_at};
use crate::logistics::{HorizontalControlInput, PlayerControlInput, relocate_player_for_test};
use crate::logistics::{PlayerWalkProjectionError, project_player_adjacent_walk};
use crate::material::CommodityKey;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::{validate_admit_player, validate_respawn_player};
use crate::simulation::{
    PlayerDirectControlInput, SecondaryActionOutcome, SecondaryControlIntent, advance_control_tick,
    advance_control_tick_with_direct_input, advance_control_tick_with_input,
};
use crate::spatial::{ViewDirection, VoxelBounds};
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
};
use crate::survival::{Vitality, player_record};
use crate::world::{WorldGenerationSeed, set_generated_block_for_test, validate_generate_chunk};

fn occupied_temple() -> (Registries, AppState, [VoxelCoord; 3]) {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x0054_454D_504C_4553);
    // This authored replay seed resolves its ancient precinct to column (0,0).
    let chunk = ChunkCoord::new(0, 0);
    let mut state = AppState::new();
    validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let sample = VoxelCoord::new(
        chunk.world_min_x().unwrap() + 8,
        -19,
        chunk.world_min_z().unwrap() + 8,
    );
    let route = temple_warden_route_at(&registries, &state, sample).unwrap();
    validate_admit_player(&registries, &state, route[2])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    (registries, state, route)
}

fn advance_to(registries: &Registries, state: &mut AppState, target: u64) {
    while state.tick().value() < target {
        let _ = advance_control_tick(registries, state).unwrap();
    }
}

fn visible_guard(registries: &Registries, state: &AppState) -> Option<VoxelCoord> {
    crate::kobolds::available_nearby_kobold_scouts(registries, state)
        .find(|sighting| sighting.role() == crate::kobolds::KoboldRole::TempleWarden)
        .map(|sighting| sighting.position())
}

fn defending_guard(registries: &Registries, state: &AppState) -> Option<bool> {
    crate::kobolds::available_nearby_kobold_scouts(registries, state)
        .find(|sighting| sighting.role() == crate::kobolds::KoboldRole::TempleWarden)
        .map(|sighting| sighting.is_defending())
}

fn carry_field_box_material(registries: &Registries, state: &mut AppState) {
    let definition = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let _ = deposit_lot_for_test(
        registries,
        state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
        definition.assembly_profile().input_mass(),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
}

fn plan_timber_member_at(
    registries: &Registries,
    state: &mut AppState,
    at: VoxelCoord,
) -> crate::structural::StructuralElementId {
    let bounds = VoxelBounds::new(at, VoxelCoord::new(at.x() + 1, at.y() + 1, at.z() + 1)).unwrap();
    let geometry = StructuralElementGeometry::new(
        bounds,
        BASIC_TIMBER_POST_LENGTH,
        BASIC_TIMBER_POST_CROSS_SECTION,
    )
    .unwrap();
    add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        false,
    )
    .unwrap()
}

fn materialize_timber_member_at(registries: &Registries, state: &mut AppState, at: VoxelCoord) {
    let member = plan_timber_member_at(registries, state, at);
    materialize_structural_element_for_test(registries, state, member, FORM_LOG);
}

#[test]
fn heavy_castes_are_solid_on_the_actual_continuous_player_walk_sweep() {
    let (registries, mut state, route) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    let original = state.logistics().player().unwrap().body().foot_position();
    assert!(crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        route[1]
    ));
    assert!(crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        VoxelCoord::new(route[1].x(), route[1].y() + 1, route[1].z()),
    ));
    assert!(
        !crate::kobolds::temple_caste_occupies_voxel(&registries, &state, route[0]),
        "a displaced guardian must not block its vacant ancestral post"
    );
    assert!(matches!(
        project_player_adjacent_walk(&registries, &state, route[1]),
        Err(PlayerWalkProjectionError::DestinationUnreachableBeforeBoundary { .. })
    ));
    let forward = PlayerControlInput::new(HorizontalControlInput::new(0, 1_000_000).unwrap());
    for _ in 0..68 {
        let _ = advance_control_tick_with_input(&registries, &mut state, forward).unwrap();
    }
    let player = state.logistics().player().unwrap();
    let advanced = player.body().foot_position();
    assert!(
        advanced.z_nanometers() > original.z_nanometers(),
        "the player must still advance through the open part of the hall"
    );
    assert_eq!(
        player.position(),
        route[2],
        "the warden must hold its occupied cell"
    );
    assert!(
        advanced.z_nanometers() < route[1].z() * 1_000_000_000,
        "continuous collision must stop at the defender's face"
    );
    assert_eq!(visible_guard(&registries, &state), Some(route[1]));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn pointed_storage_cannot_materialize_inside_a_temple_warden() {
    let (registries, mut state, route) = occupied_temple();
    let guard = route[0];
    relocate_player_for_test(&mut state, route[1]);
    let wall = VoxelCoord::new(guard.x(), guard.y(), guard.z() + 1);
    set_generated_block_for_test(registries.world(), &mut state, wall, BLOCK_STATE_STONE);
    assert!(crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        guard
    ));
    carry_field_box_material(&registries, &mut state);
    let before = state.clone();
    let facing_guard = ViewDirection::new(0, -600_000, 800_000).unwrap();
    let input = PlayerDirectControlInput::NEUTRAL.with_secondary(
        SecondaryControlIntent::place_storage(STORAGE_ROUGH_TIMBER_FIELD_BOX, facing_guard),
    );
    let outcome = advance_control_tick_with_direct_input(&registries, &mut state, input).unwrap();
    assert_eq!(
        outcome.secondary_action(),
        Some(&SecondaryActionOutcome::StoragePlacementLocationOccupied { position: guard })
    );
    assert_eq!(state.inventory(), before.inventory());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn trusted_load_rejects_stationary_enclosure_overlapping_a_temple_warden() {
    let (registries, mut state, route) = occupied_temple();
    carry_field_box_material(&registries, &mut state);
    // The inventory allocation primitive deliberately delegates world-space
    // authorization to its caller. Simulate tampered persisted custody by
    // skipping that caller's placement check.
    let _ = validate_place_player_storage_at(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        route[0],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert!(crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        route[0]
    ));
    assert!(
        validate_loaded_state(&registries, &state).is_err(),
        "trusted validation must reject infrastructure embedded in a resident"
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let admitted = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries);
    assert!(admitted.is_err());
}

#[test]
fn trusted_load_rejects_timber_member_overlapping_a_temple_warden() {
    let (registries, mut state, route) = occupied_temple();
    let head = VoxelCoord::new(route[0].x(), route[0].y() + 1, route[0].z());
    materialize_timber_member_at(&registries, &mut state, head);
    assert!(crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        head
    ));
    assert!(
        validate_loaded_state(&registries, &state).is_err(),
        "the trusted graph cannot admit constructed matter inside the guard"
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
}

#[test]
fn trusted_load_allows_empty_blueprint_but_not_embodied_timber_at_warden_post() {
    let (registries, mut state, route) = occupied_temple();
    let head = VoxelCoord::new(route[0].x(), route[0].y() + 1, route[0].z());
    let member = plan_timber_member_at(&registries, &mut state, head);
    assert!(
        state
            .structures()
            .get_element(member)
            .unwrap()
            .embodied_material()
            .is_empty()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);

    materialize_structural_element_for_test(&registries, &mut state, member, FORM_LOG);
    assert!(
        !state
            .structures()
            .get_element(member)
            .unwrap()
            .embodied_material()
            .is_empty()
    );
    assert!(
        validate_loaded_state(&registries, &state).is_err(),
        "adding real material at the occupied post must invalidate the persisted structure"
    );
}

#[test]
fn trusted_load_rejects_member_inside_the_archivists_full_footprint() {
    let (registries, mut state, route) = occupied_temple();
    let [_, archivist] = crate::world::temple_residents_at(&registries, &state, route[0]).unwrap();
    let upper_far_corner = VoxelCoord::new(archivist.x() + 1, archivist.y() + 2, archivist.z() - 1);
    assert!(crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        upper_far_corner
    ));
    materialize_timber_member_at(&registries, &mut state, upper_far_corner);
    assert!(
        validate_loaded_state(&registries, &state).is_err(),
        "the archivist's full two-by-two-by-three body cannot contain a timber member"
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
}

#[test]
fn warden_cannot_advance_into_a_real_stationary_enclosure() {
    let (registries, mut state, route) = occupied_temple();
    carry_field_box_material(&registries, &mut state);
    let stockpile = validate_place_player_storage_at(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        route[1],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(
        state.logistics().stationary_stockpile_position(stockpile),
        Some(route[1])
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 5);
    advance_to(&registries, &mut replay, 5);
    assert_eq!(state, replay);
    assert_eq!(
        state
            .kobold_state()
            .temple_warden_step(ChunkCoord::containing(route[0])),
        0
    );
    assert_eq!(
        state.logistics().stationary_stockpile_position(stockpile),
        Some(route[1])
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn vacated_warden_post_can_be_occupied_but_blocks_its_later_retreat() {
    let (registries, mut state, route) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    let chunk = ChunkCoord::containing(route[0]);
    assert_eq!(state.kobold_state().temple_warden_step(chunk), 1);
    assert!(!crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        route[0]
    ));
    carry_field_box_material(&registries, &mut state);
    let stockpile = validate_place_player_storage_at(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        route[0],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(route[2].x() + 4, route[2].y(), route[2].z()),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 10);
    advance_to(&registries, &mut replay, 10);
    assert_eq!(state, replay);
    assert_eq!(state.kobold_state().temple_warden_step(chunk), 1);
    assert_eq!(
        state.logistics().stationary_stockpile_position(stockpile),
        Some(route[0])
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn generated_world_scout_clearance_respects_constructed_enclosures_and_temple_castes() {
    let (registries, mut state, route) = occupied_temple();
    let position = route[1];
    let distant_player = Some(route[2]);
    assert!(
        crate::kobolds::patrol::scout_can_enter(&registries, &state, position, distant_player),
        "the empty hall cell must be genuinely traversable"
    );
    let mut airborne = state.clone();
    let above = VoxelCoord::new(position.x(), position.y() + 1, position.z());
    set_generated_block_for_test(
        registries.world(),
        &mut airborne,
        VoxelCoord::new(position.x(), position.y() + 2, position.z()),
        BlockStateId::AIR,
    );
    relocate_player_for_test(&mut airborne, above);
    assert_eq!(validate_loaded_state(&registries, &airborne), Ok(()));
    assert!(
        !crate::kobolds::patrol::scout_can_enter(&registries, &airborne, position, Some(above)),
        "a player occupying the scout's upper body cell must prevent entry"
    );
    assert!(
        !crate::kobolds::patrol::scout_can_enter(&registries, &state, route[0], distant_player),
        "a standing temple warden must block a second small actor"
    );
    let mut member_state = state.clone();
    let head = VoxelCoord::new(position.x(), position.y() + 1, position.z());
    materialize_timber_member_at(&registries, &mut member_state, head);
    assert!(
        !crate::kobolds::patrol::scout_can_enter(
            &registries,
            &member_state,
            position,
            distant_player
        ),
        "a member in the upper occupied cell must obstruct the scout"
    );
    assert_eq!(validate_loaded_state(&registries, &member_state), Ok(()));
    carry_field_box_material(&registries, &mut state);
    let _ = validate_place_player_storage_at(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        position,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert!(
        !crate::kobolds::patrol::scout_can_enter(&registries, &state, position, distant_player),
        "a stationary constructed enclosure must block the scout's body"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn empty_structural_blueprints_are_not_physical_patrol_obstacles() {
    let (registries, mut state, route) = occupied_temple();
    let head = VoxelCoord::new(route[1].x(), route[1].y() + 1, route[1].z());
    let member = plan_timber_member_at(&registries, &mut state, head);
    assert_eq!(
        state
            .structures()
            .get_element(member)
            .unwrap()
            .embodied_mass(),
        crate::core::quantity::Mass::ZERO
    );
    assert!(
        crate::kobolds::patrol::scout_can_enter(&registries, &state, route[1], Some(route[2])),
        "a planned member has no embodied matter and cannot block physical passage"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 5);
    advance_to(&registries, &mut replay, 5);
    assert_eq!(state, replay);
    assert_eq!(
        state
            .kobold_state()
            .temple_warden_step(ChunkCoord::containing(route[0])),
        1,
        "a warden can cross an inert blueprint but not constructed timber"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn warden_cannot_advance_into_an_authored_timber_member() {
    let (registries, mut state, route) = occupied_temple();
    let at = VoxelCoord::new(route[1].x(), route[1].y() + 1, route[1].z());
    materialize_timber_member_at(&registries, &mut state, at);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    advance_to(&registries, &mut state, 5);
    assert_eq!(
        state
            .kobold_state()
            .temple_warden_step(ChunkCoord::containing(route[0])),
        0,
        "the member in the warden's upper body cell must block its advance"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn falling_onto_a_temple_warden_lands_above_its_real_body_and_replays() {
    let (registries, mut state, route) = occupied_temple();
    let warden = route[0];
    // The guard has a two-voxel body. Open the existing hall roof to admit
    // an airborne player without altering the resident or its supporting floor.
    for y in (warden.y() + 2)..=(warden.y() + 5) {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(warden.x(), y, warden.z()),
            BlockStateId::AIR,
        );
    }
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(warden.x(), warden.y() + 3, warden.z()),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let landing_y = (warden.y() + 2) * crate::spatial::NANOMETERS_PER_VOXEL;
    let mut landed = false;
    for _ in 0..32 {
        let movement = advance_control_tick(&registries, &mut state).unwrap();
        let replay_movement = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(movement, replay_movement);
        assert_eq!(state, replay);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let body = state.logistics().player().unwrap().body();
        assert!(
            body.foot_position().y_nanometers() >= landing_y,
            "an airborne player must never fall into the resident's occupied voxels"
        );
        if body.foot_position().y_nanometers() == landing_y {
            landed = true;
            assert_eq!(body.vertical_velocity().nanometers_per_second(), 0);
        }
    }
    assert!(
        landed,
        "the guard's head must act as a physical landing surface"
    );
}

#[test]
fn jump_and_stance_change_cannot_pass_through_a_stranded_warden_from_below() {
    let (registries, mut state, route) = occupied_temple();
    let warden = route[0];
    // Excavation beneath a stationary caste may remove its original support.
    // The resident still has a real physical footprint until it can move.
    for offset in [-2, -1] {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(warden.x(), warden.y() + offset, warden.z()),
            BlockStateId::AIR,
        );
    }
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(warden.x(), warden.y() - 3, warden.z()),
        BLOCK_STATE_STONE,
    );
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(warden.x(), warden.y() - 2, warden.z()),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let crouched_jump = PlayerControlInput::NEUTRAL
        .with_crouch(true)
        .with_jump(true);
    for _ in 0..2 {
        let _ = advance_control_tick_with_input(&registries, &mut state, crouched_jump).unwrap();
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
    let before = state.logistics().player().unwrap().body();
    assert_eq!(before.stance(), crate::logistics::PlayerStance::Crouching);
    let locomotion = registries.survival().physiology().locomotion();
    let standing_height =
        i64::try_from(locomotion.standing_height().micrometers()).unwrap() * 1_000;
    let crouching_height =
        i64::try_from(locomotion.crouching_height().micrometers()).unwrap() * 1_000;
    assert!(
        before.foot_position().y_nanometers() + standing_height
            > warden.y() * crate::spatial::NANOMETERS_PER_VOXEL,
        "standing at the current airborne foot position would overlap the resident"
    );
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let moved = advance_control_tick(&registries, &mut state).unwrap();
    assert_eq!(
        moved,
        advance_control_tick(&registries, &mut replay).unwrap()
    );
    assert_eq!(state, replay);
    let after = state.logistics().player().unwrap().body();
    assert_eq!(
        after.stance(),
        crate::logistics::PlayerStance::Crouching,
        "uncrouching under a living obstruction must fail as it does under a stone ceiling"
    );
    assert_eq!(
        after.foot_position().y_nanometers(),
        warden.y() * crate::spatial::NANOMETERS_PER_VOXEL - crouching_height
    );
    assert_eq!(after.vertical_velocity().nanometers_per_second(), 0);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn moving_warden_cannot_enter_the_airborne_players_upper_body() {
    let (registries, mut state, route) = occupied_temple();
    let target = route[1];
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(target.x(), target.y() + 2, target.z()),
        BlockStateId::AIR,
    );
    // One control step short of the next five-tick warden beat. The player
    // remains in the upper voxel during the warden's due decision.
    while state.control_tick().value()
        < 5 * crate::core::time::CONTROL_TICKS_PER_SIMULATION_TICK - 1
    {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(visible_guard(&registries, &state), Some(route[0]));
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(target.x(), target.y() + 1, target.z()),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert!(!crate::logistics::player_body_occupies_voxel(
        &registries,
        &state,
        target
    ));
    assert!(crate::logistics::player_body_occupies_voxel(
        &registries,
        &state,
        VoxelCoord::new(target.x(), target.y() + 1, target.z())
    ));
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let consequence = advance_control_tick(&registries, &mut state).unwrap();
    assert_eq!(
        consequence,
        advance_control_tick(&registries, &mut replay).unwrap()
    );
    assert_eq!(state, replay);
    assert_eq!(
        visible_guard(&registries, &state),
        Some(route[0]),
        "a moving two-voxel resident must not enter the airborne player's body"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn player_admission_and_trusted_load_reject_overlapping_temple_residents() {
    let (registries, state, route) = occupied_temple();
    let mut unadmitted = AppState::new();
    let seed = WorldGenerationSeed::new(0x0054_454D_504C_4553);
    validate_generate_chunk(
        &registries,
        &unadmitted,
        seed,
        ChunkCoord::containing(route[0]),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut unadmitted)
    .unwrap();
    assert!(
        validate_admit_player(&registries, &unadmitted, route[0]).is_err(),
        "wardens are not passable initial spawn points"
    );
    let archivist = VoxelCoord::new(route[2].x(), route[2].y(), route[2].z() - 2);
    assert!(
        validate_admit_player(&registries, &unadmitted, archivist).is_err(),
        "the much larger archivist also occupies real world space"
    );
    let mut overlapping = state;
    relocate_player_for_test(&mut overlapping, route[0]);
    assert!(validate_loaded_state(&registries, &overlapping).is_err());
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &overlapping)).unwrap();
    assert!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .is_err(),
        "the trusted-load boundary must refuse impossible contact"
    );
}

#[test]
fn massive_archivist_occupies_a_real_two_by_two_three_high_body() {
    let (registries, state, route) = occupied_temple();
    let archivist = VoxelCoord::new(route[2].x(), route[2].y(), route[2].z() - 2);
    for dx in 0..=1 {
        for dz in -1..=0 {
            for dy in 0..=2 {
                let voxel =
                    VoxelCoord::new(archivist.x() + dx, archivist.y() + dy, archivist.z() + dz);
                assert_eq!(
                    state.world().block_at(voxel),
                    Some(crate::world::BlockStateId::AIR),
                    "the massive resident needs actual excavated volume"
                );
                assert!(crate::kobolds::temple_caste_occupies_voxel(
                    &registries,
                    &state,
                    voxel
                ));
            }
        }
    }
    for voxel in [
        VoxelCoord::new(archivist.x() - 1, archivist.y(), archivist.z()),
        VoxelCoord::new(archivist.x() + 2, archivist.y(), archivist.z()),
        VoxelCoord::new(archivist.x(), archivist.y() + 3, archivist.z()),
    ] {
        assert!(!crate::kobolds::temple_caste_occupies_voxel(
            &registries,
            &state,
            voxel
        ));
    }
}

#[test]
fn respawn_falls_back_when_a_warden_now_guards_the_original_spawn() {
    let (registries, mut state, route) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(route[2].x(), route[2].y(), route[2].z() - 1),
    );
    advance_to(&registries, &mut state, 10);
    assert_eq!(visible_guard(&registries, &state), Some(route[2]));
    let death = VoxelCoord::new(route[2].x() + 3, route[2].y(), route[2].z());
    relocate_player_for_test(&mut state, death);
    let living = state.survival().player().copied().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            living.metabolic_energy(),
            living.hydration(),
            Vitality::ZERO,
            living.nutrition(),
            living.vitality_recovery_remainder(),
        ),
    );
    let outcome = validate_respawn_player(&registries, &state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(!outcome.used_spawn_anchor());
    assert_eq!(outcome.position(), death);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn intruder_draws_a_warden_forward_by_one_real_cell_then_it_withdraws() {
    let (registries, mut state, route) = occupied_temple();
    assert_eq!(visible_guard(&registries, &state), Some(route[0]));
    assert_eq!(defending_guard(&registries, &state), Some(false));
    advance_to(&registries, &mut state, 5);
    assert_eq!(visible_guard(&registries, &state), Some(route[1]));
    assert_eq!(defending_guard(&registries, &state), Some(true));
    assert_eq!(state.kobold_state().temple_wardens.len(), 1);
    assert_eq!(state.kobold_state().temple_wardens[0].step, 1);
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(visible_guard(&registries, &replay), Some(route[1]));
    advance_to(&registries, &mut state, 10);
    advance_to(&registries, &mut replay, 10);
    assert_eq!(state, replay);
    assert_eq!(visible_guard(&registries, &state), Some(route[1]));
    assert_eq!(
        state.kobold_state().temple_wardens[0].step,
        1,
        "the warden cannot occupy the living intruder's voxel"
    );
    let elsewhere = VoxelCoord::new(route[2].x() + 4, route[2].y(), route[2].z());
    relocate_player_for_test(&mut state, elsewhere);
    advance_to(&registries, &mut state, 15);
    assert!(state.kobold_state().temple_wardens.is_empty());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn a_warden_takes_a_second_supported_step_but_never_enters_the_player() {
    let (registries, mut state, route) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(route[2].x(), route[2].y(), route[2].z() - 1),
    );
    advance_to(&registries, &mut state, 10);
    assert_eq!(visible_guard(&registries, &state), Some(route[2]));
    assert_eq!(state.kobold_state().temple_wardens[0].step, 2);
    assert_eq!(defending_guard(&registries, &state), Some(true));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(route[2].x() + 4, route[2].y(), route[2].z()),
    );
    advance_to(&registries, &mut state, 15);
    assert_eq!(state.kobold_state().temple_wardens[0].step, 1);
    advance_to(&registries, &mut state, 20);
    assert!(state.kobold_state().temple_wardens.is_empty());
}

#[test]
fn blocking_the_real_hall_throat_prevents_a_warden_move() {
    let (registries, mut state, route) = occupied_temple();
    set_generated_block_for_test(registries.world(), &mut state, route[1], BLOCK_STATE_STONE);
    advance_to(&registries, &mut state, 5);
    assert!(state.kobold_state().temple_wardens.is_empty());
    assert_eq!(visible_guard(&registries, &state), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn missing_stone_support_prevents_warden_advance() {
    let (registries, mut state, route) = occupied_temple();
    let floor = VoxelCoord::new(route[1].x(), route[1].y() - 1, route[1].z());
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        floor,
        crate::world::BlockStateId::AIR,
    );
    advance_to(&registries, &mut state, 5);
    assert_eq!(visible_guard(&registries, &state), Some(route[0]));
    assert!(state.kobold_state().temple_wardens.is_empty());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn removed_floor_strands_a_displaced_warden_without_corrupting_save_continuation() {
    let (registries, mut state, route) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    let floor = VoxelCoord::new(route[1].x(), route[1].y() - 1, route[1].z());
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        floor,
        crate::world::BlockStateId::AIR,
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 10);
    advance_to(&registries, &mut replay, 10);
    assert_eq!(state, replay);
    assert_eq!(visible_guard(&registries, &state), Some(route[1]));
    assert_eq!(state.kobold_state().temple_wardens[0].step, 1);
}

#[test]
fn internal_control_transaction_snapshot_preserves_and_isolates_temple_guard_progress() {
    let (registries, mut state, route) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    let mut staged = state.snapshot_for_control_transaction();
    assert_eq!(staged, state);
    relocate_player_for_test(
        &mut staged,
        VoxelCoord::new(route[2].x() + 4, route[2].y(), route[2].z()),
    );
    advance_to(&registries, &mut staged, 10);
    assert!(staged.kobold_state().temple_wardens.is_empty());
    assert_eq!(
        state.kobold_state().temple_wardens[0].step,
        1,
        "a rejected staged boundary cannot alter original persistent residents"
    );
    assert_eq!(validate_loaded_state(&registries, &staged), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn displaced_warden_finishes_walking_home_after_player_leaves_temple_chunk() {
    let (registries, mut state, route) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    assert_eq!(state.kobold_state().temple_wardens[0].step, 1);
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(route[2].x() + 24, route[2].y(), route[2].z()),
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 10);
    advance_to(&registries, &mut replay, 10);
    assert_eq!(state, replay);
    assert!(
        state.kobold_state().temple_wardens.is_empty(),
        "a displaced defender must finish its physical retreat rather than freeze forever offscreen"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn hostile_saved_warden_cursors_are_rejected_without_spawning_actor() {
    let (registries, mut state, _) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    let save = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut out_of_range = save.clone();
    out_of_range["state"]["systems"]["kobolds"]["temple_wardens"][0]["step"] = serde_json::json!(3);
    let loaded = serde_json::from_value::<LoadedSaveEnvelope>(out_of_range).unwrap();
    assert!(loaded.into_state(&registries).is_err());

    let mut duplicate = save.clone();
    let same = duplicate["state"]["systems"]["kobolds"]["temple_wardens"][0].clone();
    duplicate["state"]["systems"]["kobolds"]["temple_wardens"]
        .as_array_mut()
        .unwrap()
        .push(same);
    let loaded = serde_json::from_value::<LoadedSaveEnvelope>(duplicate).unwrap();
    assert!(loaded.into_state(&registries).is_err());

    let mut false_home = save.clone();
    false_home["state"]["systems"]["kobolds"]["temple_wardens"][0]["chunk"]["x"] =
        serde_json::json!(8);
    let loaded = serde_json::from_value::<LoadedSaveEnvelope>(false_home).unwrap();
    assert!(loaded.into_state(&registries).is_err());

    let mut missing = save;
    missing["state"]["systems"]["kobolds"]
        .as_object_mut()
        .unwrap()
        .remove("temple_wardens");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());
}
