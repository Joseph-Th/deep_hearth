//! Contract tests for equipment maintenance execution.

use super::super::maintenance_resolution::EquipmentMaintenanceMaterialResolution;
use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
    CapabilityRequirement, CapabilityValue, CapabilityValueKind,
};
use crate::content::{
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_PICK, FORM_CHIP, FORM_HANDLE, FORM_LOG,
    FORM_REINFORCEMENT, FORM_SCRAP, FORM_TOOL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_REKNAP_STONE_SCRAP_TOOL, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
    make_test_registries_with_equipment, make_test_registries_with_sensible_heating,
};
use crate::core::quantity::{
    AggregateMass, Area, Energy, Force, Length, Power, Temperature, Volume,
};
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::{SimulationTick, TickSpan};
use crate::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use crate::energy::{
    EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId, PreciseEnergy,
    add_energy_store_with_initial_for_fixture, calculate_explicit_energy_accounting,
};
use crate::equipment::{
    EquipmentDefinition, EquipmentDefinitionId, EquipmentMaintenanceProfile, add_equipment,
    degrade_equipment_condition_for_test, validate_assemble_equipment,
    validate_disassemble_equipment, validate_upgrade_equipment,
};

use crate::inventory::{
    MaterialLotSelection, StockpileId, add_solid_stockpile_for_test, deposit_composed_lot_for_test,
    deposit_lot_for_test, validate_explicit_consumption_selection, validate_mount_stockpile,
};
use crate::labor::PlayerWorkValidationError;
use crate::logistics::{
    PlayerEquipmentAccessError, PlayerStockpileAccessError, validate_initialize_player_logistics,
    validate_place_ground_stockpile,
};
use crate::maintenance::MaintenanceThresholds;
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{
    ProcessDefinition, ProcessId, StartProcessCommitError, validate_start_process,
};
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralLoadKind, add_structural_element, calculate_aggregate_weight_force_ceiling,
    materialize_structural_element_for_test, validate_activate_structural_element,
};
use crate::survival::{SurvivalExertion, Vitality, initialize_player_survival, player_record};
use crate::thermal::{
    SensibleHeatingProcessDefinition, SensibleHeatingRequest, resolve_sensible_heating_process,
};

const TEST_CAPABILITY: CapabilityId = CapabilityId::new(812_001);
const TEST_DEFINITION: EquipmentDefinitionId = EquipmentDefinitionId::new(812_001);
const HEATING_POWER: CapabilityId = CapabilityId::new(812_002);
const MAX_TEMPERATURE: CapabilityId = CapabilityId::new(812_003);
const MAX_BATCH_MASS: CapabilityId = CapabilityId::new(812_004);
const ENERGY_DEFINITION: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(812_001);
const HEATING_PROCESS: ProcessId = ProcessId::new(812_001);

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("maintenance condition fixture failed: {error}"),
    }
}

#[test]
fn maintenance_rejects_known_remote_equipment() {
    let registries = registries();
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
        .unwrap_or_else(|error| panic!("remote maintenance equipment failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("remote maintenance source failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("remote maintenance spent failed: {error}"));
    add_material(&registries, &mut state, source, Mass::from_milligrams(7));
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote maintenance logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote maintenance logistics commit failed: {error}"));
    let equipment_position = VoxelCoord::new(1, 0, 0);
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        revision,
        revision + 1,
        equipment,
        equipment_position,
    );
    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("remote maintenance resolution failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_equipment_maintenance(&registries, &state, resolution),
        Err(EquipmentMaintenanceError::EquipmentAccess(
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
fn trusted_load_rejects_active_maintenance_with_remote_equipment() {
    let registries = registries_with_service_duration(TickSpan::new(4));
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
        .unwrap_or_else(|error| panic!("remote-load maintenance equipment failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("remote-load maintenance source failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("remote-load maintenance spent failed: {error}"));
    add_material(&registries, &mut state, source, Mass::from_milligrams(7));
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote-load maintenance logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote-load maintenance logistics commit failed: {error}"));
    let logistics_revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        logistics_revision,
        logistics_revision + 1,
        equipment,
        player_position,
    );
    validate_place_ground_stockpile(&state, source, player_position)
        .unwrap_or_else(|error| panic!("remote-load maintenance source placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("remote-load maintenance source placement commit failed: {error}")
        });
    validate_place_ground_stockpile(&state, spent, player_position)
        .unwrap_or_else(|error| panic!("remote-load maintenance spent placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("remote-load maintenance spent placement commit failed: {error}")
        });
    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("remote-load maintenance resolution failed: {error}"));
    let _ = validate_equipment_maintenance(&registries, &state, resolution)
        .unwrap_or_else(|error| panic!("remote-load maintenance validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote-load maintenance commit failed: {error}"));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let equipment_position = VoxelCoord::new(1, 0, 0);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("remote-load maintenance serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["equipment_locations"]
        [equipment.value().to_string()] = serde_json::json!({"x": 1, "y": 0, "z": 0});
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("remote-load maintenance decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::EquipmentMaintenanceAccess(
                PlayerEquipmentAccessError::RemoteKnownEquipment {
                    equipment,
                    equipment_position,
                    player_position,
                }
            )
        )))
    );
}

#[test]
fn maintenance_rejects_known_remote_replacement_source() {
    let registries = registries();
    let mut state = AppState::new();
    initialize_service_player(&registries, &mut state);
    let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
        .unwrap_or_else(|error| panic!("remote-source maintenance equipment failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("remote-source maintenance source failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("remote-source maintenance spent failed: {error}"));
    add_material(&registries, &mut state, source, Mass::from_milligrams(7));
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote-source maintenance logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("remote-source maintenance logistics commit failed: {error}")
        });
    let logistics_revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        logistics_revision,
        logistics_revision + 1,
        equipment,
        player_position,
    );
    validate_place_ground_stockpile(&state, spent, player_position)
        .unwrap_or_else(|error| panic!("remote-source maintenance spent placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("remote-source maintenance spent placement commit failed: {error}")
        });
    let source_position = VoxelCoord::new(1, 0, 0);
    validate_place_ground_stockpile(&state, source, source_position)
        .unwrap_or_else(|error| panic!("remote-source maintenance placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("remote-source maintenance placement commit failed: {error}")
        });
    let resolution = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(equipment, source, spent),
    )
    .unwrap_or_else(|error| panic!("remote-source maintenance resolution failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_equipment_maintenance(&registries, &state, resolution),
        Err(EquipmentMaintenanceError::MaterialSourceAccess(
            PlayerStockpileAccessError::RemoteKnownStockpile {
                stockpile: source,
                stockpile_position: source_position,
                player_position,
            }
        ))
    );
    assert_eq!(state, before);
}

fn active_test_exertion() -> SurvivalExertion {
    SurvivalExertion::new(Energy::from_nanojoules(1), Volume::ZERO)
}

fn initialize_service_player(registries: &Registries, state: &mut AppState) {
    initialize_player_survival(registries, state)
        .unwrap_or_else(|error| panic!("maintenance player-survival fixture failed: {error}"));
}

fn make_next_tick_fatal(registries: &Registries, state: &mut AppState) {
    let physiology = registries.survival().physiology();
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("fatal maintenance fixture player disappeared"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            Energy::ZERO,
            player.hydration(),
            Vitality::from_parts_per_million_unchecked(
                physiology.starvation_vitality_loss_ppm_per_tick(),
            ),
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
}

fn finish_service(
    registries: &Registries,
    state: &mut AppState,
    completes_at: SimulationTick,
) -> EquipmentMaintenanceOutcome {
    let mut completion = None;
    while state.tick() < completes_at {
        let tick = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("maintenance completion tick failed: {error}"));
        if let Some(outcome) = tick.equipment_maintenance() {
            assert!(completion.replace(outcome).is_none());
        }
    }
    assert_eq!(state.player_work().active(), None);
    completion.unwrap_or_else(|| panic!("maintenance reached completion tick without an outcome"))
}

#[path = "maintenance_execution_tests/component_contracts.rs"]
mod component_contracts;

fn registries() -> Registries {
    registries_with_service_duration(TickSpan::new(1))
}

fn registries_with_service_duration(full_service_duration: TickSpan) -> Registries {
    let profile = match CapabilityProfile::new([(
        TEST_CAPABILITY,
        CapabilityValue::Mass(Mass::from_milligrams(50_000)),
    )]) {
        Ok(profile) => profile,
        Err(error) => panic!("maintenance capability fixture failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("equipment maintenance fixture failed: {error}"),
    };
    make_test_registries_with_equipment(
        CapabilityDefinition::new(
            TEST_CAPABILITY,
            "maintenance fixture supported mass",
            CapabilityValueKind::Mass,
        ),
        EquipmentDefinition::new(
            TEST_DEFINITION,
            "maintenance fixture press",
            Mass::from_milligrams(40_000),
            profile,
            thresholds,
        )
        .with_maintenance_profile(EquipmentMaintenanceProfile::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(7),
            CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
            condition(700_000),
            full_service_duration,
            active_test_exertion(),
        )),
    )
}

fn occupied_registries() -> Registries {
    let profile = match CapabilityProfile::new([
        (
            HEATING_POWER,
            CapabilityValue::Power(Power::from_microwatts(1_000_000)),
        ),
        (
            MAX_TEMPERATURE,
            CapabilityValue::Temperature(Temperature::from_millikelvin(400_000)),
        ),
        (
            MAX_BATCH_MASS,
            CapabilityValue::Mass(Mass::from_milligrams(20)),
        ),
    ]) {
        Ok(profile) => profile,
        Err(error) => panic!("maintenance occupancy capability fixture failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("maintenance occupancy maintenance fixture failed: {error}"),
    };
    make_test_registries_with_sensible_heating(
        vec![
            CapabilityDefinition::new(
                HEATING_POWER,
                "maintenance occupancy heating power",
                CapabilityValueKind::Power,
            ),
            CapabilityDefinition::new(
                MAX_TEMPERATURE,
                "maintenance occupancy maximum temperature",
                CapabilityValueKind::Temperature,
            ),
            CapabilityDefinition::new(
                MAX_BATCH_MASS,
                "maintenance occupancy maximum batch mass",
                CapabilityValueKind::Mass,
            ),
        ],
        EquipmentDefinition::new(
            TEST_DEFINITION,
            "maintenance occupancy heater",
            Mass::from_milligrams(40_000),
            profile,
            thresholds,
        )
        .with_maintenance_profile(EquipmentMaintenanceProfile::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(7),
            CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
            condition(700_000),
            TickSpan::new(1),
            active_test_exertion(),
        )),
        vec![EnergyStoreDefinition::new_with_transfer_limits(
            ENERGY_DEFINITION,
            "maintenance occupancy battery",
            EnergyCarrier::Electrical,
            Energy::from_nanojoules(1_000_000_000),
            Power::ZERO,
            Power::from_microwatts(500_000),
        )],
        ProcessDefinition::new(
            HEATING_PROCESS,
            "maintenance occupancy sensible heating",
            vec![
                CapabilityRequirement::new(
                    HEATING_POWER,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Power(Power::from_picowatts(1)),
                ),
                CapabilityRequirement::new(
                    MAX_TEMPERATURE,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Temperature(Temperature::from_millikelvin(1)),
                ),
                CapabilityRequirement::new(
                    MAX_BATCH_MASS,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(1)),
                ),
            ],
        ),
        SensibleHeatingProcessDefinition::new(
            HEATING_PROCESS,
            HEATING_POWER,
            MAX_TEMPERATURE,
            MAX_BATCH_MASS,
            EnergyCarrier::Electrical,
            1,
        ),
    )
}

fn explicit_energy(registries: &Registries, state: &AppState) -> PreciseEnergy {
    calculate_explicit_energy_accounting(registries, state)
        .unwrap_or_else(|error| panic!("maintenance energy accounting failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("maintenance exact energy total overflowed"))
}

fn add_material(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    mass: Mass,
) -> crate::inventory::MaterialLotId {
    match deposit_lot_for_test(
        registries,
        state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        mass,
        Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("maintenance material fixture failed: {error}"),
    }
}

fn forge_maintenance_resolution(
    state: &AppState,
    equipment: EquipmentId,
    source: StockpileId,
    lot: crate::inventory::MaterialLotId,
    mass: Mass,
    spent: StockpileId,
    after: Condition,
) -> EquipmentMaintenanceResolution {
    forge_maintenance_resolution_with_selections(
        state,
        equipment,
        source,
        &[MaterialLotSelection::new(lot, mass)],
        spent,
        after,
    )
}

fn forge_maintenance_resolution_with_selections(
    state: &AppState,
    equipment: EquipmentId,
    source: StockpileId,
    selections: &[MaterialLotSelection],
    spent_destination: StockpileId,
    condition_after: Condition,
) -> EquipmentMaintenanceResolution {
    let record = state
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("maintenance binding fixture references unknown equipment"));
    let material = validate_explicit_consumption_selection(state.inventory(), source, selections)
        .unwrap_or_else(|error| panic!("maintenance binding fixture selection failed: {error:?}"));
    EquipmentMaintenanceResolution {
        equipment,
        expected_equipment_revision: state.equipment().revision(),
        condition_before: record.condition(),
        condition_after,
        material,
        spent: CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
        spent_destination,
        material_mode: EquipmentMaintenanceMaterialResolution::AggregateWearStock,
        duration: TickSpan::new(1),
        exertion: SurvivalExertion::REST,
    }
}

fn active_support(
    registries: &Registries,
    state: &mut AppState,
    x: i64,
) -> crate::structural::StructuralElementId {
    let bounds = match VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("maintenance support bounds fixture failed: {error}"),
    };
    let element = match add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    ) {
        Ok(element) => element,
        Err(error) => panic!("maintenance support fixture failed: {error}"),
    };
    materialize_structural_element_for_test(registries, state, element, FORM_LOG);
    let activation = match validate_activate_structural_element(registries, state, element) {
        Ok(token) => token,
        Err(error) => panic!("maintenance support activation failed: {error}"),
    };
    if let Err(error) = activation.commit(state) {
        panic!("maintenance support activation commit failed: {error}");
    }
    element
}

#[path = "maintenance_execution_tests/lifecycle.rs"]
mod lifecycle;

#[path = "maintenance_execution_tests/material_contracts.rs"]
mod material_contracts;

#[path = "maintenance_execution_tests/integration.rs"]
mod integration;
