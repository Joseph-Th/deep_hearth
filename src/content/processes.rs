//! Built-in workshop material transformations with physical resolver ownership.

use crate::capability::{CapabilityComparison, CapabilityRequirement, CapabilityValue};
use crate::core::quantity::{Mass, MassFlow, Power, Temperature};
use crate::production::{ProcessDefinition, ProcessId, ProductionRegistry};

use super::capabilities::{
    CAPABILITY_COOLING_POWER, CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW,
    CAPABILITY_HEATING_POWER, CAPABILITY_THERMAL_BATCH, CAPABILITY_THERMAL_MAX_TEMPERATURE,
};

pub const PROCESS_CRUSH_ORE: ProcessId = ProcessId::new(1);
pub const PROCESS_MELT_PURE_COPPER: ProcessId = ProcessId::new(2);
pub const PROCESS_CAST_PURE_COPPER: ProcessId = ProcessId::new(3);

pub(crate) fn build_production_registry() -> ProductionRegistry {
    let mut registry = ProductionRegistry::new();
    for process in [
        ProcessDefinition::new_selected_batch(
            PROCESS_CRUSH_ORE,
            "crush ore",
            vec![
                CapabilityRequirement::new(
                    CAPABILITY_CRUSHER_FLOW,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
                ),
                CapabilityRequirement::new(
                    CAPABILITY_CRUSHER_BATCH,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(1)),
                ),
            ],
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_MELT_PURE_COPPER,
            "melt pure copper",
            vec![
                CapabilityRequirement::new(
                    CAPABILITY_HEATING_POWER,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Power(Power::from_microwatts(1_000_000)),
                ),
                CapabilityRequirement::new(
                    CAPABILITY_THERMAL_MAX_TEMPERATURE,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Temperature(Temperature::from_millikelvin(1_200_000)),
                ),
                CapabilityRequirement::new(
                    CAPABILITY_THERMAL_BATCH,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(1)),
                ),
            ],
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_CAST_PURE_COPPER,
            "cast pure copper",
            vec![
                CapabilityRequirement::new(
                    CAPABILITY_COOLING_POWER,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Power(Power::from_microwatts(1_000_000)),
                ),
                CapabilityRequirement::new(
                    CAPABILITY_THERMAL_MAX_TEMPERATURE,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Temperature(Temperature::from_millikelvin(1_400_000)),
                ),
                CapabilityRequirement::new(
                    CAPABILITY_THERMAL_BATCH,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(1)),
                ),
            ],
        ),
    ] {
        registry.register_process(process);
    }
    registry
}
