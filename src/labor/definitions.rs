//! Immutable player-labor definitions; subsystem execution owns runtime admission and mutation.

mod construction;
mod gathering;
mod manual_power;
mod prospecting;
mod registry;

pub use construction::{StructuralConstructionDefinition, StructuralConstructionMethodId};
pub use gathering::{SurfaceGatheringDefinition, SurfaceGatheringMethodId};
pub use manual_power::{ManualPowerDefinition, ManualPowerMethodId};
pub(crate) use prospecting::ProspectingRegionError;
pub use prospecting::{
    ProspectingDefinition, ProspectingEquipmentProfile, ProspectingMethodId,
    ProspectingSpatialResolution,
};
pub use registry::LaborRegistry;

#[cfg(test)]
#[path = "definitions_tests.rs"]
mod tests;
