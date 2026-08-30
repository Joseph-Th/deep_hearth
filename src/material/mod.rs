//! Material identities, authored definitions, physical-state policy, and registry facade.

mod assembly;
mod composition;
mod definitions;
mod identity;
mod lot;
mod particle;
mod phase;
mod properties;
mod registry;

pub use assembly::MaterialAssemblyProfile;
pub use composition::{
    CompositionComponent, CompositionConstraint, CompositionConstraintError, CompositionError,
    MaterialComposition,
};
pub use definitions::{
    FormDefinition, MaterialDefinition, MaterialFormCohesion, MaterialPhase,
    ParticleSizeStatePolicy,
};
pub use identity::{CommodityKey, FormId, MaterialId};
pub use lot::{MaterialInputSpec, MaterialInputSpecError, MaterialLotSpec, MaterialLotSpecError};
pub use particle::{
    ParticleSizeClass, ParticleSizeClassError, ParticleSizeDistribution,
    ParticleSizeDistributionError, ParticleSizeRange, ParticleSizeRangeError,
};
pub use phase::{
    MaterialPhaseStateError, ParticleSizeStateError, validate_material_particle_size_state,
    validate_material_phase_state,
};
pub use properties::{
    FusionProperties, MaterialProperties, StructuralProperties, ThermalProperties,
};
pub use registry::MaterialRegistry;

/// Normalization scale used by runtime material compositions.
pub const COMPOSITION_PARTS_PER_MILLION: u32 = 1_000_000;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
