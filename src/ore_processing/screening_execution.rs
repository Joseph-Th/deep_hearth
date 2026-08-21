//! Exact particle-size screening resolution and persisted-job audit.

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
use crate::maintenance::{
    ActiveConditionDurationError, Condition, calculate_usable_condition_after_active_ticks,
};
use crate::material::{
    CommodityKey, FormId, MaterialComposition, MaterialLotSpec, MaterialLotSpecError,
    ParticleSizeDistribution, ParticleSizeDistributionError, ParticleSizeRange,
};
use crate::production::{
    ProcessId, ProcessInputError, ProcessOutputStream, ProcessResolution, ProcessResolutionError,
    ProductionJobId, ProductionJobRecord, validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::{
    MassFlowDurationError, ScreeningProcessDefinition, calculate_mass_flow_duration_ceiling,
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

/// Failure while partitioning selected material into exact screen products.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreeningBatchError {
    EmptyInput,
    InputFormMismatch {
        expected: FormId,
        found: FormId,
    },
    MissingParticleSize,
    UnresolvedParticleClass {
        aperture: crate::core::quantity::Length,
        class: ParticleSizeRange,
    },
    UnrepresentableClassMass {
        mass: Mass,
        undersize_weight: u64,
        total_weight: u64,
    },
    MassOverflow,
    Distribution(ParticleSizeDistributionError),
    Output(MaterialLotSpecError),
}

impl Display for ScreeningBatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("screening batch contains no material"),
            Self::InputFormMismatch { expected, found } => write!(
                formatter,
                "screening batch requires input form {} but selected form {}",
                expected.value(),
                found.value()
            ),
            Self::MissingParticleSize => {
                formatter.write_str("screening input has no particulate size distribution")
            }
            Self::UnresolvedParticleClass { aperture, class } => write!(
                formatter,
                "screen aperture {} um intersects unresolved particle class {}..={} um",
                aperture.micrometers(),
                class.minimum_diameter().micrometers(),
                class.maximum_diameter().micrometers()
            ),
            Self::UnrepresentableClassMass {
                mass,
                undersize_weight,
                total_weight,
            } => write!(
                formatter,
                "screening {} mg with undersize weight {undersize_weight}/{total_weight} cannot be represented exactly at whole-milligram mass resolution",
                mass.milligrams()
            ),
            Self::MassOverflow => formatter.write_str("screening output mass overflowed"),
            Self::Distribution(error) => write!(
                formatter,
                "screening could not preserve a classified particle distribution: {error}"
            ),
            Self::Output(error) => write!(
                formatter,
                "screening output specification could not preserve its material profile: {error}"
            ),
        }
    }
}

impl Error for ScreeningBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Distribution(error) => Some(error),
            Self::Output(error) => Some(error),
            Self::InputFormMismatch {
                expected: _expected,
                found: _found,
            } => None,
            Self::UnresolvedParticleClass {
                aperture: _aperture,
                class: _class,
            } => None,
            Self::UnrepresentableClassMass {
                mass: _mass,
                undersize_weight: _undersize_weight,
                total_weight: _total_weight,
            } => None,
            Self::EmptyInput | Self::MissingParticleSize | Self::MassOverflow => None,
        }
    }
}

struct ScreeningOutputs {
    streams: Vec<ProcessOutputStream>,
    undersize_mass: Mass,
    oversize_mass: Mass,
}

fn resolve_screening_outputs(
    definition: ScreeningProcessDefinition,
    traces: &[ConsumedMaterialTrace],
) -> Result<ScreeningOutputs, ScreeningBatchError> {
    if traces.is_empty() {
        return Err(ScreeningBatchError::EmptyInput);
    }

    let mut grouped = BTreeMap::<
        (
            CommodityKey,
            Temperature,
            MaterialComposition,
            ParticleSizeDistribution,
        ),
        Mass,
    >::new();
    for trace in traces {
        let profile = trace.profile();
        let input_form = profile.commodity().form();
        if input_form != definition.input_form() {
            return Err(ScreeningBatchError::InputFormMismatch {
                expected: definition.input_form(),
                found: input_form,
            });
        }
        let distribution = profile
            .particle_size_distribution()
            .cloned()
            .ok_or(ScreeningBatchError::MissingParticleSize)?;
        let commodity = CommodityKey::new(profile.commodity().material(), definition.output_form());
        let key = (
            commodity,
            profile.temperature(),
            profile.composition().clone(),
            distribution,
        );
        let current = grouped.get(&key).copied().unwrap_or(Mass::ZERO);
        grouped.insert(
            key,
            current
                .checked_add(trace.mass())
                .ok_or(ScreeningBatchError::MassOverflow)?,
        );
    }

    let mut undersize = Vec::new();
    let mut oversize = Vec::new();
    let mut undersize_mass = Mass::ZERO;
    let mut oversize_mass = Mass::ZERO;
    for ((commodity, temperature, composition, distribution), mass) in grouped {
        let mut undersize_classes = Vec::new();
        let mut oversize_classes = Vec::new();
        let mut undersize_weight = 0_u64;
        for class in distribution.classes() {
            let range = class.range();
            if range.maximum_diameter() <= definition.aperture() {
                undersize_weight = undersize_weight
                    .checked_add(u64::from(class.weight()))
                    .ok_or(ScreeningBatchError::MassOverflow)?;
                undersize_classes.push(*class);
            } else if range.minimum_diameter() > definition.aperture() {
                oversize_classes.push(*class);
            } else {
                return Err(ScreeningBatchError::UnresolvedParticleClass {
                    aperture: definition.aperture(),
                    class: range,
                });
            }
        }

        let total_weight = distribution.total_weight();
        let weighted_mass = u128::from(mass.milligrams()) * u128::from(undersize_weight);
        let total_weight_u128 = u128::from(total_weight);
        if weighted_mass % total_weight_u128 != 0 {
            return Err(ScreeningBatchError::UnrepresentableClassMass {
                mass,
                undersize_weight,
                total_weight,
            });
        }
        let undersize_milligrams = weighted_mass / total_weight_u128;
        let undersize_milligrams =
            u64::try_from(undersize_milligrams).map_err(|_| ScreeningBatchError::MassOverflow)?;
        let group_undersize = Mass::from_milligrams(undersize_milligrams);
        let group_oversize = mass
            .checked_sub(group_undersize)
            .ok_or(ScreeningBatchError::MassOverflow)?;

        if !group_undersize.is_zero() {
            let distribution = ParticleSizeDistribution::new(undersize_classes)
                .map_err(ScreeningBatchError::Distribution)?;
            undersize.push(
                MaterialLotSpec::with_composition_and_particle_size(
                    commodity,
                    group_undersize,
                    temperature,
                    composition.clone(),
                    distribution,
                )
                .map_err(ScreeningBatchError::Output)?,
            );
            undersize_mass = undersize_mass
                .checked_add(group_undersize)
                .ok_or(ScreeningBatchError::MassOverflow)?;
        }
        if !group_oversize.is_zero() {
            let distribution = ParticleSizeDistribution::new(oversize_classes)
                .map_err(ScreeningBatchError::Distribution)?;
            oversize.push(
                MaterialLotSpec::with_composition_and_particle_size(
                    commodity,
                    group_oversize,
                    temperature,
                    composition,
                    distribution,
                )
                .map_err(ScreeningBatchError::Output)?,
            );
            oversize_mass = oversize_mass
                .checked_add(group_oversize)
                .ok_or(ScreeningBatchError::MassOverflow)?;
        }
    }

    let mut streams = Vec::with_capacity(2);
    if !undersize.is_empty() {
        streams.push(ProcessOutputStream::new(
            ScreeningProcessDefinition::UNDERSIZE_STREAM,
            undersize,
        ));
    }
    if !oversize.is_empty() {
        streams.push(ProcessOutputStream::new(
            ScreeningProcessDefinition::OVERSIZE_STREAM,
            oversize,
        ));
    }
    Ok(ScreeningOutputs {
        streams,
        undersize_mass,
        oversize_mass,
    })
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

    let processing_rate = match provider.get_capability(definition.mass_flow_capability()) {
        Some(CapabilityValue::MassFlow(rate)) => rate,
        Some(_) | None => return Err(ScreeningResolutionError::MissingMassFlowCapability),
    };
    let maximum_batch_mass = match provider.get_capability(definition.max_batch_mass_capability()) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(_) | None => {
            return Err(ScreeningResolutionError::MissingMaximumBatchMassCapability);
        }
    };
    let selected_mass = inputs.input_mass();
    if selected_mass > maximum_batch_mass {
        return Err(ScreeningResolutionError::BatchMassExceeded {
            selected: selected_mass,
            maximum: maximum_batch_mass,
        });
    }

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
    let throughput_duration = calculate_mass_flow_duration_ceiling(
        processing_rate,
        selected_mass,
        registries.core().physical_tick_duration(),
    )
    .map_err(ScreeningResolutionError::ThroughputDuration)?;
    let available_power = energy_supply.max_output_power();
    let energy_duration = calculate_power_duration_ceiling(
        available_power,
        required_energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(ScreeningResolutionError::EnergyDuration)?;
    let duration = std::cmp::max(throughput_duration, energy_duration);
    let condition_after = calculate_usable_condition_after_active_ticks(
        definition.condition_wear_ppm_per_processing_tick(),
        provider.condition(),
        throughput_duration,
    )
    .map_err(ScreeningResolutionError::ConditionDuration)?;
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
    let processing_rate = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        definition.mass_flow_capability(),
    ) {
        Some(CapabilityValue::MassFlow(rate)) => rate,
        Some(_) | None => {
            return Err(ScreeningJobValidationError::MissingMassFlowCapability { job: job.id() });
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
                ScreeningJobValidationError::MissingMaximumBatchMassCapability { job: job.id() },
            );
        }
    };
    if job.consumed_mass() > maximum_batch_mass {
        return Err(ScreeningJobValidationError::BatchMassExceeded {
            job: job.id(),
            selected: job.consumed_mass(),
            maximum: maximum_batch_mass,
        });
    }
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
    let throughput_duration = calculate_mass_flow_duration_ceiling(
        processing_rate,
        job.consumed_mass(),
        registries.core().physical_tick_duration(),
    )
    .map_err(|error| ScreeningJobValidationError::ThroughputDuration {
        job: job.id(),
        error,
    })?;
    let energy_duration = calculate_power_duration_ceiling(
        energy_definition.max_output_power(),
        required_energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(|error| ScreeningJobValidationError::EnergyDuration {
        job: job.id(),
        error,
    })?;
    let required_duration = std::cmp::max(throughput_duration, energy_duration);
    let stored_duration = job.active_duration().value();
    if stored_duration != required_duration.value() {
        return Err(ScreeningJobValidationError::DurationMismatch {
            job: job.id(),
            stored_ticks: stored_duration,
            required_ticks: required_duration.value(),
        });
    }
    let required_condition_after = calculate_usable_condition_after_active_ticks(
        definition.condition_wear_ppm_per_processing_tick(),
        provider.condition(),
        throughput_duration,
    )
    .map_err(|error| ScreeningJobValidationError::ConditionDuration {
        job: job.id(),
        error,
    })?;
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
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
        CapabilityRequirement, CapabilityValueKind,
    };
    use crate::content::{
        FORM_CRUSHED, MATERIAL_COPPER, MATERIAL_SLAG, make_test_registries_with_screening,
    };
    use crate::core::quantity::{Length, MassSpecificEnergy};
    use crate::core::state::{StateValidationError, validate_loaded_state};
    use crate::core::time::WorldSeed;
    use crate::energy::{
        EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store_with_initial_for_test,
    };
    use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId, add_equipment};
    use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_spec_for_test};
    use crate::maintenance::MaintenanceThresholds;
    use crate::material::{CompositionComponent, ParticleSizeClass};
    use crate::matter::calculate_matter_accounting;
    use crate::ore_processing::{ScreeningOperatingProfile, ScreeningProcessDefinition};
    use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
    use crate::production::{ProcessDefinition, ProcessOutputRoute, validate_start_process_routed};
    use crate::simulation::advance_tick;

    const FLOW_CAPABILITY: CapabilityId = CapabilityId::new(971_001);
    const BATCH_CAPABILITY: CapabilityId = CapabilityId::new(971_002);
    const SCREEN: EquipmentDefinitionId = EquipmentDefinitionId::new(971_001);
    const ENERGY_STORE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(971_001);
    const PROCESS: ProcessId = ProcessId::new(971_001);
    const TEMPERATURE: Temperature = Temperature::from_millikelvin(300_000);

    fn distribution() -> ParticleSizeDistribution {
        let class = |minimum, maximum, weight| {
            let range = ParticleSizeRange::new(
                Length::from_micrometers(minimum),
                Length::from_micrometers(maximum),
            )
            .unwrap_or_else(|error| panic!("screening range fixture failed: {error}"));
            ParticleSizeClass::new(range, weight)
                .unwrap_or_else(|error| panic!("screening class fixture failed: {error}"))
        };
        ParticleSizeDistribution::new(vec![
            class(500, 2_000, 4),
            class(2_001, 5_000, 4),
            class(5_001, 10_000, 2),
        ])
        .unwrap_or_else(|error| panic!("screening distribution fixture failed: {error}"))
    }

    fn composition() -> MaterialComposition {
        MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_COPPER, 400_000),
            CompositionComponent::new(MATERIAL_SLAG, 600_000),
        ])
        .unwrap_or_else(|error| panic!("screening composition fixture failed: {error}"))
    }

    fn registries_with_power(aperture: Length, max_output_power: Power) -> Registries {
        let capabilities = CapabilityProfile::new([
            (
                FLOW_CAPABILITY,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(200)),
            ),
            (
                BATCH_CAPABILITY,
                CapabilityValue::Mass(Mass::from_milligrams(100)),
            ),
        ])
        .unwrap_or_else(|error| panic!("screening capability fixture failed: {error}"));
        let warning = Condition::new(600_000)
            .unwrap_or_else(|error| panic!("screening warning fixture failed: {error}"));
        let critical = Condition::new(250_000)
            .unwrap_or_else(|error| panic!("screening critical fixture failed: {error}"));
        let thresholds = MaintenanceThresholds::new(warning, critical)
            .unwrap_or_else(|error| panic!("screening maintenance fixture failed: {error}"));
        let equipment = EquipmentDefinition::new(
            SCREEN,
            "test dry screen",
            Mass::from_milligrams(1_000_000),
            capabilities,
            thresholds,
        );
        let process = ProcessDefinition::new_selected_batch(
            PROCESS,
            "test dry screening",
            vec![CapabilityRequirement::new(
                FLOW_CAPABILITY,
                CapabilityComparison::AtLeast,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
            )],
        );
        make_test_registries_with_screening(
            vec![
                CapabilityDefinition::new(
                    FLOW_CAPABILITY,
                    "screen material throughput",
                    CapabilityValueKind::MassFlow,
                ),
                CapabilityDefinition::new(
                    BATCH_CAPABILITY,
                    "screen maximum batch mass",
                    CapabilityValueKind::Mass,
                ),
            ],
            equipment,
            EnergyStoreDefinition::new(
                ENERGY_STORE,
                "test screen mechanical buffer",
                EnergyCarrier::Mechanical,
                Energy::from_nanojoules(1_000_000),
                max_output_power,
            ),
            process,
            ScreeningProcessDefinition::new(
                PROCESS,
                FORM_CRUSHED,
                FORM_CRUSHED,
                aperture,
                ScreeningOperatingProfile::new(
                    FLOW_CAPABILITY,
                    BATCH_CAPABILITY,
                    EnergyCarrier::Mechanical,
                    MassSpecificEnergy::from_nanojoules_per_milligram(100),
                    1_000,
                ),
            ),
        )
    }

    #[cfg(feature = "test-soak")]
    fn registries(aperture: Length) -> Registries {
        registries_with_power(aperture, Power::from_microwatts(100))
    }

    struct Fixture {
        registries: Registries,
        state: AppState,
        source: StockpileId,
        lot: crate::inventory::MaterialLotId,
        equipment: EquipmentId,
        energy: EnergyStoreId,
    }

    fn fixture_with_power(aperture: Length, max_output_power: Power) -> Fixture {
        let registries = registries_with_power(aperture, max_output_power);
        let mut state = AppState::new(WorldSeed::new(0x9710_0001));
        let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
            .unwrap_or_else(|error| panic!("screening source fixture failed: {error}"));
        let input = MaterialLotSpec::with_composition_and_particle_size(
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
            Mass::from_milligrams(10),
            TEMPERATURE,
            composition(),
            distribution(),
        )
        .unwrap_or_else(|error| panic!("screening input fixture failed: {error}"));
        let lot = deposit_lot_spec_for_test(&registries, &mut state, source, input)
            .unwrap_or_else(|error| panic!("screening lot fixture failed: {error}"));
        let equipment = add_equipment(&registries, &mut state, SCREEN, Condition::PRISTINE)
            .unwrap_or_else(|error| panic!("screening equipment fixture failed: {error}"));
        let energy = add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ENERGY_STORE,
            Energy::from_nanojoules(1_000_000),
        )
        .unwrap_or_else(|error| panic!("screening energy fixture failed: {error}"));
        Fixture {
            registries,
            state,
            source,
            lot,
            equipment,
            energy,
        }
    }

    fn fixture(aperture: Length) -> Fixture {
        fixture_with_power(aperture, Power::from_microwatts(100))
    }

    fn resolve(fixture: &Fixture) -> Result<ResolvedScreening, ScreeningResolutionError> {
        resolve_screening_process(
            &fixture.registries,
            &fixture.state,
            ScreeningRequest::new(
                PROCESS,
                fixture.source,
                &[MaterialLotSelection::new(
                    fixture.lot,
                    Mass::from_milligrams(10),
                )],
                fixture.equipment,
                fixture.energy,
            ),
        )
    }

    #[test]
    fn screening_partitions_resolved_size_classes_without_changing_material_identity() {
        let fixture = fixture(Length::from_micrometers(2_000));
        let resolved = resolve(&fixture)
            .unwrap_or_else(|error| panic!("screening resolution failed: {error}"));
        assert_eq!(resolved.undersize_mass(), Mass::from_milligrams(4));
        assert_eq!(resolved.oversize_mass(), Mass::from_milligrams(6));
        let streams = resolved.process_resolution().output_streams();
        assert_eq!(streams.len(), 2);
        assert_eq!(
            streams[0].id(),
            ScreeningProcessDefinition::UNDERSIZE_STREAM
        );
        assert_eq!(streams[1].id(), ScreeningProcessDefinition::OVERSIZE_STREAM);
        let fines = &streams[0].outputs()[0];
        let coarse = &streams[1].outputs()[0];
        assert_eq!(
            fines.commodity(),
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)
        );
        assert_eq!(coarse.commodity(), fines.commodity());
        assert_eq!(fines.composition(), &composition());
        assert_eq!(coarse.composition(), &composition());
        assert_eq!(fines.temperature(), TEMPERATURE);
        assert_eq!(coarse.temperature(), TEMPERATURE);
        let fines_distribution = fines
            .particle_size_distribution()
            .unwrap_or_else(|| panic!("screening fines output lost particle-size state"));
        let coarse_distribution = coarse
            .particle_size_distribution()
            .unwrap_or_else(|| panic!("screening coarse output lost particle-size state"));
        assert_eq!(fines_distribution.classes().len(), 1);
        assert_eq!(coarse_distribution.classes().len(), 2);
    }

    #[test]
    fn weak_screen_power_extends_time_without_fabricating_processing_wear() {
        let fixture = fixture_with_power(
            Length::from_micrometers(2_000),
            Power::from_picowatts(100_000),
        );
        let resolved = resolve(&fixture)
            .unwrap_or_else(|error| panic!("power-limited screening resolution failed: {error}"));

        assert_eq!(resolved.throughput_duration(), TickSpan::new(1));
        assert_eq!(resolved.energy_duration(), TickSpan::new(3));
        assert_eq!(resolved.process_resolution().duration(), TickSpan::new(3));
        assert_eq!(resolved.condition_before(), Condition::PRISTINE);
        assert_eq!(
            resolved.condition_after(),
            Condition::new(999_000)
                .unwrap_or_else(|error| panic!("screening wear fixture failed: {error}"))
        );
    }

    #[test]
    fn screening_refuses_to_guess_yield_when_aperture_intersects_an_unresolved_class() {
        let fixture = fixture(Length::from_micrometers(3_000));
        assert!(matches!(
            resolve(&fixture),
            Err(ScreeningResolutionError::Batch(
                ScreeningBatchError::UnresolvedParticleClass {
                    aperture: _aperture,
                    class: _class,
                }
            ))
        ));
        assert_eq!(
            fixture
                .state
                .inventory()
                .get_stockpile(fixture.source)
                .unwrap_or_else(|| panic!("screening source stockpile disappeared"))
                .stored_mass(),
            Mass::from_milligrams(10)
        );
    }

    #[test]
    fn screening_refuses_fractional_class_mass_at_current_mass_resolution() {
        let fixture = fixture(Length::from_micrometers(2_000));
        let source = fixture
            .state
            .inventory()
            .get_stockpile(fixture.source)
            .unwrap_or_else(|| panic!("screening source stockpile disappeared"));
        assert_eq!(source.stored_mass(), Mass::from_milligrams(10));

        let selections = [MaterialLotSelection::new(
            fixture.lot,
            Mass::from_milligrams(9),
        )];
        let request = ScreeningRequest::new(
            PROCESS,
            fixture.source,
            &selections,
            fixture.equipment,
            fixture.energy,
        );
        assert!(matches!(
            resolve_screening_process(&fixture.registries, &fixture.state, request),
            Err(ScreeningResolutionError::Batch(
                ScreeningBatchError::UnrepresentableClassMass {
                    mass,
                    undersize_weight: 2,
                    total_weight: 5,
                }
            )) if mass == Mass::from_milligrams(9)
        ));
        assert_eq!(
            fixture
                .state
                .inventory()
                .get_stockpile(fixture.source)
                .unwrap_or_else(|| panic!("screening source stockpile disappeared"))
                .stored_mass(),
            Mass::from_milligrams(10)
        );
    }

    #[test]
    fn routed_screening_completion_conserves_matter_and_validates_while_in_flight() {
        let mut fixture = fixture(Length::from_micrometers(2_000));
        let undersize =
            add_solid_stockpile_for_test(&mut fixture.state, Mass::from_milligrams(100))
                .unwrap_or_else(|error| panic!("undersize destination fixture failed: {error}"));
        let oversize = add_solid_stockpile_for_test(&mut fixture.state, Mass::from_milligrams(100))
            .unwrap_or_else(|error| panic!("oversize destination fixture failed: {error}"));
        let initial_matter = calculate_matter_accounting(&fixture.state)
            .unwrap_or_else(|error| panic!("screening matter accounting failed: {error}"))
            .total();
        let resolved = resolve(&fixture)
            .unwrap_or_else(|error| panic!("screening resolution failed: {error}"));
        let duration = resolved.process_resolution().duration();
        let start = validate_start_process_routed(
            &fixture.registries,
            &fixture.state,
            resolved.process_resolution(),
            fixture.source,
            &[
                ProcessOutputRoute::new(ScreeningProcessDefinition::UNDERSIZE_STREAM, undersize),
                ProcessOutputRoute::new(ScreeningProcessDefinition::OVERSIZE_STREAM, oversize),
            ],
        )
        .unwrap_or_else(|error| panic!("screening start validation failed: {error}"));
        start
            .commit(&mut fixture.state)
            .unwrap_or_else(|error| panic!("screening start commit failed: {error}"));
        validate_loaded_state(&fixture.registries, &fixture.state)
            .unwrap_or_else(|error| panic!("in-flight screening state failed audit: {error}"));
        for _ in 0..duration.value() {
            advance_tick(&fixture.registries, &mut fixture.state)
                .unwrap_or_else(|error| panic!("screening completion tick failed: {error}"));
        }
        assert_eq!(
            fixture
                .state
                .inventory()
                .get_stockpile(undersize)
                .unwrap_or_else(|| panic!("screening undersize stockpile disappeared"))
                .stored_mass(),
            Mass::from_milligrams(4)
        );
        assert_eq!(
            fixture
                .state
                .inventory()
                .get_stockpile(oversize)
                .unwrap_or_else(|| panic!("screening oversize stockpile disappeared"))
                .stored_mass(),
            Mass::from_milligrams(6)
        );
        assert_eq!(
            calculate_matter_accounting(&fixture.state)
                .unwrap_or_else(|error| panic!(
                    "completed screening matter accounting failed: {error}"
                ))
                .total(),
            initial_matter
        );
    }

    #[test]
    fn screening_job_round_trip_rejects_tampered_output_distribution() {
        let mut fixture = fixture(Length::from_micrometers(2_000));
        let undersize =
            add_solid_stockpile_for_test(&mut fixture.state, Mass::from_milligrams(100))
                .unwrap_or_else(|error| panic!("undersize destination fixture failed: {error}"));
        let oversize = add_solid_stockpile_for_test(&mut fixture.state, Mass::from_milligrams(100))
            .unwrap_or_else(|error| panic!("oversize destination fixture failed: {error}"));
        let resolved = resolve(&fixture)
            .unwrap_or_else(|error| panic!("screening resolution failed: {error}"));
        let start = validate_start_process_routed(
            &fixture.registries,
            &fixture.state,
            resolved.process_resolution(),
            fixture.source,
            &[
                ProcessOutputRoute::new(ScreeningProcessDefinition::UNDERSIZE_STREAM, undersize),
                ProcessOutputRoute::new(ScreeningProcessDefinition::OVERSIZE_STREAM, oversize),
            ],
        )
        .unwrap_or_else(|error| panic!("screening start validation failed: {error}"));
        let job = start
            .commit(&mut fixture.state)
            .unwrap_or_else(|error| panic!("screening start commit failed: {error}"));

        let encoded = serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state))
            .unwrap_or_else(|error| panic!("screening save serialization failed: {error}"));
        let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
            .unwrap_or_else(|error| panic!("screening save decode failed: {error}"));
        let loaded = decoded
            .into_state(&fixture.registries)
            .unwrap_or_else(|error| panic!("screening save validation failed: {error}"));
        assert_eq!(loaded, fixture.state);

        let mut tampered =
            serde_json::to_value(SaveEnvelope::new(&fixture.registries, &fixture.state))
                .unwrap_or_else(|error| panic!("screening tamper serialization failed: {error}"));
        tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["output_streams"]
            [0]["outputs"][0]["particle_size"]["classes"][0]["range"]["maximum_diameter"] =
            serde_json::json!(1_999_u64);
        let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
            .unwrap_or_else(|error| panic!("screening tampered save decode failed: {error}"));
        assert_eq!(
            tampered.into_state(&fixture.registries),
            Err(LoadError::InvalidState(StateValidationError::ScreeningJob(
                ScreeningJobValidationError::OutputMismatch { job }
            )))
        );
    }

    #[cfg(feature = "test-soak")]
    fn run_screening_soak(seed: WorldSeed) -> AppState {
        const OPERATIONS: u64 = 300;
        const BATCH_MILLIGRAMS: u64 = 10;
        let registries = registries(Length::from_micrometers(2_000));
        let mut state = AppState::new(seed);
        let total_mass = Mass::from_milligrams(OPERATIONS * BATCH_MILLIGRAMS);
        let source = add_solid_stockpile_for_test(&mut state, total_mass)
            .unwrap_or_else(|error| panic!("screening soak source failed: {error}"));
        let undersize = add_solid_stockpile_for_test(&mut state, total_mass)
            .unwrap_or_else(|error| panic!("screening soak undersize storage failed: {error}"));
        let oversize = add_solid_stockpile_for_test(&mut state, total_mass)
            .unwrap_or_else(|error| panic!("screening soak oversize storage failed: {error}"));
        let input = MaterialLotSpec::with_composition_and_particle_size(
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
            total_mass,
            TEMPERATURE,
            composition(),
            distribution(),
        )
        .unwrap_or_else(|error| panic!("screening soak input failed: {error}"));
        let lot = deposit_lot_spec_for_test(&registries, &mut state, source, input)
            .unwrap_or_else(|error| panic!("screening soak lot seed failed: {error}"));
        let equipment = add_equipment(&registries, &mut state, SCREEN, Condition::PRISTINE)
            .unwrap_or_else(|error| panic!("screening soak equipment failed: {error}"));
        let energy = add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ENERGY_STORE,
            Energy::from_nanojoules(1_000_000),
        )
        .unwrap_or_else(|error| panic!("screening soak energy failed: {error}"));
        let initial_matter = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("screening soak matter accounting failed: {error}"))
            .total();

        for operation in 0..OPERATIONS {
            let selection = [MaterialLotSelection::new(
                lot,
                Mass::from_milligrams(BATCH_MILLIGRAMS),
            )];
            let resolved = resolve_screening_process(
                &registries,
                &state,
                ScreeningRequest::new(PROCESS, source, &selection, equipment, energy),
            )
            .unwrap_or_else(|error| panic!("screening soak resolution failed: {error}"));
            assert_eq!(resolved.undersize_mass(), Mass::from_milligrams(4));
            assert_eq!(resolved.oversize_mass(), Mass::from_milligrams(6));
            assert_eq!(resolved.process_resolution().duration(), TickSpan::new(1));
            let start = validate_start_process_routed(
                &registries,
                &state,
                resolved.process_resolution(),
                source,
                &[
                    ProcessOutputRoute::new(
                        ScreeningProcessDefinition::UNDERSIZE_STREAM,
                        undersize,
                    ),
                    ProcessOutputRoute::new(ScreeningProcessDefinition::OVERSIZE_STREAM, oversize),
                ],
            )
            .unwrap_or_else(|error| panic!("screening soak start failed: {error}"));
            start
                .commit(&mut state)
                .unwrap_or_else(|error| panic!("screening soak commit failed: {error}"));

            if operation == OPERATIONS / 2 {
                let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
                    .unwrap_or_else(|error| panic!("screening soak serialization failed: {error}"));
                let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
                    .unwrap_or_else(|error| panic!("screening soak decode failed: {error}"));
                state = decoded
                    .into_state(&registries)
                    .unwrap_or_else(|error| panic!("screening soak resume failed: {error}"));
            }

            advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("screening soak completion failed: {error}"));
            if operation % 25 == 0 {
                validate_loaded_state(&registries, &state)
                    .unwrap_or_else(|error| panic!("screening soak audit failed: {error}"));
            }
        }

        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("screening soak final audit failed: {error}"));
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("screening soak final matter failed: {error}"))
                .total(),
            initial_matter
        );
        assert_eq!(
            state
                .energy()
                .get_store(energy)
                .unwrap_or_else(|| panic!("screening soak energy store disappeared"))
                .stored(),
            Energy::from_nanojoules(700_000)
        );
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .unwrap_or_else(|| panic!("screening soak equipment disappeared"))
                .condition(),
            Condition::new(700_000)
                .unwrap_or_else(|error| panic!("screening soak final condition failed: {error}"))
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(undersize)
                .unwrap_or_else(|| panic!("screening soak undersize storage disappeared"))
                .stored_mass(),
            Mass::from_milligrams(1_200)
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(oversize)
                .unwrap_or_else(|| panic!("screening soak oversize storage disappeared"))
                .stored_mass(),
            Mass::from_milligrams(1_800)
        );
        assert_eq!(state.inventory().lot_ids(undersize).count(), 1);
        assert_eq!(state.inventory().lot_ids(oversize).count(), 1);
        state
    }

    #[cfg(feature = "test-soak")]
    #[test]
    #[ignore = "long-horizon soak"]
    fn screening_soak_preserves_conservation_persistence_and_replay() {
        let seed = WorldSeed::new(0x9710_50A5);
        let first = run_screening_soak(seed);
        let second = run_screening_soak(seed);
        assert_eq!(first, second);
    }
}
