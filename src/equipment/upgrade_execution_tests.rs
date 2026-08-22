//! Tests for the sibling upgrade execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
    FORM_FLYWHEEL, FORM_HANDLE, FORM_REINFORCEMENT, FORM_TOOL, MANUAL_POWER_HAND_CRANK,
    MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
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
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::survival::initialize_player_survival;

fn assemble_stone_pick(registries: &Registries, state: &mut AppState) -> EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("upgrade pick source fixture failed: {error}"));
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
        .unwrap_or_else(|error| panic!("upgrade pick material fixture failed: {error}"));
    }
    validate_assemble_equipment(registries, state, EQUIPMENT_STONE_PICK, source)
        .unwrap_or_else(|error| panic!("upgrade pick assembly validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("upgrade pick assembly commit failed: {error}"))
}

fn reinforcement_source(registries: &Registries, state: &mut AppState) -> StockpileId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("upgrade reinforcement source failed: {error}"));
    deposit_lot_for_test(
        registries,
        state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("upgrade reinforcement material failed: {error}"));
    source
}

fn assemble_stone_crank(registries: &Registries, state: &mut AppState) -> EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("upgrade crank source fixture failed: {error}"));
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
        .unwrap_or_else(|error| panic!("upgrade crank material fixture failed: {error}"));
    }
    validate_assemble_equipment(registries, state, EQUIPMENT_STONE_HAND_CRANK, source)
        .unwrap_or_else(|error| panic!("upgrade crank assembly validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("upgrade crank assembly commit failed: {error}"))
}

fn assemble_store(registries: &Registries, state: &mut AppState) -> crate::energy::EnergyStoreId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("upgrade store source fixture failed: {error}"));
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
        .unwrap_or_else(|error| panic!("upgrade store material fixture failed: {error}"));
    }
    validate_assemble_energy_store(registries, state, ENERGY_STONE_FLYWHEEL_DRIVE, source)
        .unwrap_or_else(|error| panic!("upgrade store assembly validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("upgrade store assembly commit failed: {error}"))
}

#[test]
fn additive_upgrade_preserves_identity_wear_and_world_matter() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA66D_0001));
    let pick = assemble_stone_pick(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    let wear = decide_equipment_wear(&state, pick, 87_654)
        .unwrap_or_else(|error| panic!("upgrade wear decision failed: {error}"));
    apply_equipment_condition_plan(&mut state, wear)
        .unwrap_or_else(|error| panic!("upgrade wear commit failed: {error}"));
    let before_record = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("upgrade pick disappeared before upgrade"));
    let condition_before = before_record.condition();
    let created_at_before = before_record.created_at();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("upgrade initial matter accounting failed: {error}"))
        .total();

    let upgraded = validate_upgrade_equipment(
        &registries,
        &state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        reinforcement,
    )
    .unwrap_or_else(|error| panic!("pick upgrade validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("pick upgrade commit failed: {error}"));

    assert_eq!(upgraded, pick);
    let record = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("upgraded pick disappeared"));
    assert_eq!(record.definition(), EQUIPMENT_COPPER_REINFORCED_PICK);
    assert_eq!(record.condition(), condition_before);
    assert_eq!(record.created_at(), created_at_before);
    assert_eq!(record.embodied_mass(), Mass::from_milligrams(1_020_000));
    assert_eq!(record.embodied_material().len(), 3);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("upgrade final matter accounting failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("upgraded state audit failed: {error}"));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("upgraded state serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("upgraded state decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("upgraded state load validation failed: {error}"));
    assert_eq!(loaded, state);
}

#[test]
fn intervening_equipment_mutation_invalidates_upgrade_token() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA66D_0002));
    let pick = assemble_stone_pick(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    let token = validate_upgrade_equipment(
        &registries,
        &state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        reinforcement,
    )
    .unwrap_or_else(|error| panic!("stale upgrade validation failed: {error}"));
    let expected = state.equipment().revision();
    let wear = decide_equipment_wear(&state, pick, 1)
        .unwrap_or_else(|error| panic!("stale upgrade wear decision failed: {error}"));
    apply_equipment_condition_plan(&mut state, wear)
        .unwrap_or_else(|error| panic!("stale upgrade wear commit failed: {error}"));

    assert_eq!(
        token.commit(&mut state),
        Err(EquipmentUpgradeCommitError::StaleEquipment {
            expected,
            actual: expected + 1,
        })
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.definition()),
        Some(EQUIPMENT_STONE_PICK)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(reinforcement)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(20_000))
    );
}

#[test]
fn manual_power_start_invalidates_prior_crank_upgrade_without_equipment_revision_change() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA66D_0003));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("upgrade race survival setup failed: {error}"));
    let crank = assemble_stone_crank(&registries, &mut state);
    let store = assemble_store(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    let token = validate_upgrade_equipment(
        &registries,
        &state,
        crank,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        reinforcement,
    )
    .unwrap_or_else(|error| panic!("upgrade race validation failed: {error}"));
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
    .unwrap_or_else(|error| panic!("upgrade race manual-power validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("upgrade race manual-power commit failed: {error}"));
    assert_eq!(
        state.equipment().revision(),
        equipment_revision,
        "manual-power admission should reserve the crank without front-loading wear"
    );

    assert_eq!(
        token.commit(&mut state),
        Err(EquipmentUpgradeCommitError::EquipmentBusyManualPower { equipment: crank })
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(crank)
            .map(|record| record.definition()),
        Some(EQUIPMENT_STONE_HAND_CRANK)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(reinforcement)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(20_000))
    );
}
