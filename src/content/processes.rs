//! Built-in workshop material transformations with physical resolver ownership.

use crate::capability::{CapabilityComparison, CapabilityRequirement, CapabilityValue};
use crate::core::quantity::{Mass, MassFlow, Power, Temperature};
use crate::material::{CommodityKey, MaterialInputSpec};
use crate::production::{ProcessDefinition, ProcessId, ProductionRegistry};

use super::capabilities::{
    CAPABILITY_COOLING_POWER, CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW,
    CAPABILITY_GRINDER_BATCH, CAPABILITY_GRINDER_FLOW, CAPABILITY_HEATING_POWER,
    CAPABILITY_SCREEN_BATCH, CAPABILITY_SCREEN_FLOW, CAPABILITY_SEPARATOR_BATCH,
    CAPABILITY_SEPARATOR_FLOW, CAPABILITY_THERMAL_BATCH, CAPABILITY_THERMAL_MAX_TEMPERATURE,
};
use super::{
    FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
};

pub const PROCESS_CRUSH_ORE: ProcessId = ProcessId::new(1);
pub const PROCESS_MELT_PURE_COPPER: ProcessId = ProcessId::new(2);
pub const PROCESS_CAST_PURE_COPPER: ProcessId = ProcessId::new(3);
pub const PROCESS_SCREEN_CRUSHED_ORE: ProcessId = ProcessId::new(4);
pub const PROCESS_GRIND_CRUSHED_ORE: ProcessId = ProcessId::new(5);
pub const PROCESS_FINE_GRIND_SCREEN_OVERSIZE: ProcessId = ProcessId::new(6);
pub const PROCESS_KNAP_STONE_TOOL: ProcessId = ProcessId::new(7);
pub const PROCESS_SHAPE_WOOD_HANDLE: ProcessId = ProcessId::new(9);
pub const PROCESS_SHAPE_STONE_FLYWHEEL: ProcessId = ProcessId::new(10);
pub const PROCESS_COLD_WORK_COPPER_REINFORCEMENT: ProcessId = ProcessId::new(11);
pub const PROCESS_SEPARATE_NATIVE_COPPER: ProcessId = ProcessId::new(12);

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
                    CapabilityValue::Power(Power::from_microwatts(100_000_000_000)),
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
                    CapabilityValue::Power(Power::from_microwatts(100_000_000_000)),
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
        ProcessDefinition::new_selected_batch(
            PROCESS_SCREEN_CRUSHED_ORE,
            "screen crushed ore",
            vec![
                CapabilityRequirement::new(
                    CAPABILITY_SCREEN_FLOW,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
                ),
                CapabilityRequirement::new(
                    CAPABILITY_SCREEN_BATCH,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(1)),
                ),
            ],
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_GRIND_CRUSHED_ORE,
            "grind crushed ore",
            vec![
                CapabilityRequirement::new(
                    CAPABILITY_GRINDER_FLOW,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
                ),
                CapabilityRequirement::new(
                    CAPABILITY_GRINDER_BATCH,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(1)),
                ),
            ],
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
            "fine grind screen oversize",
            vec![
                CapabilityRequirement::new(
                    CAPABILITY_GRINDER_FLOW,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
                ),
                CapabilityRequirement::new(
                    CAPABILITY_GRINDER_BATCH,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(1)),
                ),
            ],
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_SEPARATE_NATIVE_COPPER,
            "separate native copper from crushed ore",
            vec![
                CapabilityRequirement::new(
                    CAPABILITY_SEPARATOR_FLOW,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
                ),
                CapabilityRequirement::new(
                    CAPABILITY_SEPARATOR_BATCH,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(1)),
                ),
            ],
        ),
        ProcessDefinition::new(
            PROCESS_KNAP_STONE_TOOL,
            "knap stone tool",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
                Mass::from_milligrams(1_000_000),
            )],
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_SHAPE_WOOD_HANDLE,
            "shape wood handle",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(1_000_000),
            )],
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_SHAPE_STONE_FLYWHEEL,
            "shape stone flywheel",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
                Mass::from_milligrams(1_000_000),
            )],
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
            "cold-work native copper reinforcement",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
                Mass::from_milligrams(20_000),
            )],
            Vec::new(),
        ),
    ] {
        registry.register_process(process);
    }
    registry
}
