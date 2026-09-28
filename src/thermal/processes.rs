//! Owns thermal process definitions, shared batch physics, runtime resolution, and persistence replay.

mod heating;
mod registry;
mod sensible_batch;
mod validation;

pub use heating::{
    ResolvedSensibleHeating, SensibleHeatingRequest, SensibleHeatingResolutionError,
    resolve_sensible_heating_process,
};
pub use registry::{
    CastingPhaseChange, CastingProcessDefinition, MeltingProcessDefinition, PhaseChangeForms,
    PhaseChangeProcessProfile, SensibleHeatingProcessDefinition, ThermalRegistry,
};
pub use validation::ThermalJobValidationError;
pub(crate) use validation::validate_loaded_thermal_job;

#[cfg(all(
    test,
    any(not(feature = "test-unit-shard"), feature = "test-unit-thermal")
))]
#[path = "processes_tests.rs"]
mod tests;
