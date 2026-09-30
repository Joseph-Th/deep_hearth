//! Resolves condition-adjusted equipment capabilities from immutable definitions and runtime state.

mod capability;
mod errors;
mod provider;

pub use capability::project_equipment_capability;
pub(crate) use capability::{
    evaluate_equipment_capabilities_at_condition, resolve_equipment_capability,
};
pub use errors::EquipmentProviderError;
pub(crate) use provider::{
    EquipmentProviderOccupancy, ValidatedEquipmentUse, resolve_equipment_provider_with_occupancy,
};
pub use provider::{
    ResolvedEquipmentProvider, resolve_available_equipment_provider, resolve_equipment_provider,
};

#[cfg(test)]
#[path = "equipment_integration_tests.rs"]
mod tests;
