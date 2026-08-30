//! Pure-material casting/solidification with exact heat release into a finite thermal-energy sink.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityEvaluationError, CapabilityId, evaluate_capabilities};
use crate::core::quantity::{Energy, Mass, Power, Temperature};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyCarrier, EnergySinkError, EnergyStoreId, PowerDurationError, validate_energy_sink_access,
    validate_energy_sink_release,
};
use crate::equipment::{EquipmentId, EquipmentProviderError, resolve_equipment_provider};
use crate::inventory::MaterialLotSelection;
use crate::inventory::StockpileId;
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::material::{CommodityKey, FormId, MaterialComposition, MaterialId, MaterialLotSpec};
use crate::production::{
    ProcessId, ProcessInputError, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    ProcessResolutionError, ProductionJobId, validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::equipment_physics::{
    ThermalBatchLimitError, ThermalPowerTemperatureError, ThermalTransferTimingError,
    resolve_thermal_power_temperature_limits, resolve_thermal_transfer_timing,
    validate_thermal_batch_mass,
};
use super::phase_change_batch::{
    PurePhaseChangeBatchError, PurePhaseChangeDirection, resolve_pure_phase_change_batch,
};
use super::{PhaseChangeForms, PhaseChangeProcessProfile, calculate_phase_sensible_heat};
#[cfg(test)]
use super::{calculate_fusion_heat, calculate_sensible_heat};

/// Immutable declaration that one selected-batch process solidifies pure liquid matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastingPhaseChange {
    forms: PhaseChangeForms,
    output_temperature: Temperature,
}

impl CastingPhaseChange {
    #[must_use]
    pub const fn new(forms: PhaseChangeForms, output_temperature: Temperature) -> Self {
        assert!(
            output_temperature.millikelvin() > 0,
            "casting output temperature must be above absolute zero"
        );
        Self {
            forms,
            output_temperature,
        }
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
    pub const fn output_temperature(self) -> Temperature {
        self.output_temperature
    }
}

/// Immutable declaration that one selected-batch process solidifies pure liquid matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastingProcessDefinition {
    process: ProcessId,
    profile: PhaseChangeProcessProfile,
    material: MaterialId,
    phase_change: CastingPhaseChange,
}

impl CastingProcessDefinition {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        profile: PhaseChangeProcessProfile,
        material: MaterialId,
        phase_change: CastingPhaseChange,
    ) -> Self {
        Self {
            process,
            profile,
            material,
            phase_change,
        }
    }

    #[must_use]
    pub const fn process(self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn cooling_power_capability(self) -> CapabilityId {
        self.profile.transfer_power_capability()
    }

    #[must_use]
    pub const fn max_temperature_capability(self) -> CapabilityId {
        self.profile.max_temperature_capability()
    }

    #[must_use]
    pub const fn max_batch_mass_capability(self) -> CapabilityId {
        self.profile.max_batch_mass_capability()
    }

    #[must_use]
    pub const fn energy_carrier(self) -> EnergyCarrier {
        self.profile.energy_carrier()
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn liquid_form(self) -> FormId {
        self.phase_change.liquid_form()
    }

    #[must_use]
    pub const fn solid_form(self) -> FormId {
        self.phase_change.solid_form()
    }

    /// Temperature of the solid lot after the casting cycle removes latent and sensible heat.
    #[must_use]
    pub const fn output_temperature(self) -> Temperature {
        self.phase_change.output_temperature()
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.profile.condition_wear_ppm_per_active_tick()
    }
}

/// Failure while deriving solidification physics from exact consumed liquid traces.
pub type CastingBatchError = PurePhaseChangeBatchError;

fn resolve_casting_batch(
    materials: &crate::material::MaterialRegistry,
    material: MaterialId,
    liquid_form: FormId,
    solid_form: FormId,
    output_temperature: Temperature,
    traces: &[crate::inventory::ConsumedMaterialTrace],
) -> Result<super::phase_change_batch::PurePhaseChangeBatch, CastingBatchError> {
    let mut batch = resolve_pure_phase_change_batch(
        materials,
        material,
        &[liquid_form],
        solid_form,
        PurePhaseChangeDirection::Solidify,
        traces,
    )?;
    let solid_cooling = calculate_phase_sensible_heat(
        materials,
        batch.output.mass(),
        CommodityKey::new(batch.material, solid_form),
        batch.output.composition(),
        batch.melting_point,
        output_temperature,
    )
    .map_err(|error| PurePhaseChangeBatchError::SolidCooling {
        material: batch.material,
        error,
    })?;
    batch.transfer_energy = batch
        .transfer_energy
        .checked_add(solid_cooling.energy())
        .ok_or(PurePhaseChangeBatchError::EnergyOverflow)?;
    batch.output = MaterialLotSpec::with_composition(
        CommodityKey::new(batch.material, solid_form),
        batch.output.mass(),
        output_temperature,
        MaterialComposition::pure(batch.material),
    )
    .map_err(PurePhaseChangeBatchError::Output)?;
    Ok(batch)
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
        definition.material(),
        definition.liquid_form(),
        definition.solid_form(),
        definition.output_temperature(),
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
    let energy_sink_access = validate_energy_sink_access(registries, state, energy_sink)
        .map_err(CastingResolutionError::EnergySink)?;
    let provided_carrier = energy_sink_access.carrier();
    if provided_carrier != definition.energy_carrier() {
        return Err(CastingResolutionError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: provided_carrier,
        });
    }
    let timing = resolve_thermal_transfer_timing(
        registries,
        limits.transfer_power(),
        energy_sink_access.max_input_power(),
        batch.transfer_energy,
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
    let energy_sink = validate_energy_sink_release(
        registries,
        energy_sink_access,
        batch.transfer_energy,
        duration,
    )
    .map_err(CastingResolutionError::EnergySink)?;
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
        released_energy: batch.transfer_energy,
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

mod validation;

pub(super) use validation::validate_loaded_casting_job;

#[cfg(test)]
#[path = "casting_execution_tests.rs"]
mod tests;
