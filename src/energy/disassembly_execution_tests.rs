//! Contract tests for energy-store disassembly and matter recovery.

use super::*;
use crate::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, FORM_FLYWHEEL, FORM_HANDLE,
    MANUAL_POWER_HAND_CRANK, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::Temperature;
use crate::core::state::{apply_clock_advance, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::energy::{calculate_explicit_energy_accounting, validate_assemble_energy_store};
use crate::equipment::validate_assemble_equipment;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::{ManualPowerRequest, validate_start_manual_power};
use crate::logistics::validate_initialize_player_logistics;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::spatial::VoxelCoord;
use crate::survival::initialize_player_survival;

fn assembled_store(registries: &Registries, state: &mut AppState) -> EnergyStoreId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("store disassembly source failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            registries,
            state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("store disassembly material failed: {error}"));
    }
    validate_assemble_energy_store(registries, state, ENERGY_STONE_FLYWHEEL_DRIVE, source)
        .unwrap_or_else(|error| panic!("store disassembly assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("store disassembly assembly commit failed: {error}"))
}

#[test]
fn disassembly_removes_energy_store_location() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(2, 0, 4);
    let store = assembled_store(&registries, &mut state);
    let destination =
        validate_initialize_player_logistics(&state, position, Mass::from_milligrams(2_000_000))
            .unwrap_or_else(|error| panic!("located store disassembly logistics failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("located store disassembly logistics commit failed: {error}")
            })
            .carried_stockpile();
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_energy_store_placement(
        revision,
        revision + 1,
        store,
        position,
    );
    assert_eq!(
        state.logistics().energy_store_position(store),
        Some(position)
    );

    let _ = validate_disassemble_energy_store(&registries, &state, store, destination)
        .unwrap_or_else(|error| panic!("located store disassembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("located store disassembly commit failed: {error}"));

    assert_eq!(state.logistics().energy_store_position(store), None);
    assert!(state.energy().get_store(store).is_none());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

fn assembled_crank(registries: &Registries, state: &mut AppState) -> crate::equipment::EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("store-disassembly crank source failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            registries,
            state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("store-disassembly crank material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, EQUIPMENT_STONE_HAND_CRANK, source)
        .unwrap_or_else(|error| panic!("store-disassembly crank assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("store-disassembly crank commit failed: {error}"))
}

#[test]
fn empty_store_disassembly_recovers_exact_matter_without_reusing_identity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let store = assembled_store(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("store disassembly destination failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("store disassembly matter before failed: {error}"))
        .total();
    let energy_before = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("store disassembly energy before failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("store disassembly energy before overflowed"));

    let outcome = validate_disassemble_energy_store(&registries, &state, store, destination)
        .unwrap_or_else(|error| panic!("store disassembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("store disassembly commit failed: {error}"));
    assert_eq!(outcome.recovered_lots().len(), 2);
    assert!(state.energy().get_store(store).is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(1_100_000))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("store disassembly matter after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("store disassembly energy after failed: {error}"))
            .total(),
        Some(energy_before),
        "disassembling an energy store must transfer material thermal energy back to inventory"
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("store disassembly state audit failed: {error}"));

    let replacement = assembled_store(&registries, &mut state);
    assert!(
        replacement > store,
        "energy-store IDs must remain monotonic after disassembly"
    );
}

#[test]
fn elapsed_disassembly_validation_cannot_backdate_recovered_energy_store_matter() {
    let registries = build_registries();
    let mut state = AppState::new();
    let store = assembled_store(&registries, &mut state);
    let destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_100_000)).unwrap();
    let before_matter = calculate_matter_accounting(&state).unwrap().total();
    let selected =
        validate_disassemble_energy_store(&registries, &state, store, destination).unwrap();
    let expected = state.tick();
    let inventory_revision = state.inventory().revision();
    apply_clock_advance(&mut state, SimulationTick::new(expected.value() + 1));
    assert_eq!(state.inventory().revision(), inventory_revision);
    let before_commit = state.clone();
    assert_eq!(
        selected.commit(&mut state).err(),
        Some(EnergyStoreDisassemblyCommitError::StaleSimulationTick {
            expected,
            actual: state.tick(),
        })
    );
    assert_eq!(state, before_commit);

    let recovered = validate_disassemble_energy_store(&registries, &state, store, destination)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(recovered.recovered_lots().len(), 2);
    assert!(state.energy().get_store(store).is_none());
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        before_matter
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn nonempty_store_cannot_be_disassembled_and_destroy_energy() {
    let registries = build_registries();
    let mut state = AppState::new();
    let store = assembled_store(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("nonempty store destination failed: {error}"));
    state
        .energy_state_mut()
        .add_stored_energy(store, Energy::from_nanojoules(1));
    let before = state.clone();

    assert_eq!(
        validate_disassemble_energy_store(&registries, &state, store, destination).err(),
        Some(EnergyStoreDisassemblyError::StoreNotEmpty {
            store,
            stored: Energy::from_nanojoules(1),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn manual_power_start_invalidates_prior_empty_store_disassembly_without_energy_revision_change() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("store-disassembly survival setup failed: {error}"));
    let store = assembled_store(&registries, &mut state);
    let crank = assembled_crank(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("store-disassembly race destination failed: {error}"));
    let token = validate_disassemble_energy_store(&registries, &state, store, destination)
        .unwrap_or_else(|error| panic!("store-disassembly race validation failed: {error}"));
    let energy_revision = state.energy().revision();
    validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            store,
            Energy::from_nanojoules(1_000_000_000),
        ),
    )
    .unwrap_or_else(|error| {
        panic!("store-disassembly race manual-power validation failed: {error}")
    })
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("store-disassembly race manual-power commit failed: {error}"));
    assert_eq!(
        state.energy().revision(),
        energy_revision,
        "manual-power admission should not need an energy mutation to reserve an empty destination"
    );

    assert_eq!(
        token.commit(&mut state),
        Err(EnergyStoreDisassemblyCommitError::StoreBusyManualPower { store })
    );
    assert!(state.energy().get_store(store).is_some());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
}

#[test]
fn disassembly_cannot_interrupt_exclusive_player_work_or_commit_after_work_starts() {
    use crate::content::{FORM_LOG, MATERIAL_WOOD, PROCESS_SHAPE_WOOD_BOARDS};
    use crate::crafting::validate_start_player_manual_craft;
    use crate::player::validate_admit_player;
    use crate::simulation::advance_tick;
    use std::num::NonZeroU64;

    let registries = build_registries();
    let mut state = AppState::new();
    let store = assembled_store(&registries, &mut state);
    let position = VoxelCoord::new(0, 0, 0);
    crate::logistics::place_energy_store_for_fixture(&mut state, store, position);
    let destination = validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        destination,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(3_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let prepared =
        validate_disassemble_energy_store(&registries, &state, store, destination).unwrap();
    let job = validate_start_player_manual_craft(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        NonZeroU64::new(1).unwrap(),
        None,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let busy = state.clone();
    let original_matter = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(
        validate_disassemble_energy_store(&registries, &state, store, destination).err(),
        Some(EnergyStoreDisassemblyError::PlayerBusy)
    );
    assert!(matches!(
        prepared.commit(&mut state),
        Err(EnergyStoreDisassemblyCommitError::StaleInventory { expected, actual })
            if expected != actual
    ));
    assert_eq!(state, busy);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        original_matter
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes =
        serde_json::to_vec(&crate::persistence::SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<crate::persistence::LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let completed = state.production().get_job(job).unwrap().completes_at();
    while state.tick() < completed {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    let outcome = validate_disassemble_energy_store(&registries, &state, store, destination)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(outcome.recovered_lots().len(), 2);
    assert!(state.energy().get_store(store).is_none());
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        original_matter
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn energy_store_mutations_reject_remote_material_custody_before_unknown_store_identity() {
    use crate::content::ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE;
    use crate::energy::{
        CombustionError, EnergyStoreUpgradeError, validate_combust_fuel,
        validate_upgrade_energy_store,
    };
    use crate::logistics::{PlayerStockpileAccessError, validate_place_ground_stockpile};
    use crate::player::validate_admit_player;

    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let remote_position = VoxelCoord::new(1, 0, 0);
    let remote =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_100_000)).unwrap();
    validate_place_ground_stockpile(&state, remote, remote_position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let unknown = EnergyStoreId::new(state.energy().next_store_id());
    assert!(state.energy().get_store(unknown).is_none());
    let before = state.clone();
    let access = PlayerStockpileAccessError::RemoteKnownStockpile {
        stockpile: remote,
        stockpile_position: remote_position,
        player_position: position,
    };
    assert_eq!(
        validate_disassemble_energy_store(&registries, &state, unknown, remote).err(),
        Some(EnergyStoreDisassemblyError::DestinationAccess(access))
    );
    assert_eq!(
        validate_upgrade_energy_store(
            &registries,
            &state,
            unknown,
            ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
            remote,
        )
        .err(),
        Some(EnergyStoreUpgradeError::SourceAccess(access))
    );
    assert_eq!(
        validate_combust_fuel(&registries, &state, unknown, remote).err(),
        Some(CombustionError::SourceAccess(access))
    );
    // Once the material endpoint is authorized, a nonexistent energy store still
    // yields its ordinary typed identity rejection.
    assert_eq!(
        validate_disassemble_energy_store(&registries, &state, unknown, carried).err(),
        Some(EnergyStoreDisassemblyError::UnknownStore { store: unknown })
    );
    assert_eq!(
        validate_upgrade_energy_store(
            &registries,
            &state,
            unknown,
            ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
            carried,
        )
        .err(),
        Some(EnergyStoreUpgradeError::UnknownStore { store: unknown })
    );
    assert_eq!(
        validate_combust_fuel(&registries, &state, unknown, carried).err(),
        Some(CombustionError::UnknownStore { store: unknown })
    );
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
