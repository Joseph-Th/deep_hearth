//! Exact particle-size screening resolution and persisted-job audit.

use crate::capability::evaluate_capabilities;
use crate::core::quantity::{Energy, Mass, MassFlow, Power};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{EnergyStoreId, calculate_mass_specific_energy, validate_energy_supply};
use crate::equipment::{EquipmentId, resolve_equipment_provider};
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::maintenance::Condition;
use crate::production::{
    ProcessId, ProcessResolution, ProductionJobRecord, validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::powered_physics::{
    PoweredOreBottleneck, PoweredOreEquipmentError, PoweredOreTimingError,
    classify_powered_ore_bottleneck, resolve_powered_ore_equipment, resolve_powered_ore_job_replay,
    resolve_powered_ore_timing, validate_powered_ore_job_replay,
};

mod errors;
mod outputs;

pub use errors::{ScreeningJobValidationError, ScreeningResolutionError};
pub use outputs::ScreeningBatchError;
use outputs::resolve_screening_outputs;

#[cfg(test)]
use crate::core::quantity::Temperature;
#[cfg(test)]
use crate::material::{
    CommodityKey, MaterialComposition, MaterialLotSpec, ParticleSizeDistribution, ParticleSizeRange,
};

/// Runtime request to classify one explicitly selected particulate batch by an authored aperture.
#[derive(Clone, Copy, Debug)]
pub struct ScreeningRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
}

impl<'selection> ScreeningRequest<'selection> {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        source: StockpileId,
        selections: &'selection [MaterialLotSelection],
        equipment: EquipmentId,
        energy_store: EnergyStoreId,
    ) -> Self {
        Self {
            process,
            source,
            selections,
            equipment,
            energy_store,
        }
    }
}

/// Fully resolved screening operation ready for the canonical production start transaction.
#[must_use]
#[derive(Debug)]
pub struct ResolvedScreening {
    resolution: ProcessResolution,
    equipment: ScreeningEquipmentProfile,
    constraints: ScreeningConstraintProfile,
    partition: ScreeningPartition,
}

#[derive(Debug)]
struct ScreeningEquipmentProfile {
    id: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
}

#[derive(Debug)]
struct ScreeningConstraintProfile {
    processing_rate: MassFlow,
    required_energy: Energy,
    available_power: Power,
    throughput_duration: TickSpan,
    energy_duration: TickSpan,
}

#[derive(Debug)]
struct ScreeningPartition {
    undersize_mass: Mass,
    oversize_mass: Mass,
}

impl ResolvedScreening {
    pub const fn process_resolution(&self) -> &ProcessResolution {
        &self.resolution
    }

    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment.id
    }

    #[must_use]
    pub const fn condition_before(&self) -> Condition {
        self.equipment.condition_before
    }

    #[must_use]
    pub const fn condition_after(&self) -> Condition {
        self.equipment.condition_after
    }

    #[must_use]
    pub const fn processing_rate(&self) -> MassFlow {
        self.constraints.processing_rate
    }

    #[must_use]
    pub const fn required_energy(&self) -> Energy {
        self.constraints.required_energy
    }

    #[must_use]
    pub const fn available_power(&self) -> Power {
        self.constraints.available_power
    }

    #[must_use]
    pub const fn throughput_duration(&self) -> TickSpan {
        self.constraints.throughput_duration
    }

    #[must_use]
    pub const fn energy_duration(&self) -> TickSpan {
        self.constraints.energy_duration
    }

    #[must_use]
    pub const fn undersize_mass(&self) -> Mass {
        self.partition.undersize_mass
    }

    #[must_use]
    pub const fn oversize_mass(&self) -> Mass {
        self.partition.oversize_mass
    }

    #[must_use]
    pub fn bottleneck(&self) -> PoweredOreBottleneck {
        classify_powered_ore_bottleneck(
            self.constraints.throughput_duration,
            self.constraints.energy_duration,
        )
    }
}

/// Resolves exact dry screening from selected particulate matter and runtime equipment.
///
/// Relative size-class weights are converted to whole-milligram stream masses only after identical
/// physical input profiles have been aggregated. This makes the result independent of lot
/// fragmentation. If the weighted partition is not exactly representable at whole-milligram mass
/// resolution, resolution is refused rather than silently reclassifying a fractional amount into the
/// wrong particle-size stream.
pub fn resolve_screening_process(
    registries: &Registries,
    state: &AppState,
    request: ScreeningRequest<'_>,
) -> Result<ResolvedScreening, ScreeningResolutionError> {
    let ScreeningRequest {
        process,
        source,
        selections,
        equipment,
        energy_store,
    } = request;
    let definition = registries
        .ore_processing()
        .get_screening(process)
        .ok_or(ScreeningResolutionError::UnknownScreeningProcess { process })?;
    let inputs = validate_selected_process_inputs(registries, state, process, source, selections)
        .map_err(ScreeningResolutionError::Input)?;
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(ScreeningResolutionError::Equipment)?;
    let process_definition = registries
        .production()
        .get_process(process)
        .ok_or(ScreeningResolutionError::UnknownScreeningProcess { process })?;
    evaluate_capabilities(
        registries.capabilities(),
        &provider,
        process_definition.capability_requirements(),
    )
    .map_err(ScreeningResolutionError::Capability)?;

    let selected_mass = inputs.input_mass();
    let powered_equipment = resolve_powered_ore_equipment(
        provider.definition(),
        provider.condition(),
        definition.mass_flow_capability(),
        definition.max_batch_mass_capability(),
        selected_mass,
    )
    .map_err(|error| match error {
        PoweredOreEquipmentError::MissingMassFlowCapability => {
            ScreeningResolutionError::MissingMassFlowCapability
        }
        PoweredOreEquipmentError::MissingMaximumBatchMassCapability => {
            ScreeningResolutionError::MissingMaximumBatchMassCapability
        }
        PoweredOreEquipmentError::BatchMassExceeded { selected, maximum } => {
            ScreeningResolutionError::BatchMassExceeded { selected, maximum }
        }
    })?;
    let processing_rate = powered_equipment.processing_rate();

    let outputs = resolve_screening_outputs(definition, inputs.consumed_inputs())
        .map_err(ScreeningResolutionError::Batch)?;
    let required_energy =
        calculate_mass_specific_energy(selected_mass, definition.specific_energy());
    let energy_supply = validate_energy_supply(registries, state, energy_store, required_energy)
        .map_err(ScreeningResolutionError::Energy)?;
    let provided_carrier = energy_supply.trace().carrier();
    if provided_carrier != definition.energy_carrier() {
        return Err(ScreeningResolutionError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: provided_carrier,
        });
    }
    let available_power = energy_supply.max_output_power();
    let timing = resolve_powered_ore_timing(
        registries,
        processing_rate,
        selected_mass,
        required_energy,
        available_power,
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    )
    .map_err(|error| match error {
        PoweredOreTimingError::Throughput(error) => {
            ScreeningResolutionError::ThroughputDuration(error)
        }
        PoweredOreTimingError::Energy(error) => ScreeningResolutionError::EnergyDuration(error),
        PoweredOreTimingError::Condition(error) => {
            ScreeningResolutionError::ConditionDuration(error)
        }
    })?;
    let throughput_duration = timing.throughput_duration();
    let energy_duration = timing.energy_duration();
    let duration = timing.duration();
    let condition_after = timing.condition_after();
    let equipment_use = provider.validated_use();
    let resolution = inputs
        .resolve_with_energy_and_equipment(
            duration,
            outputs.streams,
            energy_supply,
            equipment_use,
            condition_after,
        )
        .map_err(ScreeningResolutionError::Resolution)?;

    Ok(ResolvedScreening {
        resolution,
        equipment: ScreeningEquipmentProfile {
            id: equipment,
            condition_before: provider.condition(),
            condition_after,
        },
        constraints: ScreeningConstraintProfile {
            processing_rate,
            required_energy,
            available_power,
            throughput_duration,
            energy_duration,
        },
        partition: ScreeningPartition {
            undersize_mass: outputs.undersize_mass,
            oversize_mass: outputs.oversize_mass,
        },
    })
}

pub(crate) fn validate_loaded_screening_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), ScreeningJobValidationError> {
    let Some(definition) = registries.ore_processing().get_screening(job.process()) else {
        return Ok(());
    };
    let replay = resolve_powered_ore_job_replay(registries, job, definition.operating_profile())
        .map_err(|error| ScreeningJobValidationError::Powered {
            job: job.id(),
            error,
        })?;
    let expected =
        resolve_screening_outputs(definition, job.consumed_inputs()).map_err(|error| {
            ScreeningJobValidationError::Batch {
                job: job.id(),
                error,
            }
        })?;
    if job.output_streams().len() != expected.streams.len() {
        return Err(ScreeningJobValidationError::OutputMismatch { job: job.id() });
    }
    for expected_stream in &expected.streams {
        let Some(stored_stream) = job
            .output_streams()
            .iter()
            .find(|stream| stream.id() == expected_stream.id())
        else {
            return Err(ScreeningJobValidationError::OutputMismatch { job: job.id() });
        };
        if stored_stream.outputs() != expected_stream.outputs() {
            return Err(ScreeningJobValidationError::OutputMismatch { job: job.id() });
        }
    }
    validate_powered_ore_job_replay(registries, job, replay).map_err(|error| {
        ScreeningJobValidationError::Powered {
            job: job.id(),
            error,
        }
    })
}

#[cfg(test)]
#[path = "screening_execution_tests.rs"]
mod tests;
