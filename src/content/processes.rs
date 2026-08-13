//! Built-in material transformations; registrations stay empty until their physical authorization systems exist.

use crate::production::ProductionRegistry;

pub(crate) fn build_production_registry() -> ProductionRegistry {
    // Do not register simplified recipes that bypass temperature, energy, tooling, skill, or
    // equipment capability. The registry infrastructure is production-ready; authored processes
    // arrive with the systems that can faithfully authorize them.
    ProductionRegistry::new()
}
