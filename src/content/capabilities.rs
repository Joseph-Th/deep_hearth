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
pub(crate) const CAPABILITY_SCREEN_FLOW: CapabilityId = CapabilityId::new(7);
pub(crate) const CAPABILITY_SCREEN_BATCH: CapabilityId = CapabilityId::new(8);
pub(crate) const CAPABILITY_GRINDER_FLOW: CapabilityId = CapabilityId::new(9);
pub(crate) const CAPABILITY_GRINDER_BATCH: CapabilityId = CapabilityId::new(10);
pub(crate) const CAPABILITY_MINING_FLOW: CapabilityId = CapabilityId::new(11);
pub(crate) const CAPABILITY_MINING_MAX_BATCH: CapabilityId = CapabilityId::new(12);
pub(crate) const CAPABILITY_MINING_MAX_HARDNESS: CapabilityId = CapabilityId::new(13);
pub(crate) const CAPABILITY_MANUAL_POWER_OUTPUT: CapabilityId = CapabilityId::new(14);
pub(crate) const CAPABILITY_SEPARATOR_FLOW: CapabilityId = CapabilityId::new(15);
pub(crate) const CAPABILITY_SEPARATOR_BATCH: CapabilityId = CapabilityId::new(16);
pub(crate) const CAPABILITY_TREADLE_POWER_OUTPUT: CapabilityId = CapabilityId::new(17);

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
        CapabilityDefinition::new(
            CAPABILITY_SCREEN_FLOW,
            "screen material throughput",
            CapabilityValueKind::MassFlow,
        ),
        CapabilityDefinition::new(
            CAPABILITY_SCREEN_BATCH,
            "screen maximum batch mass",
            CapabilityValueKind::Mass,
        ),
        CapabilityDefinition::new(
            CAPABILITY_GRINDER_FLOW,
            "grinder material throughput",
            CapabilityValueKind::MassFlow,
        ),
        CapabilityDefinition::new(
            CAPABILITY_GRINDER_BATCH,
            "grinder maximum batch mass",
            CapabilityValueKind::Mass,
        ),
        CapabilityDefinition::new(
            CAPABILITY_MINING_FLOW,
            "mining material throughput",
            CapabilityValueKind::MassFlow,
        ),
        CapabilityDefinition::new(
            CAPABILITY_MINING_MAX_BATCH,
            "mining maximum batch mass",
            CapabilityValueKind::Mass,
        ),
        CapabilityDefinition::new(
            CAPABILITY_MINING_MAX_HARDNESS,
            "mining maximum material hardness",
            CapabilityValueKind::Pressure,
        ),
        CapabilityDefinition::new(
            CAPABILITY_MANUAL_POWER_OUTPUT,
            "direct manual mechanical power output",
            CapabilityValueKind::Power,
        ),
        CapabilityDefinition::new(
            CAPABILITY_SEPARATOR_FLOW,
            "separator material throughput",
            CapabilityValueKind::MassFlow,
        ),
        CapabilityDefinition::new(
            CAPABILITY_SEPARATOR_BATCH,
            "separator maximum batch mass",
            CapabilityValueKind::Mass,
        ),
        CapabilityDefinition::new(
            CAPABILITY_TREADLE_POWER_OUTPUT,
            "foot treadle mechanical power output",
            CapabilityValueKind::Power,
        ),
    ] {
        registry.register_capability(definition);
    }
    registry
}
