//! Ordinary-player contracts for conserved dropped matter and capacity-limited recovery.

use deep_hearth::content::gameplay_fixture::{seed_ground_stockpile, seed_lot};
use deep_hearth::content::{FORM_CHIP, FORM_LOG, MATERIAL_STONE, MATERIAL_WOOD, build_registries};
use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::{MaterialStackProjection, project_material_stacks};
use deep_hearth::logistics::validate_pickup_stack_from_ground;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::player::validate_admit_player;
use deep_hearth::simulation::advance_control_tick;
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::surface::{available_player_world_drops, validate_drop_player_material};

fn stack_for(
    registries: &deep_hearth::registry::Registries,
    state: &AppState,
    stockpile: deep_hearth::inventory::StockpileId,
    commodity: CommodityKey,
) -> MaterialStackProjection {
    project_material_stacks(registries, state, stockpile)
        .unwrap_or_else(|error| panic!("world-drop gameplay stack projection failed: {error}"))
        .into_iter()
        .find(|stack| stack.commodity() == commodity)
        .unwrap_or_else(|| panic!("world-drop gameplay stack {commodity:?} disappeared"))
}

#[test]
fn ordinary_drop_pickup_fills_remaining_capacity_and_leaves_conserved_remainder() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let source = seed_ground_stockpile(&mut state, position, Mass::from_milligrams(30_000_000));
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let wood = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    seed_lot(
        &registries,
        &mut state,
        source,
        stone,
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
    );
    seed_lot(
        &registries,
        &mut state,
        source,
        wood,
        Mass::from_milligrams(24_800_000),
        Temperature::from_millikelvin(293_150),
    );
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("world-drop gameplay admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("world-drop gameplay admission commit failed: {error}"))
        .carried_stockpile();

    let stone_stack = stack_for(&registries, &state, source, stone);
    validate_pickup_stack_from_ground(
        &registries,
        &state,
        source,
        &stone_stack,
        Mass::from_milligrams(500_000),
    )
    .unwrap_or_else(|error| panic!("world-drop gameplay stone pickup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("world-drop gameplay stone pickup commit failed: {error}"));
    let wood_stack = stack_for(&registries, &state, source, wood);
    validate_pickup_stack_from_ground(
        &registries,
        &state,
        source,
        &wood_stack,
        Mass::from_milligrams(24_300_000),
    )
    .unwrap_or_else(|error| panic!("world-drop gameplay initial wood pickup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("world-drop gameplay initial wood pickup commit failed: {error}")
    });

    let dropped = validate_drop_player_material(&registries, &state, stone)
        .unwrap_or_else(|error| panic!("world-drop gameplay direct drop failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("world-drop gameplay direct drop commit failed: {error}"));
    assert_eq!(dropped.dropped_mass(), Mass::from_milligrams(500_000));
    assert_eq!(dropped.resources().len(), 1);
    let resource = dropped.resources()[0];
    let projected = available_player_world_drops(&state).collect::<Vec<_>>();
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].resource(), resource);
    assert_eq!(projected[0].mass(), Mass::from_milligrams(500_000));
    assert!(!projected[0].automatic_pickup_ready());

    let remaining_wood = stack_for(&registries, &state, source, wood);
    validate_pickup_stack_from_ground(
        &registries,
        &state,
        source,
        &remaining_wood,
        Mass::from_milligrams(500_000),
    )
    .unwrap_or_else(|error| panic!("world-drop gameplay final wood pickup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("world-drop gameplay final wood pickup commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.available_capacity()),
        Some(Mass::from_milligrams(200_000))
    );
    let matter_before_pickup = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("world-drop gameplay matter baseline failed: {error}"));

    // Pickup becomes eligible exactly two seconds (40 control steps) after the drop.
    // The boundary step itself may transfer matter, not the step after it.
    for _ in 0..39 {
        let outcome = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("world-drop gameplay delay tick failed: {error}"));
        assert_eq!(outcome.surface_drop_pickup(), None);
        assert!(
            !available_player_world_drops(&state)
                .next()
                .unwrap_or_else(|| panic!("delayed drop disappeared"))
                .automatic_pickup_ready()
        );
    }
    let outcome = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("world-drop gameplay pickup tick failed: {error}"));
    let pickup = outcome
        .surface_drop_pickup()
        .unwrap_or_else(|| panic!("world-drop gameplay partial pickup did not occur"));
    assert_eq!(pickup.resource(), resource);
    assert_eq!(pickup.destination(), carried);
    assert_eq!(pickup.picked_mass(), Mass::from_milligrams(200_000));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(stone)),
        Some(Mass::from_milligrams(200_000))
    );
    let remainder = available_player_world_drops(&state).collect::<Vec<_>>();
    assert_eq!(remainder.len(), 1);
    assert_eq!(remainder[0].resource(), resource);
    assert_eq!(remainder[0].mass(), Mass::from_milligrams(300_000));
    assert!(remainder[0].automatic_pickup_ready());
    let matter_after_pickup = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("world-drop gameplay matter audit failed: {error}"));
    assert_eq!(matter_after_pickup.total(), matter_before_pickup.total());
}
