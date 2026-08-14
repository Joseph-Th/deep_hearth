//! Built-in fluid definitions; registrations remain empty until phase-aware world fluid content is authored.

use crate::fluid::{FluidDefinition, FluidRegistry};

pub(crate) fn build_fluid_registry() -> FluidRegistry {
    FluidRegistry::new(std::iter::empty::<FluidDefinition>())
}
