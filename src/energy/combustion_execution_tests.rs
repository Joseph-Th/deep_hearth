//! Contract tests for material-backed local campfire combustion and warmth.

use crate::content::{
    CAMPFIRE_CHIP_FUEL_MASS, CAMPFIRE_CHIP_FUEL_THERMAL_ENERGY, CAMPFIRE_FUEL_MASS,
    CAMPFIRE_FUEL_THERMAL_ENERGY, ENERGY_STONE_RING_CAMPFIRE, FORM_BOARD, FORM_CHIP, FORM_LOG,
    FORM_LUMP, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_SHAPE_WOOD_BOARDS, build_registries,
};
use crate::core::quantity::{AggregateMass, Mass};
use crate::core::state::{AppState, validate_loaded_state};
use crate::crafting::validate_start_player_manual_craft;
use crate::inventory::deposit_bulk_for_test;
use crate::logistics::{
    PlayerActiveSelection, relocate_player_for_test, validate_set_active_selection,
};
use crate::material::CommodityKey;
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

use super::{CombustionError, validate_combust_fuel, validate_combust_selected_fuel};

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
fn campfire_refueling_requires_idle_player_attention_and_preserves_work() {
    let (registries, mut state, carried, fire) = campfire_fixture();
    let _ = deposit_bulk_for_test(
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
    let _ = deposit_bulk_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
    )
    .unwrap_or_else(|error| panic!("busy assembly stone supply failed: {error}"));
    let _ = deposit_bulk_for_test(
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
fn passive_campfire_heat_warms_only_a_colocated_player() {
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

    let remote = VoxelCoord::new(1, 0, 0);
    relocate_player_for_test(&mut state, remote);
    let remote_before = state
        .survival()
        .player()
        .map(|player| player.thermal_comfort_ppm())
        .unwrap_or(0);
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("remote exposure tick failed: {error}"));
    assert_eq!(
        state
            .survival()
            .player()
            .map(|player| player.thermal_comfort_ppm()),
        remote_before.checked_sub(1_000),
        "a remote burning campfire must not warm the player"
    );
    assert!(
        state
            .energy()
            .get_store(fire)
            .is_some_and(|store| !store.stored().is_zero())
    );
}
