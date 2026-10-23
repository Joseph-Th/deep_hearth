//! Multi-reactant lead smelting by iron precipitation.

use super::super::capabilities::{
    CAPABILITY_SULFIDE_PRECIPITATION_FLOW, CAPABILITY_THERMAL_BATCH,
    CAPABILITY_THERMAL_MAX_TEMPERATURE,
};
use super::{PROCESS_PRECIPITATE_LEAD_FROM_GALENA, mass_flow_resolver_requirements};
use crate::capability::{CapabilityComparison, CapabilityRequirement, CapabilityValue};
use crate::core::quantity::Temperature;
use crate::production::ProcessDefinition;

pub(super) fn definitions() -> [ProcessDefinition; 1] {
    let mut requirements = mass_flow_resolver_requirements(
        CAPABILITY_SULFIDE_PRECIPITATION_FLOW,
        CAPABILITY_THERMAL_BATCH,
    );
    requirements.push(CapabilityRequirement::new(
        CAPABILITY_THERMAL_MAX_TEMPERATURE,
        CapabilityComparison::AtLeast,
        CapabilityValue::Temperature(Temperature::from_millikelvin(1_325_000)),
    ));
    [ProcessDefinition::new(
        PROCESS_PRECIPITATE_LEAD_FROM_GALENA,
        "precipitate lead from crushed galena and reclaimed iron",
        requirements,
    )]
}
