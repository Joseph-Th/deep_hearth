//! Ore/material preparation definitions and scalar throughput physics; sibling execution code resolves exact selected batches.

mod comminution_execution;

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU16;

use crate::capability::{CapabilityId, CapabilityRegistry, CapabilityValueKind};
use crate::core::quantity::{Mass, MassFlow, MassSpecificEnergy};
use crate::core::time::TickSpan;
use crate::energy::EnergyCarrier;
use crate::material::{
    FormId, MaterialPhase, MaterialRegistry, ParticleSizeRange, ParticleSizeStatePolicy,
};
use crate::production::{ProcessId, ProcessInputPolicy, ProductionRegistry};

pub use comminution_execution::{
    ComminutionBatchError, ComminutionBottleneck, ComminutionJobValidationError,
    ComminutionRequest, ComminutionResolutionError, ResolvedComminution,
    resolve_comminution_process,
};

pub(crate) use comminution_execution::validate_loaded_comminution_job;

/// Immutable declaration that one selected-batch process reduces solid material to a finer form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComminutionProcessDefinition {
    process: ProcessId,
    input_form: FormId,
    output_form: FormId,
    output_particle_size: ParticleSizeRange,
    operating: ComminutionOperatingProfile,
}

/// Immutable equipment/work envelope used to resolve one comminution process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComminutionOperatingProfile {
    mass_flow_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
    specific_energy: MassSpecificEnergy,
    condition_wear_ppm_per_active_tick: u32,
}

impl ComminutionOperatingProfile {
    #[must_use]
    pub const fn new(
        mass_flow_capability: CapabilityId,
        max_batch_mass_capability: CapabilityId,
        energy_carrier: EnergyCarrier,
        specific_energy: MassSpecificEnergy,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert!(
            !specific_energy.is_zero(),
            "comminution mass-specific energy must be nonzero"
        );
        Self {
            mass_flow_capability,
            max_batch_mass_capability,
            energy_carrier,
            specific_energy,
            condition_wear_ppm_per_active_tick,
        }
    }
}

impl ComminutionProcessDefinition {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        input_form: FormId,
        output_form: FormId,
        output_particle_size: ParticleSizeRange,
        operating: ComminutionOperatingProfile,
    ) -> Self {
        Self {
            process,
            input_form,
            output_form,
            output_particle_size,
            operating,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn input_form(self) -> FormId {
        self.input_form
    }

    #[must_use]
    pub const fn output_form(self) -> FormId {
        self.output_form
    }

    #[must_use]
    pub const fn output_particle_size(self) -> ParticleSizeRange {
        self.output_particle_size
    }

    #[must_use]
    pub const fn mass_flow_capability(self) -> CapabilityId {
        self.operating.mass_flow_capability
    }

    #[must_use]
    pub const fn max_batch_mass_capability(self) -> CapabilityId {
        self.operating.max_batch_mass_capability
    }

    #[must_use]
    pub const fn energy_carrier(self) -> EnergyCarrier {
        self.operating.energy_carrier
    }

    #[must_use]
    pub const fn specific_energy(self) -> MassSpecificEnergy {
        self.operating.specific_energy
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.operating.condition_wear_ppm_per_active_tick
    }
}

/// Immutable lookup table for ore/material-preparation process semantics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OreProcessingRegistry {
    comminution: BTreeMap<ProcessId, ComminutionProcessDefinition>,
}

impl OreProcessingRegistry {
    pub(crate) fn new(definitions: impl IntoIterator<Item = ComminutionProcessDefinition>) -> Self {
        let mut comminution = BTreeMap::new();
        for definition in definitions {
            let process = definition.process();
            assert!(
                comminution.insert(process, definition).is_none(),
                "duplicate comminution definition for process {}",
                process.value()
            );
        }
        Self { comminution }
    }

    #[must_use]
    pub fn get_comminution(&self, process: ProcessId) -> Option<ComminutionProcessDefinition> {
        self.comminution.get(&process).copied()
    }

    pub(crate) fn process_ids(&self) -> impl Iterator<Item = ProcessId> + '_ {
        self.comminution.keys().copied()
    }

    pub(crate) fn validate_references(
        &self,
        production: &ProductionRegistry,
        capabilities: &CapabilityRegistry,
        materials: &MaterialRegistry,
    ) {
        for definition in self.comminution.values().copied() {
            let process = match production.get_process(definition.process()) {
                Some(process) => process,
                None => panic!(
                    "comminution definition references missing process {}",
                    definition.process().value()
                ),
            };
            assert!(
                matches!(process.input_policy(), ProcessInputPolicy::SelectedBatch),
                "comminution process {} must use selected-batch input policy",
                definition.process().value()
            );
            let rate = match capabilities.get_capability(definition.mass_flow_capability()) {
                Some(capability) => capability,
                None => panic!(
                    "comminution process {} references missing mass-flow capability {}",
                    definition.process().value(),
                    definition.mass_flow_capability().value()
                ),
            };
            assert_eq!(
                rate.kind(),
                CapabilityValueKind::MassFlow,
                "comminution process {} throughput capability must be MassFlow",
                definition.process().value()
            );
            let maximum = match capabilities.get_capability(definition.max_batch_mass_capability())
            {
                Some(capability) => capability,
                None => panic!(
                    "comminution process {} references missing maximum-batch capability {}",
                    definition.process().value(),
                    definition.max_batch_mass_capability().value()
                ),
            };
            assert_eq!(
                maximum.kind(),
                CapabilityValueKind::Mass,
                "comminution process {} maximum-batch capability must be Mass",
                definition.process().value()
            );
            for (form, role) in [
                (definition.input_form(), "input"),
                (definition.output_form(), "output"),
            ] {
                let authored = match materials.get_form(form) {
                    Some(authored) => authored,
                    None => panic!(
                        "comminution process {} references missing {role} form {}",
                        definition.process().value(),
                        form.value()
                    ),
                };
                assert_eq!(
                    authored.phase(),
                    MaterialPhase::Solid,
                    "comminution process {} {role} form {} must be solid",
                    definition.process().value(),
                    form.value()
                );
            }
            let output_form = match materials.get_form(definition.output_form()) {
                Some(output_form) => output_form,
                None => unreachable!("comminution output form was resolved above"),
            };
            assert_eq!(
                output_form.particle_size_policy(),
                ParticleSizeStatePolicy::Required,
                "comminution process {} output form {} must require particle-size state",
                definition.process().value(),
                definition.output_form().value()
            );
        }
    }
}

/// Failure to convert material throughput into a whole authoritative tick span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MassFlowDurationError {
    ZeroRate,
    TickRangeExceeded,
}

impl Display for MassFlowDurationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroRate => formatter.write_str("material processing rate must be nonzero"),
            Self::TickRangeExceeded => {
                formatter.write_str("material processing duration exceeds authoritative tick range")
            }
        }
    }
}

impl Error for MassFlowDurationError {}

/// Returns the minimum whole tick span required to process an exact mass at a constant mass flow.
pub fn calculate_mass_flow_duration_ceiling(
    rate: MassFlow,
    mass: Mass,
    ticks_per_second: NonZeroU16,
) -> Result<TickSpan, MassFlowDurationError> {
    if rate.is_zero() {
        return Err(MassFlowDurationError::ZeroRate);
    }
    if mass.is_zero() {
        return Ok(TickSpan::ZERO);
    }
    let numerator = u128::from(mass.milligrams()) * u128::from(ticks_per_second.get());
    let denominator = u128::from(rate.milligrams_per_second());
    let ticks = 1 + (numerator - 1) / denominator;
    let ticks = u64::try_from(ticks).map_err(|_| MassFlowDurationError::TickRangeExceeded)?;
    Ok(TickSpan::new(ticks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mass_flow_duration_returns_first_tick_that_can_finish_batch() {
        let ticks_per_second = match NonZeroU16::new(20) {
            Some(value) => value,
            None => panic!("test tick rate must be nonzero"),
        };
        assert_eq!(
            calculate_mass_flow_duration_ceiling(
                MassFlow::from_milligrams_per_second(30),
                Mass::from_milligrams(3),
                ticks_per_second,
            ),
            Ok(TickSpan::new(2))
        );
        assert_eq!(
            calculate_mass_flow_duration_ceiling(
                MassFlow::from_milligrams_per_second(60),
                Mass::from_milligrams(3),
                ticks_per_second,
            ),
            Ok(TickSpan::new(1))
        );
    }

    #[test]
    fn mass_flow_duration_rejects_zero_rate_and_preserves_zero_mass() {
        let ticks_per_second = match NonZeroU16::new(20) {
            Some(value) => value,
            None => panic!("test tick rate must be nonzero"),
        };
        assert_eq!(
            calculate_mass_flow_duration_ceiling(
                MassFlow::ZERO,
                Mass::from_milligrams(1),
                ticks_per_second,
            ),
            Err(MassFlowDurationError::ZeroRate)
        );
        assert_eq!(
            calculate_mass_flow_duration_ceiling(
                MassFlow::from_milligrams_per_second(1),
                Mass::ZERO,
                ticks_per_second,
            ),
            Ok(TickSpan::ZERO)
        );
    }
}
