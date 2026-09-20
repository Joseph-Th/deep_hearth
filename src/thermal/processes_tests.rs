//! Focused tests for thermal process admission, occupancy, suspension, and replay semantics.

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityImprovement,
    CapabilityProfile, CapabilityRequirement, CapabilityValue, CapabilityValueKind,
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
    CapabilityConditionCurve, CapabilityConditionPoint, EquipmentDefinition, EquipmentDefinitionId,
    EquipmentId, EquipmentProviderError, EquipmentSupportCommitError, add_equipment,
    validate_mount_equipment, validate_unmount_equipment,
};
use crate::inventory::{
    MaterialLotSelection, StockpileId, StockpileStorageProfile, add_solid_stockpile_for_test,
    add_stockpile, deposit_composed_lot_for_test, deposit_lot_for_test, validate_mount_stockpile,
    validate_unmount_stockpile,
};
use crate::maintenance::{Condition, MaintenanceThresholds};
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};

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
use crate::thermal::{PhaseSensibleHeatError, SensibleHeatError, calculate_phase_sensible_heat};

const HEATING_POWER: CapabilityId = CapabilityId::new(920_001);
const MAX_TEMPERATURE: CapabilityId = CapabilityId::new(920_002);
const MAX_BATCH_MASS: CapabilityId = CapabilityId::new(920_003);
const HEATER: EquipmentDefinitionId = EquipmentDefinitionId::new(920_001);
const BATTERY: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(920_001);
const COMPATIBLE_BATTERY: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(920_002);
const PROCESS: ProcessId = ProcessId::new(920_001);

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("thermal test condition fixture failed: {error}"),
    }
}

#[path = "processes_tests/foundational.rs"]
mod foundational;

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
    let mut energy_definitions = vec![energy];
    if carrier != EnergyCarrier::Electrical {
        energy_definitions.push(EnergyStoreDefinition::new_with_transfer_limits(
            COMPATIBLE_BATTERY,
            "test compatible electrical battery",
            EnergyCarrier::Electrical,
            Energy::from_nanojoules(1_000_000_000),
            Power::ZERO,
            energy_output_power,
        ));
    }
    let process = ProcessDefinition::new(
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
            CapabilityDefinition::new_with_improvement(
                HEATING_POWER,
                "heating transfer power",
                CapabilityValueKind::Power,
                CapabilityImprovement::Higher,
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
        energy_definitions,
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

#[path = "processes_tests/execution.rs"]
mod execution;

#[path = "processes_tests/validation.rs"]
mod validation;

#[path = "processes_tests/lifecycle.rs"]
mod lifecycle;
