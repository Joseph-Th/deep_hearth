//! Ordinary-player contracts for familiar carried/container transfer gestures.

use deep_hearth::content::gameplay_fixture::{
    seed_assembled_equipment_at, seed_ground_stockpile, seed_lot, seed_stockpile,
};
use deep_hearth::content::{
    EQUIPMENT_STONE_PICK, FORM_CHIP, FORM_HANDLE, FORM_LOG, FORM_TOOL, MATERIAL_STONE,
    MATERIAL_WOOD, build_registries,
};
use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::{MaterialStackProjection, StockpileId, StockpileStorageProfile};
use deep_hearth::logistics::{
    project_player_inventory, validate_drop_equipment, validate_pickup_equipment,
    validate_quick_move_player_stack,
};
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

#[test]
fn dropped_tool_remains_visible_in_player_projection_and_can_be_picked_back_up() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let source = seed_stockpile(
        &mut state,
        Mass::from_milligrams(1_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_lot(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
        Temperature::from_millikelvin(293_150),
    );
    seed_lot(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
    );
    let pick = seed_assembled_equipment_at(
        &registries,
        &mut state,
        EQUIPMENT_STONE_PICK,
        source,
        position,
    );
    validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("equipment-projection admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("equipment-projection admission commit failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("equipment-projection matter baseline failed: {error}"));

    let view = project_player_inventory(&registries, &state)
        .unwrap_or_else(|error| panic!("equipment-projection initial view failed: {error}"));
    assert!(view.equipment().is_empty());
    assert_eq!(view.local_equipment().len(), 1);
    assert_eq!(view.local_equipment()[0].equipment(), pick);
    assert_eq!(view.local_equipment()[0].definition(), EQUIPMENT_STONE_PICK);
    assert_eq!(
        view.local_equipment()[0].mass(),
        Mass::from_milligrams(1_000_000)
    );

    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("equipment-projection pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("equipment-projection pickup commit failed: {error}"));
    let carried = project_player_inventory(&registries, &state)
        .unwrap_or_else(|error| panic!("equipment-projection carried view failed: {error}"));
    assert_eq!(carried.equipment().len(), 1);
    assert_eq!(carried.equipment()[0].equipment(), pick);
    assert!(carried.local_equipment().is_empty());

    validate_drop_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("equipment-projection drop failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("equipment-projection drop commit failed: {error}"));
    let dropped = project_player_inventory(&registries, &state)
        .unwrap_or_else(|error| panic!("equipment-projection dropped view failed: {error}"));
    assert!(dropped.equipment().is_empty());
    assert_eq!(dropped.local_equipment().len(), 1);
    assert_eq!(dropped.local_equipment()[0].equipment(), pick);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("equipment-projection matter audit failed: {error}"))
            .total(),
        matter_before.total()
    );
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
