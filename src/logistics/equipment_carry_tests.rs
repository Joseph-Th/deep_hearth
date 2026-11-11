//! Contracts for portable equipment custody, body capacity, movement, and persistence.

use crate::content::{
    EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_LOG, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD,
    build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, StateValidationError};
use crate::equipment::validate_assemble_equipment;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::logistics::{
    LogisticsValidationError, assess_player_carrying, place_equipment_for_fixture,
    relocate_player_for_test, validate_player_equipment_access,
};
use crate::material::CommodityKey;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;

use super::{
    PlayerEquipmentCarryCommitError, PlayerEquipmentCarryError, validate_drop_equipment,
    validate_pickup_equipment,
};

const ORIGIN: VoxelCoord = VoxelCoord::new(0, 0, 0);
const ROOM_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn admitted_pick_fixture() -> (
    crate::registry::Registries,
    AppState,
    crate::equipment::EquipmentId,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("equipment carry assembly stockpile failed: {error}"));
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
            &registries,
            &mut state,
            source,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        )
        .unwrap_or_else(|error| panic!("equipment carry assembly material failed: {error}"));
    }
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, source)
        .unwrap_or_else(|error| panic!("equipment carry pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("equipment carry pick assembly commit failed: {error}"));
    place_equipment_for_fixture(&mut state, pick, ORIGIN);
    validate_admit_player(&registries, &state, ORIGIN)
        .unwrap_or_else(|error| panic!("equipment carry player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("equipment carry player admission commit failed: {error}"));
    (registries, state, pick)
}

#[test]
fn portable_equipment_shares_body_capacity_and_round_trips_persistence() {
    let (registries, mut state, pick) = admitted_pick_fixture();
    assert!(state.logistics().has_valid_equipment_position_index());
    assert_eq!(
        state.logistics().equipment_locations_near(ORIGIN, 0),
        vec![(pick, ORIGIN)]
    );
    let before = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("equipment carry initial assessment disappeared"));
    assert_eq!(before.capacity(), Mass::from_milligrams(25_000_000));
    assert_eq!(before.available(), Mass::from_milligrams(25_000_000));
    assert_eq!(before.equipment(), Mass::ZERO);

    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("equipment carry pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("equipment carry pickup commit failed: {error}"));
    assert!(state.logistics().has_valid_equipment_position_index());
    assert!(
        state
            .logistics()
            .equipment_locations_near(ORIGIN, 2)
            .is_empty()
    );
    let carried = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("equipment carry assessment disappeared"));
    assert_eq!(carried.capacity(), Mass::from_milligrams(25_000_000));
    assert_eq!(carried.stored(), Mass::from_milligrams(1_000_000));
    assert_eq!(carried.equipment(), Mass::from_milligrams(1_000_000));
    assert_eq!(carried.available(), Mass::from_milligrams(24_000_000));
    assert_eq!(state.logistics().equipment_position(pick), Some(ORIGIN));
    assert_eq!(
        state
            .available_local_equipment()
            .map(|record| record.id())
            .collect::<Vec<_>>(),
        vec![pick],
        "carried equipment must remain visible through exact-local actor observation"
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("carried equipment serialization failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("carried equipment decode failed: {error}"));
    assert_eq!(
        loaded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("carried equipment trusted load failed: {error}")),
        state
    );

    validate_drop_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("equipment carry drop failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("equipment carry drop commit failed: {error}"));
    assert!(state.logistics().has_valid_equipment_position_index());
    assert_eq!(
        state.logistics().equipment_locations_near(ORIGIN, 0),
        vec![(pick, ORIGIN)]
    );
    let dropped = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("equipment carry final assessment disappeared"));
    assert_eq!(dropped.capacity(), Mass::from_milligrams(25_000_000));
    assert_eq!(dropped.available(), Mass::from_milligrams(25_000_000));
    assert_eq!(dropped.equipment(), Mass::ZERO);
    assert_eq!(
        state.logistics().stationary_equipment_position(pick),
        Some(ORIGIN)
    );
}

#[test]
fn carried_equipment_follows_player_relocation_and_remains_accessible() {
    let (registries, mut state, pick) = admitted_pick_fixture();
    let destination = VoxelCoord::new(1, 0, 0);
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("moving carried-equipment pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("moving carried-equipment pickup commit failed: {error}"));
    assert_eq!(state.logistics().equipment_position(pick), Some(ORIGIN));

    relocate_player_for_test(&mut state, destination);
    assert_eq!(
        state.logistics().equipment_position(pick),
        Some(destination)
    );
    assert_eq!(validate_player_equipment_access(&state, pick), Ok(()));
}

#[test]
fn trusted_load_rejects_duplicate_carried_equipment_identity() {
    let (registries, mut state, pick) = admitted_pick_fixture();
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("duplicate carried-equipment pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("duplicate carried-equipment pickup commit failed: {error}")
        });
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("duplicate carried-equipment serialization failed: {error}")
        });
    encoded["state"]["systems"]["logistics"]["carried_equipment"] =
        serde_json::json!([pick.value(), pick.value()]);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("duplicate carried-equipment decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::CarriedEquipmentNotStrictlyOrdered {
                previous: pick,
                current: pick,
            }
        )))
    );
}

#[test]
fn trusted_load_rejects_equipment_in_carried_and_stationary_custody() {
    let (registries, mut state, pick) = admitted_pick_fixture();
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("dual-custody pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("dual-custody pickup commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("dual-custody serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["equipment_locations"][pick.value().to_string()] =
        serde_json::json!({"x": ORIGIN.x(), "y": ORIGIN.y(), "z": ORIGIN.z()});
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("dual-custody decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::CarriedEquipmentAlsoLocated { equipment: pick }
        )))
    );
}

#[test]
fn trusted_load_rejects_carried_tool_capacity_that_no_longer_matches_body_envelope() {
    let (registries, mut state, pick) = admitted_pick_fixture();
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("capacity-tamper pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("capacity-tamper pickup commit failed: {error}"));
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("capacity-tamper fixture lost player"));
    let expected = registries.survival().physiology().carried_mass_capacity();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("capacity-tamper serialization failed: {error}"));
    let current = encoded["state"]["systems"]["inventory"]["stockpiles"]
        [carried.value().to_string()]["capacity"]
        .as_u64()
        .unwrap_or_else(|| panic!("serialized carried capacity is not a u64"));
    encoded["state"]["systems"]["inventory"]["stockpiles"][carried.value().to_string()]["capacity"] =
        serde_json::json!(current + 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("capacity-tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::PlayerCarriedCapacityMismatch {
            stockpile: carried,
            expected,
            actual: Mass::from_milligrams(expected.milligrams() + 1),
        })
    );
}

#[test]
fn trusted_load_rejects_overflowing_carried_tool_capacity_envelope() {
    let (registries, mut state, pick) = admitted_pick_fixture();
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("capacity-overflow pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("capacity-overflow pickup commit failed: {error}"));
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("capacity-overflow fixture lost player"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("capacity-overflow serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["stockpiles"][carried.value().to_string()]["capacity"] =
        serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("capacity-overflow decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::PlayerCarriedCapacityOverflow { stockpile: carried })
    );
}

#[test]
fn equipment_pickup_rejects_existing_material_over_body_envelope_without_mutation() {
    let (registries, mut state, pick) = admitted_pick_fixture();
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("equipment carry fixture lost player"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(24_500_000),
        ROOM_TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("equipment carry overload material failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_pickup_equipment(&registries, &state, pick).err(),
        Some(PlayerEquipmentCarryError::CapacityExceeded {
            equipment: pick,
            capacity_after_pickup: Mass::from_milligrams(24_000_000),
            committed: Mass::from_milligrams(24_500_000),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn portable_tool_cannot_be_left_suspended_by_an_airborne_player() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::core::state::validate_loaded_state;
    use crate::logistics::player_body_is_grounded;
    use crate::matter::calculate_matter_accounting;
    use crate::simulation::advance_control_tick;
    use crate::world::{
        ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
        validate_generate_chunk,
    };

    let (registries, mut state, pick) = admitted_pick_fixture();
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    // A valid player can leave a ledge and fall. Introduce the generated terrain
    // after moving the controlled fixture above it, then use real gravity samples.
    relocate_player_for_test(&mut state, VoxelCoord::new(5, 10, 5));
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xB00C_CAEA),
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
    relocate_player_for_test(&mut state, VoxelCoord::new(5, floor + 3, 5));
    let _ = advance_control_tick(&registries, &mut state).unwrap();
    assert!(!player_body_is_grounded(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        validate_drop_equipment(&registries, &state, pick).err(),
        Some(PlayerEquipmentCarryError::PlayerAirborne),
        "a stationary tool would otherwise hang where the player was falling"
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
        validate_drop_equipment(&registries, actor, pick)
            .unwrap()
            .commit(actor)
            .unwrap();
        assert_eq!(
            actor.logistics().stationary_equipment_position(pick),
            Some(VoxelCoord::new(5, floor + 1, 5))
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
fn validated_equipment_drop_rejects_removed_footing_before_commit() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::logistics::player_body_is_grounded;
    use crate::world::{
        BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
        set_generated_block_for_test, validate_generate_chunk,
    };

    let (registries, mut state, pick) = admitted_pick_fixture();
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    relocate_player_for_test(&mut state, VoxelCoord::new(32, 40, 32));
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xD0A5_F007),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let floor_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(5, y, 5)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    relocate_player_for_test(&mut state, VoxelCoord::new(5, floor_y + 1, 5));
    assert!(player_body_is_grounded(&registries, &state));
    let drop = validate_drop_equipment(&registries, &state, pick).unwrap();
    let expected_world_revision = state.world().revision();

    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(5, floor_y, 5),
        BlockStateId::AIR,
    );
    assert!(!player_body_is_grounded(&registries, &state));
    let before = state.clone();
    assert_eq!(
        drop.commit(&mut state).err(),
        Some(PlayerEquipmentCarryCommitError::StaleWorldRevision {
            expected: expected_world_revision,
            actual: before.world().revision(),
        }),
    );
    assert_eq!(
        state, before,
        "rejected drop must not change either custody owner"
    );
    assert!(state.logistics().is_equipment_carried(pick));
}
