//! Heating, melting, and casting process definitions.

use crate::production::ProcessDefinition;

use super::super::capabilities::{CAPABILITY_COOLING_POWER, CAPABILITY_HEATING_POWER};

use super::{
    PROCESS_CAST_PURE_COPPER, PROCESS_HEAT_MATERIAL_BATCH, PROCESS_MELT_PURE_COPPER,
    thermal_resolver_requirements,
};

pub(super) fn definitions() -> [ProcessDefinition; 3] {
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
    ]
}
