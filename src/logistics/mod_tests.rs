//! Contracts for persistent player world location and carried custody.

use super::*;
use crate::content::{FORM_LOG, MATERIAL_WOOD, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::inventory::{
    MaterialLotSelection, MaterialRelocationError, add_solid_stockpile_for_test,
    deposit_lot_for_test,
};
use crate::material::CommodityKey;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::spatial::VoxelCoord;

#[test]
fn initialization_binds_player_position_to_inventory_owned_carried_capacity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(7, -3, 11);
    let capacity = Mass::from_milligrams(25_000_000);
    let validated = validate_initialize_player_logistics(&state, position, capacity)
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
        Err(
            InitializePlayerLogisticsCommitError::StaleInventoryRevision {
                expected: 0,
                actual: 1,
            }
        )
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
