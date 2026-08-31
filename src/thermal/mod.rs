//! Owns deterministic material heat calculations and thermal process resolution.

mod casting_execution;
mod equipment_physics;
mod melting_execution;
mod phase_change_batch;
mod physics;
mod planning;
mod processes;

pub use casting_execution::{
    CastingBatchError, CastingJobValidationError, CastingPhaseChange, CastingProcessDefinition,
    CastingRequest, CastingResolutionError, ResolvedCasting, resolve_casting_process,
};
pub use melting_execution::{
    MeltingBatchError, MeltingJobValidationError, MeltingProcessDefinition, MeltingRequest,
    MeltingResolutionError, ResolvedMelting, resolve_melting_process,
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
    ResolvedSensibleHeating, SensibleHeatingProcessDefinition, SensibleHeatingRequest,
    SensibleHeatingResolutionError, ThermalJobValidationError, ThermalRegistry,
    resolve_sensible_heating_process,
};

pub(crate) use processes::validate_loaded_thermal_job;

use crate::capability::CapabilityId;
use crate::energy::EnergyCarrier;
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
use crate::material::FormId;

/// Shared equipment, carrier, and wear contract for one authored pure phase-change process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhaseChangeProcessProfile {
    transfer_power_capability: CapabilityId,
    max_temperature_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
    condition_wear_ppm_per_active_tick: u32,
}

impl PhaseChangeProcessProfile {
    #[must_use]
    pub const fn new(
        transfer_power_capability: CapabilityId,
        max_temperature_capability: CapabilityId,
        max_batch_mass_capability: CapabilityId,
        energy_carrier: EnergyCarrier,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            transfer_power_capability,
            max_temperature_capability,
            max_batch_mass_capability,
            energy_carrier,
            condition_wear_ppm_per_active_tick,
        }
    }

    #[must_use]
    pub const fn transfer_power_capability(self) -> CapabilityId {
        self.transfer_power_capability
    }

    #[must_use]
    pub const fn max_temperature_capability(self) -> CapabilityId {
        self.max_temperature_capability
    }

    #[must_use]
    pub const fn max_batch_mass_capability(self) -> CapabilityId {
        self.max_batch_mass_capability
    }

    #[must_use]
    pub const fn energy_carrier(self) -> EnergyCarrier {
        self.energy_carrier
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }
}

/// Authored material-form transition owned by one phase-change process definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhaseChangeForms {
    input: FormId,
    output: FormId,
}

impl PhaseChangeForms {
    #[must_use]
    pub const fn new(input: FormId, output: FormId) -> Self {
        Self { input, output }
    }

    #[must_use]
    pub const fn input(self) -> FormId {
        self.input
    }

    #[must_use]
    pub const fn output(self) -> FormId {
        self.output
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
