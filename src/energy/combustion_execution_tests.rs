//! Contract tests for material-backed local campfire combustion and warmth.

use crate::content::{
    CAMPFIRE_BOARD_FUEL_MASS, CAMPFIRE_BOARD_FUEL_THERMAL_ENERGY, CAMPFIRE_CHIP_FUEL_MASS,
    CAMPFIRE_CHIP_FUEL_THERMAL_ENERGY, CAMPFIRE_FUEL_MASS, CAMPFIRE_FUEL_THERMAL_ENERGY,
    ENERGY_STONE_RING_CAMPFIRE, FORM_BOARD, FORM_CHIP, FORM_LOG, FORM_LUMP, MATERIAL_STONE,
    MATERIAL_WOOD, PROCESS_SHAPE_WOOD_BOARDS, build_registries,
};
use crate::core::quantity::{AggregateMass, Mass};
use crate::core::state::{AppState, validate_loaded_state};
use crate::crafting::validate_start_player_manual_craft;
use crate::inventory::{
    MaterialStackProjectionError, deposit_bulk_for_test, deposit_composed_lot_for_test,
    deposit_lot_for_test, project_material_stacks,
};
use crate::logistics::{
    PlayerActiveSelection, relocate_player_for_test, validate_set_active_selection,
};
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{
    PlayerDirectControlInput, SecondaryActionOutcome, SecondaryControlIntent,
    advance_control_tick_with_direct_input, advance_tick,
};
use crate::spatial::{VIEW_DIRECTION_PARTS_PER_MILLION, ViewDirection, VoxelCoord};
use crate::survival::player_record_with_thermal_comfort;
use std::num::NonZeroU64;

use crate::energy::validate_assemble_energy_store;

use super::{
    CombustionError, validate_combust_fuel, validate_combust_selected_fuel,
    validate_combust_stack_fuel,
};

#[test]
fn prepared_weapon_prevents_firelighting_and_stales_prior_fuel_commit() {
    use crate::combat::{validate_cancel_primitive_preparation, validate_prepare_primitive_strike};
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::equipment::validate_assemble_equipment;
    use crate::inventory::deposit_lot_for_test;
    use crate::logistics::validate_pickup_equipment;

    let (registries, mut state, carried, fire) = campfire_fixture();
    let assembly = registries
        .equipment()
        .get_equipment(EQUIPMENT_WOODEN_CLUB)
        .unwrap()
        .assembly_profile()
        .unwrap()
        .clone();
    for input in assembly.inputs() {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            crate::core::quantity::Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let club = validate_assemble_equipment(&registries, &state, EQUIPMENT_WOODEN_CLUB, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_pickup_equipment(&registries, &state, club)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::equipment(club)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();

    let token = validate_combust_fuel(&registries, &state, fire, carried).unwrap();
    let previous_combat_revision = state.combat().revision();
    let facing = ViewDirection::new(1_000_000, 0, 0).unwrap();
    let _ = validate_prepare_primitive_strike(&registries, &state, facing)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let current_combat_revision = state.combat().revision();
    let material_before = calculate_matter_accounting(&state).unwrap().total();
    let prepared = state.clone();
    assert_eq!(
        validate_combust_fuel(&registries, &state, fire, carried).err(),
        Some(CombustionError::PreparedStrikeActive),
    );
    assert_eq!(
        validate_combust_selected_fuel(
            &registries,
            &state,
            fire,
            carried,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        )
        .err(),
        Some(CombustionError::PreparedStrikeActive),
    );
    assert_eq!(
        token.commit(&mut state),
        Err(super::CombustionCommitError::StaleCombatRevision {
            expected: previous_combat_revision,
            actual: current_combat_revision,
        })
    );
    assert_eq!(state, prepared);
    assert_eq!(
        state.energy().get_store(fire).unwrap().stored(),
        crate::core::quantity::Energy::ZERO
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        material_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(replay, state);
    for game in [&mut state, &mut replay] {
        let _ = validate_cancel_primitive_preparation(game)
            .unwrap()
            .commit(game)
            .unwrap();
        let _ = validate_combust_fuel(&registries, game, fire, carried)
            .unwrap()
            .commit(game)
            .unwrap();
        assert!(
            game.energy().get_store(fire).unwrap().stored() > crate::core::quantity::Energy::ZERO
        );
    }
    assert_eq!(replay, state);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        material_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn real_woodworking_chips_can_refuel_camp_without_consuming_reserved_logs() {
    let (registries, mut state, carried, fire) = campfire_fixture();
    let logs = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let chips = CommodityKey::new(MATERIAL_WOOD, FORM_CHIP);
    let boards = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    deposit_bulk_for_test(
        &registries,
        &mut state,
        carried,
        logs,
        Mass::from_milligrams(3_000_000),
    )
    .unwrap_or_else(|error| panic!("extra timber for woodworking rejected: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("chips baseline matter failed: {error}"))
        .total();
    let craft = |state: &AppState, batches: u64| {
        validate_start_player_manual_craft(
            &registries,
            state,
            PROCESS_SHAPE_WOOD_BOARDS,
            NonZeroU64::new(batches).unwrap_or_else(|| panic!("test batch count must be positive")),
            None,
        )
    };
    let first = craft(&state, 2)
        .unwrap_or_else(|error| panic!("two woodworking batches rejected: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("two woodworking batches commit failed: {error}"));
    let first_end = state
        .production()
        .get_job(first)
        .unwrap_or_else(|| panic!("first woodworking job vanished"))
        .completes_at();
    while state.tick() < first_end {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("first woodworking continuation failed: {error}"));
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap_or_else(|| panic!("carried stockpile vanished"))
            .get_mass(chips),
        Mass::from_milligrams(400_000)
    );
    let before_rejection = state.clone();
    assert!(
        matches!(
            validate_combust_selected_fuel(&registries, &state, fire, carried, chips),
            Err(CombustionError::InsufficientFuel { available, required, .. })
                if available == Mass::from_milligrams(400_000) && required == CAMPFIRE_CHIP_FUEL_MASS
        ),
        "insufficient sawdust must not silently burn a log instead"
    );
    assert_eq!(state, before_rejection);

    let final_board = craft(&state, 1)
        .unwrap_or_else(|error| panic!("final woodworking batch rejected: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("final woodworking batch commit failed: {error}"));
    let second_end = state
        .production()
        .get_job(final_board)
        .unwrap_or_else(|| panic!("final woodworking job vanished"))
        .completes_at();
    while state.tick() < second_end {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("final woodworking continuation failed: {error}"));
    }
    let before_burn = state
        .inventory()
        .get_stockpile(carried)
        .unwrap_or_else(|| panic!("woodworking carrier disappeared"));
    assert_eq!(before_burn.get_mass(chips), Mass::from_milligrams(600_000));
    assert_eq!(
        before_burn.get_mass(boards),
        Mass::from_milligrams(2_400_000)
    );
    assert_eq!(before_burn.get_mass(logs), CAMPFIRE_FUEL_MASS);
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(chips)),
    )
    .unwrap_or_else(|error| panic!("chip quick selection rejected: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("chip quick selection commit failed: {error}"));
    let upward = ViewDirection::new(0, VIEW_DIRECTION_PARTS_PER_MILLION, 0)
        .unwrap_or_else(|error| panic!("campfire view failed: {error}"));
    let outcome = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(upward)),
    )
    .unwrap_or_else(|error| panic!("chip-fired player action failed: {error}"));
    assert_eq!(
        outcome.secondary_action(),
        Some(&SecondaryActionOutcome::CombustionFuelAdded {
            store: fire,
            released: CAMPFIRE_CHIP_FUEL_THERMAL_ENERGY,
        })
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap_or_else(|| panic!("chip fuel carrier disappeared"))
            .get_mass(chips),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap_or_else(|| panic!("remaining firewood carrier disappeared"))
            .get_mass(logs),
        CAMPFIRE_FUEL_MASS
    );
    assert_eq!(
        state
            .energy()
            .get_store(fire)
            .unwrap_or_else(|| panic!("chip-fired camp disappeared"))
            .stored(),
        CAMPFIRE_CHIP_FUEL_THERMAL_ENERGY
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("chip-fired final matter accounting failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let snapshot = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("chip-fired save failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&snapshot)
        .unwrap_or_else(|error| panic!("chip-fired save parse failed: {error}"));
    state = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("chip-fired load failed: {error}"));
    // The original log route works after chip burning and save/load, with cumulative finite heat.
    let log_released = validate_combust_fuel(&registries, &state, fire, carried)
        .unwrap_or_else(|error| panic!("post-chip log fueling failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("post-chip log commit failed: {error}"));
    assert_eq!(log_released, CAMPFIRE_FUEL_THERMAL_ENERGY);
    assert_eq!(
        state
            .energy()
            .get_store(fire)
            .unwrap_or_else(|| panic!("mixed-fuel camp disappeared"))
            .stored(),
        CAMPFIRE_CHIP_FUEL_THERMAL_ENERGY
            .checked_add(CAMPFIRE_FUEL_THERMAL_ENERGY)
            .unwrap_or_else(|| panic!("mixed heat exceeds physical range"))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("mixed-fuel matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn recovered_timber_boards_can_fuel_campfire_without_burning_other_wood() {
    let (registries, mut state, carried, fire) = campfire_fixture();
    let boards = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let logs = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    deposit_bulk_for_test(
        &registries,
        &mut state,
        carried,
        boards,
        Mass::from_milligrams(750_000),
    )
    .unwrap();
    let total = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();

    let released = validate_combust_selected_fuel(&registries, &state, fire, carried, boards)
        .expect("500 g of shaped timber should provide a campfire charge")
        .commit(&mut state)
        .unwrap();
    assert_eq!(released, CAMPFIRE_BOARD_FUEL_THERMAL_ENERGY);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(boards),
        Mass::from_milligrams(250_000)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(logs),
        CAMPFIRE_FUEL_MASS
    );
    assert_eq!(state.energy().get_store(fire).unwrap().stored(), released);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    assert_ne!(state, before);
    assert_eq!(CAMPFIRE_BOARD_FUEL_MASS, Mass::from_milligrams(500_000));
}

fn campfire_fixture() -> (
    crate::registry::Registries,
    AppState,
    crate::inventory::StockpileId,
    crate::energy::EnergyStoreId,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    let origin = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, origin)
        .unwrap_or_else(|error| panic!("campfire player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("campfire player admission commit failed: {error}"));
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("campfire fixture player disappeared"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            CAMPFIRE_FUEL_MASS,
        ),
    ] {
        deposit_bulk_for_test(&registries, &mut state, carried, commodity, mass)
            .unwrap_or_else(|error| panic!("campfire material setup failed: {error}"));
    }
    let fire =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap_or_else(|error| panic!("campfire assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("campfire assembly commit failed: {error}"));
    (registries, state, carried, fire)
}

#[test]
fn selected_fuel_stack_burns_the_chosen_cohort_and_replays_exactly() {
    use crate::core::quantity::Temperature;

    let (registries, mut state, carried, fire) = campfire_fixture();
    let fuel = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let warm = Temperature::from_millikelvin(320_150);
    let warm_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        fuel,
        CAMPFIRE_FUEL_MASS,
        warm,
    )
    .unwrap();
    let stacks = project_material_stacks(&registries, &state, carried).unwrap();
    let warm_stack = stacks
        .iter()
        .find(|stack| stack.temperature() == warm)
        .unwrap();
    assert_eq!(warm_stack.mass(), CAMPFIRE_FUEL_MASS);
    assert_eq!(
        stacks
            .iter()
            .filter(|stack| stack.commodity() == fuel)
            .count(),
        2
    );

    // The ordinary hotbar form-only operation legitimately draws the older,
    // cooler lot. A numbered fuel-stack command must not behave that way.
    let mut form_only = state.clone();
    validate_combust_selected_fuel(&registries, &form_only, fire, carried, fuel)
        .unwrap()
        .commit(&mut form_only)
        .unwrap();
    assert_eq!(
        form_only.inventory().get_lot(warm_lot).unwrap().mass(),
        CAMPFIRE_FUEL_MASS
    );
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for world in [&mut state, &mut replay] {
        let chosen = project_material_stacks(&registries, world, carried)
            .unwrap()
            .into_iter()
            .find(|stack| stack.temperature() == warm)
            .unwrap();
        validate_combust_stack_fuel(&registries, world, fire, carried, &chosen)
            .unwrap()
            .commit(world)
            .unwrap();
        assert!(world.inventory().get_lot(warm_lot).is_none());
        assert_eq!(
            world
                .inventory()
                .get_stockpile(carried)
                .unwrap()
                .get_mass(fuel),
            CAMPFIRE_FUEL_MASS,
            "the older cool fuel remains in custody"
        );
        assert_eq!(
            calculate_matter_accounting(world).unwrap().total(),
            matter_before
        );
        assert_eq!(validate_loaded_state(&registries, world), Ok(()));
    }
    assert_eq!(state, replay);
}

#[test]
fn selected_fuel_stack_rejects_short_impure_and_stale_cohorts_without_mutation() {
    use crate::core::quantity::Temperature;

    let (registries, mut state, carried, fire) = campfire_fixture();
    let fuel = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let warm = Temperature::from_millikelvin(320_150);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        fuel,
        Mass::from_milligrams(10_000),
        warm,
    )
    .unwrap();
    let short = project_material_stacks(&registries, &state, carried)
        .unwrap()
        .into_iter()
        .find(|stack| stack.temperature() == warm)
        .unwrap();
    let before = state.clone();
    assert!(matches!(
        validate_combust_stack_fuel(&registries, &state, fire, carried, &short),
        Err(CombustionError::InsufficientFuel { source, available, required })
            if source == carried && available == Mass::from_milligrams(10_000)
                && required == CAMPFIRE_FUEL_MASS
    ));
    assert_eq!(
        state, before,
        "another full log must not subsidize a short stack"
    );

    let impure = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_STONE, 100_000),
        CompositionComponent::new(MATERIAL_WOOD, 900_000),
    ])
    .unwrap();
    deposit_composed_lot_for_test(
        &registries,
        &mut state,
        carried,
        fuel,
        CAMPFIRE_FUEL_MASS,
        Temperature::from_millikelvin(300_150),
        impure,
    )
    .unwrap();
    let impure_stack = project_material_stacks(&registries, &state, carried)
        .unwrap()
        .into_iter()
        .find(|stack| stack.composition().pure_material().is_none())
        .unwrap();
    let before = state.clone();
    assert!(matches!(
        validate_combust_stack_fuel(&registries, &state, fire, carried, &impure_stack),
        Err(CombustionError::ImpureFuel { fuel: rejected }) if rejected == fuel
    ));
    assert_eq!(state, before);

    let stale = short;
    let before = state.clone();
    assert!(matches!(
        validate_combust_stack_fuel(&registries, &state, fire, carried, &stale),
        Err(CombustionError::StackSelection(
            MaterialStackProjectionError::StaleInventoryRevision { .. }
        ))
    ));
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn campfire_refueling_requires_idle_player_attention_and_preserves_work() {
    let (registries, mut state, carried, fire) = campfire_fixture();
    deposit_bulk_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(3_000_000),
    )
    .unwrap_or_else(|error| panic!("busy refueling woodworking supply failed: {error}"));
    let pending_fuel = validate_combust_fuel(&registries, &state, fire, carried)
        .unwrap_or_else(|error| panic!("busy refueling pre-work validation failed: {error}"));
    let job = validate_start_player_manual_craft(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        NonZeroU64::new(1).unwrap_or_else(|| panic!("one batch must exist")),
        None,
    )
    .unwrap_or_else(|error| panic!("busy refueling woodworking admission failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("busy refueling woodworking commit failed: {error}"));
    let before = state.clone();
    assert_eq!(
        validate_combust_fuel(&registries, &state, fire, carried).err(),
        Some(CombustionError::PlayerBusy)
    );
    assert_eq!(
        pending_fuel.commit(&mut state),
        Err(super::CombustionCommitError::PlayerBecameBusy)
    );
    assert_eq!(state, before);
    assert_eq!(
        state.energy().get_store(fire).map(|record| record.stored()),
        Some(crate::core::quantity::Energy::ZERO)
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("busy refueling save failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("busy refueling decode failed: {error}"));
    let mut replay = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("busy refueling trusted load failed: {error}"));
    let completed_at = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("busy refueling work disappeared"))
        .completes_at();
    while state.tick() < completed_at {
        let next = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("busy refueling work completion failed: {error}"));
        let replayed = advance_tick(&registries, &mut replay)
            .unwrap_or_else(|error| panic!("busy refueling replay failed: {error}"));
        assert_eq!(next, replayed);
    }
    assert_eq!(state, replay);
    assert!(state.player_work().active().is_none());
    let total = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("busy refueling conservation baseline failed: {error}"))
        .total();
    let _ = validate_combust_fuel(&registries, &state, fire, carried)
        .unwrap_or_else(|error| panic!("completed woodworking must permit refueling: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("completed woodworking refueling failed: {error}"));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("post-work fuel conservation failed: {error}"))
            .total(),
        total
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn assembling_another_campfire_requires_idle_player_attention() {
    let (registries, mut state, carried, _) = campfire_fixture();
    deposit_bulk_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
    )
    .unwrap_or_else(|error| panic!("busy assembly stone supply failed: {error}"));
    deposit_bulk_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(3_000_000),
    )
    .unwrap_or_else(|error| panic!("busy assembly woodworking supply failed: {error}"));
    let pending =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap_or_else(|error| panic!("busy assembly pre-work validation failed: {error}"));
    let job = validate_start_player_manual_craft(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        NonZeroU64::new(1).unwrap_or_else(|| panic!("one batch must exist")),
        None,
    )
    .unwrap_or_else(|error| panic!("busy assembly woodworking admission failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("busy assembly woodworking commit failed: {error}"));
    let before = state.clone();
    assert_eq!(
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .err(),
        Some(crate::energy::EnergyStoreAssemblyError::PlayerBusy)
    );
    assert_eq!(
        pending.commit(&mut state),
        Err(crate::energy::EnergyStoreAssemblyCommitError::PlayerBecameBusy)
    );
    assert_eq!(state, before);
    let completes_at = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("busy assembly woodworking job vanished"))
        .completes_at();
    while state.tick() < completes_at {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("busy assembly woodworking tick failed: {error}"));
    }
    assert!(state.player_work().active().is_none());
    let total = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("busy assembly conservation baseline failed: {error}"))
        .total();
    let _ =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap_or_else(|error| panic!("completed woodworking must allow assembly: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("completed woodworking assembly failed: {error}"));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("post-work assembly conservation failed: {error}"))
            .total(),
        total
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn authored_combustion_conserves_fuel_matter_and_creates_finite_heat() {
    let (registries, mut state, carried, fire) = campfire_fixture();
    let before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("campfire pre-burn matter accounting failed: {error}"));

    let released = validate_combust_fuel(&registries, &state, fire, carried)
        .unwrap_or_else(|error| panic!("campfire fuel validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("campfire fuel commit failed: {error}"));
    let after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("campfire post-burn matter accounting failed: {error}"));

    assert_eq!(released, CAMPFIRE_FUEL_THERMAL_ENERGY);
    assert_eq!(
        state.energy().get_store(fire).map(|store| store.stored()),
        Some(released)
    );
    assert_eq!(after.total(), before.total());
    assert_eq!(
        after.combusted(),
        AggregateMass::from_mass(CAMPFIRE_FUEL_MASS)
    );
    assert_eq!(
        before.stored().checked_sub(after.stored()),
        Some(AggregateMass::from_mass(CAMPFIRE_FUEL_MASS))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn passive_campfire_heat_reaches_a_neighbor_but_not_a_distant_player() {
    let (registries, mut state, carried, fire) = campfire_fixture();
    validate_combust_fuel(&registries, &state, fire, carried)
        .unwrap_or_else(|error| panic!("campfire fuel validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("campfire fuel commit failed: {error}"));
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("campfire survival player disappeared"));
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            player.metabolic_energy(),
            player.hydration(),
            player.vitality(),
            player.nutrition(),
            player.vitality_recovery_remainder(),
            700_000,
        ),
    );

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("campfire warming tick failed: {error}"));
    assert_eq!(
        state
            .survival()
            .player()
            .map(|player| player.thermal_comfort_ppm()),
        Some(706_500),
        "one local fire tick should gain 7,500 ppm and pay 1,000 ppm unsheltered loss"
    );

    let adjacent = VoxelCoord::new(1, 0, 0);
    relocate_player_for_test(&mut state, adjacent);
    let adjacent_before = state
        .survival()
        .player()
        .map(|player| player.thermal_comfort_ppm())
        .unwrap_or(0);
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("adjacent exposure tick failed: {error}"));
    assert_eq!(
        state
            .survival()
            .player()
            .map(|player| player.thermal_comfort_ppm()),
        adjacent_before.checked_add(2_750),
        "a player one voxel from the fire should receive half its dissipated warmth"
    );
    relocate_player_for_test(&mut state, VoxelCoord::new(2, 0, 0));
    let distant_before = state.survival().player().unwrap().thermal_comfort_ppm();
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("distant exposure tick failed: {error}"));
    assert_eq!(
        state
            .survival()
            .player()
            .map(|player| player.thermal_comfort_ppm()),
        distant_before.checked_sub(1_000),
        "a fire two voxels away must not heat the player"
    );
    assert!(
        state
            .energy()
            .get_store(fire)
            .is_some_and(|store| !store.stored().is_zero())
    );
}
