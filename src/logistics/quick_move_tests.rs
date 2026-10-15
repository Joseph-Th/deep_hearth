//! Contracts for familiar maximum-fit carried/container quick-move gestures.

use super::*;

use crate::content::{FORM_LOG, MATERIAL_WOOD, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_lot_for_test, project_material_stacks,
};
use crate::logistics::validate_place_ground_stockpile;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;

const POSITION: VoxelCoord = VoxelCoord::new(0, 0, 0);
const TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn local_container_with_wood(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    capacity: Mass,
    mass: Mass,
) -> StockpileId {
    let stockpile = add_solid_stockpile_for_test(state, capacity)
        .unwrap_or_else(|error| panic!("quick-move container allocation failed: {error}"));
    if !mass.is_zero() {
        deposit_lot_for_test(
            registries,
            state,
            stockpile,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            mass,
            TEMPERATURE,
        )
        .unwrap_or_else(|error| panic!("quick-move container material failed: {error}"));
    }
    validate_place_ground_stockpile(state, stockpile, POSITION)
        .unwrap_or_else(|error| panic!("quick-move container placement failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("quick-move container placement commit failed: {error}"));
    stockpile
}

#[test]
fn quick_move_from_local_container_fills_carried_capacity_and_keeps_remainder() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = local_container_with_wood(
        &registries,
        &mut state,
        Mass::from_milligrams(30_000_000),
        Mass::from_milligrams(25_500_000),
    );
    let carried = validate_admit_player(&registries, &state, POSITION)
        .unwrap_or_else(|error| panic!("quick-move player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("quick-move player admission commit failed: {error}"))
        .carried_stockpile();
    let stack = project_material_stacks(&registries, &state, source)
        .unwrap_or_else(|error| panic!("quick-move source projection failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("quick-move source stack disappeared"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("quick-move matter baseline failed: {error}"));

    let outcome = validate_quick_move_player_stack(&registries, &state, &stack, carried)
        .unwrap_or_else(|error| panic!("quick-move pickup validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("quick-move pickup commit failed: {error}"));
    assert_eq!(outcome.source(), source);
    assert_eq!(outcome.destination(), carried);
    assert_eq!(outcome.mass(), Mass::from_milligrams(25_000_000));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(500_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.available_capacity()),
        Some(Mass::ZERO)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("quick-move matter audit failed: {error}"))
            .total(),
        matter_before.total()
    );
}

#[test]
fn quick_move_from_carried_custody_uses_container_remaining_capacity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let destination = local_container_with_wood(
        &registries,
        &mut state,
        Mass::from_milligrams(1_000_000),
        Mass::from_milligrams(800_000),
    );
    let carried = validate_admit_player(&registries, &state, POSITION)
        .unwrap_or_else(|error| panic!("quick-move drop admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("quick-move drop admission commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(500_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("quick-move carried material failed: {error}"));
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("quick-move carried projection failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("quick-move carried stack disappeared"));

    let outcome = validate_quick_move_player_stack(&registries, &state, &stack, destination)
        .unwrap_or_else(|error| panic!("quick-move drop validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("quick-move drop commit failed: {error}"));
    assert_eq!(outcome.mass(), Mass::from_milligrams(200_000));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(1_000_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(300_000))
    );
}
