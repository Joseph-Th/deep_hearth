//! Tests for the sibling disassembly execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK, FORM_FLYWHEEL,
    FORM_HANDLE, FORM_SCRAP, FORM_TOOL, MANUAL_POWER_HAND_CRANK, MATERIAL_STONE, MATERIAL_WOOD,
    build_registries,
};
use crate::core::quantity::{Energy, Temperature};
use crate::core::state::validate_loaded_state;
use crate::core::time::WorldSeed;
use crate::energy::validate_assemble_energy_store;
use crate::equipment::{
    apply_equipment_condition_plan, decide_equipment_wear, validate_assemble_equipment,
};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::{ManualPowerRequest, validate_start_manual_power};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::survival::initialize_player_survival;

fn assembled_pick(registries: &Registries, state: &mut AppState) -> EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("disassembly pick source failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
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
        .unwrap_or_else(|error| panic!("disassembly pick material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, EQUIPMENT_STONE_PICK, source)
        .unwrap_or_else(|error| panic!("disassembly pick assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("disassembly pick assembly commit failed: {error}"))
}

fn assembled_crank(registries: &Registries, state: &mut AppState) -> EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("disassembly crank source failed: {error}"));
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
        .unwrap_or_else(|error| panic!("disassembly crank material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, EQUIPMENT_STONE_HAND_CRANK, source)
        .unwrap_or_else(|error| panic!("disassembly crank assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("disassembly crank assembly commit failed: {error}"))
}

fn assembled_store(registries: &Registries, state: &mut AppState) -> crate::energy::EnergyStoreId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("disassembly store source failed: {error}"));
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
        .unwrap_or_else(|error| panic!("disassembly store material failed: {error}"));
    }
    validate_assemble_energy_store(registries, state, ENERGY_STONE_FLYWHEEL_DRIVE, source)
        .unwrap_or_else(|error| panic!("disassembly store assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("disassembly store assembly commit failed: {error}"))
}

#[test]
fn pristine_disassembly_recovers_exact_matter_without_reusing_identity() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xD15A_0001));
    let pick = assembled_pick(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("disassembly destination failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("disassembly matter before failed: {error}"))
        .total();

    let outcome = validate_disassemble_equipment(&registries, &state, pick, destination)
        .unwrap_or_else(|error| panic!("disassembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("disassembly commit failed: {error}"));
    assert_eq!(outcome.recovered_lots().len(), 2);
    assert!(state.equipment().get_equipment(pick).is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(1_000_000))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("disassembly matter after failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("disassembly state audit failed: {error}"));

    let replacement = assembled_pick(&registries, &mut state);
    assert!(
        replacement > pick,
        "equipment IDs must remain monotonic after disassembly"
    );
}

#[test]
fn worn_equipment_recovers_as_same_material_scrap_without_resetting_components() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xD15A_0002));
    let pick = assembled_pick(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("worn disassembly destination failed: {error}"));
    let wear = decide_equipment_wear(&state, pick, 1)
        .unwrap_or_else(|error| panic!("worn disassembly wear decision failed: {error}"));
    apply_equipment_condition_plan(&mut state, wear)
        .unwrap_or_else(|error| panic!("worn disassembly wear commit failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("worn disassembly matter before failed: {error}"))
        .total();

    let outcome = validate_disassemble_equipment(&registries, &state, pick, destination)
        .unwrap_or_else(|error| panic!("worn disassembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("worn disassembly commit failed: {error}"));
    assert!(state.equipment().get_equipment(pick).is_none());
    let recovered = outcome
        .recovered_lots()
        .iter()
        .map(|lot| {
            state
                .inventory()
                .get_lot(*lot)
                .unwrap_or_else(|| panic!("worn recovery lot disappeared"))
                .commodity()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        recovered,
        std::collections::BTreeSet::from([
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP),
        ])
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("worn disassembly matter after failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("worn disassembly state audit failed: {error}"));
}

#[test]
fn manual_power_start_invalidates_prior_pristine_disassembly_without_equipment_revision_change() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xD15A_0003));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("disassembly race survival setup failed: {error}"));
    let crank = assembled_crank(&registries, &mut state);
    let store = assembled_store(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("disassembly race destination failed: {error}"));
    let token = validate_disassemble_equipment(&registries, &state, crank, destination)
        .unwrap_or_else(|error| panic!("disassembly race validation failed: {error}"));
    let equipment_revision = state.equipment().revision();
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
    .unwrap_or_else(|error| panic!("disassembly race manual-power validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("disassembly race manual-power commit failed: {error}"));
    assert_eq!(
        state.equipment().revision(),
        equipment_revision,
        "manual-power admission should reserve the crank without front-loading wear"
    );

    assert_eq!(
        token.commit(&mut state),
        Err(EquipmentDisassemblyCommitError::EquipmentBusyManualPower { equipment: crank })
    );
    assert!(state.equipment().get_equipment(crank).is_some());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
}
