//! Ordinary-player contracts for familiar carried/container transfer gestures.

use deep_hearth::content::gameplay_fixture::{
    admit_player_at_hunger_warning_boundary, seed_assembled_equipment_at, seed_ground_stockpile,
    seed_lot, seed_preexisting_world_age, seed_preused_assembled_equipment_at, seed_stockpile,
};
use deep_hearth::content::{
    EQUIPMENT_STONE_PICK, FORM_CHIP, FORM_FOOD, FORM_HANDLE, FORM_LOG, FORM_SCRAP, FORM_TOOL,
    MATERIAL_BERRIES, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::core::state::AppState;
use deep_hearth::core::time::SimulationTick;
use deep_hearth::equipment::{
    player_equipment_maintenance_options, validate_player_disassemble_equipment,
    validate_start_player_equipment_maintenance,
};
use deep_hearth::inventory::{MaterialStackProjection, StockpileId, StockpileStorageProfile};
use deep_hearth::logistics::{
    project_player_inventory, validate_drop_equipment, validate_pickup_equipment,
    validate_quick_move_player_stack,
};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::player::validate_admit_player;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::survival::validate_eat_stack_to_full;
use deep_hearth::world::WorldGenerationSeed;
use deep_hearth::world_generation::validate_generate_initial_wilderness;

use super::direct_consumption_timing::finish_direct_consumption_work;
use super::exact_local_runtime::{
    generated_wilderness_surface_spawn, walk_continuous_body_adjacent_and_settle,
};

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
fn player_can_service_one_worn_tool_and_recover_another_through_visible_inventory_actions() {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x5345_5256_4943_455F);
    let position = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, position)
        .unwrap_or_else(|error| panic!("equipment-lifecycle wilderness setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("equipment-lifecycle wilderness setup commit failed: {error}")
        });
    let assembly = seed_stockpile(
        &mut state,
        Mass::from_milligrams(2_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_lot(
        &registries,
        &mut state,
        assembly,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(1_600_000),
        Temperature::from_millikelvin(293_150),
    );
    seed_lot(
        &registries,
        &mut state,
        assembly,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(400_000),
        Temperature::from_millikelvin(293_150),
    );
    let worn = Condition::new(900_000)
        .unwrap_or_else(|error| panic!("equipment-lifecycle condition failed: {error}"));
    let service_pick = seed_preused_assembled_equipment_at(
        &registries,
        &mut state,
        EQUIPMENT_STONE_PICK,
        assembly,
        position,
        worn,
    );
    let recovery_pick = seed_preused_assembled_equipment_at(
        &registries,
        &mut state,
        EQUIPMENT_STONE_PICK,
        assembly,
        position,
        worn,
    );
    let replacement = seed_ground_stockpile(&mut state, position, Mass::from_milligrams(800_000));
    let replacement_commodity = CommodityKey::new(MATERIAL_STONE, FORM_TOOL);
    seed_lot(
        &registries,
        &mut state,
        replacement,
        replacement_commodity,
        Mass::from_milligrams(800_000),
        Temperature::from_millikelvin(293_150),
    );
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("equipment-lifecycle admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("equipment-lifecycle admission commit failed: {error}"))
        .carried_stockpile();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("equipment-lifecycle matter baseline failed: {error}"))
        .total();
    let route = state.available_player_steps().collect::<Vec<_>>();
    assert_eq!(
        route.len(),
        1,
        "equipment-lifecycle spawn should expose one opening leg"
    );
    let away_ticks = walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        route[0],
        "equipment service outbound",
    );
    let return_ticks = walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        position,
        "equipment service return",
    );
    assert!(away_ticks > 0 && return_ticks > 0);

    let initial_view = project_player_inventory(&registries, &state)
        .unwrap_or_else(|error| panic!("equipment-lifecycle initial projection failed: {error}"));
    assert_eq!(initial_view.local_equipment().len(), 2);
    assert!(initial_view.equipment().is_empty());
    let needs = player_equipment_maintenance_options(&registries, &state)
        .unwrap_or_else(|error| panic!("equipment-lifecycle maintenance catalog failed: {error}"));
    assert_eq!(needs.len(), 2);
    assert!(needs.iter().all(|option| !option.replacement_ready()));
    let service_need = needs
        .iter()
        .copied()
        .find(|option| option.equipment() == service_pick)
        .unwrap_or_else(|| panic!("equipment-lifecycle service pick disappeared from catalog"));
    assert_eq!(service_need.replacement(), replacement_commodity);
    assert_eq!(
        service_need.required_replacement_mass(),
        Mass::from_milligrams(800_000)
    );

    let replacement_stack =
        stack_from_local_container(&registries, &state, replacement, replacement_commodity);
    let moved = validate_quick_move_player_stack(&registries, &state, &replacement_stack, carried)
        .unwrap_or_else(|error| panic!("equipment-lifecycle replacement move failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("equipment-lifecycle replacement move commit failed: {error}")
        });
    assert_eq!(moved.mass(), service_need.required_replacement_mass());
    let ready = player_equipment_maintenance_options(&registries, &state)
        .unwrap_or_else(|error| panic!("equipment-lifecycle ready catalog failed: {error}"))
        .into_iter()
        .find(|option| option.equipment() == service_pick)
        .unwrap_or_else(|| panic!("equipment-lifecycle ready service pick disappeared"));
    assert!(ready.replacement_ready());

    let service = validate_start_player_equipment_maintenance(&registries, &state, service_pick)
        .unwrap_or_else(|error| panic!("equipment-lifecycle service start failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("equipment-lifecycle service commit failed: {error}"));
    assert_eq!(service.material_mass(), ready.required_replacement_mass());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(ready.spent())),
        Some(ready.required_replacement_mass()),
        "service should return the replaced working stone as carried spent material immediately"
    );
    let mut completion_seen = false;
    while state.tick() < service.completes_at() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("equipment-lifecycle service tick failed: {error}"));
        match outcome.equipment_maintenance() {
            Some(completion) => {
                assert!(
                    !completion_seen,
                    "maintenance completion must fire exactly once"
                );
                assert_eq!(outcome.tick(), service.completes_at());
                assert_eq!(completion.equipment(), service_pick);
                assert_eq!(completion.condition_after(), ready.target_condition());
                completion_seen = true;
            }
            None => assert!(outcome.tick() < service.completes_at()),
        }
    }
    assert!(completion_seen);
    assert_eq!(
        state
            .equipment()
            .get_equipment(service_pick)
            .map(|record| record.condition()),
        Some(ready.target_condition())
    );

    let recovered = validate_player_disassemble_equipment(&registries, &state, recovery_pick)
        .unwrap_or_else(|error| panic!("equipment-lifecycle recovery validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("equipment-lifecycle recovery commit failed: {error}"));
    assert_eq!(recovered.recovered_lots().len(), 2);
    assert!(state.equipment().get_equipment(recovery_pick).is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP))),
        Some(Mass::from_milligrams(1_600_000)),
        "service and recovery should retain both worn stone components as authored scrap"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE))),
        Some(Mass::from_milligrams(200_000)),
        "recovery should preserve the obsolete pick's intact handle"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))),
        Some(Mass::ZERO),
        "worn working stone must not reappear as pristine reusable tool stock"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("equipment-lifecycle matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        project_player_inventory(&registries, &state)
            .unwrap_or_else(|error| panic!("equipment-lifecycle final projection failed: {error}"))
            .local_equipment()
            .iter()
            .map(|equipment| equipment.equipment())
            .collect::<Vec<_>>(),
        vec![service_pick],
        "service keeps the chosen tool while recovery removes the obsolete one from world custody"
    );
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
fn player_can_choose_one_visible_food_freshness_stack_without_lot_identity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let container = seed_ground_stockpile(&mut state, position, Mass::from_milligrams(1_000_000));
    seed_lot(
        &registries,
        &mut state,
        container,
        berries,
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
    );
    seed_preexisting_world_age(&mut state, SimulationTick::new(1));
    seed_lot(
        &registries,
        &mut state,
        container,
        berries,
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
    );
    let carried = admit_player_at_hunger_warning_boundary(&registries, &mut state, position)
        .carried_stockpile();

    let initial = project_player_inventory(&registries, &state)
        .unwrap_or_else(|error| panic!("food-stack initial projection failed: {error}"));
    let local = initial
        .local_containers()
        .iter()
        .find(|projection| projection.stockpile() == container)
        .unwrap_or_else(|| panic!("food-stack local container disappeared"));
    let berry_stacks = local
        .stacks()
        .iter()
        .filter(|stack| stack.commodity() == berries)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(berry_stacks.len(), 2);
    assert_ne!(
        berry_stacks[0].ambient_storage_age_parts(),
        berry_stacks[1].ambient_storage_age_parts(),
        "different pre-admission food ages must remain distinct visible stacks"
    );

    let first = berry_stacks[0].clone();
    let first_move = validate_quick_move_player_stack(&registries, &state, &first, carried)
        .unwrap_or_else(|error| panic!("food-stack first quick-move failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("food-stack first quick-move commit failed: {error}"));
    assert_eq!(first_move.mass(), Mass::from_milligrams(500_000));
    let second = stack_from_local_container(&registries, &state, container, berries);
    let second_move = validate_quick_move_player_stack(&registries, &state, &second, carried)
        .unwrap_or_else(|error| panic!("food-stack second quick-move failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("food-stack second quick-move commit failed: {error}"));
    assert_eq!(second_move.mass(), Mass::from_milligrams(500_000));

    let carried_view = project_player_inventory(&registries, &state)
        .unwrap_or_else(|error| panic!("food-stack carried projection failed: {error}"));
    let carried_berries = carried_view
        .carried()
        .stacks()
        .iter()
        .filter(|stack| stack.commodity() == berries)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(carried_berries.len(), 2);
    let selected = carried_berries
        .iter()
        .min_by_key(|stack| stack.ambient_storage_age_parts())
        .cloned()
        .unwrap_or_else(|| panic!("food-stack fresher carried cohort disappeared"));
    let unselected = carried_berries
        .iter()
        .max_by_key(|stack| stack.ambient_storage_age_parts())
        .cloned()
        .unwrap_or_else(|| panic!("food-stack older carried cohort disappeared"));
    assert_ne!(
        selected.ambient_storage_age_parts(),
        unselected.ambient_storage_age_parts()
    );
    let selected_before = selected.mass();
    let unselected_before = unselected.mass();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("food-stack matter baseline failed: {error}"));

    let outcome = validate_eat_stack_to_full(&registries, &state, &selected)
        .unwrap_or_else(|error| panic!("food-stack selected eating failed: {error}"))
        .unwrap_or_else(|| panic!("hunger-warning player unexpectedly needed no meal"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("food-stack selected eating commit failed: {error}"));
    assert!(outcome.total_mass() > Mass::ZERO);
    let _elapsed = finish_direct_consumption_work(
        &registries,
        &mut state,
        outcome.completes_at(),
        "selected projected food stack",
    );

    let after = project_player_inventory(&registries, &state)
        .unwrap_or_else(|error| panic!("food-stack post-meal projection failed: {error}"));
    let remaining = after
        .carried()
        .stacks()
        .iter()
        .filter(|stack| stack.commodity() == berries)
        .collect::<Vec<_>>();
    let selected_remaining = selected_before
        .checked_sub(outcome.total_mass())
        .unwrap_or_else(|| panic!("food-stack meal exceeded selected stack"));
    if selected_remaining.is_zero() {
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].mass(), unselected_before);
    } else {
        assert_eq!(remaining.len(), 2);
        let remaining_selected = remaining
            .iter()
            .min_by_key(|stack| stack.ambient_storage_age_parts())
            .unwrap_or_else(|| panic!("food-stack selected remainder disappeared"));
        let remaining_unselected = remaining
            .iter()
            .max_by_key(|stack| stack.ambient_storage_age_parts())
            .unwrap_or_else(|| panic!("food-stack unselected remainder disappeared"));
        assert_eq!(remaining_unselected.mass(), unselected_before);
        assert_eq!(remaining_selected.mass(), selected_remaining);
    }
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("food-stack matter audit failed: {error}"))
            .total(),
        matter_before.total()
    );
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
