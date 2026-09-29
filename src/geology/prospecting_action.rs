//! Timed player field prospecting that converts bounded regional observation into geological knowledge.

mod abundance;
mod errors;
mod hardness;
mod resource_mass;
mod start;
mod tick;

pub(in crate::geology) use abundance::resolve_historical_region_abundance_bounds;
pub use errors::{FieldProspectingCommitError, FieldProspectingStartError};
pub(in crate::geology) use hardness::resolve_historical_region_excavation_hardness;
pub(in crate::geology) use resource_mass::resource_mass_band_matches_resolution;
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
