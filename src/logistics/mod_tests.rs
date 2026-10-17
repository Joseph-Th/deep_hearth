//! Contracts for persistent player world location and carried custody.

use super::pending_secondary::PlayerPendingSecondaryUseCommitError;
use super::*;
use crate::content::{
    FORM_LOG, FORM_ROUGH_BOX_BODY, MATERIAL_WOOD, STORAGE_ROUGH_TIMBER_FIELD_BOX, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::inventory::{
    MaterialLotSelection, MaterialRelocationError, MaterialStackProjectionError, StockpileId,
    add_solid_stockpile_for_test, deposit_lot_for_test, project_material_stacks,
    validate_place_player_storage,
};
use crate::material::CommodityKey;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick;
use crate::spatial::{VIEW_DIRECTION_PARTS_PER_MILLION, ViewDirection, VoxelCoord};
use crate::structural::StructuralElementId;
use crate::survival::{Vitality, player_record_with_thermal_comfort};

fn positive_x_view() -> ViewDirection {
    ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0)
        .unwrap_or_else(|error| panic!("positive-x view direction failed: {error}"))
}

fn mark_admitted_player_dead(state: &mut AppState) {
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("dead-buffer fixture lost survival player"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record_with_thermal_comfort(
            player.metabolic_energy(),
            player.hydration(),
            Vitality::ZERO,
            player.nutrition(),
            player.vitality_recovery_remainder(),
            player.thermal_comfort_ppm(),
        ),
    );
}

fn off_boundary_admitted_player() -> (crate::registry::Registries, AppState) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("buffer fixture admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("buffer fixture admission commit failed: {error}"));
    let _ = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("buffer fixture control advance failed: {error}"));
    assert!(!state.control_tick().is_simulation_boundary());
    (registries, state)
}

#[test]
fn dead_player_cannot_buffer_primary_or_secondary_direct_actions() {
    let (_registries, mut state) = off_boundary_admitted_player();
    mark_admitted_player_dead(&mut state);
    let before = state.clone();

    assert_eq!(
        validate_buffer_player_secondary_use(&state, positive_x_view()).err(),
        Some(PlayerPendingSecondaryUseError::PlayerDead)
    );
    assert_eq!(
        validate_buffer_player_primary_break(
            &state,
            StructuralElementId::new(1),
            positive_x_view(),
        )
        .err(),
        Some(PlayerPendingPrimaryBreakError::PlayerDead)
    );
    assert_eq!(state, before);
    assert!(!state.logistics().has_pending_direct_action());
}

#[test]
fn buffered_direct_action_validation_tracks_survival_revision() {
    let (_registries, mut state) = off_boundary_admitted_player();
    let validated = validate_buffer_player_secondary_use(&state, positive_x_view())
        .unwrap_or_else(|error| panic!("stale buffer validation failed: {error}"));
    let expected = state.survival().revision();
    mark_admitted_player_dead(&mut state);
    let actual = state.survival().revision();
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(PlayerPendingSecondaryUseCommitError::StaleSurvivalRevision { expected, actual })
    );
    assert_eq!(state, before);
    assert!(!state.logistics().has_pending_direct_action());
}

#[test]
fn trusted_load_rejects_dead_player_with_buffered_direct_action() {
    let (registries, state) = off_boundary_admitted_player();
    let mut secondary = state.clone();
    validate_buffer_player_secondary_use(&secondary, positive_x_view())
        .unwrap_or_else(|error| panic!("secondary buffer validation failed: {error}"))
        .commit(&mut secondary)
        .unwrap_or_else(|error| panic!("secondary buffer commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &secondary))
        .unwrap_or_else(|error| panic!("secondary dead-buffer serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["vitality"] = serde_json::json!(0_u32);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("secondary dead-buffer decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::PendingSecondaryUsePlayerDead
        )))
    );

    let mut primary = state;
    validate_buffer_player_primary_break(&primary, StructuralElementId::new(1), positive_x_view())
        .unwrap_or_else(|error| panic!("primary buffer validation failed: {error}"))
        .commit(&mut primary)
        .unwrap_or_else(|error| panic!("primary buffer commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &primary))
        .unwrap_or_else(|error| panic!("primary dead-buffer serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["vitality"] = serde_json::json!(0_u32);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("primary dead-buffer decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::PendingPrimaryBreakPlayerDead
        )))
    );
}

#[test]
fn authored_capacity_initialization_binds_player_position_and_carried_custody() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(7, -3, 11);
    let capacity = registries.survival().physiology().carried_mass_capacity();
    let validated = validate_player_logistics_initialization(&registries, &state, position)
        .unwrap_or_else(|error| panic!("player logistics initialization failed: {error}"));
    let expected_stockpile = validated.carried_stockpile();

    let player = validated
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player logistics commit failed: {error}"));
    assert_eq!(player.position(), position);
    assert_eq!(player.carried_stockpile(), expected_stockpile);
    let carrying = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("initialized player carrying assessment disappeared"));
    assert_eq!(carrying.position(), position);
    assert_eq!(carrying.stockpile(), expected_stockpile);
    assert_eq!(carrying.capacity(), capacity);
    assert_eq!(carrying.stored(), Mass::ZERO);
    assert_eq!(carrying.available(), capacity);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn trusted_load_rejects_upward_velocity_above_canonical_jump_launch() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("upward-velocity player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("upward-velocity player admission commit failed: {error}"));
    let physiology = registries.survival().physiology();
    let maximum_upward = physiology
        .locomotion()
        .jump_speed_for_load(Mass::ZERO, physiology.carried_mass_capacity())
        .nanometers_per_second();
    assert!(
        maximum_upward
            < physiology
                .locomotion()
                .maximum_fall_speed()
                .nanometers_per_second(),
        "test requires the canonical upward launch ceiling to be below terminal fall speed"
    );
    let forged = maximum_upward
        .checked_add(1)
        .and_then(|velocity| i64::try_from(velocity).ok())
        .unwrap_or_else(|| panic!("authored upward velocity ceiling cannot be incremented"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("upward-velocity serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["player"]["body"]["vertical_velocity"]["nanometers_per_second"] =
        serde_json::json!(forged);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("upward-velocity tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::PlayerVerticalVelocityExceedsMaximum {
                nanometers_per_second: forged,
                maximum_nanometers_per_second: maximum_upward,
            },
        )))
    );
}

#[test]
fn local_ground_observation_rejects_dangling_logistics_reference() {
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("dangling ground observation player setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("dangling ground observation player setup commit failed: {error}")
        });
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_ground_placement(
        revision,
        revision + 1,
        StockpileId::new(u32::MAX),
        position,
    );

    assert!(
        std::panic::catch_unwind(|| state.available_local_ground_stockpiles().count()).is_err(),
        "local ground-stockpile observation must reject a dangling logistics reference"
    );
}

#[test]
fn ground_stockpile_allocation_creates_empty_custody_at_requested_voxel() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(-5, 2, 9);
    let capacity = Mass::from_milligrams(12_000_000);
    let validated = validate_allocate_ground_stockpile(&state, position, capacity)
        .unwrap_or_else(|error| panic!("ground allocation validation failed: {error}"));
    let expected = validated.stockpile();

    let stockpile = validated
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("ground allocation commit failed: {error}"));

    assert_eq!(stockpile, expected);
    assert_eq!(
        state.logistics().stationary_stockpile_position(stockpile),
        Some(position)
    );
    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .unwrap_or_else(|| panic!("allocated ground stockpile disappeared"));
    assert_eq!(record.capacity(), capacity);
    assert_eq!(record.stored_mass(), Mass::ZERO);
    assert!(record.enclosure().is_none());
    assert!(record.supported_by().is_none());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn player_ground_stockpile_allocation_derives_current_voxel_and_requires_admission() {
    let registries = build_registries();
    let mut state = AppState::new();
    let capacity = Mass::from_milligrams(12_000_000);
    assert_eq!(
        validate_allocate_player_ground_stockpile(&state, capacity).err(),
        Some(PlayerGroundStockpileAllocationError::PlayerNotInitialized)
    );

    let position = VoxelCoord::new(-5, 2, 9);
    validate_initialize_player_logistics(&state, position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("player-local ground allocation setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("player-local ground allocation setup commit failed: {error}")
        });
    let stockpile = validate_allocate_player_ground_stockpile(&state, capacity)
        .unwrap_or_else(|error| panic!("player-local ground allocation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player-local ground allocation commit failed: {error}"));

    assert_eq!(
        state.logistics().stationary_stockpile_position(stockpile),
        Some(position),
        "ordinary loose storage allocation must derive the admitted player's exact voxel"
    );
    assert_eq!(
        state
            .available_local_ground_stockpiles()
            .map(|record| record.id())
            .collect::<Vec<_>>(),
        vec![stockpile]
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn ground_stockpile_allocation_rejects_stale_inventory_without_half_location() {
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let validated = validate_allocate_ground_stockpile(&state, position, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("stale ground allocation validation failed: {error}"));
    let expected_stockpile = validated.stockpile();
    let competing = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| {
            panic!("stale ground allocation competing mutation failed: {error}")
        });
    assert_eq!(competing, expected_stockpile);
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(
            GroundStockpileAllocationCommitError::StaleInventoryRevision {
                expected: 0,
                actual: 1,
            }
        )
    );
    assert_eq!(state, before);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(expected_stockpile)
            .map(|record| record.capacity()),
        Some(Mass::from_milligrams(1))
    );
    assert_eq!(
        state
            .logistics()
            .stationary_stockpile_position(expected_stockpile),
        None
    );
}

#[test]
fn same_voxel_pickup_and_partial_drop_preserve_inventory_custody() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(3, 1, -4);
    let carried =
        validate_initialize_player_logistics(&state, position, Mass::from_milligrams(100))
            .unwrap_or_else(|error| panic!("pickup logistics setup failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("pickup logistics commit failed: {error}"))
            .carried_stockpile();
    let ground = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("pickup ground stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        ground,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("pickup ground lot failed: {error}"));
    validate_place_ground_stockpile(&state, ground, position)
        .unwrap_or_else(|error| panic!("pickup ground placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("pickup ground placement commit failed: {error}"));

    validate_pickup_from_ground(
        &registries,
        &state,
        ground,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
    )
    .unwrap_or_else(|error| panic!("same-voxel pickup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("same-voxel pickup commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_lot(lot)
            .map(|record| record.stockpile()),
        Some(carried),
        "full-lot pickup should preserve stable lot identity"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ground)
            .map(|record| record.stored_mass()),
        Some(Mass::ZERO)
    );

    validate_drop_to_ground(
        &registries,
        &state,
        ground,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(4))],
    )
    .unwrap_or_else(|error| panic!("same-voxel partial drop failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("same-voxel partial drop commit failed: {error}"));
    assert_eq!(
        state.inventory().get_lot(lot).map(|record| record.mass()),
        Some(Mass::from_milligrams(6))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ground)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(4))
    );
    assert_eq!(
        assess_player_carrying(&state).map(|assessment| assessment.stored()),
        Some(Mass::from_milligrams(6))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn familiar_stack_transfer_moves_partial_mass_through_real_storage_without_lot_ids() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("stack-transfer player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stack-transfer player admission commit failed: {error}"));
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("stack-transfer player disappeared"));
    let body = CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY);
    let cargo = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        body,
        Mass::from_milligrams(1_600_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stack-transfer field-box body failed: {error}"));
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        cargo,
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stack-transfer cargo failed: {error}"));
    let storage =
        validate_place_player_storage(&registries, &state, STORAGE_ROUGH_TIMBER_FIELD_BOX)
            .unwrap_or_else(|error| panic!("stack-transfer storage placement failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("stack-transfer storage placement commit failed: {error}")
            });
    let matter_before = crate::matter::calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("stack-transfer matter baseline failed: {error}"))
        .total();

    let stale_stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("stack-transfer carried projection failed: {error}"))
        .into_iter()
        .find(|stack| stack.commodity() == cargo)
        .unwrap_or_else(|| panic!("stack-transfer cargo stack disappeared"));
    let _ = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("stack-transfer stale revision fixture failed: {error}"));
    assert!(matches!(
        validate_drop_stack_to_ground(
            &registries,
            &state,
            storage,
            &stale_stack,
            Mass::from_milligrams(400_000),
        ),
        Err(GroundMaterialTransferError::Stack(
            MaterialStackProjectionError::StaleInventoryRevision { .. }
        ))
    ));

    let carried_stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("stack-transfer reprojection failed: {error}"))
        .into_iter()
        .find(|stack| stack.commodity() == cargo)
        .unwrap_or_else(|| panic!("stack-transfer reprojected cargo disappeared"));
    validate_drop_stack_to_ground(
        &registries,
        &state,
        storage,
        &carried_stack,
        Mass::from_milligrams(400_000),
    )
    .unwrap_or_else(|error| panic!("familiar stack drop failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("familiar stack drop commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(storage)
            .map(|stockpile| stockpile.get_mass(cargo)),
        Some(Mass::from_milligrams(400_000))
    );

    let stored_stack = project_material_stacks(&registries, &state, storage)
        .unwrap_or_else(|error| panic!("stored stack projection failed: {error}"))
        .into_iter()
        .find(|stack| stack.commodity() == cargo)
        .unwrap_or_else(|| panic!("stored cargo stack disappeared"));
    validate_pickup_stack_from_ground(
        &registries,
        &state,
        storage,
        &stored_stack,
        Mass::from_milligrams(150_000),
    )
    .unwrap_or_else(|error| panic!("familiar stack pickup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("familiar stack pickup commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(cargo)),
        Some(Mass::from_milligrams(750_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(storage)
            .map(|stockpile| stockpile.get_mass(cargo)),
        Some(Mass::from_milligrams(250_000))
    );
    assert_eq!(
        crate::matter::calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("stack-transfer final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn pickup_rejects_remote_ground_stockpile_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("remote pickup logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote pickup logistics commit failed: {error}"));
    let ground = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("remote pickup ground stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        ground,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("remote pickup lot failed: {error}"));
    let ground_position = VoxelCoord::new(1, 0, 0);
    validate_place_ground_stockpile(&state, ground, ground_position)
        .unwrap_or_else(|error| panic!("remote pickup ground placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote pickup ground placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_pickup_from_ground(
            &registries,
            &state,
            ground,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
        )
        .err(),
        Some(GroundMaterialTransferError::GroundStockpileNotAtPlayer {
            stockpile: ground,
            ground: ground_position,
            player: player_position,
        })
    );
    assert_eq!(state, before);
}

fn ground_transfer_fixture() -> (
    crate::registry::Registries,
    AppState,
    StockpileId,
    crate::inventory::MaterialLotId,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    let origin = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, origin)
        .unwrap_or_else(|error| panic!("ground transfer player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("ground transfer player admission commit failed: {error}"));
    let ground = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000_000))
        .unwrap_or_else(|error| panic!("ground-transfer fixture stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        ground,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(12_500_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("ground-transfer fixture lot failed: {error}"));
    validate_place_ground_stockpile(&state, ground, origin)
        .unwrap_or_else(|error| panic!("ground-transfer fixture placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("ground-transfer fixture placement commit failed: {error}"));
    (registries, state, ground, lot)
}

#[test]
fn dead_admitted_player_cannot_pick_up_ground_material() {
    let (registries, state, ground, lot) = ground_transfer_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("dead pickup serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["vitality"] = serde_json::json!(0);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("dead pickup decode failed: {error}"));
    let dead = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("idle dead pickup player should load: {error}"));
    let before = dead.clone();

    assert_eq!(
        validate_pickup_from_ground(
            &registries,
            &dead,
            ground,
            &[MaterialLotSelection::new(
                lot,
                Mass::from_milligrams(12_500_000),
            )],
        )
        .err(),
        Some(GroundMaterialTransferError::PlayerDead)
    );
    assert_eq!(dead, before);
}

#[test]
fn pickup_respects_finite_carried_capacity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let carried = validate_initialize_player_logistics(&state, position, Mass::from_milligrams(5))
        .unwrap_or_else(|error| panic!("capacity pickup logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("capacity pickup logistics commit failed: {error}"))
        .carried_stockpile();
    let ground = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("capacity pickup ground stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        ground,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("capacity pickup lot failed: {error}"));
    validate_place_ground_stockpile(&state, ground, position)
        .unwrap_or_else(|error| panic!("capacity pickup ground placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("capacity pickup ground placement commit failed: {error}"));

    assert!(matches!(
        validate_pickup_from_ground(
            &registries,
            &state,
            ground,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
        ),
        Err(GroundMaterialTransferError::Inventory(
            MaterialRelocationError::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            }
        )) if stockpile == carried
            && capacity == Mass::from_milligrams(5)
            && committed == Mass::ZERO
            && requested == Mass::from_milligrams(10)
    ));
}

#[test]
fn ground_transfer_token_rejects_logistics_change_before_commit() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, position, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("stale transfer logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale transfer logistics commit failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("stale transfer source failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale transfer lot failed: {error}"));
    validate_place_ground_stockpile(&state, source, position)
        .unwrap_or_else(|error| panic!("stale transfer source placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale transfer source placement commit failed: {error}"));
    let validated = validate_pickup_from_ground(
        &registries,
        &state,
        source,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
    )
    .unwrap_or_else(|error| panic!("stale transfer validation failed: {error}"));
    let competing = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("stale transfer competing stockpile failed: {error}"));
    validate_place_ground_stockpile(&state, competing, position)
        .unwrap_or_else(|error| panic!("stale transfer competing placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("stale transfer competing placement commit failed: {error}")
        });
    let expected = state.logistics().revision() - 1;
    let actual = state.logistics().revision();
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(GroundMaterialTransferCommitError::StaleLogisticsRevision { expected, actual })
    );
    assert_eq!(state, before);
}

#[test]
fn persisted_logistics_rejects_carried_custody_that_is_also_structurally_mounted() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_initialize_player_logistics(
        &state,
        VoxelCoord::new(2, 3, 4),
        Mass::from_milligrams(5_000_000),
    )
    .unwrap_or_else(|error| panic!("mounted logistics setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mounted logistics setup commit failed: {error}"));
    let stockpile = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("mounted logistics player disappeared"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mounted logistics serialization failed: {error}"));
    let key = stockpile.value().to_string();
    encoded["state"]["systems"]["inventory"]["stockpiles"][key]["supported_by"] =
        serde_json::json!(1_u64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mounted logistics decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::CarriedStockpileMounted {
                stockpile,
                element: crate::structural::StructuralElementId::new(1),
            }
        )))
    );
}

#[test]
fn initialization_rejects_zero_capacity_and_duplicate_player_without_mutation() {
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    assert_eq!(
        validate_initialize_player_logistics(&state, position, Mass::ZERO).err(),
        Some(InitializePlayerLogisticsError::ZeroCarriedCapacity)
    );
    assert!(state.logistics().player().is_none());
    assert!(state.inventory().stockpiles().next().is_none());

    validate_initialize_player_logistics(&state, position, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("first player logistics validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("first player logistics commit failed: {error}"));
    let before = state.clone();
    assert_eq!(
        validate_initialize_player_logistics(
            &state,
            VoxelCoord::new(1, 0, 0),
            Mass::from_milligrams(2_000_000),
        )
        .err(),
        Some(InitializePlayerLogisticsError::AlreadyInitialized)
    );
    assert_eq!(state, before);
}

#[test]
fn initialization_rejects_stale_inventory_before_installing_logistics() {
    let mut state = AppState::new();
    let validated = validate_initialize_player_logistics(
        &state,
        VoxelCoord::new(0, 0, 0),
        Mass::from_milligrams(5_000_000),
    )
    .unwrap_or_else(|error| panic!("stale logistics validation failed: {error}"));
    add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)).unwrap_or_else(|error| {
        panic!("stale logistics competing inventory mutation failed: {error}")
    });
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(InitializePlayerLogisticsCommitError::Inventory {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before);
    assert!(state.logistics().player().is_none());
}

#[test]
fn persisted_logistics_rejects_a_missing_carried_stockpile() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_initialize_player_logistics(
        &state,
        VoxelCoord::new(2, 3, 4),
        Mass::from_milligrams(5_000_000),
    )
    .unwrap_or_else(|error| panic!("persisted logistics setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("persisted logistics setup commit failed: {error}"));
    let stockpile = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("persisted logistics player disappeared"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("persisted logistics serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["stockpiles"] = serde_json::json!({});
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("persisted logistics decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::UnknownCarriedStockpile { stockpile }
        )))
    );
}
