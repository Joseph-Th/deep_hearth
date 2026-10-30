//! Contract tests for compact block-world storage, generation, persistence, and accounting.

use crate::content::{
    BLOCK_STATE_STONE, EQUIPMENT_STONE_PICK, FORM_CHIP, FORM_HANDLE, FORM_TOOL, MATERIAL_STONE,
    MATERIAL_WOOD, MINING_METHOD_HAND_PICK, build_registries,
};
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, TickSpan};
use crate::equipment::{EquipmentId, validate_assemble_equipment};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::{EatingWork, PlayerWork, PlayerWorkStartError, PlayerWorkValidationError};
use crate::logistics::{
    HorizontalControlInput, PlayerActiveSelection, PlayerControlInput, PlayerEquipmentCarryError,
    PlayerQuickAccessSlot, place_equipment_for_fixture, validate_drop_equipment,
    validate_pickup_equipment, validate_set_active_selection, validate_set_quick_access_slot,
};
use crate::maintenance::calculate_condition_after_active_ticks;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::player::{PlayerAdmissionCommitError, validate_admit_player};
use crate::registry::Registries;
use crate::simulation::{
    ActiveSelectionDropOutcome, PlayerDirectControlInput, PrimaryActionOutcome,
    PrimaryControlIntent, QuickAccessSelectionOutcome, advance_control_tick,
    advance_control_tick_with_direct_input, advance_control_tick_with_input,
};
use crate::spatial::{CubeFace, VIEW_DIRECTION_PARTS_PER_MILLION, ViewDirection, VoxelCoord};
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};
use crate::survival::{Vitality, player_record, project_survival_resource_budget};

use super::*;

#[path = "world_tests/batch_admission.rs"]
mod batch_admission;
#[path = "world_tests/chunk_coordinates.rs"]
mod chunk_coordinates;
#[path = "world_tests/excavation_selection.rs"]
mod excavation_selection;
#[path = "world_tests/graded_ore.rs"]
mod graded_ore;

#[path = "world_tests/graded_hematite.rs"]
mod graded_hematite;
#[path = "world_tests/palette.rs"]
mod palette;
#[path = "world_tests/progression.rs"]
mod progression;
#[path = "world_tests/revision_headroom.rs"]
mod revision_headroom;
#[path = "world_tests/targeting_structures.rs"]
mod targeting_structures;

const SEED: WorldGenerationSeed = WorldGenerationSeed::new(0xD33F_4845_4152_5448);

#[test]
fn world_registry_rejects_cell_mass_that_can_overflow_a_section_summary() {
    let heavy_block = BlockDefinitionId::new(99);
    let heavy_state = BlockStateId::new(99);
    let excessive_mass = Mass::from_milligrams(
        u64::MAX
            / u64::try_from(SECTION_CELL_COUNT)
                .unwrap_or_else(|_| unreachable!("section cell count fits u64"))
            + 1,
    );

    assert!(
        std::panic::catch_unwind(|| {
            WorldRegistry::new(
                [
                    BlockDefinition::new(
                        BlockDefinitionId::AIR,
                        "air",
                        BlockStateId::AIR,
                        BlockPhysicalDefinition::new(
                            None,
                            Mass::ZERO,
                            BlockCollisionShape::Empty,
                            false,
                        ),
                        None,
                    ),
                    BlockDefinition::new(
                        heavy_block,
                        "unrepresentably heavy cell",
                        heavy_state,
                        BlockPhysicalDefinition::new(
                            Some(CommodityKey::new(MATERIAL_STONE, FORM_CHIP)),
                            excessive_mass,
                            BlockCollisionShape::Empty,
                            false,
                        ),
                        None,
                    ),
                ],
                [
                    BlockStateDefinition::new(BlockStateId::AIR, BlockDefinitionId::AIR),
                    BlockStateDefinition::new(heavy_state, heavy_block),
                ],
            )
        })
        .is_err(),
        "authored cell mass must fit the section-level exact mass summary at maximum occupancy"
    );
}

fn generate(state: &mut AppState, coord: ChunkCoord) {
    let registries = build_registries();
    validate_generate_chunk(&registries, state, SEED, coord, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("terrain generation validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("terrain generation commit failed: {error}"));
}

#[test]
fn simulation_boundary_applies_body_motion_before_primary_action() {
    let (registries, mut state, _pick, _target) = generated_excavation_fixture();
    for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("boundary-primary setup control failed: {error}"));
    }
    let before = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("boundary-primary player disappeared before input"))
        .body();
    let movement = PlayerControlInput::new(
        HorizontalControlInput::new(0, VIEW_DIRECTION_PARTS_PER_MILLION)
            .unwrap_or_else(|error| panic!("boundary-primary movement input failed: {error}")),
    );
    let input = PlayerDirectControlInput::new(movement)
        .with_primary(PrimaryControlIntent::break_target(positive_x_view()));

    let control = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("boundary-primary control failed: {error}"));
    assert!(control.simulation().is_some());
    assert_eq!(
        control.primary_action(),
        Some(&PrimaryActionOutcome::BlockExcavationRejected(
            BlockExcavationError::PlayerWork(PlayerWorkStartError::PlayerMoving)
        ))
    );
    let after = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("boundary-primary player disappeared after input"))
        .body();
    assert_ne!(after.foot_position(), before.foot_position());
    assert_ne!(after.horizontal_velocity(), before.horizontal_velocity());
    assert_eq!(state.player_work().active(), None);
}

#[test]
fn simulation_boundary_uses_work_state_after_slow_completion_for_body_motion() {
    let (registries, mut state, _pick, _target) = generated_excavation_fixture();
    for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("completion-boundary setup control failed: {error}"));
    }
    let start = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(positive_x_view())),
    )
    .unwrap_or_else(|error| panic!("completion-boundary excavation start failed: {error}"));
    let completes_at = match start.primary_action() {
        Some(PrimaryActionOutcome::BlockExcavationStarted(start)) => start.completes_at(),
        other => panic!("completion-boundary excavation did not start: {other:?}"),
    };
    assert!(completes_at.is_simulation_boundary());
    while state.control_tick().value() + 1 < completes_at.value() {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("completion-boundary work advance failed: {error}"));
    }
    assert!(state.player_work().active().is_some());
    let before = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("completion-boundary player disappeared before completion"))
        .body();
    let movement = PlayerControlInput::new(
        HorizontalControlInput::new(0, VIEW_DIRECTION_PARTS_PER_MILLION)
            .unwrap_or_else(|error| panic!("completion-boundary movement input failed: {error}")),
    );

    let control = advance_control_tick_with_input(&registries, &mut state, movement)
        .unwrap_or_else(|error| panic!("completion-boundary control failed: {error}"));

    assert_eq!(control.control_tick(), completes_at);
    assert!(control.simulation().is_some());
    assert!(control.block_excavation().is_some());
    assert_eq!(state.player_work().active(), None);
    let after = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("completion-boundary player disappeared after completion"))
        .body();
    assert_ne!(after.foot_position(), before.foot_position());
    assert_ne!(after.horizontal_velocity(), before.horizontal_velocity());
}

fn assemble_stone_pick_at(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    position: VoxelCoord,
) -> EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("block excavation pick source failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            registries,
            state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("block excavation pick material failed: {error}"));
    }
    let pick = validate_assemble_equipment(registries, state, EQUIPMENT_STONE_PICK, source)
        .unwrap_or_else(|error| panic!("block excavation pick assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("block excavation pick commit failed: {error}"));
    place_equipment_for_fixture(state, pick, position);
    pick
}

/// Reuse the real equipment assembly/carry path for hard temple copper.
fn assemble_iron_pick_at(
    registries: &Registries,
    state: &mut AppState,
    position: VoxelCoord,
) -> EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000)).unwrap();
    for (commodity, mass) in [
        (
            CommodityKey::new(crate::content::MATERIAL_IRON, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            registries,
            state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let pick = validate_assemble_equipment(
        registries,
        state,
        crate::content::EQUIPMENT_IRON_PICK,
        source,
    )
    .unwrap()
    .commit(state)
    .unwrap();
    place_equipment_for_fixture(state, pick, position);
    pick
}

fn mine_entire_temple_device_with_direct_control(
    registries: &Registries,
    state: &mut AppState,
    target: VoxelCoord,
    aim: ViewDirection,
) -> (Mass, u32) {
    let authored_state = state.world().block_at(target).unwrap();
    let original = state
        .world()
        .represented_mass_at(registries.world(), target)
        .unwrap();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let initial_total = calculate_matter_accounting(state).unwrap().total();
    let mut recovered = 0_u64;
    let mut batches = 0_u32;
    // Every batch is a real player primary action and a complete work tick
    // sequence. No direct world mutation is allowed to create a breach.
    while state.world().block_at(target) == Some(authored_state) {
        assert!(batches < 64, "the finite temple device must be excavatable");
        let control = advance_control_tick_with_direct_input(
            registries,
            state,
            PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::break_target(aim)),
        )
        .unwrap();
        let started = match control.primary_action() {
            Some(PrimaryActionOutcome::BlockExcavationStarted(started)) => *started,
            other => panic!("temple device must authorize carried iron-tool excavation: {other:?}"),
        };
        assert_eq!(started.target(), target);
        assert_eq!(state.world().block_at(target), Some(authored_state));
        if batches == 0 {
            let encoded = serde_json::to_vec(&SaveEnvelope::new(registries, state)).unwrap();
            let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
                .unwrap()
                .into_state(registries)
                .unwrap();
            assert_eq!(
                loaded, *state,
                "in-progress player work must replay losslessly"
            );
        }
        let mut completed = None;
        while state.control_tick() < started.completes_at() {
            let outcome = advance_control_tick(registries, state).unwrap();
            if let Some(receipt) = outcome.block_excavation() {
                assert!(completed.replace(receipt).is_none());
            }
        }
        let receipt = completed.expect("timed temple-device mining must finish");
        assert_eq!(receipt.target(), target);
        let lot = match receipt.output() {
            BlockExcavationOutputReceipt::Carried { lot } => lot,
            BlockExcavationOutputReceipt::Surface { resource } => {
                panic!("enough carried storage existed; unexpected world drop {resource:?}")
            }
        };
        let fragment = state.inventory().get_lot(lot).unwrap();
        assert_eq!(fragment.stockpile(), carried);
        assert_eq!(
            fragment.commodity(),
            CommodityKey::new(crate::content::MATERIAL_COPPER, crate::content::FORM_SCRAP),
            "the mined machine must yield actual recoverable copper, not generic mass"
        );
        recovered += receipt.excavated_mass().milligrams();
        batches += 1;
        assert_eq!(
            recovered
                + state
                    .world()
                    .represented_mass_at(registries.world(), target)
                    .unwrap()
                    .milligrams(),
            original.milligrams(),
        );
        assert_eq!(
            calculate_matter_accounting(state).unwrap().total(),
            initial_total
        );
        if batches == 1 {
            assert_eq!(
                state.world().block_at(target),
                Some(authored_state),
                "partial mining must not falsely signal a missing device"
            );
        }
    }
    assert!(
        batches > 1,
        "a large installed machine needs multiple tool actions"
    );
    assert_eq!(recovered, original.milligrams());
    assert_eq!(state.world().block_at(target), Some(BlockStateId::AIR));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        original
    );
    assert_eq!(validate_loaded_state(registries, state), Ok(()));
    (original, batches)
}

#[test]
fn direct_player_excavation_of_a_real_temple_drive_triggers_the_guard_watch() {
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

    let sample = VoxelCoord::new(8, -19, 8);
    let route = temple_warden_route_at(&registries, &state, sample).unwrap();
    let center = route[2];
    let drive = VoxelCoord::new(center.x() + 5, center.y(), center.z());
    let player = VoxelCoord::new(center.x() + 2, center.y(), center.z());
    assert_eq!(
        state.world().block_at(drive),
        Some(crate::content::BLOCK_STATE_TEMPLE_DRIVE)
    );
    let original = state
        .world()
        .represented_mass_at(registries.world(), drive)
        .unwrap();
    assert_eq!(original, Mass::from_milligrams(17_920_000));
    assert_eq!(state.world().block_at(player), Some(BlockStateId::AIR));

    let pick = assemble_iron_pick_at(&registries, &mut state, player);
    validate_admit_player(&registries, &state, player)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::equipment(pick)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();

    // The eye is above the floor-level drive. Aim down a physically clear
    // generated passage rather than moving the authored machine into a fixture.
    let aim = ViewDirection::new(928_477, -371_391, 0).unwrap();
    assert_eq!(
        raycast_player_block(&registries, &state, aim)
            .unwrap()
            .map(PlayerBlockTarget::voxel),
        Some(drive),
    );

    let initial_total = calculate_matter_accounting(&state).unwrap().total();
    let (mined, _) =
        mine_entire_temple_device_with_direct_control(&registries, &mut state, drive, aim);
    assert_eq!(mined, original);

    // Let the actual resident notice the excavated housing on its next third
    // engine round. Nothing teleports to the machine on the completion tick.
    crate::logistics::relocate_player_for_test(
        &mut state,
        VoxelCoord::new(center.x() - 4, center.y(), center.z()),
    );
    let round = ((state.tick().value() / 360) + 1) * 360;
    while state.tick().value() < round + 30 {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    let by_drive = VoxelCoord::new(center.x() + 4, center.y(), center.z());
    crate::logistics::relocate_player_for_test(
        &mut state,
        VoxelCoord::new(by_drive.x(), by_drive.y(), by_drive.z() + 1),
    );
    let seen = crate::kobolds::available_nearby_kobold_scouts(&registries, &state)
        .find(|sighting| sighting.role() == crate::kobolds::KoboldRole::TempleWarden)
        .expect("local player can observe the inspector beside the removed drive");
    assert_eq!(seen.position(), by_drive);
    assert!(
        seen.is_wary(),
        "copper removed by a player gives the same actual machine watch"
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial_total
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn direct_player_excavation_of_the_generated_archive_causes_a_real_caste_vigil() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x0054_454D_504C_4553),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let route = temple_warden_route_at(&registries, &state, VoxelCoord::new(8, -19, 8)).unwrap();
    let center = route[2];
    let archive = VoxelCoord::new(center.x() - 5, center.y(), center.z());
    let player = VoxelCoord::new(center.x() - 2, center.y(), center.z());
    let original = state
        .world()
        .represented_mass_at(registries.world(), archive)
        .unwrap();
    assert_eq!(
        state.world().block_at(archive),
        Some(crate::content::BLOCK_STATE_TEMPLE_ARCHIVE)
    );
    assert_eq!(original, Mass::from_milligrams(8_960_000));
    assert_eq!(state.world().block_at(player), Some(BlockStateId::AIR));

    let pick = assemble_iron_pick_at(&registries, &mut state, player);
    validate_admit_player(&registries, &state, player)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::equipment(pick)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();

    let aim = ViewDirection::new(-928_477, -371_391, 0).unwrap();
    assert_eq!(
        raycast_player_block(&registries, &state, aim)
            .unwrap()
            .map(PlayerBlockTarget::voxel),
        Some(archive),
        "the original copper archive must be targetable from the actual chamber"
    );
    let before = calculate_matter_accounting(&state).unwrap().total();
    let (mined, _) =
        mine_entire_temple_device_with_direct_control(&registries, &mut state, archive, aim);
    assert_eq!(mined, original);

    // After the work, the player stays at the actual open maintenance throat.
    // The archivist must walk the full route before discovering the missing shell.
    crate::logistics::relocate_player_for_test(
        &mut state,
        VoxelCoord::new(center.x() - 4, center.y(), center.z()),
    );
    let start = ((state.tick().value() / 240) + 1) * 240;
    while state.tick().value() < start + 40 {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.world().block_at(archive), Some(BlockStateId::AIR));
    let archive_seen = crate::kobolds::available_nearby_kobold_scouts(&registries, &state)
        .find(|seen| seen.role() == crate::kobolds::KoboldRole::TempleArchivist)
        .expect("the attending archivist is visible beside the missing shell");
    assert!(archive_seen.is_defending());
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);

    let next_round = ((state.tick().value() / 120) + 1) * 120;
    while state.tick().value() < next_round + 10 {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    // The observing player is still within the archivist's locally simulated
    // sanctuary. Its watcher communicates only through the actual line of sight.
    crate::logistics::relocate_player_for_test(
        &mut state,
        VoxelCoord::new(center.x() + 1, center.y(), center.z() + 1),
    );
    let guard = crate::kobolds::available_nearby_kobold_scouts(&registries, &state)
        .find(|seen| seen.role() == crate::kobolds::KoboldRole::TempleWarden)
        .expect("the central-hall warden is locally observable");
    assert_eq!(guard.position(), center);
    assert!(
        guard.is_wary(),
        "excavating the real archive warns the actual warden"
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

fn generated_excavation_fixture() -> (
    crate::registry::Registries,
    AppState,
    EquipmentId,
    VoxelCoord,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("block excavation terrain generation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("block excavation terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("block excavation terrain column has no solid surface"));
    let player_position = VoxelCoord::new(0, surface_y + 1, 0);
    let eye_y = player_position.y() + 1;
    for x in 1..=4 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, eye_y, 0),
            BlockStateId::AIR,
        );
    }
    let target = VoxelCoord::new(3, eye_y, 0);
    set_generated_block_for_test(registries.world(), &mut state, target, BLOCK_STATE_STONE);
    let pick = assemble_stone_pick_at(&registries, &mut state, player_position);
    validate_admit_player(&registries, &state, player_position)
        .unwrap_or_else(|error| panic!("block excavation player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("block excavation player commit failed: {error}"));
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("block excavation pick pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("block excavation pick pickup commit failed: {error}"));
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::equipment(pick)),
    )
    .unwrap_or_else(|error| panic!("block excavation active-pick validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("block excavation active-pick commit failed: {error}"));
    (registries, state, pick, target)
}

#[test]
fn quick_access_switch_precedes_primary_action_on_the_same_control_step() {
    let (registries, mut state, pick, _target) = generated_excavation_fixture();
    let tool_slot = PlayerQuickAccessSlot::new(1)
        .unwrap_or_else(|| panic!("same-step quick-access tool slot disappeared"));
    validate_set_quick_access_slot(
        &registries,
        &state,
        tool_slot,
        Some(PlayerActiveSelection::equipment(pick)),
    )
    .unwrap_or_else(|error| panic!("same-step quick-access tool binding failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("same-step quick-access tool binding commit failed: {error}"));
    assert_eq!(state.logistics().active_selection(), None);

    let input = PlayerDirectControlInput::NEUTRAL
        .with_quick_access_slot(tool_slot)
        .with_primary(PrimaryControlIntent::break_target(positive_x_view()));
    let control = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("same-step quick-access control failed: {error}"));

    assert_eq!(
        control.quick_access_selection(),
        Some(&QuickAccessSelectionOutcome::Selected {
            slot: tool_slot,
            selection: Some(PlayerActiveSelection::equipment(pick)),
        })
    );
    assert!(matches!(
        control.primary_action(),
        Some(PrimaryActionOutcome::BlockExcavationStarted(_))
    ));
    assert_eq!(state.logistics().active_quick_access_slot(), tool_slot);
}

fn make_next_tick_fatal(registries: &crate::registry::Registries, state: &mut AppState) {
    let physiology = registries.survival().physiology();
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("fatal block excavation fixture player disappeared"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            Energy::ZERO,
            player.hydration(),
            Vitality::from_parts_per_million_unchecked(
                physiology.starvation_vitality_loss_ppm_per_tick(),
            ),
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
}

fn generated_targeting_player_at(x: i64, z: i64) -> (crate::registry::Registries, AppState, i64) {
    let registries = build_registries();
    let mut state = AppState::new();
    let coord = ChunkCoord::containing(VoxelCoord::new(x, 0, z));
    validate_generate_chunk(&registries, &state, SEED, coord, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("targeting terrain generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("targeting terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(x, *y, z)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("targeting terrain column has no solid surface"));
    let floor_y = surface_y + 1;
    validate_admit_player(&registries, &state, VoxelCoord::new(x, floor_y, z))
        .unwrap_or_else(|error| panic!("targeting player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("targeting player commit failed: {error}"));
    (registries, state, floor_y)
}

#[test]
fn partial_cell_mass_keeps_collision_until_the_last_milligram_is_removed() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("partial-cell fixture has no stone surface"));
    let target = VoxelCoord::new(0, surface_y, 0);
    let default_mass = registries
        .world()
        .represented_mass_for_state(BLOCK_STATE_STONE)
        .unwrap_or_else(|| panic!("stone represented mass disappeared"));
    let revision = state.world().revision();
    state.world_state_mut().apply_excavated_cell_mass(
        revision,
        revision + 1,
        target,
        BLOCK_STATE_STONE,
        default_mass,
        Mass::from_milligrams(1),
    );
    assert_eq!(state.world().block_at(target), Some(BLOCK_STATE_STONE));
    assert_eq!(
        state
            .world()
            .represented_mass_at(registries.world(), target),
        Some(Mass::from_milligrams(1))
    );
    assert_eq!(
        validate_loaded_world(registries.world(), state.world()),
        Ok(())
    );

    let revision = state.world().revision();
    state.world_state_mut().apply_excavated_cell_mass(
        revision,
        revision + 1,
        target,
        BLOCK_STATE_STONE,
        default_mass,
        Mass::ZERO,
    );
    assert_eq!(state.world().block_at(target), Some(BlockStateId::AIR));
    assert_eq!(
        state
            .world()
            .represented_mass_at(registries.world(), target),
        Some(Mass::ZERO)
    );
    assert_eq!(
        validate_loaded_world(registries.world(), state.world()),
        Ok(())
    );
}

#[test]
fn fatal_tick_cancels_unfinished_block_excavation_without_extracting_matter() {
    let (registries, mut state, pick, target) = generated_excavation_fixture();
    let batch = Mass::from_milligrams(200_000);
    let source_mass = state
        .world()
        .represented_mass_at(registries.world(), target)
        .unwrap_or_else(|| panic!("fatal block excavation target mass disappeared"));
    let carried = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("fatal block excavation player disappeared"))
        .carried_stockpile();
    let condition_before = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("fatal block excavation pick disappeared"))
        .condition();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("fatal block excavation matter baseline failed: {error}"));
    let start = validate_start_player_block_excavation(
        &registries,
        &state,
        BlockExcavationRequest::new(positive_x_view(), MINING_METHOD_HAND_PICK, pick, batch),
    )
    .unwrap_or_else(|error| panic!("fatal block excavation validation failed: {error}"));
    let start = start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("fatal block excavation commit failed: {error}"));
    assert!(start.completes_at().value() > state.tick().value() + 1);
    make_next_tick_fatal(&registries, &mut state);

    let mut excavation_outcome = None;
    while state.tick().value() < 1 {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("fatal block excavation control tick failed: {error}"));
        if let Some(outcome) = control.block_excavation() {
            excavation_outcome = Some(outcome);
        }
    }
    assert_eq!(excavation_outcome, None);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(state.world().block_at(target), Some(BLOCK_STATE_STONE));
    assert_eq!(
        state
            .world()
            .represented_mass_at(registries.world(), target),
        Some(source_mass)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| (stockpile.stored_mass(), stockpile.reserved_inbound())),
        Some((Mass::ZERO, Mass::ZERO))
    );
    let expected_condition = calculate_condition_after_active_ticks(
        registries
            .mining()
            .get_method(MINING_METHOD_HAND_PICK)
            .unwrap_or_else(|| panic!("fatal block excavation mining method disappeared"))
            .condition_wear_ppm_per_active_tick(),
        condition_before,
        TickSpan::new(1),
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.condition()),
        Some(expected_condition)
    );
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    let matter_after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("fatal block excavation matter final failed: {error}"));
    assert_eq!(
        matter_after.total(),
        matter_before.total(),
        "world {:?}->{:?}, photosynthesis {:?}->{:?}, surface {:?}->{:?}, stored {:?}->{:?}",
        matter_before.world(),
        matter_after.world(),
        matter_before.photosynthetic_input(),
        matter_after.photosynthetic_input(),
        matter_before.surface(),
        matter_after.surface(),
        matter_before.stored(),
        matter_after.stored(),
    );
}

#[test]
fn trusted_load_rejects_block_excavation_source_mass_tamper() {
    let (registries, mut state, pick, _target) = generated_excavation_fixture();
    let _start = validate_start_player_block_excavation(
        &registries,
        &state,
        BlockExcavationRequest::new(
            positive_x_view(),
            MINING_METHOD_HAND_PICK,
            pick,
            Mass::from_milligrams(200_000),
        ),
    )
    .unwrap_or_else(|error| panic!("block excavation tamper validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("block excavation tamper commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("block excavation tamper serialization failed: {error}"));
    let source_mass = encoded["state"]["systems"]["player_work"]["active"]
        ["BlockExcavation"]["work"]["source_mass_before"]
        .as_u64()
        .unwrap_or_else(|| panic!("serialized block excavation source mass is not an integer"));
    encoded["state"]["systems"]["player_work"]["active"]["BlockExcavation"]["work"]["source_mass_before"] =
        serde_json::json!(source_mass - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("block excavation tamper decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::BlockExcavationSourceMassMismatch
        )))
    );
}

#[test]
fn trusted_load_rejects_active_block_excavation_with_invalid_body_motion() {
    let (registries, mut state, pick, _target) = generated_excavation_fixture();
    let _start = validate_start_player_block_excavation(
        &registries,
        &state,
        BlockExcavationRequest::new(
            positive_x_view(),
            MINING_METHOD_HAND_PICK,
            pick,
            Mass::from_milligrams(200_000),
        ),
    )
    .unwrap_or_else(|error| panic!("moving-work load validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("moving-work load commit failed: {error}"));
    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("moving-work serialization failed: {error}"));
    let mut moving = encoded.clone();
    moving["state"]["systems"]["logistics"]["player"]["body"]["horizontal_velocity"]["x_nanometers_per_second"] =
        serde_json::json!(1_i64);
    let moving: LoadedSaveEnvelope = serde_json::from_value(moving)
        .unwrap_or_else(|error| panic!("moving-work tamper decode failed: {error}"));
    assert_eq!(
        moving.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::PlayerMoving
        )))
    );

    let mut upward = encoded.clone();
    upward["state"]["systems"]["logistics"]["player"]["body"]["vertical_velocity"]["nanometers_per_second"] =
        serde_json::json!(1_i64);
    let upward: LoadedSaveEnvelope = serde_json::from_value(upward)
        .unwrap_or_else(|error| panic!("upward-work tamper decode failed: {error}"));
    assert_eq!(
        upward.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::PlayerMoving
        )))
    );

    let mut airborne = encoded;
    let foot_y = airborne["state"]["systems"]["logistics"]["player"]["body"]
        ["foot_position"]["y_nanometers"]
        .as_i64()
        .unwrap_or_else(|| panic!("serialized active-work foot height is not an i64"));
    airborne["state"]["systems"]["logistics"]["player"]["body"]["foot_position"]["y_nanometers"] = serde_json::json!(
        foot_y
            .checked_add(1)
            .unwrap_or_else(|| panic!("active-work foot height cannot be incremented"))
    );
    let airborne: LoadedSaveEnvelope = serde_json::from_value(airborne)
        .unwrap_or_else(|error| panic!("airborne-work tamper decode failed: {error}"));
    assert_eq!(
        airborne.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::PlayerAirborne
        )))
    );
}

#[test]
fn chunk_generation_preserves_active_excavation_world_revision_headroom() {
    let (registries, state, pick, _target) = generated_excavation_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("world-headroom serialization failed: {error}"));
    encoded["state"]["systems"]["world"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("world-headroom decode failed: {error}"));
    let mut state = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("near-exhausted world fixture should load: {error}"));
    let _ = validate_start_player_block_excavation(
        &registries,
        &state,
        BlockExcavationRequest::new(
            positive_x_view(),
            MINING_METHOD_HAND_PICK,
            pick,
            Mass::from_milligrams(200_000),
        ),
    )
    .unwrap_or_else(|error| panic!("world-headroom excavation validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("world-headroom excavation commit failed: {error}"));
    assert_eq!(state.world().revision(), u64::MAX - 1);
    let before = state.clone();

    assert_eq!(
        validate_generate_player_neighborhood(&registries, &state).err(),
        Some(ChunkGenerationError::WorldRevisionExhausted)
    );
    assert_eq!(state, before);
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            SEED,
            ChunkCoord::new(1, 0),
            BLOCK_STATE_STONE,
        )
        .err(),
        Some(ChunkGenerationError::WorldRevisionExhausted)
    );
    assert_eq!(state, before);
}

#[test]
fn player_block_excavation_conserves_partial_cell_matter_and_round_trips_active_work() {
    let (registries, mut state, pick, target) = generated_excavation_fixture();
    let batch = Mass::from_milligrams(200_000);
    let source_mass = state
        .world()
        .represented_mass_at(registries.world(), target)
        .unwrap_or_else(|| panic!("block excavation target mass disappeared"));
    let carried = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("block excavation player disappeared"))
        .carried_stockpile();
    let condition_before = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("block excavation pick disappeared"))
        .condition();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("block excavation matter baseline failed: {error}"));

    let start = validate_start_player_block_excavation(
        &registries,
        &state,
        BlockExcavationRequest::new(positive_x_view(), MINING_METHOD_HAND_PICK, pick, batch),
    )
    .unwrap_or_else(|error| panic!("block excavation validation failed: {error}"));
    let outcome = start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("block excavation start commit failed: {error}"));
    assert_eq!(outcome.target(), target);
    assert_eq!(outcome.excavated_mass(), batch);
    assert_eq!(state.world().block_at(target), Some(BLOCK_STATE_STONE));
    assert_eq!(
        state
            .world()
            .represented_mass_at(registries.world(), target),
        Some(source_mass)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(batch)
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("active block excavation serialization failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("active block excavation decode failed: {error}"));
    assert_eq!(
        loaded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("active block excavation trusted load failed: {error}")),
        state
    );

    let mut completion = None;
    while state.control_tick() < outcome.completes_at() {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("block excavation control tick failed: {error}"));
        if let Some(excavation) = control.block_excavation() {
            assert!(completion.replace(excavation).is_none());
        }
    }
    let completion = completion.unwrap_or_else(|| panic!("block excavation produced no outcome"));
    let remaining = source_mass
        .checked_sub(batch)
        .unwrap_or_else(|| panic!("block excavation test batch exceeds source mass"));
    assert_eq!(completion.target(), target);
    assert_eq!(completion.excavated_mass(), batch);
    assert_eq!(completion.remaining_mass(), remaining);
    assert_eq!(state.world().block_at(target), Some(BLOCK_STATE_STONE));
    assert_eq!(
        state
            .world()
            .represented_mass_at(registries.world(), target),
        Some(remaining)
    );
    let lot = match completion.output() {
        BlockExcavationOutputReceipt::Carried { lot } => lot,
        BlockExcavationOutputReceipt::Surface { resource } => {
            panic!(
                "carried excavation unexpectedly produced surface resource {}",
                resource.value()
            )
        }
    };
    let output = state
        .inventory()
        .get_lot(lot)
        .unwrap_or_else(|| panic!("block excavation output lot disappeared"));
    assert_eq!(output.stockpile(), carried);
    assert_eq!(output.mass(), batch);
    assert_eq!(
        output.commodity(),
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(Mass::ZERO)
    );
    assert!(
        state
            .equipment()
            .get_equipment(pick)
            .is_some_and(|record| record.condition() < condition_before)
    );
    assert_eq!(state.player_work().active(), None);
    let matter_after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("block excavation final matter accounting failed: {error}"));
    assert_eq!(
        matter_after.total(),
        matter_before.total(),
        "world {:?}->{:?}, photosynthesis {:?}->{:?}, surface {:?}->{:?}, stored {:?}->{:?}",
        matter_before.world(),
        matter_after.world(),
        matter_before.photosynthetic_input(),
        matter_after.photosynthetic_input(),
        matter_before.surface(),
        matter_after.surface(),
        matter_before.stored(),
        matter_after.stored(),
    );
    assert_eq!(
        matter_after.world(),
        matter_before
            .world()
            .checked_sub(crate::core::quantity::AggregateMass::from_mass(batch))
            .and_then(|remaining| remaining.checked_add(
                matter_after
                    .photosynthetic_input()
                    .checked_sub(matter_before.photosynthetic_input())
                    .expect("photosynthetic input never decreases")
            ))
            .unwrap_or_else(|| panic!("block excavation world matter overflow"))
    );
    assert_eq!(
        matter_after.stored(),
        matter_before
            .stored()
            .checked_add(crate::core::quantity::AggregateMass::from_mass(batch))
            .unwrap_or_else(|| panic!("block excavation stored matter overflow"))
    );
}

#[test]
fn off_boundary_block_excavation_replays_and_preserves_authored_total_cost() {
    let (registries, mut state, pick, _target) = generated_excavation_fixture();
    for _ in 0..37 {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("off-boundary setup control tick failed: {error}"));
        assert!(control.simulation().is_none());
    }
    let started_at = state.control_tick();
    assert!(!started_at.is_simulation_boundary());
    let survival_before = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("off-boundary excavation survival player disappeared"));
    let condition_before = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("off-boundary excavation pick disappeared"))
        .condition();
    let batch = Mass::from_milligrams(200_000);
    let start = validate_start_player_block_excavation(
        &registries,
        &state,
        BlockExcavationRequest::new(positive_x_view(), MINING_METHOD_HAND_PICK, pick, batch),
    )
    .unwrap_or_else(|error| panic!("off-boundary excavation validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("off-boundary excavation commit failed: {error}"));
    let duration_control_ticks = start
        .completes_at()
        .checked_steps_since(started_at)
        .unwrap_or_else(|| panic!("off-boundary excavation completion preceded admission"));
    assert_eq!(
        duration_control_ticks % CONTROL_TICKS_PER_SIMULATION_TICK,
        0,
        "control-rate scheduling must preserve the authored slow-tick duration exactly"
    );
    assert_eq!(
        start.completes_at().value() % CONTROL_TICKS_PER_SIMULATION_TICK,
        started_at.value() % CONTROL_TICKS_PER_SIMULATION_TICK,
        "control-rate excavation must complete at the same phase at which it started"
    );
    let duration = TickSpan::new(duration_control_ticks / CONTROL_TICKS_PER_SIMULATION_TICK);
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("off-boundary excavation mining method disappeared"));
    let expected_survival = project_survival_resource_budget(
        registries.survival().physiology(),
        method.exertion(),
        duration,
    )
    .unwrap_or_else(|error| {
        panic!("off-boundary excavation survival projection failed: {error:?}")
    });
    let expected_condition = calculate_condition_after_active_ticks(
        method.condition_wear_ppm_per_active_tick(),
        condition_before,
        duration,
    );

    for _ in 0..20 {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("off-boundary pre-save control tick failed: {error}"));
    }
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("off-boundary excavation serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("off-boundary excavation decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("off-boundary excavation trusted load failed: {error}"));
    assert_eq!(loaded, state);

    let mut completed_off_boundary = false;
    while state.control_tick() < start.completes_at() {
        let expected = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("off-boundary source replay failed: {error}"));
        let actual = advance_control_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("off-boundary loaded replay failed: {error}"));
        assert_eq!(actual, expected);
        if expected.block_excavation().is_some() {
            completed_off_boundary = expected.simulation().is_none();
        }
    }
    assert!(
        completed_off_boundary,
        "non-boundary admission must finish on its exact control tick rather than waiting for slow simulation"
    );
    assert_eq!(loaded, state);
    assert_eq!(state.player_work().active(), None);
    let survival_after = state.survival().player().copied().unwrap_or_else(|| {
        panic!("off-boundary excavation survival player disappeared after work")
    });
    assert_eq!(
        survival_before
            .metabolic_energy()
            .checked_sub(survival_after.metabolic_energy()),
        Some(expected_survival.metabolic_energy())
    );
    assert_eq!(
        survival_before
            .hydration()
            .checked_sub(survival_after.hydration()),
        Some(expected_survival.hydration())
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.condition()),
        Some(expected_condition)
    );
}

#[test]
fn primary_excavation_requires_active_equipment_context() {
    let (registries, mut state, _pick, _target) = generated_excavation_fixture();
    validate_set_active_selection(&registries, &state, None)
        .unwrap_or_else(|error| panic!("empty-hand validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("empty-hand commit failed: {error}"));
    let input = PlayerDirectControlInput::NEUTRAL
        .with_primary(PrimaryControlIntent::break_target(positive_x_view()));

    let control = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("empty-hand primary control failed: {error}"));
    assert_eq!(
        control.primary_action(),
        Some(&PrimaryActionOutcome::BlockExcavationRejected(
            BlockExcavationError::NoActiveEquipment
        ))
    );
    assert_eq!(state.player_work().active(), None);
}

#[test]
fn direct_drop_moves_active_equipment_back_to_world_custody() {
    let (registries, mut state, pick, _target) = generated_excavation_fixture();
    let position = state
        .logistics()
        .player()
        .map(|player| player.position())
        .unwrap_or_else(|| panic!("direct-drop fixture lost player"));
    let before = crate::logistics::assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("direct-drop carrying assessment disappeared"));
    let input = PlayerDirectControlInput::NEUTRAL.with_drop_active_selection();

    let control = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("direct active-equipment drop failed: {error}"));
    assert_eq!(
        control.active_selection_drop(),
        Some(&ActiveSelectionDropOutcome::EquipmentDropped {
            equipment: pick,
            position,
        })
    );
    assert_eq!(state.logistics().active_selection(), None);
    assert!(!state.logistics().is_equipment_carried(pick));
    assert_eq!(
        state.logistics().stationary_equipment_position(pick),
        Some(position)
    );
    let after = crate::logistics::assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("direct-drop post assessment disappeared"));
    assert_eq!(after.equipment(), Mass::ZERO);
    assert_eq!(
        after.available(),
        before
            .available()
            .checked_add(before.equipment())
            .unwrap_or_else(|| panic!("direct-drop capacity recovery overflowed"))
    );
}

#[test]
fn direct_drop_reports_empty_active_hand_without_mutating_custody() {
    let (registries, mut state, pick, _target) = generated_excavation_fixture();
    validate_set_active_selection(&registries, &state, None)
        .unwrap_or_else(|error| panic!("direct-drop empty-hand validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("direct-drop empty-hand commit failed: {error}"));

    let control = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_drop_active_selection(),
    )
    .unwrap_or_else(|error| panic!("direct-drop empty-hand control failed: {error}"));
    assert_eq!(
        control.active_selection_drop(),
        Some(&ActiveSelectionDropOutcome::NoActiveSelection)
    );
    assert!(state.logistics().is_equipment_carried(pick));
    assert_eq!(state.logistics().stationary_equipment_position(pick), None);
}

#[test]
fn simultaneous_primary_use_reserves_active_tool_before_direct_drop() {
    let (registries, mut state, pick, _target) = generated_excavation_fixture();
    let input = PlayerDirectControlInput::NEUTRAL
        .with_primary(PrimaryControlIntent::break_target(positive_x_view()))
        .with_drop_active_selection();

    let control = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("simultaneous primary/drop control failed: {error}"));
    assert!(matches!(
        control.primary_action(),
        Some(PrimaryActionOutcome::BlockExcavationStarted(_))
    ));
    assert_eq!(
        control.active_selection_drop(),
        Some(&ActiveSelectionDropOutcome::EquipmentRejected(
            PlayerEquipmentCarryError::PlayerBusy
        ))
    );
    assert_eq!(
        state.logistics().active_selection(),
        Some(PlayerActiveSelection::equipment(pick))
    );
    assert!(state.logistics().is_equipment_carried(pick));
    assert_eq!(state.logistics().stationary_equipment_position(pick), None);
}

#[test]
fn held_primary_excavation_chains_the_next_batch_on_the_completion_control_step() {
    let (registries, mut state, _pick, target) = generated_excavation_fixture();
    let batch = Mass::from_milligrams(200_000);
    let source_mass = state
        .world()
        .represented_mass_at(registries.world(), target)
        .unwrap_or_else(|| panic!("held excavation target mass disappeared"));
    let input = PlayerDirectControlInput::NEUTRAL
        .with_primary(PrimaryControlIntent::break_target(positive_x_view()));

    let first_start = loop {
        let control = advance_control_tick_with_direct_input(&registries, &mut state, input)
            .unwrap_or_else(|error| panic!("held excavation initial control failed: {error}"));
        match control.primary_action() {
            Some(PrimaryActionOutcome::BlockExcavationStarted(start)) => break *start,
            Some(PrimaryActionOutcome::BlockExcavationRejected(error)) => {
                panic!("held excavation initial admission was rejected: {error}")
            }
            Some(other) => panic!("held excavation initial control returned {other:?}"),
            None => {}
        }
    };
    assert_eq!(first_start.target(), target);
    assert_eq!(first_start.excavated_mass(), batch);
    assert!(matches!(
        state.player_work().active(),
        Some(crate::labor::PlayerWork::BlockExcavation { work })
            if work.method() == MINING_METHOD_HAND_PICK
    ));

    let (first_completion, second_start) = loop {
        let control = advance_control_tick_with_direct_input(&registries, &mut state, input)
            .unwrap_or_else(|error| panic!("held excavation continuation failed: {error}"));
        match control.primary_action() {
            Some(PrimaryActionOutcome::BlockExcavationStarted(start)) => {
                let completion = control.block_excavation().unwrap_or_else(|| {
                    panic!("held excavation restarted without completing the prior batch")
                });
                break (completion, *start);
            }
            Some(PrimaryActionOutcome::BlockExcavationRejected(error)) => {
                panic!("held excavation continuation was rejected: {error}")
            }
            Some(other) => panic!("held excavation continuation returned {other:?}"),
            None => {}
        }
    };

    let remaining = source_mass
        .checked_sub(batch)
        .unwrap_or_else(|| panic!("held excavation batch exceeds source mass"));
    assert_eq!(first_completion.target(), target);
    assert_eq!(first_completion.excavated_mass(), batch);
    assert_eq!(first_completion.remaining_mass(), remaining);
    assert_eq!(second_start.target(), target);
    assert_eq!(second_start.excavated_mass(), batch);
    assert_eq!(
        state
            .world()
            .represented_mass_at(registries.world(), target),
        Some(remaining),
        "the newly chained batch must not remove matter before its own completion"
    );
    let carried = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("held excavation player disappeared"))
        .carried_stockpile();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| (stockpile.stored_mass(), stockpile.reserved_inbound())),
        Some((batch, batch)),
        "the completed batch should be stored while the chained batch owns exactly one inbound reservation"
    );
}

#[test]
fn held_primary_excavation_drops_a_full_batch_when_carried_capacity_is_short() {
    let (registries, mut state, pick, target) = generated_excavation_fixture();
    let carried = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("capacity-blocked excavation player disappeared"))
        .carried_stockpile();
    let batch = Mass::from_milligrams(200_000);
    let remaining_capacity = Mass::from_milligrams(199_999);
    let available_before = state
        .inventory()
        .get_stockpile(carried)
        .unwrap_or_else(|| panic!("capacity-blocked carried stockpile disappeared"))
        .available_capacity();
    let filler = available_before
        .checked_sub(remaining_capacity)
        .unwrap_or_else(|| panic!("capacity-blocked fixture has insufficient carried capacity"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        filler,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("capacity-blocked carried filler failed: {error}"));
    let source_mass = state
        .world()
        .represented_mass_at(registries.world(), target)
        .unwrap_or_else(|| panic!("capacity-blocked target mass disappeared"));
    let player_position = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("capacity-blocked player disappeared"))
        .position();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("capacity-blocked matter baseline failed: {error}"));
    let input = PlayerDirectControlInput::NEUTRAL
        .with_primary(PrimaryControlIntent::break_target(positive_x_view()));

    let start = loop {
        let control = advance_control_tick_with_direct_input(&registries, &mut state, input)
            .unwrap_or_else(|error| panic!("capacity-blocked control failed: {error}"));
        match control.primary_action() {
            Some(PrimaryActionOutcome::BlockExcavationStarted(start)) => break *start,
            Some(PrimaryActionOutcome::BlockExcavationRejected(error)) => {
                panic!(
                    "capacity-blocked held excavation was rejected instead of dropping output: {error}"
                )
            }
            Some(other) => panic!("capacity-blocked primary control returned {other:?}"),
            None => {}
        }
    };
    assert_eq!(start.excavated_mass(), batch);
    assert_eq!(
        start.output(),
        crate::labor::BlockExcavationOutputCustody::Surface {
            position: player_position,
        }
    );
    let work = match state.player_work().active() {
        Some(crate::labor::PlayerWork::BlockExcavation { work }) => work,
        other => panic!("capacity-blocked excavation did not retain player work: {other:?}"),
    };
    assert_eq!(
        work.output(),
        crate::labor::BlockExcavationOutputCustody::Surface {
            position: player_position,
        }
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(Mass::ZERO),
        "surface-routed excavation must not reserve carried capacity"
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("surface-routed excavation serialization failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("surface-routed excavation decode failed: {error}"));
    assert_eq!(
        loaded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!(
                "surface-routed excavation trusted load failed: {error}"
            )),
        state
    );
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("surface-route tamper serialization failed: {error}"));
    tampered["state"]["systems"]["player_work"]["active"]["BlockExcavation"]["work"]["output"]["Surface"]
        ["position"]["x"] = serde_json::json!(player_position.x() + 1);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("surface-route tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::BlockExcavationSurfacePositionMismatch {
                stored: VoxelCoord::new(
                    player_position.x() + 1,
                    player_position.y(),
                    player_position.z(),
                ),
                player: player_position,
            }
        )))
    );
    let mut exhausted = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("surface-id exhaustion serialization failed: {error}"));
    exhausted["state"]["systems"]["surface"]["next_resource_id"] = serde_json::json!(u32::MAX);
    let exhausted: LoadedSaveEnvelope = serde_json::from_value(exhausted)
        .unwrap_or_else(|error| panic!("surface-id exhaustion decode failed: {error}"));
    assert_eq!(
        exhausted.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::FutureSurfaceResourceIdCapacityExhausted {
                next_resource_id: u32::MAX,
                required: 1,
            }
        ))
    );
    assert_eq!(
        state
            .world()
            .represented_mass_at(registries.world(), target),
        Some(source_mass),
        "surface-routed excavation must not reduce represented world matter before completion"
    );

    let (completion, initial_pickup) = loop {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("surface-drop excavation advancement failed: {error}"));
        if let Some(completion) = control.block_excavation() {
            break (completion, control.surface_drop_pickup());
        }
    };
    let resource = match completion.output() {
        BlockExcavationOutputReceipt::Surface { resource } => resource,
        BlockExcavationOutputReceipt::Carried { lot } => {
            panic!(
                "capacity-blocked excavation unexpectedly created carried lot {}",
                lot.value()
            )
        }
    };
    let drop = state
        .surface()
        .get(resource)
        .unwrap_or_else(|| panic!("capacity-blocked excavation surface drop disappeared"));
    let initial_pickup = initial_pickup.unwrap_or_else(|| {
        panic!("surface-routed excavation did not fill available carried capacity")
    });
    assert_eq!(initial_pickup.resource(), resource);
    assert_eq!(initial_pickup.destination(), carried);
    assert_eq!(initial_pickup.picked_mass(), remaining_capacity);
    let surface_remainder = batch
        .checked_sub(remaining_capacity)
        .unwrap_or_else(|| panic!("initial partial pickup exceeds excavation batch"));
    assert_eq!(drop.kind(), crate::surface::SurfaceResourceKind::Drop);
    assert_eq!(drop.position(), player_position);
    assert_eq!(drop.remaining_mass(), surface_remainder);
    assert_eq!(
        drop.commodity(),
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP)
    );
    assert_eq!(drop.created_at(), state.tick());
    let encoded_drop = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("completed surface-drop serialization failed: {error}"));
    let loaded_drop: LoadedSaveEnvelope = serde_json::from_slice(&encoded_drop)
        .unwrap_or_else(|error| panic!("completed surface-drop decode failed: {error}"));
    let loaded_drop = loaded_drop
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("completed surface-drop trusted load failed: {error}"));
    assert_eq!(
        loaded_drop
            .surface()
            .get(resource)
            .map(|record| record.kind()),
        Some(crate::surface::SurfaceResourceKind::Drop)
    );
    assert_eq!(loaded_drop, state);
    let remaining = source_mass
        .checked_sub(batch)
        .unwrap_or_else(|| panic!("surface-drop batch exceeds source mass"));
    assert_eq!(
        state
            .world()
            .represented_mass_at(registries.world(), target),
        Some(remaining)
    );
    let matter_after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("surface-drop matter accounting failed: {error}"));
    assert_eq!(
        matter_after.total(),
        matter_before.total(),
        "world {:?}->{:?}, photosynthesis {:?}->{:?}, surface {:?}->{:?}, stored {:?}->{:?}",
        matter_before.world(),
        matter_after.world(),
        matter_before.photosynthetic_input(),
        matter_after.photosynthetic_input(),
        matter_before.surface(),
        matter_after.surface(),
        matter_before.stored(),
        matter_after.stored(),
    );
    assert_eq!(
        matter_after.world(),
        matter_before
            .world()
            .checked_sub(crate::core::quantity::AggregateMass::from_mass(batch))
            .and_then(|remaining| remaining.checked_add(
                matter_after
                    .photosynthetic_input()
                    .checked_sub(matter_before.photosynthetic_input())
                    .expect("photosynthetic input never decreases")
            ))
            .unwrap_or_else(|| panic!("surface-drop world matter overflow"))
    );
    assert_eq!(
        matter_after.surface(),
        matter_before
            .surface()
            .checked_add(crate::core::quantity::AggregateMass::from_mass(
                surface_remainder
            ))
            .unwrap_or_else(|| panic!("surface-drop matter overflow"))
    );
    assert_eq!(
        matter_after.stored(),
        matter_before
            .stored()
            .checked_add(crate::core::quantity::AggregateMass::from_mass(
                remaining_capacity
            ))
            .unwrap_or_else(|| panic!("surface-drop initial pickup stored matter overflow"))
    );

    validate_drop_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("surface-drop pickup capacity release failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("surface-drop pickup capacity release commit failed: {error}")
        });
    let pickup_tick = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("surface-drop automatic pickup tick failed: {error}"));
    let pickup = pickup_tick
        .surface_drop_pickup()
        .unwrap_or_else(|| panic!("fitting local excavation drop was not picked up automatically"));
    assert_eq!(pickup.resource(), resource);
    assert_eq!(pickup.destination(), carried);
    assert_eq!(pickup.picked_mass(), surface_remainder);
    let picked = state
        .inventory()
        .get_lot(pickup.lot())
        .unwrap_or_else(|| panic!("automatic surface-drop pickup lot disappeared"));
    assert_eq!(picked.stockpile(), carried);
    assert_eq!(picked.mass(), batch);
    assert_eq!(
        picked.commodity(),
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP)
    );
    let depleted = state
        .surface()
        .get(resource)
        .unwrap_or_else(|| panic!("automatic pickup removed surface identity"));
    assert_eq!(depleted.remaining_mass(), Mass::ZERO);
    assert_eq!(
        depleted.lifecycle(),
        crate::surface::SurfaceResourceLifecycle::Depleted
    );
    let matter_picked = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("automatic pickup matter accounting failed: {error}"));
    assert_eq!(matter_picked.total(), matter_before.total());
    assert_eq!(matter_picked.surface(), matter_before.surface());
    assert_eq!(
        matter_picked.stored(),
        matter_before
            .stored()
            .checked_add(crate::core::quantity::AggregateMass::from_mass(batch))
            .unwrap_or_else(|| panic!("automatic pickup stored matter overflow"))
    );
}

fn positive_x_view() -> ViewDirection {
    ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0)
        .unwrap_or_else(|error| panic!("positive-X view fixture failed: {error}"))
}

#[test]
fn player_block_targeting_requires_complete_player_admission() {
    let registries = build_registries();
    assert_eq!(
        raycast_player_block(&registries, &AppState::new(), positive_x_view()),
        Err(PlayerBlockTargetError::PlayerNotAdmitted)
    );
}

#[test]
fn player_block_targeting_hits_first_full_cube_and_reports_entry_face() {
    let (registries, mut state, floor_y) = generated_targeting_player_at(0, 0);
    let eye_voxel_y = floor_y + 1;
    for x in 1..=4 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, eye_voxel_y, 0),
            BlockStateId::AIR,
        );
    }
    let target_voxel = VoxelCoord::new(3, eye_voxel_y, 0);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        target_voxel,
        BLOCK_STATE_STONE,
    );

    let target = raycast_player_block(&registries, &state, positive_x_view())
        .unwrap_or_else(|error| panic!("player block raycast failed: {error}"))
        .unwrap_or_else(|| panic!("player block raycast missed authored target"));
    assert_eq!(target.voxel(), target_voxel);
    assert_eq!(target.state(), BLOCK_STATE_STONE);
    assert_eq!(target.face(), CubeFace::West);
}

#[test]
fn player_block_targeting_includes_exact_reach_boundary_and_excludes_beyond_it() {
    let (registries, mut state, floor_y) = generated_targeting_player_at(0, 0);
    let eye_voxel_y = floor_y + 1;
    for x in 1..=6 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, eye_voxel_y, 0),
            BlockStateId::AIR,
        );
    }
    let exact_reach = VoxelCoord::new(5, eye_voxel_y, 0);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        exact_reach,
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        raycast_player_block(&registries, &state, positive_x_view())
            .unwrap_or_else(|error| panic!("exact-reach raycast failed: {error}"))
            .map(PlayerBlockTarget::voxel),
        Some(exact_reach)
    );

    set_generated_block_for_test(
        registries.world(),
        &mut state,
        exact_reach,
        BlockStateId::AIR,
    );
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(6, eye_voxel_y, 0),
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        raycast_player_block(&registries, &state, positive_x_view())
            .unwrap_or_else(|error| panic!("beyond-reach raycast failed: {error}")),
        None
    );
}

#[test]
fn player_block_targeting_fails_closed_at_missing_chunk_authority() {
    let (registries, state, floor_y) = generated_targeting_player_at(15, 0);
    let unavailable = VoxelCoord::new(16, floor_y + 1, 0);
    assert_eq!(
        raycast_player_block(&registries, &state, positive_x_view()),
        Err(PlayerBlockTargetError::WorldAuthorityUnavailable { voxel: unavailable })
    );
}

#[test]
fn player_block_targeting_uses_deterministic_axis_precedence_at_corner_entry() {
    let (registries, mut state, floor_y) = generated_targeting_player_at(0, 0);
    let eye_voxel_y = floor_y + 1;
    for voxel in [
        VoxelCoord::new(1, eye_voxel_y, 0),
        VoxelCoord::new(0, eye_voxel_y, 1),
        VoxelCoord::new(1, eye_voxel_y, 1),
    ] {
        set_generated_block_for_test(registries.world(), &mut state, voxel, BlockStateId::AIR);
    }
    let diagonal_target = VoxelCoord::new(1, eye_voxel_y, 1);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        diagonal_target,
        BLOCK_STATE_STONE,
    );
    let direction = ViewDirection::new(707_107, 0, 707_107)
        .unwrap_or_else(|error| panic!("diagonal view fixture failed: {error}"));
    let target = raycast_player_block(&registries, &state, direction)
        .unwrap_or_else(|error| panic!("diagonal player block raycast failed: {error}"))
        .unwrap_or_else(|| panic!("diagonal player block raycast missed target"));
    assert_eq!(target.voxel(), diagonal_target);
    assert_eq!(target.face(), CubeFace::West);
}

#[test]
fn diagonal_block_targeting_checks_face_neighbors_before_corner_cell() {
    let (registries, mut state, floor_y) = generated_targeting_player_at(0, 0);
    let eye_y = floor_y + 1;
    let x_neighbor = VoxelCoord::new(1, eye_y, 0);
    let z_neighbor = VoxelCoord::new(0, eye_y, 1);
    let diagonal = VoxelCoord::new(1, eye_y, 1);
    for voxel in [x_neighbor, z_neighbor, diagonal] {
        set_generated_block_for_test(registries.world(), &mut state, voxel, BlockStateId::AIR);
    }
    let direction = ViewDirection::new(707_107, 0, 707_107)
        .unwrap_or_else(|error| panic!("diagonal occlusion view failed: {error}"));
    set_generated_block_for_test(registries.world(), &mut state, diagonal, BLOCK_STATE_STONE);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        z_neighbor,
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        raycast_player_block(&registries, &state, direction)
            .unwrap_or_else(|error| panic!("Z-neighbor targeting failed: {error}"))
            .map(PlayerBlockTarget::voxel),
        Some(z_neighbor),
        "a solid face neighbor must not be skipped to reach the diagonal block"
    );
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        x_neighbor,
        BLOCK_STATE_STONE,
    );
    let first = raycast_player_block(&registries, &state, direction)
        .unwrap_or_else(|error| panic!("two-neighbor targeting failed: {error}"))
        .unwrap_or_else(|| panic!("two blocked face neighbors were not targeted"));
    assert_eq!(first.voxel(), x_neighbor);
    assert_eq!(first.face(), CubeFace::West);
}

#[test]
fn structural_air_targeting_cannot_select_through_a_solid_corner() {
    let (registries, mut state, floor_y) = generated_targeting_player_at(0, 0);
    let eye_y = floor_y + 1;
    let x_neighbor = VoxelCoord::new(1, eye_y, 0);
    let z_neighbor = VoxelCoord::new(0, eye_y, 1);
    let diagonal = VoxelCoord::new(1, eye_y, 1);
    for voxel in [x_neighbor, z_neighbor, diagonal] {
        set_generated_block_for_test(registries.world(), &mut state, voxel, BlockStateId::AIR);
    }
    let direction = ViewDirection::new(707_107, 0, 707_107)
        .unwrap_or_else(|error| panic!("structural corner view failed: {error}"));
    let select_diagonal = |voxel, _face| (voxel == diagonal).then_some(voxel);
    assert_eq!(
        raycast_player_air_voxel(&registries, &state, direction, select_diagonal),
        Ok(Some(diagonal))
    );
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        z_neighbor,
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        raycast_player_air_voxel(&registries, &state, direction, select_diagonal),
        Ok(None),
        "structural selection must not pass an occluding terrain corner"
    );
}

#[test]
fn diagonal_targeting_fails_closed_for_missing_face_neighbor_chunk() {
    let (registries, state, floor_y) = generated_targeting_player_at(15, 15);
    let direction = ViewDirection::new(707_107, 0, 707_107)
        .unwrap_or_else(|error| panic!("missing-corner view failed: {error}"));
    assert_eq!(
        raycast_player_block(&registries, &state, direction),
        Err(PlayerBlockTargetError::WorldAuthorityUnavailable {
            voxel: VoxelCoord::new(16, floor_y + 1, 15),
        })
    );
}

#[test]
fn player_block_targeting_derives_eye_origin_from_persisted_stance() {
    let (registries, mut state, floor_y) = generated_targeting_player_at(0, 0);
    for x in 1..=4 {
        for y in floor_y + 1..=floor_y + 4 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(x, y, 0),
                BlockStateId::AIR,
            );
        }
    }
    let target_voxel = VoxelCoord::new(3, floor_y + 3, 0);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        target_voxel,
        BLOCK_STATE_STONE,
    );
    let direction = ViewDirection::new(866_025, 500_000, 0)
        .unwrap_or_else(|error| panic!("stance-sensitive view fixture failed: {error}"));

    let standing = raycast_player_block(&registries, &state, direction)
        .unwrap_or_else(|error| panic!("standing stance raycast failed: {error}"))
        .unwrap_or_else(|| panic!("standing stance raycast missed target"));
    assert_eq!(standing.voxel(), target_voxel);
    assert_eq!(standing.face(), CubeFace::West);

    let _ = advance_control_tick_with_input(
        &registries,
        &mut state,
        PlayerControlInput::NEUTRAL.with_crouch(true),
    )
    .unwrap_or_else(|error| panic!("crouching stance control tick failed: {error}"));
    let crouching = raycast_player_block(&registries, &state, direction)
        .unwrap_or_else(|error| panic!("crouching stance raycast failed: {error}"))
        .unwrap_or_else(|| panic!("crouching stance raycast missed target"));
    assert_eq!(crouching.voxel(), target_voxel);
    assert_eq!(crouching.face(), CubeFace::Bottom);
}

#[test]
fn terrain_generation_persists_one_canonical_profile_and_rejects_seed_changes() {
    let registries = build_registries();
    let mut state = AppState::new();
    let first = ChunkCoord::new(0, 0);
    generate(&mut state, first);
    let profile = WorldGenerationProfile::new(SEED, BLOCK_STATE_STONE);
    assert_eq!(state.world().generation_profile(), Some(profile));

    let requested = WorldGenerationProfile::new(
        WorldGenerationSeed::new(SEED.value().wrapping_add(1)),
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            requested.seed(),
            ChunkCoord::new(1, 0),
            requested.solid_state(),
        )
        .err(),
        Some(ChunkGenerationError::GenerationProfileConflict)
    );
    assert_eq!(state.world().chunks().len(), 1);
}

#[test]
fn player_chunk_demand_requires_profile_then_generates_missing_neighborhood_once() {
    let registries = build_registries();
    let mut missing_profile = AppState::new();
    validate_admit_player(&registries, &missing_profile, VoxelCoord::new(0, 100, 0))
        .unwrap_or_else(|error| panic!("demand missing-profile admission failed: {error}"))
        .commit(&mut missing_profile)
        .unwrap_or_else(|error| panic!("demand missing-profile commit failed: {error}"));
    assert_eq!(
        validate_generate_player_neighborhood(&registries, &missing_profile).err(),
        Some(ChunkGenerationError::GenerationProfileMissing)
    );

    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("demand terrain column has no solid surface"));
    validate_admit_player(&registries, &state, VoxelCoord::new(0, surface_y + 1, 0))
        .unwrap_or_else(|error| panic!("demand player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("demand player commit failed: {error}"));
    let revision_before = state.world().revision();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("demand matter accounting before failed: {error}"))
        .world();

    let demand = validate_generate_player_neighborhood(&registries, &state)
        .unwrap_or_else(|error| panic!("player chunk demand failed: {error}"))
        .unwrap_or_else(|| panic!("player chunk demand found no missing neighborhood"));
    assert_eq!(demand.chunks().len(), 8);
    let generated = demand
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player chunk demand commit failed: {error}"));
    assert_eq!(generated.len(), 8);
    assert_eq!(state.world().chunks().len(), 9);
    assert_eq!(state.world().revision(), revision_before + 1);
    assert_eq!(
        state.world().generation_profile(),
        Some(WorldGenerationProfile::new(SEED, BLOCK_STATE_STONE))
    );
    let matter_after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("demand matter accounting after failed: {error}"))
        .world();
    assert!(matter_after > matter_before);
    assert!(
        validate_generate_player_neighborhood(&registries, &state)
            .unwrap_or_else(|error| panic!("idempotent player chunk demand failed: {error}"))
            .is_none()
    );
}

#[test]
fn player_chunk_demand_commit_is_world_revision_bound() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("stale-demand terrain column has no solid surface"));
    validate_admit_player(&registries, &state, VoxelCoord::new(0, surface_y + 1, 0))
        .unwrap_or_else(|error| panic!("stale-demand player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale-demand player commit failed: {error}"));
    let demand = validate_generate_player_neighborhood(&registries, &state)
        .unwrap_or_else(|error| panic!("stale-demand validation failed: {error}"))
        .unwrap_or_else(|| panic!("stale-demand fixture has no missing chunks"));
    let expected = state.world().revision();
    validate_generate_chunk(
        &registries,
        &state,
        SEED,
        ChunkCoord::new(2, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("stale-demand intervening chunk failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("stale-demand intervening commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        demand.commit(&mut state),
        Err(ChunkGenerationCommitError::StaleWorldRevision {
            expected,
            actual: expected + 1,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn chunk_coordinates_use_euclidean_partitioning_across_zero() {
    for (x, expected) in [(-17, -2), (-16, -1), (-1, -1), (0, 0), (15, 0), (16, 1)] {
        assert_eq!(
            ChunkCoord::containing(VoxelCoord::new(x, 0, x)),
            ChunkCoord::new(expected, expected)
        );
    }
}

#[test]
fn trusted_load_rejects_generated_chunks_without_generation_profile() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("world profile tamper serialization failed: {error}"));
    encoded["state"]["systems"]["world"]["generation_profile"] = serde_json::Value::Null;
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("world profile tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::World(
            WorldValidationError::ChunksWithoutGenerationProfile
        )))
    );
}

#[test]
fn trusted_load_rejects_air_generation_profile() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("air profile tamper serialization failed: {error}"));
    encoded["state"]["systems"]["world"]["generation_profile"]["solid_state"] =
        serde_json::json!(BlockStateId::AIR.value());
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("air profile tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::World(
            WorldValidationError::AirGenerationSolidState
        )))
    );
}

#[test]
fn terrain_generation_is_deterministic_compact_and_seed_sensitive() {
    let registries = build_registries();
    let coord = ChunkCoord::new(-2, 3);
    let first = validate_generate_chunk(
        &registries,
        &AppState::new(),
        SEED,
        coord,
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("first terrain generation failed: {error}"));
    let replay = validate_generate_chunk(
        &registries,
        &AppState::new(),
        SEED,
        coord,
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("replay terrain generation failed: {error}"));
    assert_eq!(first.chunk(), replay.chunk());

    let different = validate_generate_chunk(
        &registries,
        &AppState::new(),
        WorldGenerationSeed::new(SEED.value() + 1),
        coord,
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("alternate terrain generation failed: {error}"));
    assert_ne!(first.chunk(), different.chunk());

    let homogeneous = first
        .chunk()
        .sections()
        .find(|section| section.non_air_count() == SECTION_CELL_COUNT as u16)
        .unwrap_or_else(|| panic!("generated terrain lost its homogeneous deep section"));
    assert_eq!(homogeneous.palette(), &[BLOCK_STATE_STONE]);
    assert_eq!(homogeneous.packed_word_count(), 0);
}

#[test]
fn generated_chunk_distinguishes_air_from_absent_world_truth() {
    let mut state = AppState::new();
    let coord = ChunkCoord::new(0, 0);
    generate(&mut state, coord);

    assert_eq!(
        state.world().block_at(VoxelCoord::new(0, WORLD_MIN_Y, 0)),
        Some(BLOCK_STATE_STONE)
    );
    assert_eq!(
        state.world().block_at(VoxelCoord::new(0, 128, 0)),
        Some(BlockStateId::AIR)
    );
    assert_eq!(
        state
            .world()
            .block_at(VoxelCoord::new(CHUNK_SIDE as i64, 0, 0)),
        None
    );
    assert_eq!(
        state
            .world()
            .block_at(VoxelCoord::new(0, WORLD_MAX_Y_EXCLUSIVE, 0)),
        None
    );
}

#[test]
fn generated_terrain_is_included_in_exact_matter_accounting() {
    let registries = build_registries();
    let mut state = AppState::new();
    // A non-temple column still contains seeded mineral lenses in host rock.
    // Compute its expected mass from each independently registered voxel state,
    // not by assuming every occupied voxel has the density of stone.
    let ordinary = (0..8)
        .map(|x| ChunkCoord::new(x, 0))
        .find(|chunk| super::generation::temple::TempleSite::for_chunk(SEED, *chunk).is_none())
        .expect("one of eight columns is not the temple site");
    generate(&mut state, ordinary);

    let mut expected = 0u128;
    for x in ordinary.world_min_x().unwrap()..ordinary.world_min_x().unwrap() + CHUNK_SIDE as i64 {
        for z in
            ordinary.world_min_z().unwrap()..ordinary.world_min_z().unwrap() + CHUNK_SIDE as i64
        {
            for y in WORLD_MIN_Y..=4 {
                let block = state.world().block_at(VoxelCoord::new(x, y, z)).unwrap();
                if block != BlockStateId::AIR {
                    expected += u128::from(
                        registries
                            .world()
                            .represented_mass_for_state(block)
                            .unwrap()
                            .milligrams(),
                    );
                }
            }
        }
    }
    let accounting = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("world matter accounting failed: {error}"));

    let woody_mass: u128 = state
        .world()
        .trees()
        .map(|tree| u128::from(tree.wood_at(state.tick()).milligrams()))
        .sum();
    assert_eq!(accounting.world().milligrams(), expected + woody_mass);
    assert_eq!(
        accounting.total().milligrams(),
        expected + woody_mass - accounting.photosynthetic_input().milligrams()
    );
}

#[test]
fn terrain_generation_rejects_unrepresentable_chunk_coordinates() {
    let registries = build_registries();
    let state = AppState::new();
    let chunk = ChunkCoord::new(i64::MAX, 0);

    assert_eq!(
        validate_generate_chunk(&registries, &state, SEED, chunk, BLOCK_STATE_STONE,).err(),
        Some(ChunkGenerationError::ChunkCoordinateOverflow { chunk })
    );
}

#[test]
fn chunk_generation_commit_is_revision_bound_and_does_not_overwrite_world_truth() {
    let registries = build_registries();
    let mut state = AppState::new();
    let first_coord = ChunkCoord::new(0, 0);
    let second_coord = ChunkCoord::new(1, 0);
    let first = validate_generate_chunk(&registries, &state, SEED, first_coord, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("first chunk validation failed: {error}"));
    let stale = validate_generate_chunk(&registries, &state, SEED, second_coord, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("stale chunk validation failed: {error}"));

    assert_eq!(first.commit(&mut state), Ok(first_coord));
    let before = state.clone();
    assert_eq!(
        stale.commit(&mut state),
        Err(ChunkGenerationCommitError::StaleWorldRevision {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before);
    assert_eq!(
        validate_generate_chunk(&registries, &state, SEED, first_coord, BLOCK_STATE_STONE,).err(),
        Some(ChunkGenerationError::AlreadyGenerated { chunk: first_coord })
    );
}

#[test]
fn terrain_generation_rejects_player_body_overlap_without_world_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player_voxel = VoxelCoord::new(0, WORLD_MIN_Y, 0);
    validate_admit_player(&registries, &state, player_voxel)
        .unwrap_or_else(|error| panic!("body-collision admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("body-collision admission commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            SEED,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .err(),
        Some(ChunkGenerationError::PlayerBodyCollision {
            voxel: player_voxel,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn terrain_generation_rejects_burying_existing_exposed_surface_matter() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, WORLD_MIN_Y, 0);
    add_unloaded_surface_drop(&registries, &mut state, position);
    let before = state.clone();
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            SEED,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE
        )
        .err(),
        Some(ChunkGenerationError::SurfaceResourceCollision { voxel: position }),
        "generated full-cube terrain cannot bury an existing exposed material parcel"
    );
    assert_eq!(state, before);
}

fn add_unloaded_surface_drop(registries: &Registries, state: &mut AppState, position: VoxelCoord) {
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        commodity,
        Mass::from_milligrams(25_000),
        Temperature::from_millikelvin(293_150),
        crate::material::MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    validate_create_surface_resources(registries, state, [spec])
        .unwrap()
        .commit(state)
        .unwrap();
}

#[test]
fn chunk_generation_surface_collision_preserves_identity_order_and_excludes_remote_history() {
    let registries = build_registries();
    let mut state = AppState::new();
    let first = VoxelCoord::new(15, WORLD_MIN_Y, 15);
    let second = VoxelCoord::new(0, WORLD_MIN_Y, 0);
    for position in [
        first,
        second,
        VoxelCoord::new(0, WORLD_MIN_Y, 32),
        VoxelCoord::new(2, 100, 2),
    ] {
        add_unloaded_surface_drop(&registries, &mut state, position);
    }
    let before = state.clone();
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            SEED,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE
        )
        .err(),
        Some(ChunkGenerationError::SurfaceResourceCollision { voxel: first }),
        "a spatial index must not change the oldest conflicting resource's diagnostic"
    );
    assert_eq!(state, before);
    let first_id = state
        .surface()
        .available_resources_at(Some(first))
        .next()
        .unwrap()
        .id();
    let revision = state.surface().revision() + 1;
    let tick = state.tick();
    state.surface_state_mut().apply_gathering(
        first_id,
        Mass::from_milligrams(25_000),
        tick,
        revision,
    );
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            SEED,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE
        )
        .err(),
        Some(ChunkGenerationError::SurfaceResourceCollision { voxel: second }),
        "depleted historical parcels must not block terrain or shadow later collision reports"
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, restored);
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &restored,
            SEED,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE
        )
        .err(),
        Some(ChunkGenerationError::SurfaceResourceCollision { voxel: second })
    );
}

#[test]
fn terrain_generation_commit_rejects_surface_matter_added_after_preflight() {
    let registries = build_registries();
    let mut state = AppState::new();
    let planned = validate_generate_chunk(
        &registries,
        &state,
        SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap();
    let expected = state.surface().revision();
    add_unloaded_surface_drop(&registries, &mut state, VoxelCoord::new(0, WORLD_MIN_Y, 0));
    let before = state.clone();
    assert_eq!(
        planned.commit(&mut state),
        Err(ChunkGenerationCommitError::StaleSurfaceRevision {
            expected,
            actual: state.surface().revision(),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn terrain_generation_preserves_surface_drop_above_generated_solid() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 100, 0);
    add_unloaded_surface_drop(&registries, &mut state, position);
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
    assert_eq!(state.world().block_at(position), Some(BlockStateId::AIR));
    assert_eq!(
        state
            .surface()
            .available_resources_at(Some(position))
            .count(),
        1
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn trusted_load_rejects_exposed_matter_inside_generated_full_cube() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    // Test-only terrain mutation forges the invalid state after otherwise
    // valid resource creation, bypassing ordinary placement checks.
    let voxel = VoxelCoord::new(0, 100, 0);
    add_unloaded_surface_drop(&registries, &mut state, voxel);
    set_generated_block_for_test(registries.world(), &mut state, voxel, BLOCK_STATE_STONE);
    let resource = state.surface().resources().next().unwrap().id();
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Err(StateValidationError::Surface(
            crate::surface::SurfaceResourceValidationError::EmbeddedInSolidWorld {
                resource,
                voxel,
            }
        )),
        "a forged surface parcel inside a solid generated cell must not be admitted"
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
fn surface_creation_cannot_place_available_matter_inside_generated_stone() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let voxel = VoxelCoord::new(0, WORLD_MIN_Y, 0);
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        voxel,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(25_000),
        Temperature::from_millikelvin(293_150),
        crate::material::MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    assert!(
        validate_create_surface_resources(&registries, &state, [spec]).is_err(),
        "ordinary surface creation must not commit a state that trusted load would reject"
    );
    assert!(state.surface().resources().next().is_none());
}

#[test]
fn surface_creation_commit_cannot_cross_world_generation_that_would_bury_it() {
    let registries = build_registries();
    let mut state = AppState::new();
    let voxel = VoxelCoord::new(0, WORLD_MIN_Y, 0);
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        voxel,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(25_000),
        Temperature::from_millikelvin(293_150),
        crate::material::MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let plan = validate_create_surface_resources(&registries, &state, [spec]).unwrap();
    let expected = state.world().revision();
    generate(&mut state, ChunkCoord::new(0, 0));
    let actual = state.world().revision();
    let before = state.clone();
    assert_eq!(
        plan.commit(&mut state),
        Err(crate::surface::SurfaceResourceCreationCommitError::WorldRevision { expected, actual })
    );
    assert_eq!(state, before);
}

#[test]
fn neighborhood_generation_rejects_burying_exposed_matter_atomically() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 100, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    generate(&mut state, ChunkCoord::new(3, 3));
    let buried = VoxelCoord::new(0, WORLD_MIN_Y, 0);
    add_unloaded_surface_drop(&registries, &mut state, buried);
    let before = state.clone();
    assert_eq!(
        validate_generate_player_neighborhood(&registries, &state).err(),
        Some(ChunkGenerationError::SurfaceResourceCollision { voxel: buried })
    );
    assert_eq!(state, before);
}

#[test]
fn neighborhood_generation_commit_rejects_new_surface_resources() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 100, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    generate(&mut state, ChunkCoord::new(3, 3));
    let plan = validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .unwrap();
    let expected = state.surface().revision();
    add_unloaded_surface_drop(&registries, &mut state, VoxelCoord::new(0, WORLD_MIN_Y, 0));
    let actual = state.surface().revision();
    let before = state.clone();
    assert_eq!(
        plan.commit(&mut state),
        Err(ChunkGenerationCommitError::StaleSurfaceRevision { expected, actual })
    );
    assert_eq!(state, before);
}

#[test]
fn validated_player_admission_cannot_cross_world_generation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player_voxel = VoxelCoord::new(0, WORLD_MIN_Y, 0);
    let admission = validate_admit_player(&registries, &state, player_voxel)
        .unwrap_or_else(|error| panic!("stale-world admission validation failed: {error}"));
    validate_generate_chunk(
        &registries,
        &state,
        SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("stale-world terrain validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("stale-world terrain commit failed: {error}"));

    assert_eq!(
        admission.commit(&mut state),
        Err(PlayerAdmissionCommitError::StaleWorldRevision {
            expected: 0,
            actual: 1,
        })
    );
}

#[test]
fn validated_chunk_cannot_cross_player_or_player_work_revision_changes() {
    let registries = build_registries();
    let mut state = AppState::new();
    let chunk = ChunkCoord::new(0, 0);
    let first = validate_generate_chunk(&registries, &state, SEED, chunk, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("stale-logistics terrain validation failed: {error}"));
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 100, 0))
        .unwrap_or_else(|error| panic!("stale-logistics player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale-logistics player commit failed: {error}"));
    let actual_logistics_revision = state.logistics().revision();
    let before = state.clone();
    assert_eq!(
        first.commit(&mut state),
        Err(ChunkGenerationCommitError::StaleLogisticsRevision {
            expected: 0,
            actual: actual_logistics_revision,
        })
    );
    assert_eq!(state, before);

    let validated = validate_generate_chunk(&registries, &state, SEED, chunk, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("stale-work terrain validation failed: {error}"));
    let expected_player_work_revision = state.player_work().revision();
    let started_at = state.tick();
    let completes_at = state
        .tick()
        .checked_add_span(TickSpan::new(1))
        .unwrap_or_else(|| panic!("stale-work fixture completion overflowed"));
    let next_player_work_revision = expected_player_work_revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("stale-work fixture exhausted player-work revisions"));
    state.player_work_state_mut().apply_start(
        expected_player_work_revision,
        next_player_work_revision,
        PlayerWork::Eating {
            work: EatingWork::new(Mass::from_milligrams(1), started_at, completes_at),
        },
    );
    let actual_player_work_revision = state.player_work().revision();
    assert_eq!(
        validated.commit(&mut state),
        Err(ChunkGenerationCommitError::StalePlayerWorkRevision {
            expected: expected_player_work_revision,
            actual: actual_player_work_revision,
        })
    );
}

#[test]
fn generated_world_round_trip_preserves_compact_cells() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(-1, 2));
    generate(&mut state, ChunkCoord::new(0, 2));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("world persistence serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("world persistence decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("world persistence trusted load failed: {error}"));

    assert_eq!(loaded, state);
}

#[test]
fn trusted_load_rejects_duplicate_section_palette_state() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("world palette tamper serialization failed: {error}"));
    let sections = encoded["state"]["systems"]["world"]["chunks"][0]["sections"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("serialized world sections are not an array"));
    let section = sections
        .iter_mut()
        .find(|section| {
            section["palette"]
                .as_array()
                .is_some_and(|palette| palette.len() > 1)
        })
        .unwrap_or_else(|| panic!("generated terrain has no mixed surface section"));
    let duplicate = section["palette"][0].clone();
    section["palette"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("serialized world palette is not an array"))
        .push(duplicate.clone());
    let duplicated_state: BlockStateId = serde_json::from_value(duplicate)
        .unwrap_or_else(|error| panic!("serialized palette state failed to decode: {error}"));
    let section_y = section["y"]
        .as_i64()
        .unwrap_or_else(|| panic!("serialized world section y is not an integer"));
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("world palette tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::World(
            WorldValidationError::DuplicateSectionPaletteState {
                section_y,
                state: duplicated_state,
            }
        )))
    );
}

#[test]
fn trusted_load_rejects_tampered_represented_mass() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let actual_mass = state
        .world()
        .get_chunk(ChunkCoord::new(0, 0))
        .and_then(|chunk| chunk.sections().next())
        .map(WorldSection::represented_mass)
        .unwrap_or_else(|| panic!("generated world lost its first terrain section"));
    let section_y = state
        .world()
        .get_chunk(ChunkCoord::new(0, 0))
        .and_then(|chunk| chunk.sections().next())
        .map(WorldSection::y)
        .unwrap_or_else(|| panic!("generated world lost its first terrain section"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("world mass tamper serialization failed: {error}"));
    encoded["state"]["systems"]["world"]["chunks"][0]["sections"][0]["represented_mass"] =
        serde_json::json!(0);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("world mass tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::World(
            WorldValidationError::SectionMassMismatch {
                chunk: ChunkCoord::new(0, 0),
                section_y,
                stored: crate::core::quantity::Mass::ZERO,
                actual: actual_mass,
            }
        )))
    );
}

#[test]
fn trusted_load_rejects_unknown_block_state_reference() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("world unknown-state serialization failed: {error}"));
    let section = &mut encoded["state"]["systems"]["world"]["chunks"][0]["sections"][0];
    let unknown = BlockStateId::new(u16::MAX);
    section["palette"][0] = serde_json::json!(unknown.value());
    let section_y = section["y"]
        .as_i64()
        .unwrap_or_else(|| panic!("serialized world section y is not an integer"));
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("world unknown-state decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::World(
            WorldValidationError::UnknownSectionPaletteState {
                chunk: ChunkCoord::new(0, 0),
                section_y,
                state: unknown,
            }
        )))
    );
}
