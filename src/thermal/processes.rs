//! Thermal process facade; child modules separate registry ownership, runtime heating resolution, and persistence replay.

mod heating;
mod registry;
mod validation;

pub use heating::{
    ResolvedSensibleHeating, SensibleHeatingRequest, SensibleHeatingResolutionError,
    resolve_sensible_heating_process,
};
pub use registry::{SensibleHeatingProcessDefinition, ThermalRegistry};
pub use validation::ThermalJobValidationError;
pub(crate) use validation::validate_loaded_thermal_job;

#[cfg(test)]
#[path = "processes_tests.rs"]
mod tests;
