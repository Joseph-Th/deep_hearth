//! Ordinary-player contracts for familiar carried/container transfer gestures.

use deep_hearth::content::gameplay_fixture::{seed_ground_stockpile, seed_lot};
use deep_hearth::content::{FORM_CHIP, FORM_LOG, MATERIAL_STONE, MATERIAL_WOOD, build_registries};
use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::{MaterialStackProjection, StockpileId};
use deep_hearth::logistics::{project_player_inventory, validate_quick_move_player_stack};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::player::validate_admit_player;
use deep_hearth::spatial::VoxelCoord;

fn stack_from_local_container(
    registries: &deep_hearth::registry::Registries,
    state: &AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
) -> MaterialStackProjection {
    let view = project_player_inventory(registries, state)
        .unwrap_or_else(|error| panic!("inventory-gesture projection failed: {error}"));
    let container = view
        .local_containers()
        .iter()
        .find(|container| container.stockpile() == stockpile)
        .unwrap_or_else(|| panic!("inventory-gesture local container disappeared"));
    container
        .stacks()
        .iter()
        .find(|stack| stack.commodity() == commodity)
        .cloned()
        .unwrap_or_else(|| panic!("inventory-gesture local stack {commodity:?} disappeared"))
}

fn stack_from_carried(
    registries: &deep_hearth::registry::Registries,
    state: &AppState,
    commodity: CommodityKey,
) -> MaterialStackProjection {
    project_player_inventory(registries, state)
        .unwrap_or_else(|error| panic!("inventory-gesture carried projection failed: {error}"))
        .carried()
        .stacks()
        .iter()
        .find(|stack| stack.commodity() == commodity)
        .cloned()
        .unwrap_or_else(|| panic!("inventory-gesture carried stack {commodity:?} disappeared"))
}

#[test]
fn ordinary_quick_move_uses_player_projection_and_moves_only_the_mass_that_fits() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let container = seed_ground_stockpile(&mut state, position, Mass::from_milligrams(30_000_000));
    let wood = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    seed_lot(
        &registries,
        &mut state,
        container,
        wood,
        Mass::from_milligrams(24_800_000),
        Temperature::from_millikelvin(293_150),
    );
    seed_lot(
        &registries,
        &mut state,
        container,
        stone,
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
    );
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("inventory-gesture player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("inventory-gesture player admission commit failed: {error}"))
        .carried_stockpile();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("inventory-gesture matter baseline failed: {error}"));

    let wood_stack = stack_from_local_container(&registries, &state, container, wood);
    let wood_move = validate_quick_move_player_stack(&registries, &state, &wood_stack, carried)
        .unwrap_or_else(|error| panic!("inventory-gesture wood quick-move failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("inventory-gesture wood quick-move commit failed: {error}"));
    assert_eq!(wood_move.mass(), Mass::from_milligrams(24_800_000));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.available_capacity()),
        Some(Mass::from_milligrams(200_000))
    );

    let stone_stack = stack_from_local_container(&registries, &state, container, stone);
    let stone_move = validate_quick_move_player_stack(&registries, &state, &stone_stack, carried)
        .unwrap_or_else(|error| panic!("inventory-gesture stone quick-move failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("inventory-gesture stone quick-move commit failed: {error}")
        });
    assert_eq!(stone_move.mass(), Mass::from_milligrams(200_000));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(container)
            .map(|stockpile| stockpile.get_mass(stone)),
        Some(Mass::from_milligrams(300_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(stone)),
        Some(Mass::from_milligrams(200_000))
    );

    let carried_stone = stack_from_carried(&registries, &state, stone);
    let returned = validate_quick_move_player_stack(&registries, &state, &carried_stone, container)
        .unwrap_or_else(|error| panic!("inventory-gesture return quick-move failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("inventory-gesture return quick-move commit failed: {error}")
        });
    assert_eq!(returned.mass(), Mass::from_milligrams(200_000));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(container)
            .map(|stockpile| stockpile.get_mass(stone)),
        Some(Mass::from_milligrams(500_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(stone)),
        Some(Mass::ZERO)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("inventory-gesture matter audit failed: {error}"))
            .total(),
        matter_before.total()
    );
}
