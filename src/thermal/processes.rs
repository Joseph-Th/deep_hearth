//! Owns thermal process definitions, shared batch physics, runtime resolution, and persistence replay.

mod cooling;
mod heating;
mod registry;
mod sensible_batch;
mod validation;

pub use cooling::{
    ResolvedSensibleCooling, SensibleCoolingBatchError, SensibleCoolingRequest,
    SensibleCoolingResolutionError, resolve_sensible_cooling_process,
};
pub use heating::{
    ResolvedSensibleHeating, SensibleHeatingRequest, SensibleHeatingResolutionError,
    resolve_sensible_heating_process,
};
pub use registry::{
    CastingPhaseChange, CastingProcessDefinition, FiringConversion, MeltingProcessDefinition,
    PhaseChangeForms, PhaseChangeProcessProfile, SensibleCoolingProcessDefinition,
    SensibleHeatingProcessDefinition, ThermalRegistry,
};
pub use validation::ThermalJobValidationError;
pub(crate) use validation::validate_loaded_thermal_job;

#[cfg(test)]
#[path = "processes_tests.rs"]
mod tests;
