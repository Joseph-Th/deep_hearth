//! Contract tests for additive equipment upgrades.

use super::*;
use crate::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
    EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR, EQUIPMENT_STONE_CRUSHER,
    EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK, EQUIPMENT_STONE_SEPARATOR, FORM_FLYWHEEL,
    FORM_HANDLE, FORM_LOG, FORM_REINFORCEMENT, FORM_TOOL, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD, PROCESS_SHAPE_WOOD_BOARDS, build_registries,
};
use crate::core::quantity::{Energy, Temperature};
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::crafting::validate_start_player_manual_craft;
use crate::energy::{calculate_explicit_energy_accounting, validate_assemble_energy_store};
use crate::equipment::{
    EquipmentValidationError, degrade_equipment_condition_for_test, validate_assemble_equipment,
};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::{ManualPowerRequest, validate_start_manual_power};
use crate::logistics::{
    PlayerEquipmentAccessError, PlayerStockpileAccessError, validate_initialize_player_logistics,
    validate_place_ground_stockpile,
};
use crate::material::{CommodityKey, MaterialPhaseStateError};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::survival::initialize_player_survival;
use std::num::NonZeroU64;

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

#[test]
fn equipment_upgrade_cannot_bypass_exclusive_manual_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    let pick = assemble_stone_pick(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    let position = VoxelCoord::new(0, 0, 0);
    crate::logistics::place_equipment_for_fixture(&mut state, pick, position);
    validate_place_ground_stockpile(&state, reinforcement, position)
        .unwrap_or_else(|error| panic!("busy upgrade source placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("busy upgrade source placement commit failed: {error}"));
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("busy upgrade player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("busy upgrade player admission commit failed: {error}"))
        .carried_stockpile();
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(3_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("busy upgrade woodworking supply failed: {error}"));
    let prepared = validate_upgrade_equipment(
        &registries,
        &state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        reinforcement,
    )
    .unwrap_or_else(|error| panic!("busy upgrade initial validation failed: {error}"));
    let job = validate_start_player_manual_craft(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        NonZeroU64::new(1).unwrap_or_else(|| panic!("one batch must exist")),
        None,
    )
    .unwrap_or_else(|error| panic!("busy upgrade woodworking admission failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("busy upgrade woodworking commit failed: {error}"));
    let before = state.clone();
    assert_eq!(
        validate_upgrade_equipment(
            &registries,
            &state,
            pick,
            EQUIPMENT_COPPER_REINFORCED_PICK,
            reinforcement,
        )
        .err(),
        Some(EquipmentUpgradeError::PlayerBusy)
    );
    assert_eq!(
        prepared.commit(&mut state),
        Err(EquipmentUpgradeCommitError::PlayerBecameBusy)
    );
    assert_eq!(state, before);

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("busy upgrade save failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("busy upgrade decode failed: {error}"));
    let mut replay = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("busy upgrade trusted load failed: {error}"));
    let completes_at = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("busy upgrade woodworking job disappeared"))
        .completes_at();
    while state.tick() < completes_at {
        let next = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("busy upgrade woodworking tick failed: {error}"));
        let replayed = advance_tick(&registries, &mut replay)
            .unwrap_or_else(|error| panic!("busy upgrade replay failed: {error}"));
        assert_eq!(next, replayed);
    }
    assert_eq!(state, replay);
    let total = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("busy upgrade matter baseline failed: {error}"))
        .total();
    let upgraded = validate_upgrade_equipment(
        &registries,
        &state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        reinforcement,
    )
    .unwrap_or_else(|error| panic!("idle upgrade validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("idle upgrade commit failed: {error}"));
    assert_eq!(upgraded, pick);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("post-work upgrade matter accounting failed: {error}"))
            .total(),
        total
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn equipment_upgrade_rejects_known_remote_equipment() {
    let registries = build_registries();
    let mut state = AppState::new();
    let equipment = assemble_stone_pick(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote upgrade logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote upgrade logistics commit failed: {error}"));
    let equipment_position = VoxelCoord::new(1, 0, 0);
    let logistics_revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        logistics_revision,
        logistics_revision + 1,
        equipment,
        equipment_position,
    );
    let before = state.clone();

    assert_eq!(
        validate_upgrade_equipment(
            &registries,
            &state,
            equipment,
            EQUIPMENT_COPPER_REINFORCED_PICK,
            reinforcement,
        )
        .err(),
        Some(EquipmentUpgradeError::EquipmentAccess(
            PlayerEquipmentAccessError::RemoteKnownEquipment {
                equipment,
                equipment_position,
                player_position,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn remote_equipment_upgrade_rejects_access_before_base_definition_state() {
    let registries = build_registries();
    let mut state = AppState::new();
    let equipment = assemble_stone_crank(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("wrong-base remote upgrade logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("wrong-base remote upgrade logistics commit failed: {error}")
        });
    let equipment_position = VoxelCoord::new(1, 0, 0);
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        revision,
        revision + 1,
        equipment,
        equipment_position,
    );
    let before = state.clone();

    assert_eq!(
        validate_upgrade_equipment(
            &registries,
            &state,
            equipment,
            EQUIPMENT_COPPER_REINFORCED_PICK,
            reinforcement,
        )
        .err(),
        Some(EquipmentUpgradeError::EquipmentAccess(
            PlayerEquipmentAccessError::RemoteKnownEquipment {
                equipment,
                equipment_position,
                player_position,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn equipment_upgrade_rejects_known_remote_material_source() {
    let registries = build_registries();
    let mut state = AppState::new();
    let equipment = assemble_stone_pick(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote upgrade source logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote upgrade source logistics commit failed: {error}"));
    let logistics_revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        logistics_revision,
        logistics_revision + 1,
        equipment,
        player_position,
    );
    let source_position = VoxelCoord::new(1, 0, 0);
    validate_place_ground_stockpile(&state, reinforcement, source_position)
        .unwrap_or_else(|error| panic!("remote upgrade source placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote upgrade source placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_upgrade_equipment(
            &registries,
            &state,
            equipment,
            EQUIPMENT_COPPER_REINFORCED_PICK,
            reinforcement,
        )
        .err(),
        Some(EquipmentUpgradeError::SourceAccess(
            PlayerStockpileAccessError::RemoteKnownStockpile {
                stockpile: reinforcement,
                stockpile_position: source_position,
                player_position,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn upgraded_equipment_load_rejects_late_unreplaceable_base_material() {
    let registries = build_registries();
    let mut state = AppState::new();
    let equipment = assemble_stone_pick(&registries, &mut state);
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("equipment chronology audit tick failed: {error}"));
    let reinforcement = reinforcement_source(&registries, &mut state);
    validate_upgrade_equipment(
        &registries,
        &state,
        equipment,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        reinforcement,
    )
    .unwrap_or_else(|error| panic!("equipment chronology upgrade validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("equipment chronology upgrade commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("equipment chronology serialization failed: {error}"));
    encoded["state"]["systems"]["equipment"]["records"][equipment.value().to_string()]["embodied_material"]
        [0]["provenance"]["latest_created_at"] = serde_json::json!(1_u64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("equipment chronology tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Equipment(
            EquipmentValidationError::EmbodiedProvenanceAfterConstruction {
                equipment,
                latest_created_at: SimulationTick::new(1),
                created_at: SimulationTick::ZERO,
            }
        )))
    );
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
fn equipment_upgrade_reports_the_exact_missing_addition() {
    let registries = build_registries();
    let mut state = AppState::new();
    let equipment = assemble_stone_pick(&registries, &mut state);
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("missing-upgrade-component stockpile failed: {error}"));

    assert_eq!(
        validate_upgrade_equipment(
            &registries,
            &state,
            equipment,
            EQUIPMENT_COPPER_REINFORCED_PICK,
            source,
        )
        .err(),
        Some(EquipmentUpgradeError::InsufficientMaterial {
            stockpile: source,
            commodity: CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            available: Mass::ZERO,
            required: Mass::from_milligrams(20_000),
        })
    );
}

#[test]
fn primitive_processing_upgrades_preserve_identity_wear_matter_and_replay() {
    let registries = build_registries();
    for (base, upgraded, expected_mass, expected_components) in [
        (
            EQUIPMENT_STONE_CRUSHER,
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            Mass::from_milligrams(2_020_000),
            4,
        ),
        (
            EQUIPMENT_STONE_SEPARATOR,
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            Mass::from_milligrams(1_220_000),
            3,
        ),
    ] {
        let mut state = AppState::new();
        let equipment = assemble_authored_equipment(&registries, &mut state, base);
        let reinforcement = reinforcement_source(&registries, &mut state);
        degrade_equipment_condition_for_test(&mut state, equipment, 123_456);
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
        assert_eq!(record.embodied_material().len(), expected_components);
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
    let mut state = AppState::new();
    let pick = assemble_stone_pick(&registries, &mut state);
    let reinforcement = reinforcement_source(&registries, &mut state);
    degrade_equipment_condition_for_test(&mut state, pick, 87_654);
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
    let mut state = AppState::new();
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
    let mut state = AppState::new();
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
    degrade_equipment_condition_for_test(&mut state, pick, 1);

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
    let mut state = AppState::new();
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
