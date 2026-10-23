//! Heating, melting, and casting process definitions.

use crate::production::ProcessDefinition;

use super::super::capabilities::{CAPABILITY_COOLING_POWER, CAPABILITY_HEATING_POWER};

use super::{
    PROCESS_CAST_IRON_INGOT, PROCESS_CAST_LEAD_INGOT, PROCESS_CAST_PURE_COPPER,
    PROCESS_CAST_TIN_INGOT, PROCESS_CAST_ZINC_INGOT, PROCESS_HEAT_MATERIAL_BATCH,
    PROCESS_MELT_PURE_COPPER, PROCESS_MELT_RECYCLED_IRON, PROCESS_MELT_RECYCLED_TIN,
    PROCESS_MELT_RECYCLED_ZINC, thermal_resolver_requirements,
};

pub(super) fn definitions() -> [ProcessDefinition; 10] {
    [
        ProcessDefinition::new(
            PROCESS_MELT_PURE_COPPER,
            "melt pure copper",
            thermal_resolver_requirements(CAPABILITY_HEATING_POWER),
        ),
        ProcessDefinition::new(
            PROCESS_HEAT_MATERIAL_BATCH,
            "sensible heat material batch",
            thermal_resolver_requirements(CAPABILITY_HEATING_POWER),
        ),
        ProcessDefinition::new(
            PROCESS_CAST_PURE_COPPER,
            "cast pure copper",
            thermal_resolver_requirements(CAPABILITY_COOLING_POWER),
        ),
        ProcessDefinition::new(
            PROCESS_MELT_RECYCLED_IRON,
            "remelt iron stock and reclaimed scrap",
            thermal_resolver_requirements(CAPABILITY_HEATING_POWER),
        ),
        ProcessDefinition::new(
            PROCESS_CAST_IRON_INGOT,
            "cast iron ingot from molten metal",
            thermal_resolver_requirements(CAPABILITY_COOLING_POWER),
        ),
        ProcessDefinition::new(
            PROCESS_MELT_RECYCLED_TIN,
            "remelt tin stock and reclaimed scrap",
            thermal_resolver_requirements(CAPABILITY_HEATING_POWER),
        ),
        ProcessDefinition::new(
            PROCESS_CAST_TIN_INGOT,
            "cast tin ingot from molten metal",
            thermal_resolver_requirements(CAPABILITY_COOLING_POWER),
        ),
        ProcessDefinition::new(
            PROCESS_MELT_RECYCLED_ZINC,
            "remelt zinc stock and reclaimed scrap",
            thermal_resolver_requirements(CAPABILITY_HEATING_POWER),
        ),
        ProcessDefinition::new(
            PROCESS_CAST_ZINC_INGOT,
            "cast zinc ingot from molten metal",
            thermal_resolver_requirements(CAPABILITY_COOLING_POWER),
        ),
        ProcessDefinition::new(
            PROCESS_CAST_LEAD_INGOT,
            "cast recovered molten lead into an ingot",
            thermal_resolver_requirements(CAPABILITY_COOLING_POWER),
        ),
    ]
}
