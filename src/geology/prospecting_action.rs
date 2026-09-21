//! Timed player field prospecting that converts bounded regional observation into geological knowledge.

mod abundance;
mod errors;
mod hardness;
mod resource_mass;
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
