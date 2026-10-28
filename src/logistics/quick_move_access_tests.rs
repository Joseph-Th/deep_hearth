//! Quick-move must authorize world locality and player life before revealing capacity.

use super::*;
use crate::content::{FORM_LOG, MATERIAL_WOOD, build_registries};
use crate::core::quantity::Temperature;
use crate::inventory::{
    MaterialStackProjectionError, add_solid_stockpile_for_test, deposit_lot_for_test,
    project_material_stacks,
};
use crate::logistics::validate_place_ground_stockpile;
use crate::material::CommodityKey;
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::survival::{Vitality, player_record_with_thermal_comfort};

const HERE: VoxelCoord = VoxelCoord::new(0, 0, 0);
const TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);
const CAPACITY: Mass = Mass::from_milligrams(1_000_000);

fn carried_wood_stack(registries: &Registries, state: &mut AppState) -> MaterialStackProjection {
    let carried = validate_admit_player(registries, state, HERE)
        .unwrap_or_else(|error| panic!("quick-move access admission failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("quick-move access admission commit failed: {error}"))
        .carried_stockpile();
    let _ = deposit_lot_for_test(
        registries,
        state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(100_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("quick-move access carried deposit failed: {error}"));
    project_material_stacks(registries, state, carried)
        .unwrap_or_else(|error| panic!("quick-move access stack projection failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("quick-move access carried stack missing"))
}

fn filled_ground_container(
    registries: &Registries,
    state: &mut AppState,
    position: VoxelCoord,
) -> StockpileId {
    let stockpile = add_solid_stockpile_for_test(state, CAPACITY)
        .unwrap_or_else(|error| panic!("quick-move access container allocation failed: {error}"));
    let _ = deposit_lot_for_test(
        registries,
        state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        CAPACITY,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("quick-move access container filling failed: {error}"));
    validate_place_ground_stockpile(state, stockpile, position)
        .unwrap_or_else(|error| panic!("quick-move access container placement failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("quick-move access container placement commit failed: {error}")
        });
    stockpile
}

#[test]
fn remote_full_container_rejects_locality_before_capacity_disclosure() {
    let registries = build_registries();
    let mut state = AppState::new();
    let remote_position = VoxelCoord::new(HERE.x() + 1, HERE.y(), HERE.z());
    let remote = filled_ground_container(&registries, &mut state, remote_position);
    let stack = carried_wood_stack(&registries, &mut state);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(remote)
            .map(|record| record.available_capacity()),
        Some(Mass::ZERO)
    );
    let before = state.clone();
    assert_eq!(
        validate_quick_move_player_stack(&registries, &state, &stack, remote).err(),
        Some(PlayerQuickMoveError::Transfer(
            GroundMaterialTransferError::GroundStockpileNotAtPlayer {
                stockpile: remote,
                ground: remote_position,
                player: HERE,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn dead_player_rejects_quick_move_before_local_capacity_disclosure() {
    let registries = build_registries();
    let mut state = AppState::new();
    let local = filled_ground_container(&registries, &mut state, HERE);
    let stack = carried_wood_stack(&registries, &mut state);
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("quick-move dead player missing"));
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            player.metabolic_energy(),
            player.hydration(),
            Vitality::ZERO,
            player.nutrition(),
            player.vitality_recovery_remainder(),
            player.thermal_comfort_ppm(),
        ),
    );
    let before = state.clone();
    assert_eq!(
        validate_quick_move_player_stack(&registries, &state, &stack, local).err(),
        Some(PlayerQuickMoveError::Transfer(
            GroundMaterialTransferError::PlayerDead
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn full_local_destination_does_not_hide_an_obsolete_source_stack() {
    let registries = build_registries();
    let mut state = AppState::new();
    let local = filled_ground_container(&registries, &mut state, HERE);
    let stale = carried_wood_stack(&registries, &mut state);
    assert_eq!(
        validate_quick_move_player_stack(&registries, &state, &stale, local).err(),
        Some(PlayerQuickMoveError::DestinationFull { stockpile: local })
    );
    let _unrelated = add_solid_stockpile_for_test(&mut state, CAPACITY)
        .unwrap_or_else(|error| panic!("quick-move stale-revision fixture failed: {error}"));
    let before = state.clone();
    assert!(matches!(
        validate_quick_move_player_stack(&registries, &state, &stale, local),
        Err(PlayerQuickMoveError::Transfer(GroundMaterialTransferError::Stack(
            MaterialStackProjectionError::StaleInventoryRevision { expected, actual }
        ))) if expected != actual
    ));
    assert_eq!(state, before);
}

#[test]
fn quick_move_rejects_elapsed_stack_snapshot_even_without_inventory_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let destination = add_solid_stockpile_for_test(&mut state, CAPACITY).unwrap();
    validate_place_ground_stockpile(&state, destination, HERE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let stack = carried_wood_stack(&registries, &mut state);
    assert!(validate_quick_move_player_stack(&registries, &state, &stack, destination).is_ok());

    let inventory_revision = state.inventory().revision();
    let projected_at = state.tick();
    let outcome = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(outcome.tick(), state.tick());
    assert_eq!(state.inventory().revision(), inventory_revision);
    let before = state.clone();
    assert_eq!(
        validate_quick_move_player_stack(&registries, &state, &stack, destination).err(),
        Some(PlayerQuickMoveError::Transfer(
            GroundMaterialTransferError::Stack(MaterialStackProjectionError::StaleSimulationTick {
                expected: projected_at,
                actual: state.tick(),
            })
        ))
    );
    assert_eq!(state, before);
    let refreshed = project_material_stacks(
        &registries,
        &state,
        state.logistics().player().unwrap().carried_stockpile(),
    )
    .unwrap();
    assert!(
        validate_quick_move_player_stack(&registries, &state, &refreshed[0], destination).is_ok()
    );
}
