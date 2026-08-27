//! Pure-material casting/solidification with exact heat release into a finite thermal-energy sink.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityEvaluationError, CapabilityId, evaluate_capabilities};
use crate::core::quantity::{Energy, Mass, Power, Temperature};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyCarrier, EnergySinkError, EnergyStoreId, PowerDurationError, validate_energy_sink,
};
use crate::equipment::{EquipmentId, EquipmentProviderError, resolve_equipment_provider};
use crate::inventory::MaterialLotSelection;
use crate::inventory::StockpileId;
use crate::maintenance::{
    ActiveConditionDurationError, Condition, assert_valid_condition_wear_ppm_per_tick,
};
use crate::material::{FormId, MaterialId};
use crate::production::{
    ProcessId, ProcessInputError, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    ProcessResolutionError, ProductionJobId, ProductionJobRecord, validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::PhaseChangeForms;
use super::equipment_physics::{
    ThermalBatchLimitError, ThermalPowerTemperatureError, ThermalTransferTimingError,
    resolve_thermal_power_temperature_limits, resolve_thermal_transfer_timing,
    validate_thermal_batch_mass,
};
use super::phase_change_batch::{
    PurePhaseChangeBatchError, PurePhaseChangeDirection, resolve_pure_phase_change_batch,
};
#[cfg(test)]
use super::{calculate_fusion_heat, calculate_sensible_heat};
#[cfg(test)]
use crate::material::{CommodityKey, MaterialComposition};

/// Immutable declaration that one selected-batch process solidifies pure liquid matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastingProcessDefinition {
    process: ProcessId,
    cooling_power_capability: CapabilityId,
    max_temperature_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
    forms: PhaseChangeForms,
    condition_wear_ppm_per_active_tick: u32,
}

impl CastingProcessDefinition {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        cooling_power_capability: CapabilityId,
        max_temperature_capability: CapabilityId,
        max_batch_mass_capability: CapabilityId,
        energy_carrier: EnergyCarrier,
        forms: PhaseChangeForms,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            process,
            cooling_power_capability,
            max_temperature_capability,
            max_batch_mass_capability,
            energy_carrier,
            forms,
            condition_wear_ppm_per_active_tick,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn cooling_power_capability(self) -> CapabilityId {
        self.cooling_power_capability
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
    pub const fn liquid_form(self) -> FormId {
        self.forms.input()
    }

    #[must_use]
    pub const fn solid_form(self) -> FormId {
        self.forms.output()
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }
}

/// Failure while deriving solidification physics from exact consumed liquid traces.
pub type CastingBatchError = PurePhaseChangeBatchError;

fn resolve_casting_batch(
    materials: &crate::material::MaterialRegistry,
    liquid_form: FormId,
    solid_form: FormId,
    traces: &[crate::inventory::ConsumedMaterialTrace],
) -> Result<super::phase_change_batch::PurePhaseChangeBatch, CastingBatchError> {
    resolve_pure_phase_change_batch(
        materials,
        liquid_form,
        solid_form,
        PurePhaseChangeDirection::Solidify,
        traces,
    )
}

/// Exact runtime selection, cooling equipment, and finite heat sink for one casting operation.
#[derive(Clone, Copy, Debug)]
pub struct CastingRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
    equipment: EquipmentId,
    energy_sink: EnergyStoreId,
}

impl<'selection> CastingRequest<'selection> {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        source: StockpileId,
        selections: &'selection [MaterialLotSelection],
        equipment: EquipmentId,
        energy_sink: EnergyStoreId,
    ) -> Self {
        Self {
            process,
            source,
            selections,
            equipment,
            energy_sink,
        }
    }
}

/// Observable physically resolved casting operation before production start.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCasting {
    resolution: ProcessResolution,
    equipment: EquipmentId,
    material: MaterialId,
    melting_point: Temperature,
    released_energy: Energy,
    transfer_power: Power,
}

impl ResolvedCasting {
    pub const fn process_resolution(&self) -> &ProcessResolution {
        &self.resolution
    }

    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn material(&self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn melting_point(&self) -> Temperature {
        self.melting_point
    }

    #[must_use]
    pub const fn released_energy(&self) -> Energy {
        self.released_energy
    }

    #[must_use]
    pub const fn transfer_power(&self) -> Power {
        self.transfer_power
    }
}

/// Failure while resolving selected liquid matter into a conserved solid casting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastingResolutionError {
    UnknownThermalProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingCoolingPower {
        capability: CapabilityId,
    },
    MissingMaximumTemperature {
        capability: CapabilityId,
    },
    MissingMaximumBatchMass {
        capability: CapabilityId,
    },
    BatchMassExceedsEquipmentCapacity {
        selected: Mass,
        maximum: Mass,
    },
    Batch(CastingBatchError),
    InputTemperatureExceedsEquipmentMaximum {
        input: Temperature,
        maximum: Temperature,
    },
    EnergySink(EnergySinkError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    Duration(PowerDurationError),
    ConditionDuration(ActiveConditionDurationError),
    Resolution(ProcessResolutionError),
}

impl Display for CastingResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownThermalProcess { process } => write!(
                formatter,
                "process {} has no casting resolver definition",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "process input binding failed: {error}"),
            Self::Equipment(error) => write!(formatter, "equipment resolution failed: {error}"),
            Self::Capability(error) => {
                write!(formatter, "equipment capability check failed: {error}")
            }
            Self::MissingCoolingPower { capability } => write!(
                formatter,
                "equipment does not expose configured cooling-power capability {}",
                capability.value()
            ),
            Self::MissingMaximumTemperature { capability } => write!(
                formatter,
                "equipment does not expose configured maximum-temperature capability {}",
                capability.value()
            ),
            Self::MissingMaximumBatchMass { capability } => write!(
                formatter,
                "equipment does not expose configured maximum-batch-mass capability {}",
                capability.value()
            ),
            Self::BatchMassExceedsEquipmentCapacity { selected, maximum } => write!(
                formatter,
                "selected batch {} mg exceeds equipment capacity {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch(error) => write!(formatter, "casting batch resolution failed: {error}"),
            Self::InputTemperatureExceedsEquipmentMaximum { input, maximum } => write!(
                formatter,
                "casting input temperature {} mK exceeds equipment maximum {} mK",
                input.millikelvin(),
                maximum.millikelvin()
            ),
            Self::EnergySink(error) => write!(formatter, "finite thermal sink failed: {error}"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "casting process releases {required:?} energy but sink stores {provided:?}"
            ),
            Self::Duration(error) => {
                write!(formatter, "casting duration calculation failed: {error}")
            }
            Self::ConditionDuration(error) => {
                write!(
                    formatter,
                    "casting exceeds equipment condition lifetime: {error}"
                )
            }
            Self::Resolution(error) => write!(formatter, "process resolution failed: {error}"),
        }
    }
}

impl Error for CastingResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Batch(error) => Some(error),
            Self::EnergySink(error) => Some(error),
            Self::Duration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownThermalProcess { process: _process } => None,
            Self::MissingCoolingPower {
                capability: _capability,
            }
            | Self::MissingMaximumTemperature {
                capability: _capability,
            }
            | Self::MissingMaximumBatchMass {
                capability: _capability,
            } => None,
            Self::BatchMassExceedsEquipmentCapacity {
                selected: _selected,
                maximum: _maximum,
            } => None,
            Self::InputTemperatureExceedsEquipmentMaximum {
                input: _input,
                maximum: _maximum,
            } => None,
            Self::WrongEnergyCarrier {
                required: _required,
                provided: _provided,
            } => None,
        }
    }
}

/// Resolves exact sensible plus latent heat release, cooling limits, sink capacity, and solid output.
pub fn resolve_casting_process(
    registries: &Registries,
    state: &AppState,
    request: CastingRequest<'_>,
) -> Result<ResolvedCasting, CastingResolutionError> {
    let CastingRequest {
        process,
        source,
        selections,
        equipment,
        energy_sink,
    } = request;
    let definition = registries
        .thermal()
        .get_casting(process)
        .ok_or(CastingResolutionError::UnknownThermalProcess { process })?;
    let inputs = validate_selected_process_inputs(registries, state, process, source, selections)
        .map_err(CastingResolutionError::Input)?;
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(CastingResolutionError::Equipment)?;
    let equipment_use = provider.validated_use();
    let process_definition = match registries.production().get_process(process) {
        Some(process_definition) => process_definition,
        None => return Err(CastingResolutionError::UnknownThermalProcess { process }),
    };
    evaluate_capabilities(
        registries.capabilities(),
        &provider,
        process_definition.capability_requirements(),
    )
    .map_err(CastingResolutionError::Capability)?;

    let limits = resolve_thermal_power_temperature_limits(
        provider.definition(),
        provider.condition(),
        definition.cooling_power_capability(),
        definition.max_temperature_capability(),
    )
    .map_err(|error| match error {
        ThermalPowerTemperatureError::MissingTransferPower => {
            CastingResolutionError::MissingCoolingPower {
                capability: definition.cooling_power_capability(),
            }
        }
        ThermalPowerTemperatureError::MissingMaximumTemperature => {
            CastingResolutionError::MissingMaximumTemperature {
                capability: definition.max_temperature_capability(),
            }
        }
    })?;
    validate_thermal_batch_mass(
        provider.definition(),
        provider.condition(),
        definition.max_batch_mass_capability(),
        inputs.input_mass(),
    )
    .map_err(|error| match error {
        ThermalBatchLimitError::MissingMaximumBatchMass => {
            CastingResolutionError::MissingMaximumBatchMass {
                capability: definition.max_batch_mass_capability(),
            }
        }
        ThermalBatchLimitError::BatchMassExceeded { selected, maximum } => {
            CastingResolutionError::BatchMassExceedsEquipmentCapacity { selected, maximum }
        }
    })?;

    let batch = resolve_casting_batch(
        registries.materials(),
        definition.liquid_form(),
        definition.solid_form(),
        inputs.consumed_inputs(),
    )
    .map_err(CastingResolutionError::Batch)?;
    if batch.hottest_input > limits.maximum_temperature() {
        return Err(
            CastingResolutionError::InputTemperatureExceedsEquipmentMaximum {
                input: batch.hottest_input,
                maximum: limits.maximum_temperature(),
            },
        );
    }
    let energy_sink = validate_energy_sink(registries, state, energy_sink, batch.phase_energy)
        .map_err(CastingResolutionError::EnergySink)?;
    let provided_carrier = energy_sink.trace().carrier();
    if provided_carrier != definition.energy_carrier() {
        return Err(CastingResolutionError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: provided_carrier,
        });
    }
    let timing = resolve_thermal_transfer_timing(
        registries,
        limits.transfer_power(),
        energy_sink.max_input_power(),
        batch.phase_energy,
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    )
    .map_err(|error| match error {
        ThermalTransferTimingError::Duration(error) => CastingResolutionError::Duration(error),
        ThermalTransferTimingError::ConditionDuration(error) => {
            CastingResolutionError::ConditionDuration(error)
        }
    })?;
    let transfer_power = timing.transfer_power();
    let duration = timing.duration();
    let equipment_condition_after = timing.condition_after();
    let resolution = inputs
        .resolve_with_equipment_and_energy_release(
            duration,
            vec![ProcessOutputStream::new(
                ProcessOutputStreamId::PRIMARY,
                vec![batch.output],
            )],
            energy_sink,
            equipment_use,
            equipment_condition_after,
        )
        .map_err(CastingResolutionError::Resolution)?;
    Ok(ResolvedCasting {
        resolution,
        equipment,
        material: batch.material,
        melting_point: batch.melting_point,
        released_energy: batch.phase_energy,
        transfer_power,
    })
}

/// Invalid persisted casting semantics discovered during exhaustive load validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastingJobValidationError {
    UnexpectedConsumedEnergy {
        job: ProductionJobId,
    },
    MissingReleasedEnergy {
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
    MissingCoolingPowerCapability {
        job: ProductionJobId,
    },
    MissingMaximumTemperatureCapability {
        job: ProductionJobId,
    },
    MissingMaximumBatchMassCapability {
        job: ProductionJobId,
    },
    BatchMassExceedsEquipmentCapacity {
        job: ProductionJobId,
        selected: Mass,
        maximum: Mass,
    },
    Batch {
        job: ProductionJobId,
        error: CastingBatchError,
    },
    InputTemperatureExceedsEquipmentMaximum {
        job: ProductionJobId,
        input: Temperature,
        maximum: Temperature,
    },
    WrongEnergyCarrier {
        job: ProductionJobId,
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    ReleasedEnergyMismatch {
        job: ProductionJobId,
        traced: Energy,
        required: Energy,
    },
    Duration {
        job: ProductionJobId,
        error: PowerDurationError,
    },
    ConditionDuration {
        job: ProductionJobId,
        error: ActiveConditionDurationError,
    },
    DurationMismatch {
        job: ProductionJobId,
        stored: TickSpan,
        required: TickSpan,
    },
    MissingEquipmentConditionOutcome {
        job: ProductionJobId,
    },
    EquipmentConditionOutcomeMismatch {
        job: ProductionJobId,
        stored: Condition,
        required: Condition,
    },
    OutputMismatch {
        job: ProductionJobId,
    },
}

impl Display for CastingJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedConsumedEnergy { job } => write!(
                formatter,
                "casting job {} unexpectedly consumes finite energy",
                job.value()
            ),
            Self::MissingReleasedEnergy { job } => write!(
                formatter,
                "casting job {} has no released-energy trace",
                job.value()
            ),
            Self::MissingEquipmentProvider { job } => write!(
                formatter,
                "casting job {} has no equipment provider",
                job.value()
            ),
            Self::UnknownEquipmentDefinition { job } => write!(
                formatter,
                "casting job {} references unavailable equipment",
                job.value()
            ),
            Self::UnknownEnergyDefinition { job } => write!(
                formatter,
                "casting job {} references unavailable thermal sink definition",
                job.value()
            ),
            Self::MissingCoolingPowerCapability { job } => write!(
                formatter,
                "casting job {} provider lacks cooling power",
                job.value()
            ),
            Self::MissingMaximumTemperatureCapability { job } => write!(
                formatter,
                "casting job {} provider lacks maximum temperature",
                job.value()
            ),
            Self::MissingMaximumBatchMassCapability { job } => write!(
                formatter,
                "casting job {} provider lacks maximum batch mass",
                job.value()
            ),
            Self::BatchMassExceedsEquipmentCapacity {
                job,
                selected,
                maximum,
            } => write!(
                formatter,
                "casting job {} batch {} mg exceeds provider capacity {} mg",
                job.value(),
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch { job, error } => write!(
                formatter,
                "casting job {} batch cannot be reproduced: {error}",
                job.value()
            ),
            Self::InputTemperatureExceedsEquipmentMaximum {
                job,
                input,
                maximum,
            } => write!(
                formatter,
                "casting job {} input {} mK exceeds provider maximum {} mK",
                job.value(),
                input.millikelvin(),
                maximum.millikelvin()
            ),
            Self::WrongEnergyCarrier {
                job,
                required,
                provided,
            } => write!(
                formatter,
                "casting job {} releases {required:?} energy but traces {provided:?}",
                job.value()
            ),
            Self::ReleasedEnergyMismatch {
                job,
                traced,
                required,
            } => write!(
                formatter,
                "casting job {} traces {} nJ released but physics requires {} nJ",
                job.value(),
                traced.nanojoules(),
                required.nanojoules()
            ),
            Self::Duration { job, error } => write!(
                formatter,
                "casting job {} duration cannot be recomputed: {error}",
                job.value()
            ),
            Self::ConditionDuration { job, error } => write!(
                formatter,
                "casting job {} exceeds equipment condition lifetime: {error}",
                job.value()
            ),
            Self::DurationMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "casting job {} stores {} ticks but physics requires {} ticks",
                job.value(),
                stored.value(),
                required.value()
            ),
            Self::MissingEquipmentConditionOutcome { job } => write!(
                formatter,
                "casting job {} has no post-operation equipment condition",
                job.value()
            ),
            Self::EquipmentConditionOutcomeMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "casting job {} stores condition {} ppm but active-time wear requires {} ppm",
                job.value(),
                stored.parts_per_million(),
                required.parts_per_million()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "casting job {} solid output does not match its consumed liquid material",
                job.value()
            ),
        }
    }
}

impl Error for CastingJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Batch { job: _job, error } => Some(error),
            Self::Duration { job: _job, error } => Some(error),
            Self::ConditionDuration { job: _job, error } => Some(error),
            Self::UnexpectedConsumedEnergy { job: _job }
            | Self::MissingReleasedEnergy { job: _job }
            | Self::MissingEquipmentProvider { job: _job }
            | Self::UnknownEquipmentDefinition { job: _job }
            | Self::UnknownEnergyDefinition { job: _job }
            | Self::MissingCoolingPowerCapability { job: _job }
            | Self::MissingMaximumTemperatureCapability { job: _job }
            | Self::MissingMaximumBatchMassCapability { job: _job }
            | Self::MissingEquipmentConditionOutcome { job: _job }
            | Self::OutputMismatch { job: _job } => None,
            Self::BatchMassExceedsEquipmentCapacity {
                job: _job,
                selected: _selected,
                maximum: _maximum,
            } => None,
            Self::InputTemperatureExceedsEquipmentMaximum {
                job: _job,
                input: _input,
                maximum: _maximum,
            } => None,
            Self::WrongEnergyCarrier {
                job: _job,
                required: _required,
                provided: _provided,
            } => None,
            Self::ReleasedEnergyMismatch {
                job: _job,
                traced: _traced,
                required: _required,
            } => None,
            Self::DurationMismatch {
                job: _job,
                stored: _stored,
                required: _required,
            } => None,
            Self::EquipmentConditionOutcomeMismatch {
                job: _job,
                stored: _stored,
                required: _required,
            } => None,
        }
    }
}

pub(super) fn validate_loaded_casting_job(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: CastingProcessDefinition,
) -> Result<(), CastingJobValidationError> {
    if job.consumed_energy().is_some() {
        return Err(CastingJobValidationError::UnexpectedConsumedEnergy { job: job.id() });
    }
    let Some(released_energy) = job.released_energy() else {
        return Err(CastingJobValidationError::MissingReleasedEnergy { job: job.id() });
    };
    let Some(provider) = job.equipment_provider() else {
        return Err(CastingJobValidationError::MissingEquipmentProvider { job: job.id() });
    };
    let Some(equipment_definition) = registries.equipment().get_equipment(provider.definition())
    else {
        return Err(CastingJobValidationError::UnknownEquipmentDefinition { job: job.id() });
    };
    let Some(energy_definition) = registries.energy().get_store(released_energy.definition())
    else {
        return Err(CastingJobValidationError::UnknownEnergyDefinition { job: job.id() });
    };
    let limits = resolve_thermal_power_temperature_limits(
        equipment_definition,
        provider.condition(),
        definition.cooling_power_capability(),
        definition.max_temperature_capability(),
    )
    .map_err(|error| match error {
        ThermalPowerTemperatureError::MissingTransferPower => {
            CastingJobValidationError::MissingCoolingPowerCapability { job: job.id() }
        }
        ThermalPowerTemperatureError::MissingMaximumTemperature => {
            CastingJobValidationError::MissingMaximumTemperatureCapability { job: job.id() }
        }
    })?;
    validate_thermal_batch_mass(
        equipment_definition,
        provider.condition(),
        definition.max_batch_mass_capability(),
        job.consumed_mass(),
    )
    .map_err(|error| match error {
        ThermalBatchLimitError::MissingMaximumBatchMass => {
            CastingJobValidationError::MissingMaximumBatchMassCapability { job: job.id() }
        }
        ThermalBatchLimitError::BatchMassExceeded { selected, maximum } => {
            CastingJobValidationError::BatchMassExceedsEquipmentCapacity {
                job: job.id(),
                selected,
                maximum,
            }
        }
    })?;
    let batch = resolve_casting_batch(
        registries.materials(),
        definition.liquid_form(),
        definition.solid_form(),
        job.consumed_inputs(),
    )
    .map_err(|error| CastingJobValidationError::Batch {
        job: job.id(),
        error,
    })?;
    if batch.hottest_input > limits.maximum_temperature() {
        return Err(
            CastingJobValidationError::InputTemperatureExceedsEquipmentMaximum {
                job: job.id(),
                input: batch.hottest_input,
                maximum: limits.maximum_temperature(),
            },
        );
    }
    if released_energy.carrier() != definition.energy_carrier() {
        return Err(CastingJobValidationError::WrongEnergyCarrier {
            job: job.id(),
            required: definition.energy_carrier(),
            provided: released_energy.carrier(),
        });
    }
    if released_energy.energy() != batch.phase_energy {
        return Err(CastingJobValidationError::ReleasedEnergyMismatch {
            job: job.id(),
            traced: released_energy.energy(),
            required: batch.phase_energy,
        });
    }
    let timing = resolve_thermal_transfer_timing(
        registries,
        limits.transfer_power(),
        energy_definition.max_input_power(),
        batch.phase_energy,
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    )
    .map_err(|error| match error {
        ThermalTransferTimingError::Duration(error) => CastingJobValidationError::Duration {
            job: job.id(),
            error,
        },
        ThermalTransferTimingError::ConditionDuration(error) => {
            CastingJobValidationError::ConditionDuration {
                job: job.id(),
                error,
            }
        }
    })?;
    let required_duration = timing.duration();
    let stored_duration = job.active_duration();
    if stored_duration != required_duration {
        return Err(CastingJobValidationError::DurationMismatch {
            job: job.id(),
            stored: stored_duration,
            required: required_duration,
        });
    }
    let required_condition_after = timing.condition_after();
    let Some(stored_condition_after) = job.equipment_condition_after() else {
        return Err(CastingJobValidationError::MissingEquipmentConditionOutcome { job: job.id() });
    };
    if stored_condition_after != required_condition_after {
        return Err(
            CastingJobValidationError::EquipmentConditionOutcomeMismatch {
                job: job.id(),
                stored: stored_condition_after,
                required: required_condition_after,
            },
        );
    }
    let Some(output_stream) = job.single_output_stream() else {
        return Err(CastingJobValidationError::OutputMismatch { job: job.id() });
    };
    if output_stream.outputs() != [batch.output] {
        return Err(CastingJobValidationError::OutputMismatch { job: job.id() });
    }
    Ok(())
}

#[cfg(test)]
#[path = "casting_execution_tests.rs"]
mod tests;
