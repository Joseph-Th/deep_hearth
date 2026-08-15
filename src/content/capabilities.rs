//! Built-in physical capability definitions used by canonical workshop equipment.

use crate::capability::{
    CapabilityDefinition, CapabilityId, CapabilityRegistry, CapabilityValueKind,
};

pub(crate) const CAPABILITY_CRUSHER_FLOW: CapabilityId = CapabilityId::new(1);
pub(crate) const CAPABILITY_CRUSHER_BATCH: CapabilityId = CapabilityId::new(2);
pub(crate) const CAPABILITY_HEATING_POWER: CapabilityId = CapabilityId::new(3);
pub(crate) const CAPABILITY_COOLING_POWER: CapabilityId = CapabilityId::new(4);
pub(crate) const CAPABILITY_THERMAL_MAX_TEMPERATURE: CapabilityId = CapabilityId::new(5);
pub(crate) const CAPABILITY_THERMAL_BATCH: CapabilityId = CapabilityId::new(6);

pub(crate) fn build_capability_registry() -> CapabilityRegistry {
    let mut registry = CapabilityRegistry::new();
    for definition in [
        CapabilityDefinition::new(
            CAPABILITY_CRUSHER_FLOW,
            "crusher material throughput",
            CapabilityValueKind::MassFlow,
        ),
        CapabilityDefinition::new(
            CAPABILITY_CRUSHER_BATCH,
            "crusher maximum batch mass",
            CapabilityValueKind::Mass,
        ),
        CapabilityDefinition::new(
            CAPABILITY_HEATING_POWER,
            "furnace heating power",
            CapabilityValueKind::Power,
        ),
        CapabilityDefinition::new(
            CAPABILITY_COOLING_POWER,
            "casting cooling power",
            CapabilityValueKind::Power,
        ),
        CapabilityDefinition::new(
            CAPABILITY_THERMAL_MAX_TEMPERATURE,
            "thermal equipment maximum temperature",
            CapabilityValueKind::Temperature,
        ),
        CapabilityDefinition::new(
            CAPABILITY_THERMAL_BATCH,
            "thermal equipment maximum batch mass",
            CapabilityValueKind::Mass,
        ),
    ] {
        registry.register_capability(definition);
    }
    registry
}
