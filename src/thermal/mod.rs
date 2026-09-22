//! Owns deterministic material heat calculations and thermal process resolution.

mod casting_execution;
mod equipment_physics;
mod melting_execution;
mod phase_change_batch;
mod physics;
mod planning;
mod processes;

pub use casting_execution::{
    CastingBatchError, CastingJobValidationError, CastingRequest, CastingResolutionError,
    ResolvedCasting, resolve_casting_process,
};
pub use melting_execution::{
    MeltingBatchError, MeltingJobValidationError, MeltingRequest, MeltingResolutionError,
    ResolvedMelting, resolve_melting_process,
};
pub use physics::{
    FusionHeat, FusionHeatError, HeatDirection, MaterialThermalEnergyError, PhaseSensibleHeatError,
    SensibleHeat, SensibleHeatError, calculate_fusion_heat, calculate_material_thermal_energy,
    calculate_phase_sensible_heat, calculate_sensible_heat,
};
pub use planning::{
    CastingLotMassConstraint, CastingLotMassEnvelope, CastingLotMassRequest,
    MeltingLotMassConstraint, MeltingLotMassEnvelope, MeltingLotMassRequest,
    assess_casting_lot_mass_envelope, assess_melting_lot_mass_envelope,
};
pub use processes::{
    CastingPhaseChange, CastingProcessDefinition, MeltingProcessDefinition, PhaseChangeForms,
    PhaseChangeProcessProfile, ResolvedSensibleHeating, SensibleHeatingProcessDefinition,
    SensibleHeatingRequest, SensibleHeatingResolutionError, ThermalJobValidationError,
    ThermalRegistry, resolve_sensible_heating_process,
};

pub(crate) use processes::validate_loaded_thermal_job;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
