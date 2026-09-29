//! Resolves condition-adjusted equipment capabilities from immutable definitions and runtime state.

mod capability;
mod errors;
mod provider;

pub use capability::project_equipment_capability;
pub(crate) use capability::{
    evaluate_equipment_capabilities_at_condition, resolve_equipment_capability,
};
pub use errors::EquipmentProviderError;
pub use provider::{
    ResolvedEquipmentProvider, resolve_available_equipment_provider, resolve_equipment_provider,
};
pub(crate) use provider::{ValidatedEquipmentUse, resolve_equipment_provider_with_occupancy};

#[cfg(test)]
#[path = "equipment_integration_tests.rs"]
mod tests;
