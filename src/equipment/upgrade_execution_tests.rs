//! Contract tests for additive equipment upgrades.

use super::*;
use crate::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
    EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR, EQUIPMENT_STONE_CRUSHER,
    EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK, EQUIPMENT_STONE_SEPARATOR, FORM_FLYWHEEL,
    FORM_HANDLE, FORM_REINFORCEMENT, FORM_TOOL, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::{Energy, Temperature};
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::energy::{calculate_explicit_energy_accounting, validate_assemble_energy_store};
use crate::equipment::{
    EquipmentValidationError, apply_equipment_condition_plan, decide_equipment_wear,
    validate_assemble_equipment,
};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::{ManualPowerRequest, validate_start_manual_power};
use crate::material::{CommodityKey, MaterialPhaseStateError};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
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

fn assemble_authored_equipment(
    registries: &Registries,
    state: &mut AppState,
    definition: EquipmentDefinitionId,
) -> EquipmentId {
    let assembly = registries
        .equipment()
        .get_equipment(definition)
        .and_then(|record| record.assembly_profile())
        .unwrap_or_else(|| panic!("upgrade fixture equipment lost its assembly profile"));
    let mass = assembly
        .inputs()
        .iter()
        .try_fold(Mass::ZERO, |total, input| total.checked_add(input.mass()))
        .unwrap_or_else(|| panic!("upgrade fixture assembly mass overflowed"));
    let source = add_solid_stockpile_for_test(state, mass)
        .unwrap_or_else(|error| panic!("upgrade fixture assembly source failed: {error}"));
    for input in assembly.inputs() {
        deposit_lot_for_test(
            registries,
            state,
            source,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("upgrade fixture assembly material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, definition, source)
        .unwrap_or_else(|error| panic!("upgrade fixture equipment assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("upgrade fixture equipment commit failed: {error}"))
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

#[test]
fn primitive_processing_upgrades_preserve_identity_wear_matter_and_replay() {
    let registries = build_registries();
    for (seed, base, upgraded, expected_mass) in [
        (
            0xA66D_1001,
            EQUIPMENT_STONE_CRUSHER,
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            Mass::from_milligrams(2_020_000),
        ),
        (
            0xA66D_1002,
            EQUIPMENT_STONE_SEPARATOR,
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            Mass::from_milligrams(1_220_000),
        ),
    ] {
        let mut state = AppState::new(WorldSeed::new(seed));
        let equipment = assemble_authored_equipment(&registries, &mut state, base);
        let reinforcement = reinforcement_source(&registries, &mut state);
        let wear = decide_equipment_wear(&state, equipment, 123_456)
            .unwrap_or_else(|error| panic!("processing upgrade wear decision failed: {error}"));
        apply_equipment_condition_plan(&mut state, wear)
            .unwrap_or_else(|error| panic!("processing upgrade wear commit failed: {error}"));
        let before = state
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("processing upgrade equipment disappeared"));
        let condition_before = before.condition();
        let created_at_before = before.created_at();
        let matter_before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("processing upgrade matter audit failed: {error}"))
            .total();
        let energy_before = calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("processing upgrade energy audit failed: {error}"))
            .total();

        let result =
            validate_upgrade_equipment(&registries, &state, equipment, upgraded, reinforcement)
                .unwrap_or_else(|error| panic!("processing upgrade validation failed: {error}"))
                .commit(&mut state)
                .unwrap_or_else(|error| panic!("processing upgrade commit failed: {error}"));

        assert_eq!(result, equipment);
        let record = state
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("processing upgrade result disappeared"));
        assert_eq!(record.definition(), upgraded);
        assert_eq!(record.condition(), condition_before);
        assert_eq!(record.created_at(), created_at_before);
        assert_eq!(record.embodied_mass(), expected_mass);
        assert_eq!(record.embodied_material().len(), 3);
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!(
                    "processing upgrade final matter audit failed: {error}"
                ))
                .total(),
            matter_before
        );
        assert_eq!(
            calculate_explicit_energy_accounting(&registries, &state)
                .unwrap_or_else(|error| panic!(
                    "processing upgrade final energy audit failed: {error}"
                ))
                .total(),
            energy_before
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("processing upgrade state audit failed: {error}"));

        let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("processing upgrade serialization failed: {error}"));
        let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
            .unwrap_or_else(|error| panic!("processing upgrade decode failed: {error}"));
        assert_eq!(
            decoded
                .into_state(&registries)
                .unwrap_or_else(|error| panic!("processing upgrade trusted load failed: {error}")),
            state
        );
    }
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
    let energy_before = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("upgrade initial energy accounting failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("upgrade initial explicit energy total overflowed"));

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
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("upgrade final energy accounting failed: {error}"))
            .total(),
        Some(energy_before),
        "additive upgrade must preserve the thermal energy of all exact embodied traces"
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
fn persisted_upgraded_equipment_rejects_impossible_embodied_phase_state() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA66D_0004));
    let pick = assemble_stone_pick(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    validate_upgrade_equipment(
        &registries,
        &state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        reinforcement,
    )
    .unwrap_or_else(|error| panic!("phase-tamper upgrade validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("phase-tamper upgrade commit failed: {error}"));

    let melting_point = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|definition| definition.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("copper fixture lost its melting point"));
    let invalid_temperature = Temperature::from_millikelvin(
        melting_point
            .millikelvin()
            .checked_add(1)
            .unwrap_or_else(|| panic!("copper melting point exhausted temperature range")),
    );
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("phase-tamper equipment serialization failed: {error}"));
    encoded["state"]["systems"]["equipment"]["records"][pick.value().to_string()]["embodied_material"]
        [2]["profile"]["temperature"] = serde_json::json!(invalid_temperature.millikelvin());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("phase-tamper equipment decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Equipment(
            EquipmentValidationError::InvalidEmbodiedPhaseState {
                equipment: pick,
                error: MaterialPhaseStateError::SolidAboveMeltingPoint {
                    material: MATERIAL_COPPER,
                    temperature: invalid_temperature,
                    melting_point,
                },
            }
        )))
    );
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
