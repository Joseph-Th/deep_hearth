//! Built-in finite fluid identities; runtime world-source generation is not implemented.

use crate::fluid::{FluidDefinition, FluidDefinitionId, FluidRegistry};

use super::MATERIAL_WATER;

pub const FLUID_WATER: FluidDefinitionId = FluidDefinitionId::new(1);

pub(crate) fn build_fluid_registry() -> FluidRegistry {
    FluidRegistry::new([FluidDefinition::new(FLUID_WATER, "water", MATERIAL_WATER)])
}
