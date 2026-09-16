//! Minimal normally validated process topologies shared by cross-owner tests.

use super::{make_test_registries_with_screening, make_test_registries_with_sensible_heating};
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
    CapabilityRequirement, CapabilityValue, CapabilityValueKind,
};
use crate::core::quantity::{
    Energy, Length, Mass, MassFlow, MassSpecificEnergy, Power, Temperature,
};
use crate::energy::{EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId};
use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId};
use crate::maintenance::{Condition, MaintenanceThresholds};
use crate::ore_processing::{PoweredOreProcessProfile, ScreeningProcessDefinition};
use crate::production::{ProcessDefinition, ProcessId};
use crate::registry::Registries;
use crate::thermal::SensibleHeatingProcessDefinition;

pub(crate) const STANDARD_TEST_HEATER: EquipmentDefinitionId = EquipmentDefinitionId::new(990_001);
pub(crate) const STANDARD_TEST_HEATING_ENERGY: EnergyStoreDefinitionId =
    EnergyStoreDefinitionId::new(990_001);
pub(crate) const STANDARD_TEST_SCREEN: EquipmentDefinitionId = EquipmentDefinitionId::new(990_011);
pub(crate) const STANDARD_TEST_SCREENING_ENERGY: EnergyStoreDefinitionId =
    EnergyStoreDefinitionId::new(990_011);

/// Builds one minimal sensible-heating topology without unrelated built-in content.
pub(crate) fn make_test_registries_with_standard_sensible_heating(
    process: ProcessId,
) -> Registries {
    const HEATING_POWER: CapabilityId = CapabilityId::new(990_001);
    const MAX_TEMPERATURE: CapabilityId = CapabilityId::new(990_002);
    const MAX_BATCH_MASS: CapabilityId = CapabilityId::new(990_003);

    let profile = CapabilityProfile::new([
        (
            HEATING_POWER,
            CapabilityValue::Power(Power::from_microwatts(1_000_000)),
        ),
        (
            MAX_TEMPERATURE,
            CapabilityValue::Temperature(Temperature::from_millikelvin(1_500_000)),
        ),
        (
            MAX_BATCH_MASS,
            CapabilityValue::Mass(Mass::from_milligrams(100_000_000)),
        ),
    ])
    .unwrap_or_else(|error| panic!("standard heating capability fixture failed: {error}"));
    let thresholds = MaintenanceThresholds::new(
        Condition::new(600_000).unwrap_or_else(|error| {
            panic!("standard heating maintenance threshold failed: {error}")
        }),
        Condition::new(250_000)
            .unwrap_or_else(|error| panic!("standard heating critical threshold failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("standard heating maintenance fixture failed: {error}"));
    let process_definition = ProcessDefinition::new(
        process,
        "standard test sensible heating",
        vec![
            CapabilityRequirement::new(
                HEATING_POWER,
                CapabilityComparison::AtLeast,
                CapabilityValue::Power(Power::from_microwatts(1)),
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
                "standard test heating power",
                CapabilityValueKind::Power,
            ),
            CapabilityDefinition::new(
                MAX_TEMPERATURE,
                "standard test maximum temperature",
                CapabilityValueKind::Temperature,
            ),
            CapabilityDefinition::new(
                MAX_BATCH_MASS,
                "standard test maximum batch mass",
                CapabilityValueKind::Mass,
            ),
        ],
        EquipmentDefinition::new(
            STANDARD_TEST_HEATER,
            "standard test heater",
            Mass::from_milligrams(1_000_000),
            profile,
            thresholds,
        ),
        vec![EnergyStoreDefinition::new_with_transfer_limits(
            STANDARD_TEST_HEATING_ENERGY,
            "standard test electrical buffer",
            EnergyCarrier::Electrical,
            Energy::from_nanojoules(10_000_000_000_000),
            Power::ZERO,
            Power::from_microwatts(1_000_000),
        )],
        process_definition,
        SensibleHeatingProcessDefinition::new(
            process,
            HEATING_POWER,
            MAX_TEMPERATURE,
            MAX_BATCH_MASS,
            EnergyCarrier::Electrical,
            1,
        ),
    )
}

/// Builds one minimal powered-screening topology for routed-output tests.
pub(crate) fn make_test_registries_with_standard_screening(process: ProcessId) -> Registries {
    const MASS_FLOW: CapabilityId = CapabilityId::new(990_011);
    const MAX_BATCH_MASS: CapabilityId = CapabilityId::new(990_012);

    let profile = CapabilityProfile::new([
        (
            MASS_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1_000)),
        ),
        (
            MAX_BATCH_MASS,
            CapabilityValue::Mass(Mass::from_milligrams(100_000_000)),
        ),
    ])
    .unwrap_or_else(|error| panic!("standard screening capability fixture failed: {error}"));
    let thresholds = MaintenanceThresholds::new(
        Condition::new(600_000).unwrap_or_else(|error| {
            panic!("standard screening maintenance threshold failed: {error}")
        }),
        Condition::new(250_000).unwrap_or_else(|error| {
            panic!("standard screening critical threshold failed: {error}")
        }),
    )
    .unwrap_or_else(|error| panic!("standard screening maintenance fixture failed: {error}"));
    let process_definition = ProcessDefinition::new(
        process,
        "standard test screening",
        vec![
            CapabilityRequirement::new(
                MASS_FLOW,
                CapabilityComparison::AtLeast,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
            ),
            CapabilityRequirement::new(
                MAX_BATCH_MASS,
                CapabilityComparison::AtLeast,
                CapabilityValue::Mass(Mass::from_milligrams(1)),
            ),
        ],
    );
    make_test_registries_with_screening(
        vec![
            CapabilityDefinition::new(
                MASS_FLOW,
                "standard test screening throughput",
                CapabilityValueKind::MassFlow,
            ),
            CapabilityDefinition::new(
                MAX_BATCH_MASS,
                "standard test screening maximum batch mass",
                CapabilityValueKind::Mass,
            ),
        ],
        EquipmentDefinition::new(
            STANDARD_TEST_SCREEN,
            "standard test screen",
            Mass::from_milligrams(1_000_000),
            profile,
            thresholds,
        ),
        vec![EnergyStoreDefinition::new_with_transfer_limits(
            STANDARD_TEST_SCREENING_ENERGY,
            "standard test mechanical buffer",
            EnergyCarrier::Mechanical,
            Energy::from_nanojoules(10_000_000_000),
            Power::ZERO,
            Power::from_microwatts(1_000_000),
        )],
        process_definition,
        ScreeningProcessDefinition::new(
            process,
            super::super::FORM_CRUSHED,
            super::super::FORM_CRUSHED,
            Length::from_micrometers(5_000),
            PoweredOreProcessProfile::new(
                MASS_FLOW,
                MAX_BATCH_MASS,
                EnergyCarrier::Mechanical,
                MassSpecificEnergy::from_nanojoules_per_milligram(100),
                1,
            ),
        ),
    )
}
