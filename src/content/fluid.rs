//! Built-in finite fluid identities; world sources remain owned by future hydrology generation.

use crate::fluid::{FluidDefinition, FluidDefinitionId, FluidRegistry};

use super::MATERIAL_WATER;

pub const FLUID_WATER: FluidDefinitionId = FluidDefinitionId::new(1);

pub(crate) fn build_fluid_registry() -> FluidRegistry {
    FluidRegistry::new([FluidDefinition::new(
        FLUID_WATER,
        "water",
        MATERIAL_WATER,
        1_000,
    )])
}
