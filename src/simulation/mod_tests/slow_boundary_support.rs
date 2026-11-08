//! Neutral fast-forward must retain control-rate falling when a slow event removes support.

use super::*;
use crate::content::{
    BASIC_TIMBER_COVER_PANEL_AREA, BASIC_TIMBER_COVER_PANEL_THICKNESS, FORM_BOARD,
    STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
};
use crate::core::state::validate_loaded_state;
use crate::logistics::player_body_is_grounded;
use crate::matter::calculate_matter_accounting;
use crate::spatial::VoxelBounds;
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
    validate_activate_structural_element, validate_start_player_structural_dismantling,
};

#[test]
fn due_dismantling_of_the_player_floor_must_replay_control_rate_gravity() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let floor = VoxelCoord::new(0, floor_y - 1, 0);
    set_generated_block_for_test(registries.world(), &mut state, floor, BlockStateId::AIR);
    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(floor, VoxelCoord::new(1, floor_y, 1)).unwrap(),
        BASIC_TIMBER_COVER_PANEL_THICKNESS,
        BASIC_TIMBER_COVER_PANEL_AREA,
    )
    .unwrap();
    let panel = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, panel, FORM_BOARD);
    let _ = validate_activate_structural_element(&registries, &state, panel)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(player_body_is_grounded(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let work = validate_start_player_structural_dismantling(
        &registries,
        &state,
        panel,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert!(work.completes_at().value() > state.tick().value() + 1);
    while state.tick().value() + 1 < work.completes_at().value() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        advance_tick(&registries, &mut state),
        Err(TickError::ControlSteppingRequired {
            current: before.control_tick(),
        }),
        "fast-forward would omit the boundary control sample after the floor vanishes"
    );
    assert_eq!(state, before);

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);
    let mut final_control = None;
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        final_control = Some(advance_control_tick(&registries, &mut replay).unwrap());
    }
    let final_control = final_control.unwrap();
    assert_eq!(replay.tick(), work.completes_at());
    assert!(
        final_control
            .simulation()
            .unwrap()
            .structural_dismantling()
            .is_some()
    );
    assert!(final_control.body_motion().is_some());
    assert!(replay.structures().get_element(panel).is_none());
    assert!(!player_body_is_grounded(&registries, &replay));
    assert!(
        replay
            .logistics()
            .player()
            .unwrap()
            .body()
            .vertical_velocity()
            .nanometers_per_second()
            < 0
    );
    assert_eq!(
        calculate_matter_accounting(&replay).unwrap().total(),
        matter_before,
        "dismantling transfers embodied matter to the reserved carried inventory"
    );
    assert_eq!(validate_loaded_state(&registries, &replay), Ok(()));
}

#[test]
fn temple_warden_departing_under_player_requires_boundary_gravity_step() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::player::validate_admit_player;
    use crate::world::{
        ChunkCoord, WorldGenerationSeed, temple_warden_route_at, validate_generate_chunk,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x0054_454D_504C_4553);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let route = temple_warden_route_at(&registries, &state, VoxelCoord::new(8, -19, 8))
        .expect("authored temple route");
    let post = route[0];
    // Stand on the warden's upper occupied voxel, with no stone floor:
    // the resident's real body is the only support at this height.
    for y in post.y() + 2..=post.y() + 4 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(post.x(), y, post.z()),
            BlockStateId::AIR,
        );
    }
    let perch = VoxelCoord::new(post.x(), post.y() + 2, post.z());
    validate_admit_player(&registries, &state, perch)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(player_body_is_grounded(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    // Quiet resident ticks retain the fast path and remain bit-identical
    // to the actual 20 Hz neutral sequence.
    let mut neutral = state.clone();
    while state.tick().value() < 4 {
        let fast = advance_tick(&registries, &mut state).unwrap();
        let mut boundary = None;
        for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
            boundary = Some(advance_control_tick(&registries, &mut neutral).unwrap());
        }
        assert_eq!(fast, *boundary.unwrap().simulation().unwrap());
        assert_eq!(state, neutral);
    }
    let before = state.clone();
    let mut control = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    let skipped = advance_tick(&registries, &mut state);
    let mut final_control = None;
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        final_control = Some(advance_control_tick(&registries, &mut control).unwrap());
    }
    assert!(
        !crate::kobolds::temple_caste_occupies_voxel(&registries, &control, post),
        "a real warden step must remove the moving floor"
    );
    assert!(final_control.unwrap().body_motion().is_some());
    assert!(!player_body_is_grounded(&registries, &control));
    assert_eq!(
        skipped,
        Err(TickError::ControlSteppingRequired {
            current: before.control_tick()
        })
    );
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &control), Ok(()));
}

#[test]
fn nearby_dismantling_preserves_fast_forward_when_the_player_floor_is_unchanged() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let nearby = VoxelCoord::new(1, floor_y - 1, 0);
    set_generated_block_for_test(registries.world(), &mut state, nearby, BlockStateId::AIR);
    let panel = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            VoxelBounds::new(nearby, VoxelCoord::new(2, floor_y, 1)).unwrap(),
            BASIC_TIMBER_COVER_PANEL_THICKNESS,
            BASIC_TIMBER_COVER_PANEL_AREA,
        )
        .unwrap(),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, panel, FORM_BOARD);
    let _ = validate_activate_structural_element(&registries, &state, panel)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let work = validate_start_player_structural_dismantling(
        &registries,
        &state,
        panel,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    while state.tick().value() + 1 < work.completes_at().value() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let before = state.clone();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    let fast = advance_tick(&registries, &mut state).unwrap();
    let mut normal = None;
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        normal = Some(advance_control_tick(&registries, &mut replay).unwrap());
    }
    assert_eq!(fast, *normal.unwrap().simulation().unwrap());
    assert_eq!(state, replay);
    assert!(state.structures().get_element(panel).is_none());
    assert_eq!(
        state.logistics().player().unwrap().body(),
        before.logistics().player().unwrap().body()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

/// A manual production output is stored on a narrow post at the player's
/// voxel. The post may share the actual floor's support chain, or stand on
/// independent ground. Both cases must preserve exact work and matter custody.
fn due_supported_craft_fixture(
    shared_support: bool,
) -> (Registries, AppState, crate::core::time::SimulationTick) {
    use crate::content::{FORM_LOG, FORM_LUMP, MATERIAL_STONE, PROCESS_KNAP_STONE_TOOL};
    use crate::core::quantity::{Area, Length, Mass, Temperature};
    use crate::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
    use crate::inventory::{
        MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_for_test,
        validate_mount_stockpile,
    };
    use crate::logistics::validate_place_ground_stockpile;
    use crate::material::CommodityKey;
    use crate::structural::validate_link_support;

    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let foot = VoxelCoord::new(0, floor_y, 0);
    let ground = VoxelCoord::new(0, floor_y - 1, 0);
    set_generated_block_for_test(registries.world(), &mut state, ground, BlockStateId::AIR);
    let floor = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            VoxelBounds::new(ground, VoxelCoord::new(1, floor_y, 1)).unwrap(),
            BASIC_TIMBER_COVER_PANEL_THICKNESS,
            BASIC_TIMBER_COVER_PANEL_AREA,
        )
        .unwrap(),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, floor, FORM_BOARD);
    let _ = validate_activate_structural_element(&registries, &state, floor)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    let source =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000)).unwrap();
    let destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000)).unwrap();
    let batch = Mass::from_milligrams(1_000_000);
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        batch,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    for stockpile in [source, destination] {
        validate_place_ground_stockpile(&state, stockpile, foot)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    let post = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            VoxelBounds::new(foot, VoxelCoord::new(1, floor_y + 1, 1)).unwrap(),
            Length::from_micrometers(1_000),
            Area::from_square_millimeters(1_000),
        ),
        !shared_support,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, post, FORM_LOG);
    if shared_support {
        let _ = validate_link_support(&registries, &state, post, floor)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    let _ = validate_activate_structural_element(&registries, &state, post)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = validate_mount_stockpile(&registries, &state, destination, post)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, batch),
            destination,
        ),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let completes_at = state.production().get_job(job).unwrap().completes_at();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    while state.tick().value() + 1 < completes_at.value() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(player_body_is_grounded(&registries, &state));
    (registries, state, completes_at)
}

#[test]
fn indexed_player_floor_members_agree_with_exact_structure_footprint() {
    use crate::logistics::{player_body_floor_structures, player_body_stands_above_structure};

    let (registries, terrain, _) = generated_grounded_player_at(0, 0);
    assert!(player_body_floor_structures(&registries, &terrain).is_empty());

    for shared_support in [true, false] {
        let (registries, state, _) = due_supported_craft_fixture(shared_support);
        let indexed = player_body_floor_structures(&registries, &state);
        let geometric = state
            .structures()
            .elements()
            .filter(|element| player_body_stands_above_structure(&registries, &state, element.id()))
            .map(|element| element.id())
            .collect::<Vec<_>>();
        assert_eq!(
            indexed, geometric,
            "indexed floors must agree with the exact body footprint for shared_support={shared_support}"
        );
        assert!(!indexed.is_empty());
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
}

#[test]
fn due_supported_output_on_player_floor_chain_requires_boundary_control() {
    let (registries, mut state, due) = due_supported_craft_fixture(true);
    let before = state.clone();
    assert_eq!(
        advance_tick(&registries, &mut state),
        Err(TickError::ControlSteppingRequired {
            current: before.control_tick()
        })
    );
    assert_eq!(state, before);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        let original = advance_control_tick(&registries, &mut state).unwrap();
        let restored = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(original, restored);
    }
    assert_eq!(state.tick(), due);
    assert_eq!(state, replay);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn unrelated_supported_output_preserves_neutral_fast_forward_parity() {
    let (registries, mut state, due) = due_supported_craft_fixture(false);
    let mut replay = state.clone();
    let fast = advance_tick(&registries, &mut state).unwrap();
    let mut final_frame = None;
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        final_frame = Some(advance_control_tick(&registries, &mut replay).unwrap());
    }
    assert_eq!(state.tick(), due);
    assert_eq!(fast, *final_frame.unwrap().simulation().unwrap());
    assert_eq!(state, replay);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

fn due_supported_construction_fixture(
    shared_support: bool,
) -> (Registries, AppState, crate::core::time::SimulationTick) {
    use crate::structural::{validate_link_support, validate_start_player_structural_construction};

    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let ground = VoxelCoord::new(0, floor_y - 1, 0);
    set_generated_block_for_test(registries.world(), &mut state, ground, BlockStateId::AIR);
    let floor = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            VoxelBounds::new(ground, VoxelCoord::new(1, floor_y, 1)).unwrap(),
            BASIC_TIMBER_COVER_PANEL_THICKNESS,
            BASIC_TIMBER_COVER_PANEL_AREA,
        )
        .unwrap(),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, floor, FORM_BOARD);
    let _ = validate_activate_structural_element(&registries, &state, floor)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    let member = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            VoxelBounds::new(
                VoxelCoord::new(1, floor_y - 1, 0),
                VoxelCoord::new(2, floor_y, 1),
            )
            .unwrap(),
            BASIC_TIMBER_COVER_PANEL_THICKNESS,
            BASIC_TIMBER_COVER_PANEL_AREA,
        )
        .unwrap(),
        !shared_support,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, member, FORM_BOARD);
    if shared_support {
        let _ = validate_link_support(&registries, &state, member, floor)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    let work = validate_start_player_structural_construction(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        member,
        &[],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    while state.tick().value() + 1 < work.completes_at().value() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(player_body_is_grounded(&registries, &state));
    (registries, state, work.completes_at())
}

#[test]
fn due_construction_on_shared_floor_support_requires_control_replay() {
    let (registries, mut state, due) = due_supported_construction_fixture(true);
    let before = state.clone();
    assert_eq!(
        advance_tick(&registries, &mut state),
        Err(TickError::ControlSteppingRequired {
            current: before.control_tick(),
        })
    );
    assert_eq!(state, before);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        let original = advance_control_tick(&registries, &mut state).unwrap();
        let restored = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(original, restored);
    }
    assert_eq!(state.tick(), due);
    assert_eq!(state, replay);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn due_independent_construction_keeps_neutral_fast_forward_parity() {
    let (registries, mut state, due) = due_supported_construction_fixture(false);
    let mut replay = state.clone();
    let fast = advance_tick(&registries, &mut state).unwrap();
    let mut final_frame = None;
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        final_frame = Some(advance_control_tick(&registries, &mut replay).unwrap());
    }
    assert_eq!(state.tick(), due);
    assert_eq!(fast, *final_frame.unwrap().simulation().unwrap());
    assert_eq!(state, replay);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
