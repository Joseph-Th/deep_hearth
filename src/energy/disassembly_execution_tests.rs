//! Tests for the sibling disassembly execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, FORM_FLYWHEEL, FORM_HANDLE,
    MANUAL_POWER_HAND_CRANK, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::Temperature;
use crate::core::state::validate_loaded_state;
use crate::core::time::WorldSeed;
use crate::energy::{calculate_explicit_energy_accounting, validate_assemble_energy_store};
use crate::equipment::validate_assemble_equipment;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::{ManualPowerRequest, validate_start_manual_power};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
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
    let mut state = AppState::new(WorldSeed::new(0xD15E_0001));
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
fn nonempty_store_cannot_be_disassembled_and_destroy_energy() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xD15E_0002));
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
    let mut state = AppState::new(WorldSeed::new(0xD15E_0003));
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
