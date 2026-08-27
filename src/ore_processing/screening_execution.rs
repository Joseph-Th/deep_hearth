//! Exact particle-size screening resolution and persisted-job audit.

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
    ProcessId, ProcessInputError, ProcessResolution, ProcessResolutionError, ProductionJobId,
    ProductionJobRecord, validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::MassFlowDurationError;
use super::powered_physics::{
    PoweredOreEquipmentError, PoweredOreTimingError, resolve_powered_ore_equipment,
    resolve_powered_ore_timing,
};

mod outputs;

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

/// Failure while resolving one exact screening operation before authoritative mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreeningResolutionError {
    UnknownScreeningProcess {
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
    Batch(ScreeningBatchError),
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

impl Display for ScreeningResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownScreeningProcess { process } => write!(
                formatter,
                "process {} has no authored screening semantics",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "screening input selection failed: {error}"),
            Self::Equipment(error) => write!(formatter, "screening equipment failed: {error}"),
            Self::Capability(error) => write!(
                formatter,
                "screening capability requirement failed: {error}"
            ),
            Self::MissingMassFlowCapability => {
                formatter.write_str("screening equipment has no usable mass-flow capability")
            }
            Self::MissingMaximumBatchMassCapability => formatter
                .write_str("screening equipment has no usable maximum-batch-mass capability"),
            Self::BatchMassExceeded { selected, maximum } => write!(
                formatter,
                "selected screening batch {} mg exceeds equipment maximum {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch(error) => write!(formatter, "screening batch resolution failed: {error}"),
            Self::Energy(error) => write!(formatter, "screening energy supply failed: {error}"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "screening requires {required:?} energy but selected source provides {provided:?}"
            ),
            Self::ThroughputDuration(error) => {
                write!(formatter, "screening throughput duration failed: {error}")
            }
            Self::EnergyDuration(error) => {
                write!(
                    formatter,
                    "screening energy delivery duration failed: {error}"
                )
            }
            Self::ConditionDuration(error) => {
                write!(
                    formatter,
                    "screening exceeds equipment condition lifetime: {error}"
                )
            }
            Self::Resolution(error) => {
                write!(formatter, "screening process resolution failed: {error}")
            }
        }
    }
}

impl Error for ScreeningResolutionError {
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
            Self::UnknownScreeningProcess { process: _process } => None,
            Self::BatchMassExceeded {
                selected: _selected,
                maximum: _maximum,
            } => None,
            Self::WrongEnergyCarrier {
                required: _required,
                provided: _provided,
            } => None,
            Self::MissingMassFlowCapability | Self::MissingMaximumBatchMassCapability => None,
        }
    }
}

/// Physical rate constraint that determines one resolved screening duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreeningBottleneck {
    Throughput,
    EnergyDelivery,
    Balanced,
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
    pub fn bottleneck(&self) -> ScreeningBottleneck {
        match self
            .constraints
            .throughput_duration
            .cmp(&self.constraints.energy_duration)
        {
            std::cmp::Ordering::Greater => ScreeningBottleneck::Throughput,
            std::cmp::Ordering::Less => ScreeningBottleneck::EnergyDelivery,
            std::cmp::Ordering::Equal => ScreeningBottleneck::Balanced,
        }
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

/// Persistent-state failure found while recomputing an in-flight screening job from its traces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreeningJobValidationError {
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
        error: ScreeningBatchError,
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

impl Display for ScreeningJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEnergy { job } => write!(
                formatter,
                "screening job {} has no consumed work-energy trace",
                job.value()
            ),
            Self::UnexpectedReleasedEnergy { job } => write!(
                formatter,
                "screening job {} contains an energy output not authorized by its resolver",
                job.value()
            ),
            Self::MissingEquipmentProvider { job } => write!(
                formatter,
                "screening job {} has no occupied equipment provider",
                job.value()
            ),
            Self::UnknownEquipmentDefinition { job } => write!(
                formatter,
                "screening job {} references an unknown equipment definition",
                job.value()
            ),
            Self::UnknownEnergyDefinition { job } => write!(
                formatter,
                "screening job {} references an unknown energy-store definition",
                job.value()
            ),
            Self::MissingMassFlowCapability { job } => write!(
                formatter,
                "screening job {} equipment lacks its authored mass-flow capability",
                job.value()
            ),
            Self::MissingMaximumBatchMassCapability { job } => write!(
                formatter,
                "screening job {} equipment lacks its authored maximum-batch capability",
                job.value()
            ),
            Self::BatchMassExceeded {
                job,
                selected,
                maximum,
            } => write!(
                formatter,
                "screening job {} selected {} mg above its persisted equipment maximum {} mg",
                job.value(),
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch { job, error } => write!(
                formatter,
                "screening job {} has invalid batch physics: {error}",
                job.value()
            ),
            Self::WrongEnergyCarrier {
                job,
                required,
                provided,
            } => write!(
                formatter,
                "screening job {} requires {required:?} energy but traces {provided:?}",
                job.value()
            ),
            Self::EnergyMismatch {
                job,
                traced,
                required,
            } => write!(
                formatter,
                "screening job {} traces {} nJ but mass-specific work requires {} nJ",
                job.value(),
                traced.nanojoules(),
                required.nanojoules()
            ),
            Self::ThroughputDuration { job, error } => write!(
                formatter,
                "screening job {} cannot recompute throughput duration: {error}",
                job.value()
            ),
            Self::EnergyDuration { job, error } => write!(
                formatter,
                "screening job {} cannot recompute work-energy delivery duration: {error}",
                job.value()
            ),
            Self::ConditionDuration { job, error } => write!(
                formatter,
                "screening job {} exceeds equipment condition lifetime: {error}",
                job.value()
            ),
            Self::DurationMismatch {
                job,
                stored_ticks,
                required_ticks,
            } => write!(
                formatter,
                "screening job {} stores duration {stored_ticks} ticks but physics require {required_ticks}",
                job.value()
            ),
            Self::MissingConditionOutcome { job } => write!(
                formatter,
                "screening job {} has no persisted equipment condition outcome",
                job.value()
            ),
            Self::ConditionOutcomeMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "screening job {} stores equipment condition {} ppm but physics require {} ppm",
                job.value(),
                stored.parts_per_million(),
                required.parts_per_million()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "screening job {} output snapshot no longer matches its consumed material traces",
                job.value()
            ),
        }
    }
}

impl Error for ScreeningJobValidationError {
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

pub(crate) fn validate_loaded_screening_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), ScreeningJobValidationError> {
    let Some(definition) = registries.ore_processing().get_screening(job.process()) else {
        return Ok(());
    };
    let consumed_energy = job
        .consumed_energy()
        .ok_or(ScreeningJobValidationError::MissingEnergy { job: job.id() })?;
    if job.released_energy().is_some() {
        return Err(ScreeningJobValidationError::UnexpectedReleasedEnergy { job: job.id() });
    }
    let provider = job
        .equipment_provider()
        .ok_or(ScreeningJobValidationError::MissingEquipmentProvider { job: job.id() })?;
    let equipment_definition = registries
        .equipment()
        .get_equipment(provider.definition())
        .ok_or(ScreeningJobValidationError::UnknownEquipmentDefinition { job: job.id() })?;
    let energy_definition = registries
        .energy()
        .get_store(consumed_energy.definition())
        .ok_or(ScreeningJobValidationError::UnknownEnergyDefinition { job: job.id() })?;
    let powered_equipment = resolve_powered_ore_equipment(
        equipment_definition,
        provider.condition(),
        definition.mass_flow_capability(),
        definition.max_batch_mass_capability(),
        job.consumed_mass(),
    )
    .map_err(|error| match error {
        PoweredOreEquipmentError::MissingMassFlowCapability => {
            ScreeningJobValidationError::MissingMassFlowCapability { job: job.id() }
        }
        PoweredOreEquipmentError::MissingMaximumBatchMassCapability => {
            ScreeningJobValidationError::MissingMaximumBatchMassCapability { job: job.id() }
        }
        PoweredOreEquipmentError::BatchMassExceeded { selected, maximum } => {
            ScreeningJobValidationError::BatchMassExceeded {
                job: job.id(),
                selected,
                maximum,
            }
        }
    })?;
    let processing_rate = powered_equipment.processing_rate();
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
    if consumed_energy.carrier() != definition.energy_carrier() {
        return Err(ScreeningJobValidationError::WrongEnergyCarrier {
            job: job.id(),
            required: definition.energy_carrier(),
            provided: consumed_energy.carrier(),
        });
    }
    let required_energy =
        calculate_mass_specific_energy(job.consumed_mass(), definition.specific_energy());
    if consumed_energy.energy() != required_energy {
        return Err(ScreeningJobValidationError::EnergyMismatch {
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
            ScreeningJobValidationError::ThroughputDuration {
                job: job.id(),
                error,
            }
        }
        PoweredOreTimingError::Energy(error) => ScreeningJobValidationError::EnergyDuration {
            job: job.id(),
            error,
        },
        PoweredOreTimingError::Condition(error) => ScreeningJobValidationError::ConditionDuration {
            job: job.id(),
            error,
        },
    })?;
    let required_duration = timing.duration();
    let stored_duration = job.active_duration().value();
    if stored_duration != required_duration.value() {
        return Err(ScreeningJobValidationError::DurationMismatch {
            job: job.id(),
            stored_ticks: stored_duration,
            required_ticks: required_duration.value(),
        });
    }
    let required_condition_after = timing.condition_after();
    let stored_condition_after = job
        .equipment_condition_after()
        .ok_or(ScreeningJobValidationError::MissingConditionOutcome { job: job.id() })?;
    if stored_condition_after != required_condition_after {
        return Err(ScreeningJobValidationError::ConditionOutcomeMismatch {
            job: job.id(),
            stored: stored_condition_after,
            required: required_condition_after,
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "screening_execution_tests.rs"]
mod tests;
