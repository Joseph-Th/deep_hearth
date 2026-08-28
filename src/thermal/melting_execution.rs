//! Pure-material melting resolution; sibling thermal code owns sensible heating and shared registry dispatch.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityEvaluationError, CapabilityId, evaluate_capabilities};
use crate::core::quantity::{Energy, Mass, Power, Temperature};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyCarrier, EnergyStoreId, EnergySupplyError, PowerDurationError, validate_energy_supply,
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
    ProcessResolutionError, ProductionJobId, validate_selected_process_inputs,
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
use crate::material::{CommodityKey, MaterialPhase};

/// Immutable declaration that one selected-batch process performs pure-material melting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeltingProcessDefinition {
    process: ProcessId,
    heating_power_capability: CapabilityId,
    max_temperature_capability: CapabilityId,
    max_batch_mass_capability: CapabilityId,
    energy_carrier: EnergyCarrier,
    forms: PhaseChangeForms,
    condition_wear_ppm_per_active_tick: u32,
}

impl MeltingProcessDefinition {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        heating_power_capability: CapabilityId,
        max_temperature_capability: CapabilityId,
        max_batch_mass_capability: CapabilityId,
        energy_carrier: EnergyCarrier,
        forms: PhaseChangeForms,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            process,
            heating_power_capability,
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
    pub const fn heating_power_capability(self) -> CapabilityId {
        self.heating_power_capability
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
    pub const fn solid_form(self) -> FormId {
        self.forms.input()
    }

    #[must_use]
    pub const fn liquid_form(self) -> FormId {
        self.forms.output()
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
    }
}

/// Failure while deriving pure melting physics from exact consumed material traces.
pub type MeltingBatchError = PurePhaseChangeBatchError;

fn resolve_melting_batch(
    materials: &crate::material::MaterialRegistry,
    solid_form: FormId,
    liquid_form: FormId,
    traces: &[crate::inventory::ConsumedMaterialTrace],
) -> Result<super::phase_change_batch::PurePhaseChangeBatch, MeltingBatchError> {
    resolve_pure_phase_change_batch(
        materials,
        solid_form,
        liquid_form,
        PurePhaseChangeDirection::Melt,
        traces,
    )
}

/// Exact runtime selection and providers requested for one melting operation.
#[derive(Clone, Copy, Debug)]
pub struct MeltingRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
}

impl<'selection> MeltingRequest<'selection> {
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

/// Observable physically resolved melting operation before production start.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedMelting {
    resolution: ProcessResolution,
    equipment: EquipmentId,
    material: MaterialId,
    melting_point: Temperature,
    required_energy: Energy,
    transfer_power: Power,
}

impl ResolvedMelting {
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
    pub const fn required_energy(&self) -> Energy {
        self.required_energy
    }

    #[must_use]
    pub const fn transfer_power(&self) -> Power {
        self.transfer_power
    }
}

/// Failure while resolving selected solid matter into a conserved molten production outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeltingResolutionError {
    UnknownThermalProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingHeatingPower {
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
    Batch(MeltingBatchError),
    MeltingPointExceedsEquipmentMaximum {
        melting_point: Temperature,
        maximum: Temperature,
    },
    Energy(EnergySupplyError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    Duration(PowerDurationError),
    ConditionDuration(ActiveConditionDurationError),
    Resolution(ProcessResolutionError),
}

impl Display for MeltingResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownThermalProcess { process } => write!(
                formatter,
                "process {} has no melting resolver definition",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "process input binding failed: {error}"),
            Self::Equipment(error) => write!(formatter, "equipment resolution failed: {error}"),
            Self::Capability(error) => {
                write!(formatter, "equipment capability check failed: {error}")
            }
            Self::MissingHeatingPower { capability } => write!(
                formatter,
                "equipment does not expose configured heating-power capability {}",
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
            Self::Batch(error) => write!(formatter, "melting batch resolution failed: {error}"),
            Self::MeltingPointExceedsEquipmentMaximum {
                melting_point,
                maximum,
            } => write!(
                formatter,
                "material melting point {} mK exceeds equipment maximum {} mK",
                melting_point.millikelvin(),
                maximum.millikelvin()
            ),
            Self::Energy(error) => write!(formatter, "finite energy supply failed: {error}"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "melting process requires {required:?} energy but store provides {provided:?}"
            ),
            Self::Duration(error) => {
                write!(formatter, "melting duration calculation failed: {error}")
            }
            Self::ConditionDuration(error) => {
                write!(
                    formatter,
                    "melting exceeds equipment condition lifetime: {error}"
                )
            }
            Self::Resolution(error) => write!(formatter, "process resolution failed: {error}"),
        }
    }
}

impl Error for MeltingResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Batch(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::Duration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownThermalProcess { process: _process } => None,
            Self::MissingHeatingPower {
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
            Self::MeltingPointExceedsEquipmentMaximum {
                melting_point: _melting_point,
                maximum: _maximum,
            } => None,
            Self::WrongEnergyCarrier {
                required: _required,
                provided: _provided,
            } => None,
        }
    }
}

/// Resolves exact sensible plus latent heat, equipment limits, energy supply, and molten output.
pub fn resolve_melting_process(
    registries: &Registries,
    state: &AppState,
    request: MeltingRequest<'_>,
) -> Result<ResolvedMelting, MeltingResolutionError> {
    let MeltingRequest {
        process,
        source,
        selections,
        equipment,
        energy_store,
    } = request;
    let definition = registries
        .thermal()
        .get_melting(process)
        .ok_or(MeltingResolutionError::UnknownThermalProcess { process })?;
    let inputs = validate_selected_process_inputs(registries, state, process, source, selections)
        .map_err(MeltingResolutionError::Input)?;
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(MeltingResolutionError::Equipment)?;
    let equipment_use = provider.validated_use();
    let process_definition = match registries.production().get_process(process) {
        Some(process_definition) => process_definition,
        None => return Err(MeltingResolutionError::UnknownThermalProcess { process }),
    };
    evaluate_capabilities(
        registries.capabilities(),
        &provider,
        process_definition.capability_requirements(),
    )
    .map_err(MeltingResolutionError::Capability)?;

    let limits = resolve_thermal_power_temperature_limits(
        provider.definition(),
        provider.condition(),
        definition.heating_power_capability(),
        definition.max_temperature_capability(),
    )
    .map_err(|error| match error {
        ThermalPowerTemperatureError::MissingTransferPower => {
            MeltingResolutionError::MissingHeatingPower {
                capability: definition.heating_power_capability(),
            }
        }
        ThermalPowerTemperatureError::MissingMaximumTemperature => {
            MeltingResolutionError::MissingMaximumTemperature {
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
            MeltingResolutionError::MissingMaximumBatchMass {
                capability: definition.max_batch_mass_capability(),
            }
        }
        ThermalBatchLimitError::BatchMassExceeded { selected, maximum } => {
            MeltingResolutionError::BatchMassExceedsEquipmentCapacity { selected, maximum }
        }
    })?;

    let batch = resolve_melting_batch(
        registries.materials(),
        definition.solid_form(),
        definition.liquid_form(),
        inputs.consumed_inputs(),
    )
    .map_err(MeltingResolutionError::Batch)?;
    if batch.melting_point > limits.maximum_temperature() {
        return Err(
            MeltingResolutionError::MeltingPointExceedsEquipmentMaximum {
                melting_point: batch.melting_point,
                maximum: limits.maximum_temperature(),
            },
        );
    }
    let energy_supply =
        validate_energy_supply(registries, state, energy_store, batch.transfer_energy)
            .map_err(MeltingResolutionError::Energy)?;
    let provided_carrier = energy_supply.trace().carrier();
    if provided_carrier != definition.energy_carrier() {
        return Err(MeltingResolutionError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: provided_carrier,
        });
    }
    let timing = resolve_thermal_transfer_timing(
        registries,
        limits.transfer_power(),
        energy_supply.max_output_power(),
        batch.transfer_energy,
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    )
    .map_err(|error| match error {
        ThermalTransferTimingError::Duration(error) => MeltingResolutionError::Duration(error),
        ThermalTransferTimingError::ConditionDuration(error) => {
            MeltingResolutionError::ConditionDuration(error)
        }
    })?;
    let transfer_power = timing.transfer_power();
    let duration = timing.duration();
    let equipment_condition_after = timing.condition_after();
    let resolution = inputs
        .resolve_with_energy_and_equipment(
            duration,
            vec![ProcessOutputStream::new(
                ProcessOutputStreamId::PRIMARY,
                vec![batch.output],
            )],
            energy_supply,
            equipment_use,
            equipment_condition_after,
        )
        .map_err(MeltingResolutionError::Resolution)?;
    Ok(ResolvedMelting {
        resolution,
        equipment,
        material: batch.material,
        melting_point: batch.melting_point,
        required_energy: batch.transfer_energy,
        transfer_power,
    })
}

/// Invalid persisted melting semantics discovered during exhaustive load validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeltingJobValidationError {
    MissingEnergy {
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
    MissingHeatingPowerCapability {
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
        error: MeltingBatchError,
    },
    MeltingPointExceedsEquipmentMaximum {
        job: ProductionJobId,
        melting_point: Temperature,
        maximum: Temperature,
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

impl Display for MeltingJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEnergy { job } => write!(
                formatter,
                "melting job {} has no consumed energy",
                job.value()
            ),
            Self::MissingEquipmentProvider { job } => write!(
                formatter,
                "melting job {} has no equipment provider",
                job.value()
            ),
            Self::UnknownEquipmentDefinition { job } => write!(
                formatter,
                "melting job {} references unavailable equipment",
                job.value()
            ),
            Self::UnknownEnergyDefinition { job } => write!(
                formatter,
                "melting job {} references unavailable energy storage",
                job.value()
            ),
            Self::MissingHeatingPowerCapability { job } => write!(
                formatter,
                "melting job {} provider lacks heating power",
                job.value()
            ),
            Self::MissingMaximumTemperatureCapability { job } => write!(
                formatter,
                "melting job {} provider lacks maximum temperature",
                job.value()
            ),
            Self::MissingMaximumBatchMassCapability { job } => write!(
                formatter,
                "melting job {} provider lacks maximum batch mass",
                job.value()
            ),
            Self::BatchMassExceedsEquipmentCapacity {
                job,
                selected,
                maximum,
            } => write!(
                formatter,
                "melting job {} batch {} mg exceeds provider capacity {} mg",
                job.value(),
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch { job, error } => write!(
                formatter,
                "melting job {} batch cannot be reproduced: {error}",
                job.value()
            ),
            Self::MeltingPointExceedsEquipmentMaximum {
                job,
                melting_point,
                maximum,
            } => write!(
                formatter,
                "melting job {} requires {} mK but provider maximum is {} mK",
                job.value(),
                melting_point.millikelvin(),
                maximum.millikelvin()
            ),
            Self::WrongEnergyCarrier {
                job,
                required,
                provided,
            } => write!(
                formatter,
                "melting job {} requires {required:?} energy but traces {provided:?}",
                job.value()
            ),
            Self::EnergyMismatch {
                job,
                traced,
                required,
            } => write!(
                formatter,
                "melting job {} traces {} nJ but physics requires {} nJ",
                job.value(),
                traced.nanojoules(),
                required.nanojoules()
            ),
            Self::Duration { job, error } => write!(
                formatter,
                "melting job {} duration cannot be recomputed: {error}",
                job.value()
            ),
            Self::ConditionDuration { job, error } => write!(
                formatter,
                "melting job {} exceeds equipment condition lifetime: {error}",
                job.value()
            ),
            Self::DurationMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "melting job {} stores {} ticks but physics requires {} ticks",
                job.value(),
                stored.value(),
                required.value()
            ),
            Self::MissingEquipmentConditionOutcome { job } => write!(
                formatter,
                "melting job {} has no post-operation equipment condition",
                job.value()
            ),
            Self::EquipmentConditionOutcomeMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "melting job {} stores condition {} ppm but active-time wear requires {} ppm",
                job.value(),
                stored.parts_per_million(),
                required.parts_per_million()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "melting job {} molten output does not match its consumed material",
                job.value()
            ),
        }
    }
}

impl Error for MeltingJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Batch { job: _job, error } => Some(error),
            Self::Duration { job: _job, error } => Some(error),
            Self::ConditionDuration { job: _job, error } => Some(error),
            Self::MissingEnergy { job: _job }
            | Self::MissingEquipmentProvider { job: _job }
            | Self::UnknownEquipmentDefinition { job: _job }
            | Self::UnknownEnergyDefinition { job: _job }
            | Self::MissingHeatingPowerCapability { job: _job }
            | Self::MissingMaximumTemperatureCapability { job: _job }
            | Self::MissingMaximumBatchMassCapability { job: _job }
            | Self::MissingEquipmentConditionOutcome { job: _job }
            | Self::OutputMismatch { job: _job } => None,
            Self::BatchMassExceedsEquipmentCapacity {
                job: _job,
                selected: _selected,
                maximum: _maximum,
            } => None,
            Self::MeltingPointExceedsEquipmentMaximum {
                job: _job,
                melting_point: _melting_point,
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

mod validation;

pub(super) use validation::validate_loaded_melting_job;

#[cfg(test)]
#[path = "melting_execution_tests.rs"]
mod tests;
