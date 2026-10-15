//! Contracts for the player-facing carried inventory and local-container projection.

use super::*;

use crate::content::{FORM_LOG, MATERIAL_WOOD, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::logistics::{
    PlayerActiveSelection, PlayerQuickAccessSlot, validate_place_ground_stockpile,
    validate_set_quick_access_slot,
};
use crate::material::CommodityKey;
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;

#[test]
fn inventory_projection_composes_carried_stacks_hotbar_and_only_local_containers() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("inventory projection player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("inventory projection player commit failed: {error}"));
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("inventory projection player disappeared"));
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("inventory projection carried material failed: {error}"));
    let slot = PlayerQuickAccessSlot::new(2)
        .unwrap_or_else(|| panic!("inventory projection slot disappeared"));
    validate_set_quick_access_slot(
        &registries,
        &state,
        slot,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap_or_else(|error| panic!("inventory projection quick-access binding failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("inventory projection binding commit failed: {error}"));

    let local = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("inventory projection local stockpile failed: {error}"));
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        local,
        commodity,
        Mass::from_milligrams(250_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("inventory projection local material failed: {error}"));
    validate_place_ground_stockpile(&state, local, position)
        .unwrap_or_else(|error| panic!("inventory projection local placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("inventory projection local placement commit failed: {error}")
        });
    let remote = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("inventory projection remote stockpile failed: {error}"));
    validate_place_ground_stockpile(&state, remote, VoxelCoord::new(1, 0, 0))
        .unwrap_or_else(|error| panic!("inventory projection remote placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("inventory projection remote placement commit failed: {error}")
        });

    let view = project_player_inventory(&registries, &state)
        .unwrap_or_else(|error| panic!("inventory projection failed: {error}"));
    assert_eq!(view.carried().stockpile(), carried);
    assert_eq!(view.carried().stacks().len(), 1);
    assert_eq!(view.carried().stacks()[0].commodity(), commodity);
    assert_eq!(
        view.carried().stacks()[0].mass(),
        Mass::from_milligrams(500_000)
    );
    assert_eq!(
        view.quick_access_slots()[slot.index()],
        Some(PlayerActiveSelection::material(commodity))
    );
    assert_eq!(
        view.local_containers()
            .iter()
            .map(PlayerStockpileProjection::stockpile)
            .collect::<Vec<_>>(),
        vec![local],
        "remote stockpiles must not leak into the ordinary container view"
    );
    assert_eq!(view.local_containers()[0].stacks().len(), 1);
    assert_eq!(
        view.local_containers()[0].stacks()[0].mass(),
        Mass::from_milligrams(250_000)
    );
}
