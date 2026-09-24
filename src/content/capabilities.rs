//! Built-in physical capability definitions used by canonical workshop equipment.

use crate::capability::{
    CapabilityDefinition, CapabilityId, CapabilityImprovement, CapabilityRegistry,
    CapabilityValueKind,
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
pub(crate) const CAPABILITY_WOODWORKING_FLOW: CapabilityId = CapabilityId::new(18);
pub(crate) const CAPABILITY_SAWING_FLOW: CapabilityId = CapabilityId::new(19);
pub(crate) const CAPABILITY_WALKING_WHEEL_POWER_OUTPUT: CapabilityId = CapabilityId::new(20);
pub(crate) const CAPABILITY_COPPER_HAMMERING_FLOW: CapabilityId = CapabilityId::new(21);
pub(crate) const CAPABILITY_POWERED_SAWING_FLOW: CapabilityId = CapabilityId::new(22);
pub(crate) const CAPABILITY_POWERED_COPPER_HAMMERING_FLOW: CapabilityId = CapabilityId::new(23);
pub(crate) const CAPABILITY_COBBING_FLOW: CapabilityId = CapabilityId::new(24);
pub(crate) const CAPABILITY_ORE_PICKING_FLOW: CapabilityId = CapabilityId::new(25);
pub(crate) const CAPABILITY_COPPER_PIERCING_FLOW: CapabilityId = CapabilityId::new(26);
pub(crate) const CAPABILITY_POWERED_COPPER_PIERCING_FLOW: CapabilityId = CapabilityId::new(27);

fn higher_is_better(
    id: CapabilityId,
    name: &'static str,
    kind: CapabilityValueKind,
) -> CapabilityDefinition {
    CapabilityDefinition::new_with_improvement(id, name, kind, CapabilityImprovement::Higher)
}

pub(crate) fn build_capability_registry() -> CapabilityRegistry {
    let mut registry = CapabilityRegistry::new();
    for definition in [
        higher_is_better(
            CAPABILITY_CRUSHER_FLOW,
            "crusher material throughput",
            CapabilityValueKind::MassFlow,
        ),
        higher_is_better(
            CAPABILITY_CRUSHER_BATCH,
            "crusher maximum batch mass",
            CapabilityValueKind::Mass,
        ),
        higher_is_better(
            CAPABILITY_HEATING_POWER,
            "furnace heating power",
            CapabilityValueKind::Power,
        ),
        higher_is_better(
            CAPABILITY_COOLING_POWER,
            "casting cooling power",
            CapabilityValueKind::Power,
        ),
        higher_is_better(
            CAPABILITY_THERMAL_MAX_TEMPERATURE,
            "thermal equipment maximum temperature",
            CapabilityValueKind::Temperature,
        ),
        higher_is_better(
            CAPABILITY_THERMAL_BATCH,
            "thermal equipment maximum batch mass",
            CapabilityValueKind::Mass,
        ),
        higher_is_better(
            CAPABILITY_SCREEN_FLOW,
            "screen material throughput",
            CapabilityValueKind::MassFlow,
        ),
        higher_is_better(
            CAPABILITY_SCREEN_BATCH,
            "screen maximum batch mass",
            CapabilityValueKind::Mass,
        ),
        higher_is_better(
            CAPABILITY_GRINDER_FLOW,
            "grinder material throughput",
            CapabilityValueKind::MassFlow,
        ),
        higher_is_better(
            CAPABILITY_GRINDER_BATCH,
            "grinder maximum batch mass",
            CapabilityValueKind::Mass,
        ),
        higher_is_better(
            CAPABILITY_MINING_FLOW,
            "mining material throughput",
            CapabilityValueKind::MassFlow,
        ),
        higher_is_better(
            CAPABILITY_MINING_MAX_BATCH,
            "mining maximum batch mass",
            CapabilityValueKind::Mass,
        ),
        higher_is_better(
            CAPABILITY_MINING_MAX_HARDNESS,
            "mining maximum material hardness",
            CapabilityValueKind::Pressure,
        ),
        higher_is_better(
            CAPABILITY_MANUAL_POWER_OUTPUT,
            "direct manual mechanical power output",
            CapabilityValueKind::Power,
        ),
        higher_is_better(
            CAPABILITY_SEPARATOR_FLOW,
            "separator material throughput",
            CapabilityValueKind::MassFlow,
        ),
        higher_is_better(
            CAPABILITY_SEPARATOR_BATCH,
            "separator maximum batch mass",
            CapabilityValueKind::Mass,
        ),
        higher_is_better(
            CAPABILITY_TREADLE_POWER_OUTPUT,
            "foot treadle mechanical power output",
            CapabilityValueKind::Power,
        ),
        higher_is_better(
            CAPABILITY_WOODWORKING_FLOW,
            "hand-tool timber shaping throughput",
            CapabilityValueKind::MassFlow,
        ),
        higher_is_better(
            CAPABILITY_SAWING_FLOW,
            "frame-saw timber ripping throughput",
            CapabilityValueKind::MassFlow,
        ),
        higher_is_better(
            CAPABILITY_WALKING_WHEEL_POWER_OUTPUT,
            "walking-wheel mechanical power output",
            CapabilityValueKind::Power,
        ),
        higher_is_better(
            CAPABILITY_COPPER_HAMMERING_FLOW,
            "human-powered copper hammering throughput",
            CapabilityValueKind::MassFlow,
        ),
        higher_is_better(
            CAPABILITY_POWERED_SAWING_FLOW,
            "mechanically powered timber sawing throughput",
            CapabilityValueKind::MassFlow,
        ),
        higher_is_better(
            CAPABILITY_POWERED_COPPER_HAMMERING_FLOW,
            "mechanically powered copper hammering throughput",
            CapabilityValueKind::MassFlow,
        ),
        higher_is_better(
            CAPABILITY_COBBING_FLOW,
            "manual ore cobbing throughput",
            CapabilityValueKind::MassFlow,
        ),
        higher_is_better(
            CAPABILITY_ORE_PICKING_FLOW,
            "manual visible-ore picking throughput",
            CapabilityValueKind::MassFlow,
        ),
        higher_is_better(
            CAPABILITY_COPPER_PIERCING_FLOW,
            "manual copper-sheet piercing throughput",
            CapabilityValueKind::MassFlow,
        ),
        higher_is_better(
            CAPABILITY_POWERED_COPPER_PIERCING_FLOW,
            "mechanically powered copper-sheet piercing throughput",
            CapabilityValueKind::MassFlow,
        ),
    ] {
        registry.register_capability(definition);
    }
    registry
}
