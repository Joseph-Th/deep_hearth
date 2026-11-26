//! Timed player field prospecting that converts bounded regional observation into geological knowledge.

mod abundance;
mod catalog;
mod errors;
mod hardness;
mod interruption;
mod resource_mass;
mod start;
mod tick;

pub(in crate::geology) use abundance::resolve_region_abundance_bounds_from_deposits;
pub use catalog::{
    PlayerProspectingInstrumentOption, PlayerProspectingOption, player_prospecting_options,
};
pub use errors::{FieldProspectingCommitError, FieldProspectingStartError};
pub(in crate::geology) use hardness::resolve_region_excavation_hardness_from_deposits;
pub(crate) use interruption::{
    FieldProspectingInterruptionPlan, apply_field_prospecting_interruption,
    decide_field_prospecting_interruption,
};
#[cfg(test)]
pub(crate) use resource_mass::resolve_region_resource_mass;
pub(in crate::geology) use resource_mass::{
    localized_resource_body, maximum_historical_resource_mass,
};
pub(crate) use start::prospecting_region_has_authority;
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
