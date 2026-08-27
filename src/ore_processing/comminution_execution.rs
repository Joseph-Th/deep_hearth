//! Exact comminution resolution and persisted-job audit for the sibling ore-processing definitions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityEvaluationError, evaluate_capabilities};
use crate::core::quantity::{Energy, Mass, MassFlow, Power};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyCarrier, EnergyStoreId, EnergySupplyError, PowerDurationError,
    calculate_mass_specific_energy, validate_energy_supply,
};
use crate::equipment::{EquipmentId, EquipmentProviderError, resolve_equipment_provider};
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::production::{
    ProcessId, ProcessInputError, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    ProcessResolutionError, ProductionJobId, ProductionJobRecord, validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::MassFlowDurationError;
use super::powered_physics::{
    PoweredOreEquipmentError, PoweredOreTimingError, resolve_powered_ore_equipment,
    resolve_powered_ore_timing,
};

mod outputs;

pub use outputs::ComminutionBatchError;
use outputs::resolve_comminution_outputs;

#[cfg(test)]
use crate::core::quantity::Temperature;
#[cfg(test)]
use crate::material::{CommodityKey, MaterialComposition, MaterialLotSpec, ParticleSizeRange};

/// Runtime request to reduce one explicitly selected solid batch to an authored finer form.
#[derive(Clone, Copy, Debug)]
pub struct ComminutionRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
}

impl<'selection> ComminutionRequest<'selection> {
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

/// Failure while resolving one exact comminution operation before any authoritative mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComminutionResolutionError {
    UnknownComminutionProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingMassFlowCapability,
    MissingMaximumBatchMassCapability,
    BatchMassExceeded {
        selected: Mass,
        maximum: Mass,
    },
    Batch(ComminutionBatchError),
    Energy(EnergySupplyError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    ThroughputDuration(MassFlowDurationError),
    EnergyDuration(PowerDurationError),
    ConditionDuration(ActiveConditionDurationError),
    Resolution(ProcessResolutionError),
}

impl Display for ComminutionResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownComminutionProcess { process } => write!(
                formatter,
                "process {} has no authored comminution semantics",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "comminution input selection failed: {error}"),
            Self::Equipment(error) => write!(formatter, "comminution equipment failed: {error}"),
            Self::Capability(error) => {
                write!(
                    formatter,
                    "comminution capability requirement failed: {error}"
                )
            }
            Self::MissingMassFlowCapability => {
                formatter.write_str("comminution equipment has no usable mass-flow capability")
            }
            Self::MissingMaximumBatchMassCapability => formatter
                .write_str("comminution equipment has no usable maximum-batch-mass capability"),
            Self::BatchMassExceeded { selected, maximum } => write!(
                formatter,
                "selected comminution batch {} mg exceeds equipment maximum {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch(error) => write!(formatter, "comminution batch resolution failed: {error}"),
            Self::Energy(error) => write!(formatter, "comminution energy supply failed: {error}"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "comminution requires {required:?} energy but selected source provides {provided:?}"
            ),
            Self::ThroughputDuration(error) => {
                write!(formatter, "comminution throughput duration failed: {error}")
            }
            Self::EnergyDuration(error) => {
                write!(
                    formatter,
                    "comminution energy delivery duration failed: {error}"
                )
            }
            Self::ConditionDuration(error) => {
                write!(
                    formatter,
                    "comminution exceeds equipment condition lifetime: {error}"
                )
            }
            Self::Resolution(error) => {
                write!(formatter, "comminution process resolution failed: {error}")
            }
        }
    }
}

impl Error for ComminutionResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Batch(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::ThroughputDuration(error) => Some(error),
            Self::EnergyDuration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownComminutionProcess { process: _process } => None,
            Self::MissingMassFlowCapability | Self::MissingMaximumBatchMassCapability => None,
            Self::BatchMassExceeded {
                selected: _selected,
                maximum: _maximum,
            } => None,
            Self::WrongEnergyCarrier {
                required: _required,
                provided: _provided,
            } => None,
        }
    }
}

/// Authoritative rate constraint that determines one resolved comminution duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComminutionBottleneck {
    Throughput,
    EnergyDelivery,
    Balanced,
}

/// Fully resolved comminution operation ready for the canonical production start transaction.
#[must_use]
#[derive(Debug)]
pub struct ResolvedComminution {
    resolution: ProcessResolution,
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
    processing_rate: MassFlow,
    required_energy: Energy,
    available_power: Power,
    throughput_duration: TickSpan,
    energy_duration: TickSpan,
}

impl ResolvedComminution {
    pub const fn process_resolution(&self) -> &ProcessResolution {
        &self.resolution
    }

    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
    }

    /// Equipment condition observed when this operation was resolved.
    #[must_use]
    pub const fn condition_before(&self) -> Condition {
        self.condition_before
    }

    /// Predicted equipment condition after the resolved active duration completes.
    #[must_use]
    pub const fn condition_after(&self) -> Condition {
        self.condition_after
    }

    #[must_use]
    pub const fn processing_rate(&self) -> MassFlow {
        self.processing_rate
    }

    #[must_use]
    pub const fn required_energy(&self) -> Energy {
        self.required_energy
    }

    #[must_use]
    pub const fn available_power(&self) -> Power {
        self.available_power
    }

    /// Duration imposed by condition-adjusted equipment material throughput alone.
    #[must_use]
    pub const fn throughput_duration(&self) -> TickSpan {
        self.throughput_duration
    }

    /// Duration imposed by the selected finite energy source's delivery power alone.
    #[must_use]
    pub const fn energy_duration(&self) -> TickSpan {
        self.energy_duration
    }

    /// Reports which physical rate constraint currently determines authoritative duration.
    #[must_use]
    pub fn bottleneck(&self) -> ComminutionBottleneck {
        match self.throughput_duration.cmp(&self.energy_duration) {
            std::cmp::Ordering::Greater => ComminutionBottleneck::Throughput,
            std::cmp::Ordering::Less => ComminutionBottleneck::EnergyDelivery,
            std::cmp::Ordering::Equal => ComminutionBottleneck::Balanced,
        }
    }
}

/// Resolves exact crushing/grinding behavior from selected solid matter and runtime equipment.
///
/// Comminution assigns an authored weighted particle-size distribution while preserving each
/// distinct composition and temperature. Particulate inputs must be strictly reduced at the
/// distribution envelope without coarsening represented fines, and constrained operations require
/// every selected feed envelope to lie inside their authored operating range. Untracked coarse inputs
/// establish their first explicit size state. It does not purify ore or invent yield bonuses. Exact
/// mass-specific work is reserved from a finite energy source, while operation duration is the
/// slower of equipment throughput and source power.
pub fn resolve_comminution_process(
    registries: &Registries,
    state: &AppState,
    request: ComminutionRequest<'_>,
) -> Result<ResolvedComminution, ComminutionResolutionError> {
    let ComminutionRequest {
        process,
        source,
        selections,
        equipment,
        energy_store,
    } = request;
    let definition = registries
        .ore_processing()
        .get_comminution(process)
        .ok_or(ComminutionResolutionError::UnknownComminutionProcess { process })?;
    let inputs = validate_selected_process_inputs(registries, state, process, source, selections)
        .map_err(ComminutionResolutionError::Input)?;
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(ComminutionResolutionError::Equipment)?;
    let process_definition = match registries.production().get_process(process) {
        Some(definition) => definition,
        None => {
            return Err(ComminutionResolutionError::UnknownComminutionProcess { process });
        }
    };
    evaluate_capabilities(
        registries.capabilities(),
        &provider,
        process_definition.capability_requirements(),
    )
    .map_err(ComminutionResolutionError::Capability)?;

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
            ComminutionResolutionError::MissingMassFlowCapability
        }
        PoweredOreEquipmentError::MissingMaximumBatchMassCapability => {
            ComminutionResolutionError::MissingMaximumBatchMassCapability
        }
        PoweredOreEquipmentError::BatchMassExceeded { selected, maximum } => {
            ComminutionResolutionError::BatchMassExceeded { selected, maximum }
        }
    })?;
    let processing_rate = powered_equipment.processing_rate();
    let outputs = resolve_comminution_outputs(definition, inputs.consumed_inputs())
        .map_err(ComminutionResolutionError::Batch)?;
    let required_energy =
        calculate_mass_specific_energy(selected_mass, definition.specific_energy());
    let energy_supply = validate_energy_supply(registries, state, energy_store, required_energy)
        .map_err(ComminutionResolutionError::Energy)?;
    let provided_carrier = energy_supply.trace().carrier();
    if provided_carrier != definition.energy_carrier() {
        return Err(ComminutionResolutionError::WrongEnergyCarrier {
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
            ComminutionResolutionError::ThroughputDuration(error)
        }
        PoweredOreTimingError::Energy(error) => ComminutionResolutionError::EnergyDuration(error),
        PoweredOreTimingError::Condition(error) => {
            ComminutionResolutionError::ConditionDuration(error)
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
            vec![ProcessOutputStream::new(
                ProcessOutputStreamId::PRIMARY,
                outputs,
            )],
            energy_supply,
            equipment_use,
            condition_after,
        )
        .map_err(ComminutionResolutionError::Resolution)?;

    Ok(ResolvedComminution {
        resolution,
        equipment,
        condition_before: provider.condition(),
        condition_after,
        processing_rate,
        required_energy,
        available_power,
        throughput_duration,
        energy_duration,
    })
}

/// Persistent-state failure found while recomputing an in-flight comminution job from its traces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComminutionJobValidationError {
    MissingEnergy {
        job: ProductionJobId,
    },
    UnexpectedReleasedEnergy {
        job: ProductionJobId,
    },
    MissingEquipmentProvider {
        job: ProductionJobId,
    },
    UnknownEquipmentDefinition {
        job: ProductionJobId,
    },
    UnknownEnergyDefinition {
        job: ProductionJobId,
    },
    MissingMassFlowCapability {
        job: ProductionJobId,
    },
    MissingMaximumBatchMassCapability {
        job: ProductionJobId,
    },
    BatchMassExceeded {
        job: ProductionJobId,
        selected: Mass,
        maximum: Mass,
    },
    Batch {
        job: ProductionJobId,
        error: ComminutionBatchError,
    },
    WrongEnergyCarrier {
        job: ProductionJobId,
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    EnergyMismatch {
        job: ProductionJobId,
        traced: Energy,
        required: Energy,
    },
    ThroughputDuration {
        job: ProductionJobId,
        error: MassFlowDurationError,
    },
    EnergyDuration {
        job: ProductionJobId,
        error: PowerDurationError,
    },
    ConditionDuration {
        job: ProductionJobId,
        error: ActiveConditionDurationError,
    },
    DurationMismatch {
        job: ProductionJobId,
        stored_ticks: u64,
        required_ticks: u64,
    },
    MissingConditionOutcome {
        job: ProductionJobId,
    },
    ConditionOutcomeMismatch {
        job: ProductionJobId,
        stored: Condition,
        required: Condition,
    },
    OutputMismatch {
        job: ProductionJobId,
    },
}

impl Display for ComminutionJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEnergy { job } => write!(
                formatter,
                "comminution job {} has no consumed work-energy trace",
                job.value()
            ),
            Self::UnexpectedReleasedEnergy { job } => write!(
                formatter,
                "comminution job {} contains an energy output not authorized by its resolver",
                job.value()
            ),
            Self::MissingEquipmentProvider { job } => write!(
                formatter,
                "comminution job {} has no occupied equipment provider",
                job.value()
            ),
            Self::UnknownEquipmentDefinition { job } => write!(
                formatter,
                "comminution job {} references an unknown equipment definition",
                job.value()
            ),
            Self::UnknownEnergyDefinition { job } => write!(
                formatter,
                "comminution job {} references an unknown energy-store definition",
                job.value()
            ),
            Self::MissingMassFlowCapability { job } => write!(
                formatter,
                "comminution job {} equipment lacks its authored mass-flow capability",
                job.value()
            ),
            Self::MissingMaximumBatchMassCapability { job } => write!(
                formatter,
                "comminution job {} equipment lacks its authored maximum-batch capability",
                job.value()
            ),
            Self::BatchMassExceeded {
                job,
                selected,
                maximum,
            } => write!(
                formatter,
                "comminution job {} selected {} mg above its persisted equipment maximum {} mg",
                job.value(),
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch { job, error } => write!(
                formatter,
                "comminution job {} has invalid batch physics: {error}",
                job.value()
            ),
            Self::WrongEnergyCarrier {
                job,
                required,
                provided,
            } => write!(
                formatter,
                "comminution job {} requires {required:?} energy but traces {provided:?}",
                job.value()
            ),
            Self::EnergyMismatch {
                job,
                traced,
                required,
            } => write!(
                formatter,
                "comminution job {} traces {} nJ but mass-specific work requires {} nJ",
                job.value(),
                traced.nanojoules(),
                required.nanojoules()
            ),
            Self::ThroughputDuration { job, error } => write!(
                formatter,
                "comminution job {} cannot recompute throughput duration: {error}",
                job.value()
            ),
            Self::EnergyDuration { job, error } => write!(
                formatter,
                "comminution job {} cannot recompute work-energy delivery duration: {error}",
                job.value()
            ),
            Self::ConditionDuration { job, error } => write!(
                formatter,
                "comminution job {} exceeds equipment condition lifetime: {error}",
                job.value()
            ),
            Self::DurationMismatch {
                job,
                stored_ticks,
                required_ticks,
            } => write!(
                formatter,
                "comminution job {} stores duration {stored_ticks} ticks but physics require {required_ticks}",
                job.value()
            ),
            Self::MissingConditionOutcome { job } => write!(
                formatter,
                "comminution job {} has no persisted equipment condition outcome",
                job.value()
            ),
            Self::ConditionOutcomeMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "comminution job {} stores equipment condition {} ppm but physics require {} ppm",
                job.value(),
                stored.parts_per_million(),
                required.parts_per_million()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "comminution job {} output snapshot no longer matches its consumed material traces",
                job.value()
            ),
        }
    }
}

impl Error for ComminutionJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Batch { job: _job, error } => Some(error),
            Self::ThroughputDuration { job: _job, error } => Some(error),
            Self::EnergyDuration { job: _job, error } => Some(error),
            Self::ConditionDuration { job: _job, error } => Some(error),
            Self::MissingEnergy { job: _job }
            | Self::UnexpectedReleasedEnergy { job: _job }
            | Self::MissingEquipmentProvider { job: _job }
            | Self::UnknownEquipmentDefinition { job: _job }
            | Self::UnknownEnergyDefinition { job: _job }
            | Self::MissingMassFlowCapability { job: _job }
            | Self::MissingMaximumBatchMassCapability { job: _job }
            | Self::MissingConditionOutcome { job: _job }
            | Self::OutputMismatch { job: _job } => None,
            Self::BatchMassExceeded {
                job: _job,
                selected: _selected,
                maximum: _maximum,
            } => None,
            Self::WrongEnergyCarrier {
                job: _job,
                required: _required,
                provided: _provided,
            } => None,
            Self::EnergyMismatch {
                job: _job,
                traced: _traced,
                required: _required,
            } => None,
            Self::DurationMismatch {
                job: _job,
                stored_ticks: _stored_ticks,
                required_ticks: _required_ticks,
            } => None,
            Self::ConditionOutcomeMismatch {
                job: _job,
                stored: _stored,
                required: _required,
            } => None,
        }
    }
}

pub(crate) fn validate_loaded_comminution_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), ComminutionJobValidationError> {
    let Some(definition) = registries.ore_processing().get_comminution(job.process()) else {
        return Ok(());
    };
    let consumed_energy = job
        .consumed_energy()
        .ok_or(ComminutionJobValidationError::MissingEnergy { job: job.id() })?;
    if job.released_energy().is_some() {
        return Err(ComminutionJobValidationError::UnexpectedReleasedEnergy { job: job.id() });
    }
    let provider = job
        .equipment_provider()
        .ok_or(ComminutionJobValidationError::MissingEquipmentProvider { job: job.id() })?;
    let equipment_definition = registries
        .equipment()
        .get_equipment(provider.definition())
        .ok_or(ComminutionJobValidationError::UnknownEquipmentDefinition { job: job.id() })?;
    let energy_definition = registries
        .energy()
        .get_store(consumed_energy.definition())
        .ok_or(ComminutionJobValidationError::UnknownEnergyDefinition { job: job.id() })?;
    let powered_equipment = resolve_powered_ore_equipment(
        equipment_definition,
        provider.condition(),
        definition.mass_flow_capability(),
        definition.max_batch_mass_capability(),
        job.consumed_mass(),
    )
    .map_err(|error| match error {
        PoweredOreEquipmentError::MissingMassFlowCapability => {
            ComminutionJobValidationError::MissingMassFlowCapability { job: job.id() }
        }
        PoweredOreEquipmentError::MissingMaximumBatchMassCapability => {
            ComminutionJobValidationError::MissingMaximumBatchMassCapability { job: job.id() }
        }
        PoweredOreEquipmentError::BatchMassExceeded { selected, maximum } => {
            ComminutionJobValidationError::BatchMassExceeded {
                job: job.id(),
                selected,
                maximum,
            }
        }
    })?;
    let processing_rate = powered_equipment.processing_rate();
    let required_outputs =
        resolve_comminution_outputs(definition, job.consumed_inputs()).map_err(|error| {
            ComminutionJobValidationError::Batch {
                job: job.id(),
                error,
            }
        })?;
    let Some(output_stream) = job.single_output_stream() else {
        return Err(ComminutionJobValidationError::OutputMismatch { job: job.id() });
    };
    if required_outputs.as_slice() != output_stream.outputs() {
        return Err(ComminutionJobValidationError::OutputMismatch { job: job.id() });
    }
    if consumed_energy.carrier() != definition.energy_carrier() {
        return Err(ComminutionJobValidationError::WrongEnergyCarrier {
            job: job.id(),
            required: definition.energy_carrier(),
            provided: consumed_energy.carrier(),
        });
    }
    let required_energy =
        calculate_mass_specific_energy(job.consumed_mass(), definition.specific_energy());
    if consumed_energy.energy() != required_energy {
        return Err(ComminutionJobValidationError::EnergyMismatch {
            job: job.id(),
            traced: consumed_energy.energy(),
            required: required_energy,
        });
    }
    let timing = resolve_powered_ore_timing(
        registries,
        processing_rate,
        job.consumed_mass(),
        required_energy,
        energy_definition.max_output_power(),
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    )
    .map_err(|error| match error {
        PoweredOreTimingError::Throughput(error) => {
            ComminutionJobValidationError::ThroughputDuration {
                job: job.id(),
                error,
            }
        }
        PoweredOreTimingError::Energy(error) => ComminutionJobValidationError::EnergyDuration {
            job: job.id(),
            error,
        },
        PoweredOreTimingError::Condition(error) => {
            ComminutionJobValidationError::ConditionDuration {
                job: job.id(),
                error,
            }
        }
    })?;
    let required_duration = timing.duration();
    let stored_duration = job.active_duration().value();
    if stored_duration != required_duration.value() {
        return Err(ComminutionJobValidationError::DurationMismatch {
            job: job.id(),
            stored_ticks: stored_duration,
            required_ticks: required_duration.value(),
        });
    }
    let required_condition_after = timing.condition_after();
    let stored_condition_after = job
        .equipment_condition_after()
        .ok_or(ComminutionJobValidationError::MissingConditionOutcome { job: job.id() })?;
    if stored_condition_after != required_condition_after {
        return Err(ComminutionJobValidationError::ConditionOutcomeMismatch {
            job: job.id(),
            stored: stored_condition_after,
            required: required_condition_after,
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "comminution_execution_tests.rs"]
mod tests;
