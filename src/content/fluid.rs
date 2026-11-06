//! Built-in fluid identities; generated spring basins hold finite water in runtime custody.

use crate::fluid::{FluidDefinition, FluidDefinitionId, FluidRegistry};

use super::MATERIAL_WATER;

pub const FLUID_WATER: FluidDefinitionId = FluidDefinitionId::new(1);

pub(crate) fn build_fluid_registry() -> FluidRegistry {
    FluidRegistry::new([FluidDefinition::new(FLUID_WATER, "water", MATERIAL_WATER)])
}
