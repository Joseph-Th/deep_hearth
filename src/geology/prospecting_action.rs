//! Timed player field prospecting that converts bounded regional observation into geological knowledge.

use crate::labor::ProspectingSpatialResolution;
use crate::spatial::VoxelBounds;

fn prospecting_observation_count(
    resolution: ProspectingSpatialResolution,
    region: VoxelBounds,
) -> Option<u32> {
    match resolution {
        ProspectingSpatialResolution::AggregateRegion => Some(1),
        ProspectingSpatialResolution::PerVoxel => {
            let voxels = region.voxel_count()?;
            u32::try_from(voxels).ok()
        }
    }
}

mod abundance;
mod errors;
mod hardness;
mod start;
mod tick;

pub use errors::{FieldProspectingCommitError, FieldProspectingStartError};
pub use start::{
    FieldProspectingRequest, ValidatedFieldProspectingStart, validate_start_field_prospecting,
};
pub use tick::FieldProspectingOutcome;
pub(crate) use tick::{
    FieldProspectingTickError, apply_field_prospecting_tick, decide_field_prospecting_tick,
};

#[cfg(test)]
#[path = "prospecting_action_tests.rs"]
mod tests;
