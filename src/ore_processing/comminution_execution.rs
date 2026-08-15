//! Exact comminution resolution and persisted-job audit for the sibling ore-processing definitions.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityEvaluationError, CapabilityValue, evaluate_capabilities};
use crate::core::quantity::{Energy, Mass, MassFlow, Power, Temperature};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyCarrier, EnergyStoreId, EnergySupplyError, PowerDurationError,
    calculate_mass_specific_energy, calculate_power_duration_ceiling, validate_energy_supply,
};
use crate::equipment::{
    EquipmentId, EquipmentProviderError, resolve_equipment_capability, resolve_equipment_provider,
};
use crate::inventory::{ConsumedMaterialTrace, MaterialLotSelection, StockpileId};
use crate::maintenance::{Condition, calculate_condition_after_active_ticks};
use crate::material::{
    CommodityKey, FormId, MaterialComposition, MaterialLotSpec, MaterialLotSpecError,
    ParticleSizeRange,
};
use crate::production::{
    ProcessId, ProcessInputError, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    ProcessResolutionError, ProductionJobId, ProductionJobRecord, validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::{
    ComminutionProcessDefinition, MassFlowDurationError, calculate_mass_flow_duration_ceiling,
};

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

/// Failure while mapping exact selected material traces to comminuted output specifications.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComminutionBatchError {
    EmptyInput,
    InputFormMismatch {
        expected: FormId,
        found: FormId,
    },
    ParticleSizeNotReduced {
        input: ParticleSizeRange,
        output: ParticleSizeRange,
    },
    MassOverflow,
    Output(MaterialLotSpecError),
}

impl Display for ComminutionBatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("comminution batch contains no material"),
            Self::InputFormMismatch { expected, found } => write!(
                formatter,
                "comminution batch requires input form {} but selected form {}",
                expected.value(),
                found.value()
            ),
            Self::ParticleSizeNotReduced { input, output } => write!(
                formatter,
                "comminution output {}..={} um does not strictly reduce input {}..={} um without coarsening fines",
                output.minimum_diameter().micrometers(),
                output.maximum_diameter().micrometers(),
                input.minimum_diameter().micrometers(),
                input.maximum_diameter().micrometers()
            ),
            Self::MassOverflow => formatter.write_str("comminution output mass overflowed"),
            Self::Output(error) => write!(
                formatter,
                "comminution output specification could not preserve its material profile: {error}"
            ),
        }
    }
}

impl Error for ComminutionBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Output(error) => Some(error),
            Self::EmptyInput
            | Self::InputFormMismatch { .. }
            | Self::ParticleSizeNotReduced { .. }
            | Self::MassOverflow => None,
        }
    }
}

fn resolve_comminution_outputs(
    definition: ComminutionProcessDefinition,
    traces: &[ConsumedMaterialTrace],
) -> Result<Vec<MaterialLotSpec>, ComminutionBatchError> {
    if traces.is_empty() {
        return Err(ComminutionBatchError::EmptyInput);
    }

    let mut grouped = BTreeMap::<(CommodityKey, Temperature, MaterialComposition), Mass>::new();
    for trace in traces {
        let profile = trace.profile();
        let input_form = profile.commodity().form();
        if input_form != definition.input_form() {
            return Err(ComminutionBatchError::InputFormMismatch {
                expected: definition.input_form(),
                found: input_form,
            });
        }
        if let Some(input_particle_size) = profile.particle_size() {
            let output_particle_size = definition.output_particle_size();
            if output_particle_size.minimum_diameter() > input_particle_size.minimum_diameter()
                || output_particle_size.maximum_diameter() >= input_particle_size.maximum_diameter()
            {
                return Err(ComminutionBatchError::ParticleSizeNotReduced {
                    input: input_particle_size,
                    output: output_particle_size,
                });
            }
        }
        let commodity = CommodityKey::new(profile.commodity().material(), definition.output_form());
        let key = (
            commodity,
            profile.temperature(),
            profile.composition().clone(),
        );
        let current = grouped.get(&key).copied().unwrap_or(Mass::ZERO);
        let next = current
            .checked_add(trace.mass())
            .ok_or(ComminutionBatchError::MassOverflow)?;
        grouped.insert(key, next);
    }

    grouped
        .into_iter()
        .map(|((commodity, temperature, composition), mass)| {
            MaterialLotSpec::with_composition_and_particle_size(
                commodity,
                mass,
                temperature,
                composition,
                definition.output_particle_size(),
            )
            .map_err(ComminutionBatchError::Output)
        })
        .collect()
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
            Self::Resolution(error) => Some(error),
            Self::UnknownComminutionProcess { .. }
            | Self::MissingMassFlowCapability
            | Self::MissingMaximumBatchMassCapability
            | Self::BatchMassExceeded { .. }
            | Self::WrongEnergyCarrier { .. } => None,
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
/// Comminution assigns an authored particle-size envelope while preserving each distinct
/// composition and temperature. Particulate inputs must be strictly reduced without coarsening
/// their represented fines; untracked coarse inputs establish their first explicit size state.
/// It does not purify ore or invent yield bonuses. Exact mass-specific work is reserved from a finite
/// energy source, while operation duration is the slower of equipment throughput and source power.
/// Concrete gameplay processes remain unregistered until real world equipment/power content exists.
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

    let processing_rate = match provider.get_capability(definition.mass_flow_capability()) {
        Some(CapabilityValue::MassFlow(rate)) => rate,
        Some(_) | None => return Err(ComminutionResolutionError::MissingMassFlowCapability),
    };
    let maximum_batch_mass = match provider.get_capability(definition.max_batch_mass_capability()) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(_) | None => {
            return Err(ComminutionResolutionError::MissingMaximumBatchMassCapability);
        }
    };
    let selected_mass = inputs.input_mass();
    if selected_mass > maximum_batch_mass {
        return Err(ComminutionResolutionError::BatchMassExceeded {
            selected: selected_mass,
            maximum: maximum_batch_mass,
        });
    }
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
    let throughput_duration = calculate_mass_flow_duration_ceiling(
        processing_rate,
        selected_mass,
        registries.core().ticks_per_second(),
    )
    .map_err(ComminutionResolutionError::ThroughputDuration)?;
    let available_power = energy_supply.max_output_power();
    let energy_duration = calculate_power_duration_ceiling(
        available_power,
        required_energy,
        registries.core().ticks_per_second(),
    )
    .map_err(ComminutionResolutionError::EnergyDuration)?;
    let duration = std::cmp::max(throughput_duration, energy_duration);
    let condition_after = calculate_condition_after_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        duration,
    );
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
            Self::Batch { error, .. } => Some(error),
            Self::ThroughputDuration { error, .. } => Some(error),
            Self::EnergyDuration { error, .. } => Some(error),
            Self::MissingEnergy { .. }
            | Self::UnexpectedReleasedEnergy { .. }
            | Self::MissingEquipmentProvider { .. }
            | Self::UnknownEquipmentDefinition { .. }
            | Self::UnknownEnergyDefinition { .. }
            | Self::MissingMassFlowCapability { .. }
            | Self::MissingMaximumBatchMassCapability { .. }
            | Self::BatchMassExceeded { .. }
            | Self::WrongEnergyCarrier { .. }
            | Self::EnergyMismatch { .. }
            | Self::DurationMismatch { .. }
            | Self::MissingConditionOutcome { .. }
            | Self::ConditionOutcomeMismatch { .. }
            | Self::OutputMismatch { .. } => None,
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
    let processing_rate = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        definition.mass_flow_capability(),
    ) {
        Some(CapabilityValue::MassFlow(rate)) => rate,
        Some(_) | None => {
            return Err(ComminutionJobValidationError::MissingMassFlowCapability { job: job.id() });
        }
    };
    let maximum_batch_mass = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        definition.max_batch_mass_capability(),
    ) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(_) | None => {
            return Err(
                ComminutionJobValidationError::MissingMaximumBatchMassCapability { job: job.id() },
            );
        }
    };
    if job.consumed_mass() > maximum_batch_mass {
        return Err(ComminutionJobValidationError::BatchMassExceeded {
            job: job.id(),
            selected: job.consumed_mass(),
            maximum: maximum_batch_mass,
        });
    }
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
    let throughput_duration = calculate_mass_flow_duration_ceiling(
        processing_rate,
        job.consumed_mass(),
        registries.core().ticks_per_second(),
    )
    .map_err(|error| ComminutionJobValidationError::ThroughputDuration {
        job: job.id(),
        error,
    })?;
    let energy_duration = calculate_power_duration_ceiling(
        energy_definition.max_output_power(),
        required_energy,
        registries.core().ticks_per_second(),
    )
    .map_err(|error| ComminutionJobValidationError::EnergyDuration {
        job: job.id(),
        error,
    })?;
    let required_duration = std::cmp::max(throughput_duration, energy_duration);
    let stored_duration = job.completes_at().value() - job.started_at().value();
    if stored_duration != required_duration.value() {
        return Err(ComminutionJobValidationError::DurationMismatch {
            job: job.id(),
            stored_ticks: stored_duration,
            required_ticks: required_duration.value(),
        });
    }
    let required_condition_after = calculate_condition_after_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        required_duration,
    );
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
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
        CapabilityRequirement, CapabilityValueKind,
    };
    use crate::content::{
        FORM_CONCENTRATE, FORM_CRUSHED, FORM_INGOT, FORM_ORE, MATERIAL_COPPER, MATERIAL_SLAG,
        make_test_registries_with_comminution,
    };
    use crate::core::quantity::{AggregateMass, Length, MassSpecificEnergy};
    use crate::core::state::{StateValidationError, validate_loaded_state};
    use crate::core::time::{TickSpan, WorldSeed};
    use crate::energy::{
        EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store_with_initial_for_test,
    };
    use crate::equipment::{
        CapabilityConditionCurve, CapabilityConditionPoint, EquipmentDefinition,
        EquipmentDefinitionId, add_equipment,
    };
    use crate::inventory::{
        add_stockpile, deposit_composed_lot_for_test, deposit_lot_spec_for_test,
    };
    use crate::maintenance::MaintenanceThresholds;
    use crate::material::CompositionComponent;
    use crate::matter::calculate_matter_accounting;
    use crate::ore_processing::{ComminutionOperatingProfile, ComminutionProcessDefinition};
    use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
    use crate::production::{ProcessDefinition, validate_start_process};
    use crate::simulation::advance_tick;

    const MASS_FLOW_CAPABILITY: CapabilityId = CapabilityId::new(970_001);
    const MAX_BATCH_MASS_CAPABILITY: CapabilityId = CapabilityId::new(970_002);
    const CRUSHER: EquipmentDefinitionId = EquipmentDefinitionId::new(970_001);
    const ENERGY_STORE_DEFINITION: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(970_001);
    const PROCESS: ProcessId = ProcessId::new(970_001);
    const INPUT_TEMPERATURE: Temperature = Temperature::from_millikelvin(300_000);
    const SPECIFIC_WORK: MassSpecificEnergy =
        MassSpecificEnergy::from_nanojoules_per_milligram(100);

    fn crushed_particle_size() -> ParticleSizeRange {
        match ParticleSizeRange::new(
            Length::from_micrometers(1),
            Length::from_micrometers(20_000),
        ) {
            Ok(range) => range,
            Err(error) => panic!("comminution particle-size fixture failed: {error}"),
        }
    }

    fn ground_particle_size() -> ParticleSizeRange {
        match ParticleSizeRange::new(Length::from_micrometers(1), Length::from_micrometers(5_000)) {
            Ok(range) => range,
            Err(error) => panic!("grinding particle-size fixture failed: {error}"),
        }
    }

    fn condition(parts_per_million: u32) -> Condition {
        match Condition::new(parts_per_million) {
            Ok(condition) => condition,
            Err(error) => panic!("comminution condition fixture failed: {error}"),
        }
    }

    fn mixed_ore_composition() -> MaterialComposition {
        match MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_COPPER, 400_000),
            CompositionComponent::new(MATERIAL_SLAG, 600_000),
        ]) {
            Ok(composition) => composition,
            Err(error) => panic!("comminution composition fixture failed: {error}"),
        }
    }

    fn make_registries_with_energy(carrier: EnergyCarrier, max_output_power: Power) -> Registries {
        make_registries_with_definition(
            carrier,
            max_output_power,
            ComminutionProcessDefinition::new(
                PROCESS,
                FORM_ORE,
                FORM_CRUSHED,
                crushed_particle_size(),
                ComminutionOperatingProfile::new(
                    MASS_FLOW_CAPABILITY,
                    MAX_BATCH_MASS_CAPABILITY,
                    EnergyCarrier::Mechanical,
                    SPECIFIC_WORK,
                    1_000,
                ),
            ),
        )
    }

    fn make_registries_with_definition(
        carrier: EnergyCarrier,
        max_output_power: Power,
        comminution_definition: ComminutionProcessDefinition,
    ) -> Registries {
        let capabilities = match CapabilityProfile::new([
            (
                MASS_FLOW_CAPABILITY,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(200)),
            ),
            (
                MAX_BATCH_MASS_CAPABILITY,
                CapabilityValue::Mass(Mass::from_milligrams(100)),
            ),
        ]) {
            Ok(profile) => profile,
            Err(error) => panic!("comminution capability fixture failed: {error}"),
        };
        let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
            Ok(thresholds) => thresholds,
            Err(error) => panic!("comminution maintenance fixture failed: {error}"),
        };
        let throughput_curve = CapabilityConditionCurve::new(
            MASS_FLOW_CAPABILITY,
            vec![CapabilityConditionPoint::new(
                Condition::FAILED,
                CapabilityValue::MassFlow(MassFlow::ZERO),
            )],
        );
        let equipment = EquipmentDefinition::new_with_capability_condition_curves(
            CRUSHER,
            "test jaw crusher",
            Mass::from_milligrams(1_000_000),
            capabilities,
            thresholds,
            vec![throughput_curve],
        );
        let process = ProcessDefinition::new_selected_batch(
            PROCESS,
            "test ore crushing",
            vec![CapabilityRequirement::new(
                MASS_FLOW_CAPABILITY,
                CapabilityComparison::AtLeast,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
            )],
        );
        make_test_registries_with_comminution(
            vec![
                CapabilityDefinition::new(
                    MASS_FLOW_CAPABILITY,
                    "material mass throughput",
                    CapabilityValueKind::MassFlow,
                ),
                CapabilityDefinition::new(
                    MAX_BATCH_MASS_CAPABILITY,
                    "maximum comminution batch mass",
                    CapabilityValueKind::Mass,
                ),
            ],
            equipment,
            EnergyStoreDefinition::new(
                ENERGY_STORE_DEFINITION,
                "test crusher work buffer",
                carrier,
                Energy::from_nanojoules(1_000_000),
                max_output_power,
            ),
            process,
            comminution_definition,
        )
    }

    fn make_registries() -> Registries {
        make_registries_with_energy(EnergyCarrier::Mechanical, Power::from_microwatts(100))
    }

    #[test]
    fn comminution_can_reduce_particle_size_without_relabeling_the_material_form() {
        let registries = make_registries_with_definition(
            EnergyCarrier::Mechanical,
            Power::from_microwatts(100),
            ComminutionProcessDefinition::new(
                PROCESS,
                FORM_CRUSHED,
                FORM_CRUSHED,
                ground_particle_size(),
                ComminutionOperatingProfile::new(
                    MASS_FLOW_CAPABILITY,
                    MAX_BATCH_MASS_CAPABILITY,
                    EnergyCarrier::Mechanical,
                    SPECIFIC_WORK,
                    1_000,
                ),
            ),
        );
        let mut state = AppState::new(WorldSeed::new(0x9700_0006));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(source) => source,
            Err(error) => panic!("grinding source fixture failed: {error}"),
        };
        let input = match MaterialLotSpec::with_composition_and_particle_size(
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
            Mass::from_milligrams(20),
            INPUT_TEMPERATURE,
            mixed_ore_composition(),
            crushed_particle_size(),
        ) {
            Ok(input) => input,
            Err(error) => panic!("grinding input specification failed: {error}"),
        };
        let lot = match deposit_lot_spec_for_test(&registries, &mut state, source, input) {
            Ok(lot) => lot,
            Err(error) => panic!("grinding input fixture failed: {error}"),
        };
        let equipment = match add_equipment(&registries, &mut state, CRUSHER, Condition::PRISTINE) {
            Ok(equipment) => equipment,
            Err(error) => panic!("grinding equipment fixture failed: {error}"),
        };
        let energy_store = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ENERGY_STORE_DEFINITION,
            Energy::from_nanojoules(1_000_000),
        ) {
            Ok(energy_store) => energy_store,
            Err(error) => panic!("grinding energy fixture failed: {error}"),
        };

        let resolved = match resolve_comminution_process(
            &registries,
            &state,
            ComminutionRequest::new(
                PROCESS,
                source,
                &[MaterialLotSelection::new(lot, Mass::from_milligrams(20))],
                equipment,
                energy_store,
            ),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("same-form grinding resolution failed: {error}"),
        };
        let outputs = resolved.process_resolution().outputs();
        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs[0].commodity(),
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)
        );
        assert_eq!(outputs[0].particle_size(), Some(ground_particle_size()));
    }

    struct Fixture {
        registries: Registries,
        state: AppState,
        source: StockpileId,
        destination: StockpileId,
        lot: crate::inventory::MaterialLotId,
        equipment: EquipmentId,
        energy_store: EnergyStoreId,
    }

    fn make_fixture_with_registries(
        registries: Registries,
        seed: WorldSeed,
        input_mass: Mass,
        equipment_condition: Condition,
    ) -> Fixture {
        let mut state = AppState::new(seed);
        let source = match add_stockpile(&mut state, Mass::from_milligrams(1_000)) {
            Ok(source) => source,
            Err(error) => panic!("comminution source fixture failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(1_000)) {
            Ok(destination) => destination,
            Err(error) => panic!("comminution destination fixture failed: {error}"),
        };
        let lot = match deposit_composed_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            input_mass,
            INPUT_TEMPERATURE,
            mixed_ore_composition(),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("comminution input fixture failed: {error}"),
        };
        let equipment = match add_equipment(&registries, &mut state, CRUSHER, equipment_condition) {
            Ok(equipment) => equipment,
            Err(error) => panic!("comminution equipment fixture failed: {error}"),
        };
        let energy_store = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ENERGY_STORE_DEFINITION,
            Energy::from_nanojoules(1_000_000),
        ) {
            Ok(energy_store) => energy_store,
            Err(error) => panic!("comminution energy fixture failed: {error}"),
        };
        Fixture {
            registries,
            state,
            source,
            destination,
            lot,
            equipment,
            energy_store,
        }
    }

    fn make_fixture(seed: WorldSeed, input_mass: Mass, equipment_condition: Condition) -> Fixture {
        make_fixture_with_registries(make_registries(), seed, input_mass, equipment_condition)
    }

    fn matter_total(state: &AppState) -> AggregateMass {
        match calculate_matter_accounting(state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("comminution matter accounting failed: {error}"),
        }
    }

    fn resolve_mass(
        fixture: &Fixture,
        state: &AppState,
        mass: Mass,
    ) -> Result<ResolvedComminution, ComminutionResolutionError> {
        resolve_comminution_process(
            &fixture.registries,
            state,
            ComminutionRequest::new(
                PROCESS,
                fixture.source,
                &[MaterialLotSelection::new(fixture.lot, mass)],
                fixture.equipment,
                fixture.energy_store,
            ),
        )
    }

    fn finish_job(registries: &Registries, state: &mut AppState, duration: TickSpan) {
        for _ in 0..duration.value() {
            if let Err(error) = advance_tick(registries, state) {
                panic!("comminution completion tick failed: {error}");
            }
        }
    }

    #[test]
    fn comminution_preserves_exact_mixed_profile_and_derates_throughput_with_wear() {
        let mut fixture = make_fixture(
            WorldSeed::new(0x9700_0001),
            Mass::from_milligrams(20),
            condition(500_000),
        );
        let initial_matter = matter_total(&fixture.state);
        let resolved = match resolve_mass(&fixture, &fixture.state, Mass::from_milligrams(20)) {
            Ok(resolved) => resolved,
            Err(error) => panic!("comminution resolution failed: {error}"),
        };
        assert_eq!(
            resolved.processing_rate(),
            MassFlow::from_milligrams_per_second(100)
        );
        assert_eq!(resolved.required_energy(), Energy::from_nanojoules(2_000));
        assert_eq!(resolved.available_power(), Power::from_microwatts(100));
        assert_eq!(resolved.condition_before(), condition(500_000));
        assert_eq!(resolved.condition_after(), condition(496_000));
        assert_eq!(resolved.process_resolution().duration(), TickSpan::new(4));
        assert_eq!(resolved.process_resolution().outputs().len(), 1);
        let output = &resolved.process_resolution().outputs()[0];
        assert_eq!(
            output.commodity(),
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)
        );
        assert_eq!(output.mass(), Mass::from_milligrams(20));
        assert_eq!(output.temperature(), INPUT_TEMPERATURE);
        assert_eq!(output.composition(), &mixed_ore_composition());
        assert_eq!(output.particle_size(), Some(crushed_particle_size()));
        assert_eq!(
            resolved.process_resolution().equipment_condition_after(),
            Some(condition(496_000))
        );

        let duration = resolved.process_resolution().duration();
        let token = match validate_start_process(
            &fixture.registries,
            &fixture.state,
            resolved.process_resolution(),
            fixture.source,
            fixture.destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("comminution start validation failed: {error}"),
        };
        if let Err(error) = token.commit(&mut fixture.state) {
            panic!("comminution start commit failed: {error}");
        }
        assert_eq!(
            validate_loaded_state(&fixture.registries, &fixture.state),
            Ok(())
        );
        assert_eq!(matter_total(&fixture.state), initial_matter);
        finish_job(&fixture.registries, &mut fixture.state, duration);

        let output = match fixture
            .state
            .inventory()
            .lots()
            .find(|lot| lot.stockpile() == fixture.destination)
        {
            Some(output) => output,
            None => panic!("completed comminution output disappeared"),
        };
        assert_eq!(
            output.commodity(),
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)
        );
        assert_eq!(output.composition(), &mixed_ore_composition());
        assert_eq!(output.particle_size(), Some(crushed_particle_size()));
        assert_eq!(matter_total(&fixture.state), initial_matter);
        assert_eq!(
            fixture
                .state
                .equipment()
                .get_equipment(fixture.equipment)
                .map(|record| record.condition()),
            Some(condition(496_000))
        );
    }

    #[test]
    fn comminution_duration_uses_slower_finite_energy_delivery() {
        let fixture = make_fixture_with_registries(
            make_registries_with_energy(EnergyCarrier::Mechanical, Power::from_microwatts(1)),
            WorldSeed::new(0x9700_0004),
            Mass::from_milligrams(20),
            Condition::PRISTINE,
        );
        let resolved = match resolve_mass(&fixture, &fixture.state, Mass::from_milligrams(20)) {
            Ok(resolved) => resolved,
            Err(error) => panic!("power-limited comminution resolution failed: {error}"),
        };

        assert_eq!(
            resolved.processing_rate(),
            MassFlow::from_milligrams_per_second(200)
        );
        assert_eq!(resolved.required_energy(), Energy::from_nanojoules(2_000));
        assert_eq!(resolved.available_power(), Power::from_microwatts(1));
        assert_eq!(resolved.condition_before(), Condition::PRISTINE);
        assert_eq!(resolved.condition_after(), condition(960_000));
        assert_eq!(resolved.process_resolution().duration(), TickSpan::new(40));
        assert_eq!(
            resolved.process_resolution().equipment_condition_after(),
            Some(condition(960_000))
        );
    }

    #[test]
    fn comminution_rejects_wrong_energy_carrier_without_mutation() {
        let fixture = make_fixture_with_registries(
            make_registries_with_energy(EnergyCarrier::Electrical, Power::from_microwatts(100)),
            WorldSeed::new(0x9700_0005),
            Mass::from_milligrams(20),
            Condition::PRISTINE,
        );
        let before = fixture.state.clone();

        assert!(matches!(
            resolve_mass(&fixture, &fixture.state, Mass::from_milligrams(20)),
            Err(ComminutionResolutionError::WrongEnergyCarrier {
                required: EnergyCarrier::Mechanical,
                provided: EnergyCarrier::Electrical,
            })
        ));
        assert_eq!(fixture.state, before);
    }

    #[test]
    fn comminution_rejects_wrong_form_and_oversized_batch_without_mutation() {
        let registries = make_registries();
        let mut state = AppState::new(WorldSeed::new(0x9700_0002));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(500)) {
            Ok(source) => source,
            Err(error) => panic!("comminution rejection source failed: {error}"),
        };
        let wrong_form_lot = match deposit_composed_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
            Mass::from_milligrams(10),
            INPUT_TEMPERATURE,
            mixed_ore_composition(),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("wrong-form comminution fixture failed: {error}"),
        };
        let oversized_lot = match deposit_composed_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(101),
            INPUT_TEMPERATURE,
            mixed_ore_composition(),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("oversized comminution fixture failed: {error}"),
        };
        let equipment = match add_equipment(&registries, &mut state, CRUSHER, Condition::PRISTINE) {
            Ok(equipment) => equipment,
            Err(error) => panic!("comminution rejection equipment failed: {error}"),
        };
        let energy_store = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ENERGY_STORE_DEFINITION,
            Energy::from_nanojoules(1_000_000),
        ) {
            Ok(energy_store) => energy_store,
            Err(error) => panic!("comminution rejection energy fixture failed: {error}"),
        };
        let before = state.clone();

        assert!(matches!(
            resolve_comminution_process(
                &registries,
                &state,
                ComminutionRequest::new(
                    PROCESS,
                    source,
                    &[MaterialLotSelection::new(
                        wrong_form_lot,
                        Mass::from_milligrams(10)
                    )],
                    equipment,
                    energy_store,
                ),
            ),
            Err(ComminutionResolutionError::Batch(
                ComminutionBatchError::InputFormMismatch {
                    expected: FORM_ORE,
                    found: FORM_INGOT,
                }
            ))
        ));
        assert!(matches!(
            resolve_comminution_process(
                &registries,
                &state,
                ComminutionRequest::new(
                    PROCESS,
                    source,
                    &[MaterialLotSelection::new(
                        oversized_lot,
                        Mass::from_milligrams(101),
                    )],
                    equipment,
                    energy_store,
                ),
            ),
            Err(ComminutionResolutionError::BatchMassExceeded { selected, maximum })
                if selected == Mass::from_milligrams(101)
                    && maximum == Mass::from_milligrams(100)
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn comminution_job_round_trip_revalidates_exact_outputs_and_continues() {
        let mut fixture = make_fixture(
            WorldSeed::new(0x9700_0003),
            Mass::from_milligrams(20),
            Condition::PRISTINE,
        );
        let resolved = match resolve_mass(&fixture, &fixture.state, Mass::from_milligrams(20)) {
            Ok(resolved) => resolved,
            Err(error) => panic!("round-trip comminution resolution failed: {error}"),
        };
        let required_energy = resolved.required_energy();
        let duration = resolved.process_resolution().duration();
        let token = match validate_start_process(
            &fixture.registries,
            &fixture.state,
            resolved.process_resolution(),
            fixture.source,
            fixture.destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("round-trip comminution start validation failed: {error}"),
        };
        let job = match token.commit(&mut fixture.state) {
            Ok(job) => job,
            Err(error) => panic!("round-trip comminution start failed: {error}"),
        };
        assert_eq!(
            validate_loaded_state(&fixture.registries, &fixture.state),
            Ok(())
        );

        let encoded =
            match serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)) {
                Ok(encoded) => encoded,
                Err(error) => panic!("comminution save serialization failed: {error}"),
            };
        let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
            Ok(decoded) => decoded,
            Err(error) => panic!("comminution save decode failed: {error}"),
        };
        let mut loaded = match decoded.into_state(&fixture.registries) {
            Ok(loaded) => loaded,
            Err(error) => panic!("comminution save validation failed: {error}"),
        };
        assert_eq!(loaded, fixture.state);

        let mut tampered =
            match serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state)) {
                Ok(encoded) => encoded,
                Err(error) => panic!("comminution tamper serialization failed: {error}"),
            };
        tampered["state"]["production"]["jobs"][job.value().to_string()]["output_streams"][0]["outputs"]
            [0]["commodity"] =
            serde_json::json!(CommodityKey::new(MATERIAL_COPPER, FORM_CONCENTRATE).value());
        let tampered: LoadedSaveEnvelope = match serde_json::from_value(tampered) {
            Ok(decoded) => decoded,
            Err(error) => panic!("comminution tampered save decode failed: {error}"),
        };
        assert_eq!(
            tampered.into_state(&fixture.registries),
            Err(LoadError::InvalidState(
                StateValidationError::ComminutionJob(
                    ComminutionJobValidationError::OutputMismatch { job }
                )
            ))
        );

        let mut tampered_particle_size =
            match serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state)) {
                Ok(encoded) => encoded,
                Err(error) => {
                    panic!("comminution particle-size tamper serialization failed: {error}")
                }
            };
        tampered_particle_size["state"]["production"]["jobs"][job.value().to_string()]["output_streams"]
            [0]["outputs"][0]["particle_size"]["maximum_diameter"] = serde_json::json!(5_000_u64);
        let tampered_particle_size: LoadedSaveEnvelope =
            match serde_json::from_value(tampered_particle_size) {
                Ok(decoded) => decoded,
                Err(error) => panic!("comminution particle-size tamper failed decode: {error}"),
            };
        assert_eq!(
            tampered_particle_size.into_state(&fixture.registries),
            Err(LoadError::InvalidState(
                StateValidationError::ComminutionJob(
                    ComminutionJobValidationError::OutputMismatch { job }
                )
            ))
        );

        let mut tampered_energy =
            match serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state)) {
                Ok(encoded) => encoded,
                Err(error) => panic!("comminution energy tamper serialization failed: {error}"),
            };
        tampered_energy["state"]["production"]["jobs"][job.value().to_string()]["consumed_energy"]
            ["energy"] = serde_json::json!(1_u64);
        let tampered_energy: LoadedSaveEnvelope = match serde_json::from_value(tampered_energy) {
            Ok(decoded) => decoded,
            Err(error) => panic!("comminution energy tamper failed decode: {error}"),
        };
        assert_eq!(
            tampered_energy.into_state(&fixture.registries),
            Err(LoadError::InvalidState(
                StateValidationError::ComminutionJob(
                    ComminutionJobValidationError::EnergyMismatch {
                        job,
                        traced: Energy::from_nanojoules(1),
                        required: required_energy,
                    }
                )
            ))
        );

        finish_job(&fixture.registries, &mut fixture.state, duration);
        finish_job(&fixture.registries, &mut loaded, duration);
        assert_eq!(loaded, fixture.state);
    }

    fn run_comminution_soak(seed: WorldSeed) -> AppState {
        let fixture = make_fixture(seed, Mass::from_milligrams(300), Condition::PRISTINE);
        let initial_matter = matter_total(&fixture.state);
        let mut state = fixture.state.clone();
        for step in 0..300_u64 {
            let resolved = match resolve_mass(&fixture, &state, Mass::from_milligrams(1)) {
                Ok(resolved) => resolved,
                Err(error) => panic!("comminution soak resolution failed at step {step}: {error}"),
            };
            let duration = resolved.process_resolution().duration();
            let token = match validate_start_process(
                &fixture.registries,
                &state,
                resolved.process_resolution(),
                fixture.source,
                fixture.destination,
            ) {
                Ok(token) => token,
                Err(error) => panic!("comminution soak start failed at step {step}: {error}"),
            };
            if let Err(error) = token.commit(&mut state) {
                panic!("comminution soak commit failed at step {step}: {error}");
            }
            finish_job(&fixture.registries, &mut state, duration);
            if step.is_multiple_of(47) {
                assert_eq!(validate_loaded_state(&fixture.registries, &state), Ok(()));
                assert_eq!(matter_total(&state), initial_matter);
            }
        }
        assert_eq!(matter_total(&state), initial_matter);
        assert_eq!(
            state
                .inventory()
                .get_stockpile(fixture.destination)
                .map(|stockpile| {
                    stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED))
                }),
            Some(Mass::from_milligrams(300))
        );
        assert_eq!(
            state
                .energy()
                .get_store(fixture.energy_store)
                .map(|store| store.stored()),
            Some(Energy::from_nanojoules(970_000))
        );
        state
    }

    #[test]
    fn repeated_comminution_preserves_matter_and_deterministic_replay() {
        let seed = WorldSeed::new(0x9700_5000);
        let first = run_comminution_soak(seed);
        let second = run_comminution_soak(seed);
        assert_eq!(first, second);
    }
}
