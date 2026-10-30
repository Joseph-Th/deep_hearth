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
fn hot_ground_lot_cannot_be_hand_picked_by_exact_stack_or_quick_move() {
    use crate::content::{FORM_CHIP, MATERIAL_STONE};
    use crate::core::state::validate_loaded_state;
    use crate::inventory::MaterialLotSelection;
    use crate::logistics::{validate_pickup_from_ground, validate_pickup_stack_from_ground};
    use crate::material::MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let registries = build_registries();
    let mut state = AppState::new();
    let ground =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000)).unwrap();
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let hot_temperature = Temperature::from_millikelvin(423_150);
    let hot_mass = Mass::from_milligrams(400_000);
    let safe_mass = Mass::from_milligrams(200_000);
    let hot = deposit_lot_for_test(
        &registries,
        &mut state,
        ground,
        commodity,
        hot_mass,
        hot_temperature,
    )
    .unwrap();
    let safe = deposit_lot_for_test(
        &registries,
        &mut state,
        ground,
        commodity,
        safe_mass,
        TEMPERATURE,
    )
    .unwrap();
    validate_place_ground_stockpile(&state, ground, POSITION)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let carried = validate_admit_player(&registries, &state, POSITION)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let stacks = project_material_stacks(&registries, &state, ground).unwrap();
    let hot_stack = stacks
        .iter()
        .find(|stack| stack.temperature() == hot_temperature)
        .unwrap();
    let safe_stack = stacks
        .iter()
        .find(|stack| stack.temperature() == TEMPERATURE)
        .unwrap();
    let baseline = state.clone();
    let expected = GroundMaterialTransferError::MaterialTooHot {
        commodity,
        temperature: hot_temperature,
        maximum: MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
    };
    assert_eq!(
        validate_pickup_from_ground(
            &registries,
            &state,
            ground,
            &[MaterialLotSelection::new(hot, hot_mass)],
        )
        .err(),
        Some(expected.clone()),
        "exact-lot pickup must reject unsafe heat",
    );
    assert_eq!(
        validate_pickup_from_ground(
            &registries,
            &state,
            ground,
            &[
                MaterialLotSelection::new(safe, safe_mass),
                MaterialLotSelection::new(hot, hot_mass),
            ],
        )
        .err(),
        Some(expected.clone()),
        "a mixed-temperature batch cannot partially transfer when one selected lot is unsafe",
    );
    assert_eq!(
        validate_pickup_stack_from_ground(&registries, &state, ground, hot_stack, hot_mass).err(),
        Some(expected.clone()),
        "stack pickup must not bypass exact-lot heat validation",
    );
    assert_eq!(
        validate_quick_move_player_stack(&registries, &state, hot_stack, carried).err(),
        Some(PlayerQuickMoveError::Transfer(expected)),
        "quick-move must not bypass heat validation",
    );
    assert_eq!(state, baseline);

    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let outcome = validate_quick_move_player_stack(&registries, &state, safe_stack, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(outcome.mass(), safe_mass);
    assert_eq!(state.inventory().get_lot(hot).unwrap().stockpile(), ground);
    assert_eq!(state.inventory().get_lot(hot).unwrap().mass(), hot_mass);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(commodity),
        safe_mass
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);
}

#[test]
fn hand_transfer_temperature_limit_is_inclusive_and_hot_carried_matter_can_be_set_down() {
    use crate::content::{FORM_CHIP, MATERIAL_STONE};
    use crate::core::state::validate_loaded_state;
    use crate::inventory::MaterialLotSelection;
    use crate::logistics::{validate_drop_to_ground, validate_pickup_from_ground};
    use crate::material::MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let registries = build_registries();
    let mut state = AppState::new();
    let ground =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    validate_place_ground_stockpile(&state, ground, POSITION)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let carried = validate_admit_player(&registries, &state, POSITION)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let mass = Mass::from_milligrams(10_000);
    let unsafe_temperature =
        Temperature::from_millikelvin(MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE.millikelvin() + 1);
    let hot_carried = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        mass,
        unsafe_temperature,
    )
    .unwrap();
    let safe_ground = deposit_lot_for_test(
        &registries,
        &mut state,
        ground,
        commodity,
        mass,
        MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
    )
    .unwrap();
    let hot_ground = deposit_lot_for_test(
        &registries,
        &mut state,
        ground,
        commodity,
        mass,
        unsafe_temperature,
    )
    .unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(
        validate_pickup_from_ground(
            &registries,
            &state,
            ground,
            &[MaterialLotSelection::new(hot_ground, mass)],
        )
        .err(),
        Some(GroundMaterialTransferError::MaterialTooHot {
            commodity,
            temperature: unsafe_temperature,
            maximum: MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
        }),
    );
    // The same threshold is inclusive, not a rounded-degree approximation.
    validate_pickup_from_ground(
        &registries,
        &state,
        ground,
        &[MaterialLotSelection::new(safe_ground, mass)],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    // Existing hot custody can still be released: dropping cannot strand it
    // inside the pack merely because the bare-hand pickup rule rejects it.
    validate_drop_to_ground(
        &registries,
        &state,
        ground,
        &[MaterialLotSelection::new(hot_carried, mass)],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(
        state.inventory().get_lot(safe_ground).unwrap().stockpile(),
        carried
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        mass
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ground)
            .unwrap()
            .stored_mass(),
        Mass::from_milligrams(2 * mass.milligrams())
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(replay, state);
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
fn quick_move_does_not_bypass_exclusive_player_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = local_container_with_wood(
        &registries,
        &mut state,
        Mass::from_milligrams(2_000_000),
        Mass::from_milligrams(1_000_000),
    );
    let carried = validate_admit_player(&registries, &state, POSITION)
        .unwrap_or_else(|error| panic!("busy quick-move admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("busy quick-move admission commit failed: {error}"))
        .carried_stockpile();
    let stack = project_material_stacks(&registries, &state, source)
        .unwrap_or_else(|error| panic!("busy quick-move stack projection failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("busy quick-move stack disappeared"));
    let prepared = validate_quick_move_player_stack(&registries, &state, &stack, carried)
        .unwrap_or_else(|error| panic!("busy quick-move initial validation failed: {error}"));
    let stone = crate::content::MATERIAL_STONE;
    let spec = crate::surface::SurfaceResourceSpec::new(
        crate::surface::SurfaceResourceKind::Opportunity,
        POSITION,
        CommodityKey::new(stone, crate::content::FORM_CHIP),
        Mass::from_milligrams(50_000),
        TEMPERATURE,
        crate::material::MaterialComposition::pure(stone),
    )
    .unwrap_or_else(|error| panic!("busy quick-move gathering source invalid: {error}"));
    let creation = crate::surface::validate_create_surface_resources(&registries, &state, [spec])
        .unwrap_or_else(|error| panic!("busy quick-move gathering creation failed: {error}"));
    let resource = creation.first_resource_id();
    creation
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("busy quick-move gathering source commit failed: {error}"));
    crate::surface::validate_start_player_surface_gathering(
        &registries,
        &state,
        crate::content::SURFACE_GATHERING_HAND_COLLECT_STONE_CHIPS,
        resource,
        Mass::from_milligrams(50_000),
    )
    .unwrap_or_else(|error| panic!("busy quick-move gathering admission failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("busy quick-move gathering commit failed: {error}"));
    let before = state.clone();
    let current_stack = project_material_stacks(&registries, &state, source)
        .unwrap_or_else(|error| panic!("busy quick-move current projection failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("busy quick-move current stack disappeared"));
    assert_eq!(
        validate_quick_move_player_stack(&registries, &state, &current_stack, carried).err(),
        Some(PlayerQuickMoveError::Transfer(
            GroundMaterialTransferError::PlayerBusy
        ))
    );
    assert_eq!(
        prepared.commit(&mut state),
        Err(GroundMaterialTransferCommitError::PlayerBecameBusy)
    );
    assert_eq!(state, before);
    let total = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("busy quick-move matter baseline failed: {error}"))
        .total();
    let encoded = serde_json::to_vec(&crate::persistence::SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("busy quick-move serialization failed: {error}"));
    let loaded: crate::persistence::LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("busy quick-move decode failed: {error}"));
    let mut replay = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("busy quick-move trusted load failed: {error}"));
    while state.player_work().active().is_some() {
        let next = crate::simulation::advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("busy quick-move work completion failed: {error}"));
        let replayed = crate::simulation::advance_control_tick(&registries, &mut replay)
            .unwrap_or_else(|error| panic!("busy quick-move replay failed: {error}"));
        assert_eq!(next, replayed);
    }
    assert_eq!(state, replay);
    let refreshed = project_material_stacks(&registries, &state, source)
        .unwrap_or_else(|error| panic!("ready quick-move projection failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("ready quick-move source disappeared"));
    let moved = validate_quick_move_player_stack(&registries, &state, &refreshed, carried)
        .unwrap_or_else(|error| panic!("ready quick-move admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("ready quick-move commit failed: {error}"));
    assert_eq!(moved.mass(), Mass::from_milligrams(1_000_000));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("post-work quick-move conservation failed: {error}"))
            .total(),
        total
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
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
