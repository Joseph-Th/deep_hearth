//! Focused tests for thermal process admission, occupancy, suspension, and replay semantics.

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
    CapabilityRequirement, CapabilityValue, CapabilityValueKind,
};
use crate::content::{
    FORM_LOG, FORM_MOLTEN, FORM_ORE, MATERIAL_COPPER, MATERIAL_WOOD,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, make_test_registries_with_sensible_heating,
};
use crate::core::quantity::{Area, Energy, Force, Mass, Power, Temperature};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::core::time::{SimulationTick, TickSpan, WorldSeed};
use crate::energy::{
    EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId, EnergyStoreId,
    EnergySupplyError, add_energy_store, add_energy_store_with_initial_for_fixture,
    calculate_explicit_energy_accounting, calculate_power_duration_ceiling, validate_energy_supply,
};
use crate::equipment::{
    CapabilityConditionCurve, CapabilityConditionPoint, EquipmentConditionCommitError,
    EquipmentConditionPlanError, EquipmentDefinition, EquipmentDefinitionId, EquipmentId,
    EquipmentProviderError, EquipmentSupportCommitError, add_equipment,
    apply_equipment_condition_plan, decide_equipment_wear, validate_mount_equipment,
    validate_unmount_equipment,
};
use crate::inventory::{
    MaterialLotSelection, StockpileId, StockpileStorageProfile, add_solid_stockpile_for_test,
    add_stockpile, deposit_lot_for_test, validate_mount_stockpile, validate_unmount_stockpile,
};
use crate::maintenance::{Condition, MaintenanceThresholds};
use crate::material::{CommodityKey, MaterialComposition};

#[cfg(feature = "test-soak")]
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{
    CompletionCommitError, ProcessDefinition, ProcessId, ProcessInputError,
    ProductionAvailabilityChange, ProductionOccupancyRelease, ProductionSuspensionReason,
    ProductionValidationError, StartProcessCommitError, StartProcessError, apply_completion_plan,
    decide_due_completions, validate_start_process,
};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementId, StructuralLifecycle, StructuralLoadKind, add_structural_element,
    materialize_structural_element_for_test, validate_activate_structural_element,
    validate_set_structural_load,
};
use crate::thermal::{PhaseSensibleHeatError, calculate_phase_sensible_heat};

const HEATING_POWER: CapabilityId = CapabilityId::new(920_001);
const MAX_TEMPERATURE: CapabilityId = CapabilityId::new(920_002);
const MAX_BATCH_MASS: CapabilityId = CapabilityId::new(920_003);
const HEATER: EquipmentDefinitionId = EquipmentDefinitionId::new(920_001);
const BATTERY: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(920_001);
const PROCESS: ProcessId = ProcessId::new(920_001);

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("thermal test condition fixture failed: {error}"),
    }
}

#[test]
fn sensible_heating_can_superheat_liquid_without_reapplying_fusion_energy() {
    let registries = make_registries_with_max_temperature(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(1_500_000),
    );
    let mut state = AppState::new(WorldSeed::new(0x9200_0101));
    let liquid_profile =
        match StockpileStorageProfile::new(false, true, Temperature::from_millikelvin(1_500_000)) {
            Ok(profile) => profile,
            Err(error) => panic!("liquid heating storage profile failed: {error}"),
        };
    let source = match add_stockpile(&mut state, Mass::from_milligrams(100), liquid_profile) {
        Ok(source) => source,
        Err(error) => panic!("liquid heating source failed: {error}"),
    };
    let destination = match add_stockpile(&mut state, Mass::from_milligrams(100), liquid_profile) {
        Ok(destination) => destination,
        Err(error) => panic!("liquid heating destination failed: {error}"),
    };
    let melting_point = Temperature::from_millikelvin(1_357_770);
    let target = Temperature::from_millikelvin(1_400_000);
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
        Mass::from_milligrams(10),
        melting_point,
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("liquid heating input failed: {error}"),
    };
    let equipment = match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
        Ok(equipment) => equipment,
        Err(error) => panic!("liquid heating equipment failed: {error}"),
    };
    let energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        BATTERY,
        Energy::from_nanojoules(1_000_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("liquid heating energy store failed: {error}"),
    };
    let initial_energy =
        match calculate_explicit_energy_accounting(&registries, &state).and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        }) {
            Ok(total) => total,
            Err(error) => panic!("liquid heating initial accounting failed: {error}"),
        };
    let expected_heat = match calculate_phase_sensible_heat(
        registries.materials(),
        Mass::from_milligrams(10),
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
        &MaterialComposition::pure(MATERIAL_COPPER),
        melting_point,
        target,
    ) {
        Ok(heat) => heat.energy(),
        Err(error) => panic!("liquid heating expected heat failed: {error}"),
    };

    let resolved = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            PROCESS,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
            equipment,
            energy_store,
            target,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("liquid sensible-heating resolution failed: {error}"),
    };
    assert_eq!(resolved.required_energy(), expected_heat);
    assert_eq!(
        resolved.process_resolution().outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN)
    );
    let duration = resolved.process_resolution().duration();
    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("liquid heating start validation failed: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("liquid heating start commit failed: {error}");
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .ok()
            .and_then(|accounting| accounting.total()),
        Some(initial_energy)
    );

    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("liquid heating completion failed: {error}");
        }
    }
    let output = match state
        .inventory()
        .lots()
        .find(|candidate| candidate.stockpile() == destination)
    {
        Some(output) => output,
        None => panic!("liquid heating output missing"),
    };
    assert_eq!(
        output.commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN)
    );
    assert_eq!(output.temperature(), target);
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .ok()
            .and_then(|accounting| accounting.total()),
        Some(initial_energy)
    );
}

fn make_registries_with_max_temperature(
    carrier: EnergyCarrier,
    maximum_temperature: Temperature,
) -> Registries {
    make_registries_with_condition_curves(carrier, maximum_temperature, Vec::new())
}

fn make_registries_with_condition_curves(
    carrier: EnergyCarrier,
    maximum_temperature: Temperature,
    curves: Vec<CapabilityConditionCurve>,
) -> Registries {
    make_registries_with_energy_output_power_condition_curves_and_support(
        carrier,
        maximum_temperature,
        Power::from_microwatts(500_000),
        curves,
        false,
    )
}

fn make_registries_with_energy_output_power(
    carrier: EnergyCarrier,
    maximum_temperature: Temperature,
    energy_output_power: Power,
) -> Registries {
    make_registries_with_energy_output_power_condition_curves_and_support(
        carrier,
        maximum_temperature,
        energy_output_power,
        Vec::new(),
        false,
    )
}

fn make_registries_with_energy_output_power_condition_curves_and_support(
    carrier: EnergyCarrier,
    maximum_temperature: Temperature,
    energy_output_power: Power,
    curves: Vec<CapabilityConditionCurve>,
    requires_structural_support: bool,
) -> Registries {
    let capabilities = match CapabilityProfile::new([
        (
            HEATING_POWER,
            CapabilityValue::Power(Power::from_microwatts(1_000_000)),
        ),
        (
            MAX_TEMPERATURE,
            CapabilityValue::Temperature(maximum_temperature),
        ),
        (
            MAX_BATCH_MASS,
            CapabilityValue::Mass(Mass::from_milligrams(20)),
        ),
    ]) {
        Ok(profile) => profile,
        Err(error) => panic!("thermal capability fixture failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("thermal maintenance fixture failed: {error}"),
    };
    let equipment = EquipmentDefinition::new_with_capability_condition_curves(
        HEATER,
        "test resistive heater",
        Mass::from_milligrams(1_000_000),
        capabilities,
        thresholds,
        curves,
    );
    let equipment = if requires_structural_support {
        equipment.with_required_structural_support()
    } else {
        equipment
    };
    let energy = EnergyStoreDefinition::new_with_transfer_limits(
        BATTERY,
        "test finite battery",
        carrier,
        Energy::from_nanojoules(1_000_000_000),
        Power::ZERO,
        energy_output_power,
    );
    let process = ProcessDefinition::new_selected_batch(
        PROCESS,
        "test sensible heating",
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
    );
    make_test_registries_with_sensible_heating(
        vec![
            CapabilityDefinition::new(
                HEATING_POWER,
                "heating transfer power",
                CapabilityValueKind::Power,
            ),
            CapabilityDefinition::new(
                MAX_TEMPERATURE,
                "maximum chamber temperature",
                CapabilityValueKind::Temperature,
            ),
            CapabilityDefinition::new(
                MAX_BATCH_MASS,
                "maximum chamber batch mass",
                CapabilityValueKind::Mass,
            ),
        ],
        equipment,
        energy,
        process,
        SensibleHeatingProcessDefinition::new(
            PROCESS,
            HEATING_POWER,
            MAX_TEMPERATURE,
            MAX_BATCH_MASS,
            EnergyCarrier::Electrical,
            1_000,
        ),
    )
}

fn make_registries_with_fixed_heater() -> Registries {
    make_registries_with_energy_output_power_condition_curves_and_support(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        Power::from_microwatts(5_000),
        Vec::new(),
        true,
    )
}

fn add_active_support(
    registries: &Registries,
    state: &mut AppState,
    x: i64,
) -> StructuralElementId {
    let bounds = match VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("heater-support bounds fixture failed: {error}"),
    };
    let support = match add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            crate::core::quantity::Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    ) {
        Ok(element) => element,
        Err(error) => panic!("heater-support structural fixture failed: {error}"),
    };
    materialize_structural_element_for_test(registries, state, support, FORM_LOG);
    let activation = match validate_activate_structural_element(registries, state, support) {
        Ok(token) => token,
        Err(error) => panic!("heater-support activation validation failed: {error}"),
    };
    if let Err(error) = activation.commit(state) {
        panic!("heater-support activation commit failed: {error}");
    }
    support
}

fn fail_support(registries: &Registries, state: &mut AppState, support: StructuralElementId) {
    let overload = match validate_set_structural_load(
        registries,
        state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    ) {
        Ok(token) => token,
        Err(error) => panic!("heater-support overload validation failed: {error}"),
    };
    if let Err(error) = overload.commit(state) {
        panic!("heater-support overload commit failed: {error}");
    }
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );
}

fn make_registries(carrier: EnergyCarrier) -> Registries {
    make_registries_with_max_temperature(carrier, Temperature::from_millikelvin(400_000))
}

fn make_loaded_fixture_at(
    carrier: EnergyCarrier,
    input_temperature: Temperature,
    initial_energy: Energy,
) -> (
    Registries,
    AppState,
    StockpileId,
    StockpileId,
    EquipmentId,
    EnergyStoreId,
) {
    let registries = make_registries(carrier);
    make_loaded_fixture_with_registries(
        registries,
        Condition::PRISTINE,
        input_temperature,
        initial_energy,
    )
}

fn make_loaded_fixture_with_registries(
    registries: Registries,
    equipment_condition: Condition,
    input_temperature: Temperature,
    initial_energy: Energy,
) -> (
    Registries,
    AppState,
    StockpileId,
    StockpileId,
    EquipmentId,
    EnergyStoreId,
) {
    let mut state = AppState::new(WorldSeed::new(0x9200_0001));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("thermal source fixture failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("thermal destination fixture failed: {error}"),
    };
    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        input_temperature,
    ) {
        panic!("thermal input fixture failed: {error}");
    }
    let equipment = match add_equipment(&registries, &mut state, HEATER, equipment_condition) {
        Ok(id) => id,
        Err(error) => panic!("thermal equipment fixture failed: {error}"),
    };
    let energy = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        BATTERY,
        initial_energy,
    ) {
        Ok(id) => id,
        Err(error) => panic!("thermal energy fixture failed: {error}"),
    };
    (registries, state, source, destination, equipment, energy)
}

fn make_loaded_fixture(
    carrier: EnergyCarrier,
) -> (
    Registries,
    AppState,
    StockpileId,
    StockpileId,
    EquipmentId,
    EnergyStoreId,
) {
    make_loaded_fixture_at(
        carrier,
        Temperature::from_millikelvin(300_000),
        Energy::from_nanojoules(500_000_000),
    )
}

fn resolve_test_sensible_heating_process(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
    target: Temperature,
) -> Result<ResolvedSensibleHeating, SensibleHeatingResolutionError> {
    let lot = match state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == source && lot.mass() >= Mass::from_milligrams(10))
    {
        Some(lot) => lot.id(),
        None => panic!("thermal test source has no selectable 10 mg lot"),
    };
    resolve_sensible_heating_process(
        registries,
        state,
        SensibleHeatingRequest::new(
            process,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
            equipment,
            energy_store,
            target,
        ),
    )
}

#[test]
fn sensible_heating_consumes_exact_energy_and_completes_with_target_temperature() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let initial_explicit_energy = match calculate_explicit_energy_accounting(&registries, &state)
        .and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        }) {
        Ok(total) => total,
        Err(error) => panic!("initial explicit energy accounting failed: {error}"),
    };
    let target = Temperature::from_millikelvin(303_000);
    let expected_heat = match calculate_phase_sensible_heat(
        registries.materials(),
        Mass::from_milligrams(10),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        &MaterialComposition::pure(MATERIAL_WOOD),
        Temperature::from_millikelvin(300_000),
        target,
    ) {
        Ok(heat) => heat.energy(),
        Err(error) => panic!("expected heat fixture failed: {error}"),
    };
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("sensible heating resolution failed: {error}"),
    };
    assert_eq!(resolved.required_energy(), expected_heat);
    assert_eq!(resolved.transfer_power(), Power::from_microwatts(500_000));
    let expected_duration = match calculate_power_duration_ceiling(
        resolved.transfer_power(),
        expected_heat,
        registries.core().physical_tick_duration(),
    ) {
        Ok(duration) => duration,
        Err(error) => panic!("thermal duration fixture failed: {error}"),
    };
    assert_eq!(resolved.process_resolution().duration(), expected_duration);
    assert_eq!(
        resolved.process_resolution().equipment_condition_after(),
        Some(condition(999_000))
    );

    let before_energy = state
        .energy()
        .get_store(energy_store)
        .map(|store| store.stored());
    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("heated process start validation failed: {error}"),
    };
    let job = match token.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("heated process start commit failed: {error}"),
    };
    assert_eq!(
        state
            .energy()
            .get_store(energy_store)
            .map(|store| store.stored()),
        before_energy.and_then(|energy| energy.checked_sub(expected_heat))
    );
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.consumed_energy()),
        resolved.process_resolution().energy_input()
    );
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_condition_after()),
        Some(condition(999_000))
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(Condition::PRISTINE)
    );
    let in_flight_explicit_energy = match calculate_explicit_energy_accounting(&registries, &state)
        .and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        }) {
        Ok(total) => total,
        Err(error) => panic!("in-flight explicit energy accounting failed: {error}"),
    };
    assert_eq!(in_flight_explicit_energy, initial_explicit_energy);

    for _ in 0..expected_duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("heated process completion tick failed: {error}");
        }
    }
    assert!(state.production().get_job(job).is_none());
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(999_000))
    );
    let output = match state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == destination)
    {
        Some(output) => output,
        None => panic!("heated output lot missing after completion"),
    };
    assert_eq!(output.mass(), Mass::from_milligrams(10));
    assert_eq!(output.temperature(), target);
    assert_eq!(
        output.composition(),
        &MaterialComposition::pure(MATERIAL_WOOD)
    );
    let final_explicit_energy = match calculate_explicit_energy_accounting(&registries, &state)
        .and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        }) {
        Ok(total) => total,
        Err(error) => panic!("final explicit energy accounting failed: {error}"),
    };
    assert_eq!(final_explicit_energy, initial_explicit_energy);
}

#[test]
fn worn_heater_derates_transfer_power_and_persisted_duration_contract() {
    let curve = CapabilityConditionCurve::new(
        HEATING_POWER,
        vec![
            CapabilityConditionPoint::new(
                Condition::FAILED,
                CapabilityValue::Power(Power::from_microwatts(1_000)),
            ),
            CapabilityConditionPoint::new(
                condition(500_000),
                CapabilityValue::Power(Power::from_microwatts(3_000)),
            ),
        ],
    );
    let registries = make_registries_with_condition_curves(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        vec![curve],
    );
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture_with_registries(
            registries,
            condition(500_000),
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        );

    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("worn-heater resolution failed: {error}"),
    };
    assert_eq!(
        resolved.required_energy(),
        Energy::from_nanojoules(51_000_000)
    );
    assert_eq!(resolved.transfer_power(), Power::from_microwatts(3_000));
    assert_eq!(resolved.process_resolution().duration().value(), 5);

    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("worn-heater process start validation failed: {error}"),
    };
    let job = match token.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("worn-heater process start commit failed: {error}"),
    };
    let provider = match state
        .production()
        .get_job(job)
        .and_then(|record| record.equipment_provider())
    {
        Some(provider) => provider,
        None => panic!("worn-heater job lost its equipment trace"),
    };
    assert_eq!(provider.condition(), condition(500_000));
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_condition_after()),
        Some(condition(495_000))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    for _ in 0..5 {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("worn-heater completion failed: {error}");
        }
    }
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(495_000))
    );
}

#[test]
fn sensible_heating_rejects_wrong_energy_carrier_before_mutation() {
    let (registries, state, source, _, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Thermal);
    let before = state.clone();

    assert_eq!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(303_000),
        ),
        Err(SensibleHeatingResolutionError::WrongEnergyCarrier {
            required: EnergyCarrier::Electrical,
            provided: EnergyCarrier::Thermal,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn sensible_heating_rejects_noop_target_before_consuming_resources() {
    let (registries, state, source, _, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let before = state.clone();

    assert_eq!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(300_000),
        ),
        Err(SensibleHeatingResolutionError::NoHeatingRequired)
    );
    assert_eq!(state, before);
}

#[test]
fn sensible_heating_rejects_target_above_equipment_limit() {
    let (registries, state, source, _, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);

    assert_eq!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(401_000),
        ),
        Err(
            SensibleHeatingResolutionError::TargetExceedsEquipmentMaximum {
                target: Temperature::from_millikelvin(401_000),
                maximum: Temperature::from_millikelvin(400_000),
            }
        )
    );
}

#[test]
fn warmer_input_reduces_required_energy_and_duration() {
    let cold_registries = make_registries_with_energy_output_power(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        Power::from_microwatts(5_000),
    );
    let (cold_registries, cold_state, cold_source, _, cold_equipment, cold_energy) =
        make_loaded_fixture_with_registries(
            cold_registries,
            Condition::PRISTINE,
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        );
    let warm_registries = make_registries_with_energy_output_power(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        Power::from_microwatts(5_000),
    );
    let (warm_registries, warm_state, warm_source, _, warm_equipment, warm_energy) =
        make_loaded_fixture_with_registries(
            warm_registries,
            Condition::PRISTINE,
            Temperature::from_millikelvin(302_000),
            Energy::from_nanojoules(500_000_000),
        );
    let target = Temperature::from_millikelvin(303_000);
    let cold = match resolve_test_sensible_heating_process(
        &cold_registries,
        &cold_state,
        PROCESS,
        cold_source,
        cold_equipment,
        cold_energy,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("cold heating resolution failed: {error}"),
    };
    let warm = match resolve_test_sensible_heating_process(
        &warm_registries,
        &warm_state,
        PROCESS,
        warm_source,
        warm_equipment,
        warm_energy,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("warm heating resolution failed: {error}"),
    };

    assert_eq!(cold.required_energy(), Energy::from_nanojoules(51_000_000));
    assert_eq!(warm.required_energy(), Energy::from_nanojoules(17_000_000));
    assert!(cold.required_energy() > warm.required_energy());
    assert_eq!(cold.process_resolution().duration().value(), 3);
    assert_eq!(warm.process_resolution().duration().value(), 1);
}

#[test]
fn selected_batch_mass_changes_heating_energy_without_static_recipe_quantity() {
    let registries = make_registries_with_energy_output_power(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        Power::from_microwatts(5_000),
    );
    let (registries, state, source, _, equipment, energy_store) =
        make_loaded_fixture_with_registries(
            registries,
            Condition::PRISTINE,
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        );
    let lot = match state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == source)
    {
        Some(lot) => lot.id(),
        None => panic!("selected-batch fixture lot missing"),
    };
    let target = Temperature::from_millikelvin(303_000);
    let five = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            PROCESS,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(5))],
            equipment,
            energy_store,
            target,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("5 mg selected-batch heating failed: {error}"),
    };
    let ten = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            PROCESS,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
            equipment,
            energy_store,
            target,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("10 mg selected-batch heating failed: {error}"),
    };

    assert_eq!(
        five.process_resolution().input_mass(),
        Mass::from_milligrams(5)
    );
    assert_eq!(
        ten.process_resolution().input_mass(),
        Mass::from_milligrams(10)
    );
    assert_eq!(five.required_energy(), Energy::from_nanojoules(25_500_000));
    assert_eq!(ten.required_energy(), Energy::from_nanojoules(51_000_000));
    assert_eq!(five.process_resolution().duration().value(), 2);
    assert_eq!(ten.process_resolution().duration().value(), 3);
}

#[test]
fn selected_batch_heating_rejects_mass_above_equipment_capacity_without_mutation() {
    let (registries, mut state, _, _, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(source) => source,
        Err(error) => panic!("batch-capacity source allocation failed: {error}"),
    };
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(21),
        Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("batch-capacity material fixture failed: {error}"),
    };
    let before = state.clone();

    assert_eq!(
        resolve_sensible_heating_process(
            &registries,
            &state,
            SensibleHeatingRequest::new(
                PROCESS,
                source,
                &[MaterialLotSelection::new(lot, Mass::from_milligrams(21))],
                equipment,
                energy_store,
                Temperature::from_millikelvin(303_000),
            ),
        ),
        Err(
            SensibleHeatingResolutionError::BatchMassExceedsEquipmentCapacity {
                selected: Mass::from_milligrams(21),
                maximum: Mass::from_milligrams(20),
            }
        )
    );
    assert_eq!(state, before);
}

#[test]
fn selected_batch_heating_uses_actual_material_heat_capacity() {
    let registries = make_registries_with_energy_output_power(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        Power::from_microwatts(5_000),
    );
    let (registries, mut state, wood_source, _, equipment, energy_store) =
        make_loaded_fixture_with_registries(
            registries,
            Condition::PRISTINE,
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        );
    let copper_source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(source) => source,
        Err(error) => panic!("copper heating source allocation failed: {error}"),
    };
    let copper_lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        copper_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("copper heating input failed: {error}"),
    };
    let wood_lot = match state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == wood_source)
    {
        Some(lot) => lot.id(),
        None => panic!("wood heating input disappeared"),
    };
    let target = Temperature::from_millikelvin(303_000);
    let wood = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            PROCESS,
            wood_source,
            &[MaterialLotSelection::new(
                wood_lot,
                Mass::from_milligrams(10),
            )],
            equipment,
            energy_store,
            target,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("wood property heating resolution failed: {error}"),
    };
    let copper = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            PROCESS,
            copper_source,
            &[MaterialLotSelection::new(
                copper_lot,
                Mass::from_milligrams(10),
            )],
            equipment,
            energy_store,
            target,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("copper property heating resolution failed: {error}"),
    };

    assert_eq!(wood.required_energy(), Energy::from_nanojoules(51_000_000));
    assert_eq!(
        copper.required_energy(),
        Energy::from_nanojoules(11_550_000)
    );
    assert_eq!(wood.process_resolution().duration().value(), 3);
    assert_eq!(copper.process_resolution().duration().value(), 1);
}

#[test]
fn sensible_heating_stops_at_material_phase_boundary() {
    let registries = make_registries_with_max_temperature(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(2_000_000),
    );
    let mut state = AppState::new(WorldSeed::new(0x9200_0020));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(source) => source,
        Err(error) => panic!("phase-boundary source allocation failed: {error}"),
    };
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("phase-boundary copper input failed: {error}"),
    };
    let equipment = match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
        Ok(equipment) => equipment,
        Err(error) => panic!("phase-boundary heater allocation failed: {error}"),
    };
    let energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        BATTERY,
        Energy::from_nanojoules(500_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("phase-boundary energy fixture failed: {error}"),
    };
    let before = state.clone();

    assert!(matches!(
        resolve_sensible_heating_process(
            &registries,
            &state,
            SensibleHeatingRequest::new(
                PROCESS,
                source,
                &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
                equipment,
                energy_store,
                Temperature::from_millikelvin(1_400_000),
            ),
        ),
        Err(SensibleHeatingResolutionError::Heat(
            PhaseSensibleHeatError::InvalidTargetState(
                crate::material::MaterialPhaseStateError::SolidAboveMeltingPoint {
                    material: _material,
                    temperature: _temperature,
                    melting_point: _melting_point,
                }
            )
        ))
    ));
    assert_eq!(state, before);
}

#[test]
fn selected_batch_heating_rejects_empty_selection_without_mutation() {
    let (registries, state, source, _, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let before = state.clone();

    assert_eq!(
        resolve_sensible_heating_process(
            &registries,
            &state,
            SensibleHeatingRequest::new(
                PROCESS,
                source,
                &[],
                equipment,
                energy_store,
                Temperature::from_millikelvin(303_000),
            ),
        ),
        Err(SensibleHeatingResolutionError::Input(
            ProcessInputError::EmptySelection
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn sensible_heating_rejects_insufficient_finite_energy_without_mutation() {
    let (registries, state, source, _, equipment, energy_store) = make_loaded_fixture_at(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(300_000),
        Energy::from_nanojoules(50_000_000),
    );
    let before = state.clone();

    assert_eq!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(303_000),
        ),
        Err(SensibleHeatingResolutionError::Energy(
            EnergySupplyError::InsufficientEnergy {
                store: energy_store,
                available: Energy::from_nanojoules(50_000_000),
                requested: Energy::from_nanojoules(51_000_000),
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn resolved_heating_energy_becomes_stale_after_independent_energy_mutation() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("stale heating fixture resolution failed: {error}"),
    };
    let expected_revision = state.energy().revision();
    if let Err(error) = add_energy_store(&registries, &mut state, BATTERY) {
        panic!("independent energy mutation failed: {error}");
    }
    let before = state.clone();

    assert_eq!(
        validate_start_process(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            destination,
        ),
        Err(crate::production::StartProcessError::StaleResolvedEnergy {
            expected_energy_revision: expected_revision,
            actual_energy_revision: expected_revision + 1,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn validated_heating_start_rejects_stale_energy_before_consuming_matter() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("atomic heating fixture resolution failed: {error}"),
    };
    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("atomic heating start validation failed: {error}"),
    };
    let expected_revision = state.energy().revision();
    if let Err(error) = add_energy_store(&registries, &mut state, BATTERY) {
        panic!("independent energy mutation failed: {error}");
    }
    let before_commit = state.clone();

    assert_eq!(
        token.commit(&mut state),
        Err(
            crate::production::StartProcessCommitError::StaleEnergyRevision {
                expected: expected_revision,
                actual: expected_revision + 1,
            }
        )
    );
    assert_eq!(state, before_commit);
    assert_eq!(state.production().jobs().count(), 0);
}

#[cfg(feature = "test-soak")]
fn run_sensible_heating_soak(seed: WorldSeed) -> AppState {
    let registries = make_registries(EnergyCarrier::Electrical);
    let mut state = AppState::new(seed);
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(200)) {
        Ok(id) => id,
        Err(error) => panic!("heating soak source allocation failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(200)) {
        Ok(id) => id,
        Err(error) => panic!("heating soak destination allocation failed: {error}"),
    };
    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(150),
        Temperature::from_millikelvin(300_000),
    ) {
        panic!("heating soak input deposit failed: {error}");
    }
    let equipment = match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
        Ok(id) => id,
        Err(error) => panic!("heating soak equipment allocation failed: {error}"),
    };
    let energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        BATTERY,
        Energy::from_nanojoules(800_000_000),
    ) {
        Ok(id) => id,
        Err(error) => panic!("heating soak energy allocation failed: {error}"),
    };
    let initial_matter = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("heating soak initial matter accounting failed: {error}"),
    };
    let initial_explicit_energy = match calculate_explicit_energy_accounting(&registries, &state)
        .and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        }) {
        Ok(total) => total,
        Err(error) => panic!("heating soak initial energy accounting failed: {error}"),
    };
    let wood = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let target = Temperature::from_millikelvin(303_000);

    for step in 0_u64..5_000 {
        let available = match state.inventory().get_stockpile(source) {
            Some(stockpile) => stockpile.get_mass(wood),
            None => panic!("heating soak source disappeared"),
        };
        if step.is_multiple_of(13) && available >= Mass::from_milligrams(10) {
            let resolved = match resolve_test_sensible_heating_process(
                &registries,
                &state,
                PROCESS,
                source,
                equipment,
                energy_store,
                target,
            ) {
                Ok(resolved) => resolved,
                Err(error) => panic!("heating soak resolution failed at step {step}: {error}"),
            };
            let token = match validate_start_process(
                &registries,
                &state,
                resolved.process_resolution(),
                source,
                destination,
            ) {
                Ok(token) => token,
                Err(error) => {
                    panic!("heating soak start validation failed at step {step}: {error}")
                }
            };
            if let Err(error) = token.commit(&mut state) {
                panic!("heating soak start commit failed at step {step}: {error}");
            }
        }
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("heating soak tick {step} failed: {error}");
        }
        if step.is_multiple_of(97) {
            if let Err(error) = validate_loaded_state(&registries, &state) {
                panic!("heating soak exhaustive audit failed at step {step}: {error}");
            }
            let matter = match calculate_matter_accounting(&state) {
                Ok(accounting) => accounting.total(),
                Err(error) => {
                    panic!("heating soak matter accounting failed at step {step}: {error}")
                }
            };
            assert_eq!(matter, initial_matter);
            let explicit_energy = match calculate_explicit_energy_accounting(&registries, &state)
                .and_then(|accounting| {
                    accounting
                        .total()
                        .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
                }) {
                Ok(total) => total,
                Err(error) => {
                    panic!("heating soak explicit energy accounting failed at step {step}: {error}")
                }
            };
            assert_eq!(explicit_energy, initial_explicit_energy);
        }
    }

    assert_eq!(state.production().jobs().count(), 0);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.get_mass(wood)),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.get_mass(wood)),
        Some(Mass::from_milligrams(150))
    );
    assert_eq!(
        state
            .energy()
            .get_store(energy_store)
            .map(|store| store.stored()),
        Some(Energy::from_nanojoules(35_000_000))
    );
    assert!(
        state
            .inventory()
            .lots()
            .filter(|lot| lot.stockpile() == destination)
            .all(|lot| lot.temperature() == target
                && lot.composition() == &MaterialComposition::pure(MATERIAL_WOOD))
    );
    let final_matter = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("heating soak final matter accounting failed: {error}"),
    };
    assert_eq!(final_matter, initial_matter);
    let final_explicit_energy = match calculate_explicit_energy_accounting(&registries, &state)
        .and_then(|accounting| {
            accounting
                .total()
                .ok_or(crate::energy::ExplicitEnergyAccountingError::Overflow)
        }) {
        Ok(total) => total,
        Err(error) => panic!("heating soak final energy accounting failed: {error}"),
    };
    assert_eq!(final_explicit_energy, initial_explicit_energy);
    state
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn sensible_heating_soak_preserves_determinism_matter_and_finite_energy() {
    let seed = WorldSeed::new(0x9200_5000);
    let first = run_sensible_heating_soak(seed);
    let second = run_sensible_heating_soak(seed);

    assert_eq!(first, second);
    assert_eq!(first.tick().value(), 5_000);
    assert_eq!(
        first
            .equipment()
            .get_equipment(EquipmentId::new(1))
            .map(|record| record.condition()),
        Some(condition(985_000))
    );
}

#[test]
fn same_tick_heating_completions_apply_all_wear_under_one_equipment_revision() {
    let (registries, mut state, source, destination, first_equipment, first_energy) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    ) {
        panic!("same-tick wear second input fixture failed: {error}");
    }
    let second_equipment = match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE)
    {
        Ok(equipment) => equipment,
        Err(error) => panic!("same-tick wear second equipment fixture failed: {error}"),
    };
    let second_energy = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        BATTERY,
        Energy::from_nanojoules(500_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("same-tick wear second energy fixture failed: {error}"),
    };
    let target = Temperature::from_millikelvin(303_000);

    let first = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        first_equipment,
        first_energy,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("same-tick wear first resolution failed: {error}"),
    };
    let duration = first.process_resolution().duration();
    let first_start = match validate_start_process(
        &registries,
        &state,
        first.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("same-tick wear first start validation failed: {error}"),
    };
    if let Err(error) = first_start.commit(&mut state) {
        panic!("same-tick wear first start commit failed: {error}");
    }

    let second = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        second_equipment,
        second_energy,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("same-tick wear second resolution failed: {error}"),
    };
    assert_eq!(second.process_resolution().duration(), duration);
    let second_start = match validate_start_process(
        &registries,
        &state,
        second.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("same-tick wear second start validation failed: {error}"),
    };
    if let Err(error) = second_start.commit(&mut state) {
        panic!("same-tick wear second start commit failed: {error}");
    }

    let equipment_revision_before_completion = state.equipment().revision();
    for _ in 1..duration.value() {
        let outcome = match advance_tick(&registries, &mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("same-tick wear pre-completion tick failed: {error}"),
        };
        assert!(outcome.production_completions().is_empty());
        assert_eq!(
            state.equipment().revision(),
            equipment_revision_before_completion
        );
    }

    let completion = match advance_tick(&registries, &mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("same-tick wear completion tick failed: {error}"),
    };
    assert_eq!(completion.production_completions().len(), 2);
    assert_eq!(
        state.equipment().revision(),
        equipment_revision_before_completion + 1
    );
    for equipment in [first_equipment, second_equipment] {
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.condition()),
            Some(condition(999_000))
        );
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn sensible_heating_rejects_heater_after_mounted_support_fails() {
    let (registries, mut state, source, _, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let support = add_active_support(&registries, &mut state, 0);
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(token) => token,
        Err(error) => panic!("heater-support mount validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("heater-support mount commit failed: {error}");
    }
    fail_support(&registries, &mut state, support);

    assert!(matches!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            equipment,
            energy_store,
            Temperature::from_millikelvin(303_000),
        ),
        Err(SensibleHeatingResolutionError::Equipment(
            EquipmentProviderError::StructuralSupportNotActive {
                equipment: rejected_equipment,
                element,
                lifecycle: StructuralLifecycle::Failed,
            }
        )) if rejected_equipment == equipment && element == support
    ));
}

#[test]
fn trusted_load_rejects_fixed_equipment_job_with_erased_support_requirement() {
    let registries = make_registries_with_fixed_heater();
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture_with_registries(
            registries,
            Condition::PRISTINE,
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        );
    let support = add_active_support(&registries, &mut state, 0);
    let _ = validate_mount_equipment(&registries, &state, equipment, support)
        .unwrap_or_else(|error| panic!("fixed heating fixture mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("fixed heating fixture mount commit failed: {error}"));
    let resolved = resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    )
    .unwrap_or_else(|error| panic!("fixed heating fixture resolution failed: {error}"));
    let job = validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("fixed heating fixture start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fixed heating fixture start commit failed: {error}"));
    assert!(
        state
            .production()
            .get_job(job)
            .is_some_and(|record| record.has_required_active_support())
    );

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("fixed heating support tamper serialization failed: {error}")
        });
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["equipment"]["requires_active_support"] =
        serde_json::json!(false);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("fixed heating support tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobEquipmentSupportRequirementMissing {
                job,
                equipment,
                definition: HEATER,
            }
        ))
    );
}

#[test]
fn trusted_load_rejects_running_job_whose_support_assignment_was_erased() {
    let registries = make_registries_with_energy_output_power(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        Power::from_microwatts(5_000),
    );
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture_with_registries(
            registries,
            Condition::PRISTINE,
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        );
    let support = add_active_support(&registries, &mut state, 0);
    let _ = validate_mount_equipment(&registries, &state, equipment, support)
        .unwrap_or_else(|error| panic!("support-state fixture mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("support-state fixture mount commit failed: {error}"));
    let resolved = resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    )
    .unwrap_or_else(|error| panic!("support-state fixture resolution failed: {error}"));
    let job = validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("support-state fixture start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("support-state fixture start commit failed: {error}"));
    assert!(
        state
            .production()
            .get_job(job)
            .is_some_and(|record| record.has_required_active_support() && !record.is_suspended())
    );

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("support-state tamper serialization failed: {error}"));
    encoded["state"]["systems"]["equipment"]["records"][equipment.value().to_string()]["supported_by"] =
        serde_json::Value::Null;
    let loads = encoded["state"]["systems"]["structures"]["elements"][support.value().to_string()]
        ["loads"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("support-state structural loads were not an object"));
    assert!(loads.remove("Equipment").is_some());
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("support-state tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobEquipmentSupportStateMismatch {
                job,
                equipment,
                requires_active_support: true,
                supported_by: None,
            }
        ))
    );
}

#[test]
fn supported_heating_suspends_on_collapse_and_resumes_after_relocation() {
    let registries = make_registries_with_energy_output_power(
        EnergyCarrier::Electrical,
        Temperature::from_millikelvin(400_000),
        Power::from_microwatts(5_000),
    );
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture_with_registries(
            registries,
            Condition::PRISTINE,
            Temperature::from_millikelvin(300_000),
            Energy::from_nanojoules(500_000_000),
        );
    let failed_support = add_active_support(&registries, &mut state, 0);
    let recovery_support = add_active_support(&registries, &mut state, 2);
    let _ = validate_mount_equipment(&registries, &state, equipment, failed_support)
        .unwrap_or_else(|error| panic!("suspension fixture mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspension fixture mount commit failed: {error}"));
    let _ = validate_mount_stockpile(&registries, &state, destination, failed_support)
        .unwrap_or_else(|error| panic!("suspension destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspension destination mount commit failed: {error}"));

    let resolved = resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    )
    .unwrap_or_else(|error| panic!("suspension fixture resolution failed: {error}"));
    let active_duration = resolved.process_resolution().duration();
    assert!(active_duration.value() > 2);
    let start = validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("suspension fixture start failed: {error}"));
    let job = start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspension fixture start commit failed: {error}"));
    let original_due = state
        .production()
        .get_job(job)
        .map(|record| record.completes_at())
        .unwrap_or_else(|| panic!("suspension fixture job disappeared"));
    let reserved_output_mass = state
        .inventory()
        .get_stockpile(destination)
        .map(|stockpile| stockpile.reserved_inbound())
        .unwrap_or_else(|| panic!("suspension fixture destination disappeared"));
    assert!(!reserved_output_mass.is_zero());

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("suspension fixture first active tick failed: {error}"));
    let suspended_at = state.tick();
    fail_support(&registries, &mut state, failed_support);
    let expected_remaining = TickSpan::new(original_due.value() - suspended_at.value());
    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("suspension transition tick failed: {error}"));
    assert_eq!(
        outcome.production_availability_changes(),
        &[ProductionAvailabilityChange::Suspended {
            job,
            reason: ProductionSuspensionReason::EquipmentSupportUnavailable { equipment },
            suspended_at,
            remaining_active_time: expected_remaining,
        }]
    );
    let suspension = state
        .production()
        .get_job(job)
        .and_then(|record| record.suspension())
        .unwrap_or_else(|| panic!("collapsed supported job did not suspend"));
    assert_eq!(suspension.remaining_active_time(), expected_remaining);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(reserved_output_mass),
        "suspension must retain the job's output capacity reservation"
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(Condition::PRISTINE)
    );
    assert_eq!(
        decide_equipment_wear(&state, equipment, 1),
        Err(EquipmentConditionPlanError::EquipmentBusy {
            equipment,
            job,
            release: ProductionOccupancyRelease::AwaitingResume,
        })
    );
    assert_eq!(
        validate_energy_supply(
            &registries,
            &state,
            energy_store,
            Energy::from_nanojoules(1),
        ),
        Err(EnergySupplyError::StoreBusy {
            store: energy_store,
            job,
            release: ProductionOccupancyRelease::AwaitingResume,
        })
    );
    let _source_mount = validate_mount_stockpile(&registries, &state, source, recovery_support)
        .unwrap_or_else(|error| {
            panic!("released production source remained spuriously relocation-locked: {error}")
        });
    let _destination_unmount = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| {
            panic!(
                "suspended production destination remained spuriously relocation-locked: {error}"
            )
        });
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("suspended heating save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("suspended heating save decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("suspended heating save validation failed: {error}"));
    assert_eq!(loaded, state);
    assert_eq!(
        loaded
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(reserved_output_mass),
        "save/reload must preserve suspended output reservation ownership"
    );

    let mut tampered =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("powered player-labor tamper serialization failed: {error}")
        });
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["suspension"]
        ["reason"] = serde_json::json!("PlayerLaborUnavailable");
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("powered player-labor tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::NonManualJobSuspendedForPlayerLabor {
                job,
                process: PROCESS,
            }
        ))
    );

    let tampered_due = SimulationTick::new(original_due.value() + 1);
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("suspended schedule tamper serialization failed: {error}"));
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["completes_at"] =
        serde_json::json!(tampered_due.value());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("suspended schedule tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::SuspensionScheduleMismatch {
                job,
                expected_due: original_due,
                actual_due: tampered_due,
            }
        )))
    );

    let mut tampered =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("suspended remaining-time tamper serialization failed: {error}")
        });
    let excessive_remaining = TickSpan::new(active_duration.value() + 1);
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["suspension"]
        ["remaining_active_time"] = serde_json::json!(excessive_remaining.value());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("suspended remaining-time tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::SuspensionRemainingExceedsActiveDuration {
                job,
                remaining: excessive_remaining,
                active_duration,
            }
        )))
    );

    let future_suspended_at = SimulationTick::new(state.tick().value() + 1);
    let future_due = future_suspended_at
        .checked_add_span(expected_remaining)
        .unwrap_or_else(|| panic!("future-suspension tamper due tick overflowed"));
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("future-suspension tamper serialization failed: {error}"));
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["suspension"]
        ["suspended_at"] = serde_json::json!(future_suspended_at.value());
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["completes_at"] =
        serde_json::json!(future_due.value());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("future-suspension tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobSuspendedInFuture {
                job,
                current: state.tick(),
                suspended_at: future_suspended_at,
            }
        ))
    );
    state = loaded;

    let _ = validate_unmount_equipment(&registries, &state, equipment)
        .unwrap_or_else(|error| panic!("suspended equipment unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspended equipment unmount commit failed: {error}"));
    let _ = validate_mount_equipment(&registries, &state, equipment, recovery_support)
        .unwrap_or_else(|error| panic!("suspended equipment remount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspended equipment remount commit failed: {error}"));

    let reason_change = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("suspension reason transition tick failed: {error}"));
    assert_eq!(
        reason_change.production_availability_changes(),
        &[ProductionAvailabilityChange::SuspensionReasonChanged {
            job,
            previous: ProductionSuspensionReason::EquipmentSupportUnavailable { equipment },
            reason: ProductionSuspensionReason::OutputSupportUnavailable {
                stockpile: destination,
            },
        }]
    );
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.suspension())
            .map(|suspension| (suspension.remaining_active_time(), suspension.reason())),
        Some((
            expected_remaining,
            ProductionSuspensionReason::OutputSupportUnavailable {
                stockpile: destination,
            }
        ))
    );

    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("suspended destination unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspended destination unmount commit failed: {error}"));
    let _ = validate_mount_stockpile(&registries, &state, destination, recovery_support)
        .unwrap_or_else(|error| panic!("suspended destination remount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspended destination remount commit failed: {error}"));

    let resumed_at = state.tick();
    let resumed_due = resumed_at
        .checked_add_span(expected_remaining)
        .unwrap_or_else(|| panic!("suspension fixture resumed due tick overflowed"));
    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("resume transition tick failed: {error}"));
    assert_eq!(
        outcome.production_availability_changes(),
        &[ProductionAvailabilityChange::Resumed {
            job,
            reason: ProductionSuspensionReason::OutputSupportUnavailable {
                stockpile: destination,
            },
            resumed_at,
            scheduled_completion: resumed_due,
        }]
    );
    assert_eq!(
        state.production().get_job(job).map(|record| (
            record.active_duration(),
            record.completes_at(),
            record.suspension()
        )),
        Some((active_duration, resumed_due, None))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(reserved_output_mass),
        "resume must not release reserved output capacity before completion"
    );

    while state.production().get_job(job).is_some() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("resumed heating completion failed: {error}"));
    }
    assert_eq!(state.tick(), resumed_due);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(10))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(Mass::ZERO),
        "completion must release exactly the reservation it materializes"
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        Some(condition(997_000))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn resolved_heating_becomes_stale_when_support_changes_before_start_validation() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let support = add_active_support(&registries, &mut state, 0);
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(token) => token,
        Err(error) => panic!("stale-support mount validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("stale-support mount commit failed: {error}");
    }
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("stale-support heating resolution failed: {error}"),
    };
    let expected_structure_revision = state.structures().revision();
    fail_support(&registries, &mut state, support);

    assert_eq!(
        validate_start_process(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            destination,
        ),
        Err(StartProcessError::StaleResolvedStructure {
            expected_structure_revision,
            actual_structure_revision: expected_structure_revision + 1,
        })
    );
}

#[test]
fn validated_heating_start_rejects_support_change_before_commit_without_consuming_resources() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let support = add_active_support(&registries, &mut state, 0);
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(token) => token,
        Err(error) => panic!("commit-race mount validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("commit-race mount commit failed: {error}");
    }
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("commit-race heating resolution failed: {error}"),
    };
    let start = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("commit-race start validation failed: {error}"),
    };
    let expected_structure_revision = state.structures().revision();
    fail_support(&registries, &mut state, support);
    let before = state.clone();

    assert_eq!(
        start.commit(&mut state),
        Err(StartProcessCommitError::StaleStructureRevision {
            expected: expected_structure_revision,
            actual: expected_structure_revision + 1,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn prevalidated_maintenance_and_mount_are_blocked_if_job_starts_first() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let support = add_active_support(&registries, &mut state, 0);
    let wear = match decide_equipment_wear(&state, equipment, 1) {
        Ok(plan) => plan,
        Err(error) => panic!("occupancy-race wear validation failed: {error}"),
    };
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(token) => token,
        Err(error) => panic!("occupancy-race mount validation failed: {error}"),
    };
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("occupancy-race heating resolution failed: {error}"),
    };
    let start = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("occupancy-race start validation failed: {error}"),
    };
    let job = match start.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("occupancy-race start commit failed: {error}"),
    };
    let completes_at = match state.production().get_job(job) {
        Some(record) => record.completes_at(),
        None => panic!("occupancy-race job disappeared"),
    };

    let before_wear = state.clone();
    assert_eq!(
        apply_equipment_condition_plan(&mut state, wear),
        Err(EquipmentConditionCommitError::EquipmentBusy {
            equipment,
            job,
            release: ProductionOccupancyRelease::Scheduled(completes_at),
        })
    );
    assert_eq!(state, before_wear);

    let before_mount = state.clone();
    assert_eq!(
        mount.commit(&mut state),
        Err(EquipmentSupportCommitError::EquipmentBusy {
            equipment,
            job,
            completes_at,
        })
    );
    assert_eq!(state, before_mount);
}

#[test]
fn heater_is_exclusive_while_job_runs_and_releases_on_completion() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    ) {
        panic!("second heater occupancy input failed: {error}");
    }
    let second_energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        BATTERY,
        Energy::from_nanojoules(500_000_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("second heater occupancy energy fixture failed: {error}"),
    };
    let target = Temperature::from_millikelvin(303_000);
    let first = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("first heater occupancy resolution failed: {error}"),
    };
    let duration = first.process_resolution().duration();
    let first_token = match validate_start_process(
        &registries,
        &state,
        first.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("first heater occupancy validation failed: {error}"),
    };
    let first_job = match first_token.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("first heater occupancy commit failed: {error}"),
    };
    let completes_at = match state.production().get_job(first_job) {
        Some(job) => job.completes_at(),
        None => panic!("first heater occupancy job disappeared"),
    };
    assert_eq!(
        state
            .production()
            .get_job(first_job)
            .and_then(|job| job.equipment_provider()),
        first.process_resolution().equipment_input()
    );

    let second = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        second_energy_store,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("second heater occupancy resolution failed: {error}"),
    };
    assert_eq!(
        validate_start_process(
            &registries,
            &state,
            second.process_resolution(),
            source,
            destination,
        ),
        Err(crate::production::StartProcessError::EquipmentBusy {
            equipment,
            job: first_job,
            release: ProductionOccupancyRelease::Scheduled(completes_at),
        })
    );
    assert_eq!(
        decide_equipment_wear(&state, equipment, 1),
        Err(EquipmentConditionPlanError::EquipmentBusy {
            equipment,
            job: first_job,
            release: ProductionOccupancyRelease::Scheduled(completes_at),
        })
    );

    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("heater occupancy completion failed: {error}");
        }
    }
    assert!(state.production().get_job(first_job).is_none());
    let post_release_wear = decide_equipment_wear(&state, equipment, 1)
        .unwrap_or_else(|error| panic!("released heater remained spuriously occupied: {error}"));
    assert_eq!(post_release_wear.equipment(), equipment);
    assert!(post_release_wear.after() < post_release_wear.before());

    let after_release = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("post-release heater resolution failed: {error}"),
    };
    let token = match validate_start_process(
        &registries,
        &state,
        after_release.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("post-release heater start failed: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("post-release heater commit failed: {error}");
    }
}

#[test]
fn finite_energy_store_is_exclusive_while_its_discharge_power_is_reserved() {
    let (registries, mut state, source, destination, first_heater, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    ) {
        panic!("energy occupancy second input failed: {error}");
    }
    let second_heater = match add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
        Ok(equipment) => equipment,
        Err(error) => panic!("energy occupancy second heater failed: {error}"),
    };
    let target = Temperature::from_millikelvin(303_000);
    let first = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        first_heater,
        energy_store,
        target,
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("energy occupancy first resolution failed: {error}"),
    };
    let duration = first.process_resolution().duration();
    let token = match validate_start_process(
        &registries,
        &state,
        first.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("energy occupancy first validation failed: {error}"),
    };
    let first_job = match token.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("energy occupancy first commit failed: {error}"),
    };
    let completes_at = match state.production().get_job(first_job) {
        Some(job) => job.completes_at(),
        None => panic!("energy occupancy first job disappeared"),
    };

    assert_eq!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            second_heater,
            energy_store,
            target,
        ),
        Err(SensibleHeatingResolutionError::Energy(
            EnergySupplyError::StoreBusy {
                store: energy_store,
                job: first_job,
                release: ProductionOccupancyRelease::Scheduled(completes_at),
            }
        ))
    );

    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("energy occupancy completion failed: {error}");
        }
    }
    assert!(state.production().get_job(first_job).is_none());
    assert!(
        resolve_test_sensible_heating_process(
            &registries,
            &state,
            PROCESS,
            source,
            second_heater,
            energy_store,
            target,
        )
        .is_ok()
    );
}

#[test]
fn due_heating_completion_rejects_stale_equipment_revision_atomically() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("completion-race heating resolution failed: {error}"),
    };
    let duration = resolved.process_resolution().duration();
    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("completion-race start validation failed: {error}"),
    };
    let job = match token.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("completion-race start commit failed: {error}"),
    };
    for _ in 1..duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("completion-race pre-due tick failed: {error}");
        }
    }
    assert_eq!(state.tick(), SimulationTick::new(duration.value() - 1));
    let due = match state.production().get_job(job) {
        Some(record) => record.completes_at(),
        None => panic!("completion-race job disappeared before due planning"),
    };
    let plan = match decide_due_completions(&registries, &state, due) {
        Ok(plan) => plan,
        Err(error) => panic!("completion-race due planning failed: {error:?}"),
    };
    let expected = state.equipment().revision();
    if let Err(error) = add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
        panic!("completion-race independent equipment mutation failed: {error}");
    }
    let before = state.clone();

    assert_eq!(
        apply_completion_plan(&mut state, plan),
        Err(CompletionCommitError::EquipmentRevisionConflict {
            expected,
            actual: expected + 1,
        })
    );
    assert_eq!(state, before);
    assert!(state.production().get_job(job).is_some());
}

#[test]
fn validated_heating_start_rejects_stale_equipment_before_consuming_other_resources() {
    let (registries, mut state, source, destination, equipment, energy_store) =
        make_loaded_fixture(EnergyCarrier::Electrical);
    let resolved = match resolve_test_sensible_heating_process(
        &registries,
        &state,
        PROCESS,
        source,
        equipment,
        energy_store,
        Temperature::from_millikelvin(303_000),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("stale equipment fixture resolution failed: {error}"),
    };
    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("stale equipment fixture validation failed: {error}"),
    };
    let expected = state.equipment().revision();
    if let Err(error) = add_equipment(&registries, &mut state, HEATER, Condition::PRISTINE) {
        panic!("independent equipment mutation failed: {error}");
    }
    let before = state.clone();

    assert_eq!(
        token.commit(&mut state),
        Err(
            crate::production::StartProcessCommitError::StaleEquipmentRevision {
                expected,
                actual: expected + 1,
            }
        )
    );
    assert_eq!(state, before);
    assert_eq!(state.production().jobs().count(), 0);
}
