//! Finite-energy, two-reactant chemical precipitation in an industrial furnace.
//!
//! PbS + Fe -> Pb + FeS. The closed reaction has no gas stream, unlike the
//! carbon-reduction/roasting equations. It is the first executable chemistry,
//! not a generic license to turn sulfide ore into elemental metal.

use std::fmt::{Display, Formatter};

use crate::capability::CapabilityEvaluationError;
use crate::content::{
    FORM_CHIP, FORM_CRUSHED, FORM_MATTE, FORM_MOLTEN, FORM_SCRAP, MATERIAL_GALENA, MATERIAL_IRON,
    MATERIAL_IRON_SULFIDE, MATERIAL_LEAD,
};
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::state::AppState;
use crate::core::throughput::MassFlowDurationError;
use crate::energy::EnergyStoreId;
use crate::energy::{EnergyCarrier, EnergySupplyError, PowerDurationError};
use crate::equipment::EquipmentId;
use crate::equipment::EquipmentProviderError;
use crate::inventory::{ConsumedMaterialTrace, MaterialLotSelection, StockpileId};
use crate::maintenance::ActiveConditionDurationError;
use crate::material::{CommodityKey, MaterialLotSpec};
use crate::production::{
    ProcessId, ProcessInputError, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    ProcessResolutionError, ProductionJobId, ProductionJobRecord, validate_process_inputs,
};
use crate::registry::Registries;

use super::powered_physics::{
    PoweredOreEquipmentError, PoweredOreJobValidationError, PoweredOreProviderError,
    PoweredOreSupplyError, PoweredOreTimingError, resolve_powered_ore_job_replay,
    resolve_powered_ore_provider, resolve_powered_ore_supply, validate_powered_ore_job_replay,
};

// Exactly one-fifth of the balanced PbS + Fe -> Pb + FeS molar charge.
// All four terms are whole milligrams; the 59.021 g unit fits the physically
// constructible four-pot 80 g arc furnace without inventing finer precision.
const GALENA_MG: u64 = 47_852;
const IRON_MG: u64 = 11_169;
const LEAD_MG: u64 = 41_440;
const MATTE_MG: u64 = 17_581;

#[derive(Clone, Copy, Debug)]
pub struct PrecipitationRequest<'a> {
    process: ProcessId,
    source: StockpileId,
    selections: &'a [MaterialLotSelection],
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
}

impl<'a> PrecipitationRequest<'a> {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        source: StockpileId,
        selections: &'a [MaterialLotSelection],
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrecipitationBatchError {
    UnrecognizedInput {
        commodity: CommodityKey,
    },
    ImpureInput {
        commodity: CommodityKey,
    },
    InputTooHot {
        current: Temperature,
        maximum: Temperature,
    },
    MissingReactant,
    InvalidMolarCharge {
        galena: Mass,
        iron: Mass,
    },
    MassOverflow,
}

impl Display for PrecipitationBatchError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PrecipitationBatchError {}

/// Deterministic reaction outputs reconstructed from immutable consumed traces.
fn resolve_precipitation_batch(
    traces: &[ConsumedMaterialTrace],
    temperature: Temperature,
) -> Result<[MaterialLotSpec; 2], PrecipitationBatchError> {
    let mut galena = 0_u64;
    let mut iron = 0_u64;
    for trace in traces {
        let profile = trace.profile();
        let commodity = profile.commodity();
        if profile.temperature() > temperature {
            return Err(PrecipitationBatchError::InputTooHot {
                current: profile.temperature(),
                maximum: temperature,
            });
        }
        if profile
            .composition()
            .parts_per_million(commodity.material())
            != 1_000_000
        {
            return Err(PrecipitationBatchError::ImpureInput { commodity });
        }
        let target = match (commodity.material(), commodity.form()) {
            (MATERIAL_GALENA, FORM_CRUSHED) => &mut galena,
            (MATERIAL_IRON, FORM_SCRAP | FORM_CHIP) => &mut iron,
            _ => return Err(PrecipitationBatchError::UnrecognizedInput { commodity }),
        };
        *target = target
            .checked_add(trace.mass().milligrams())
            .ok_or(PrecipitationBatchError::MassOverflow)?;
    }
    if galena == 0 || iron == 0 {
        return Err(PrecipitationBatchError::MissingReactant);
    }
    let charge_units = galena / GALENA_MG;
    if !galena.is_multiple_of(GALENA_MG)
        || iron
            != charge_units
                .checked_mul(IRON_MG)
                .ok_or(PrecipitationBatchError::MassOverflow)?
    {
        return Err(PrecipitationBatchError::InvalidMolarCharge {
            galena: Mass::from_milligrams(galena),
            iron: Mass::from_milligrams(iron),
        });
    }
    let lead_mass = charge_units
        .checked_mul(LEAD_MG)
        .ok_or(PrecipitationBatchError::MassOverflow)?;
    let matte_mass = charge_units
        .checked_mul(MATTE_MG)
        .ok_or(PrecipitationBatchError::MassOverflow)?;
    debug_assert_eq!(galena + iron, lead_mass + matte_mass);
    Ok([
        MaterialLotSpec::new(
            CommodityKey::new(MATERIAL_LEAD, FORM_MOLTEN),
            Mass::from_milligrams(lead_mass),
            temperature,
        ),
        MaterialLotSpec::new(
            CommodityKey::new(MATERIAL_IRON_SULFIDE, FORM_MATTE),
            Mass::from_milligrams(matte_mass),
            temperature,
        ),
    ])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrecipitationResolutionError {
    UnknownProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
    Batch(PrecipitationBatchError),
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingMassFlowCapability,
    MissingMaximumBatchMassCapability,
    BatchMassExceeded {
        selected: Mass,
        maximum: Mass,
    },
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

impl Display for PrecipitationResolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PrecipitationResolutionError {}

impl From<PoweredOreProviderError> for PrecipitationResolutionError {
    fn from(error: PoweredOreProviderError) -> Self {
        match error {
            PoweredOreProviderError::Provider(error) => Self::Equipment(error),
            PoweredOreProviderError::Capability(error) => Self::Capability(error),
            PoweredOreProviderError::Equipment(error) => match error {
                PoweredOreEquipmentError::MissingMassFlowCapability => {
                    Self::MissingMassFlowCapability
                }
                PoweredOreEquipmentError::MissingMaximumBatchMassCapability => {
                    Self::MissingMaximumBatchMassCapability
                }
                PoweredOreEquipmentError::BatchMassExceeded { selected, maximum } => {
                    Self::BatchMassExceeded { selected, maximum }
                }
            },
        }
    }
}
impl From<PoweredOreSupplyError> for PrecipitationResolutionError {
    fn from(error: PoweredOreSupplyError) -> Self {
        match error {
            PoweredOreSupplyError::Supply(error) => Self::Energy(error),
            PoweredOreSupplyError::WrongEnergyCarrier { required, provided } => {
                Self::WrongEnergyCarrier { required, provided }
            }
            PoweredOreSupplyError::Timing(error) => match error {
                PoweredOreTimingError::Throughput(error) => Self::ThroughputDuration(error),
                PoweredOreTimingError::Energy(error) => Self::EnergyDuration(error),
                PoweredOreTimingError::Condition(error) => Self::ConditionDuration(error),
            },
        }
    }
}

#[must_use]
#[derive(Debug)]
pub struct ResolvedPrecipitation {
    resolution: ProcessResolution,
    required_energy: Energy,
    equipment: EquipmentId,
}

impl ResolvedPrecipitation {
    pub const fn process_resolution(&self) -> &ProcessResolution {
        &self.resolution
    }
    pub const fn required_energy(&self) -> Energy {
        self.required_energy
    }
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
    }
}

/// Validates two **physically selected** reactants, their integer mole charge,
/// installed furnace throughput/temperature, finite electrical supply and
/// separate molten-lead versus sulfide-matte custody streams.
pub fn resolve_precipitation_process(
    registries: &Registries,
    state: &AppState,
    request: PrecipitationRequest<'_>,
) -> Result<ResolvedPrecipitation, PrecipitationResolutionError> {
    let definition = registries
        .ore_processing()
        .get_precipitation(request.process)
        .ok_or(PrecipitationResolutionError::UnknownProcess {
            process: request.process,
        })?;
    let inputs =
        validate_process_inputs(state, request.process, request.source, request.selections)
            .map_err(PrecipitationResolutionError::Input)?;
    let mass = inputs.input_mass();
    let provider = resolve_powered_ore_provider(
        registries,
        state,
        request.process,
        request.equipment,
        definition.operating_profile(),
        mass,
    )
    .map_err(PrecipitationResolutionError::from)?;
    let [lead, matte] =
        resolve_precipitation_batch(inputs.consumed_inputs(), definition.operating_temperature())
            .map_err(PrecipitationResolutionError::Batch)?;
    let supply = resolve_powered_ore_supply(
        registries,
        state,
        request.energy_store,
        definition.operating_profile(),
        mass,
        provider.processing_rate(),
        provider.condition_before(),
    )
    .map_err(PrecipitationResolutionError::from)?;
    let resolution = inputs
        .resolve_with_energy_and_equipment(
            supply.duration(),
            vec![
                ProcessOutputStream::new(ProcessOutputStreamId::PRIMARY, vec![lead]),
                ProcessOutputStream::new(ProcessOutputStreamId::RESIDUE, vec![matte]),
            ],
            supply.energy_supply(),
            provider.validated_use(),
            supply.condition_after(),
        )
        .map_err(PrecipitationResolutionError::Resolution)?;
    Ok(ResolvedPrecipitation {
        resolution,
        required_energy: supply.required_energy(),
        equipment: provider.id(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrecipitationJobValidationError {
    Powered(PoweredOreJobValidationError),
    Batch(PrecipitationBatchError),
    OutputMismatch { job: ProductionJobId },
}

impl Display for PrecipitationJobValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PrecipitationJobValidationError {}

pub(crate) fn validate_loaded_precipitation_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), PrecipitationJobValidationError> {
    let Some(definition) = registries.ore_processing().get_precipitation(job.process()) else {
        return Ok(());
    };
    let replay = resolve_powered_ore_job_replay(registries, job, definition.operating_profile())
        .map_err(PrecipitationJobValidationError::Powered)?;
    let [lead, matte] =
        resolve_precipitation_batch(job.consumed_inputs(), definition.operating_temperature())
            .map_err(PrecipitationJobValidationError::Batch)?;
    let matches = match job.output_streams() {
        [primary, residue] => {
            primary.id() == ProcessOutputStreamId::PRIMARY
                && residue.id() == ProcessOutputStreamId::RESIDUE
                && primary.outputs() == [lead]
                && residue.outputs() == [matte]
        }
        _ => false,
    };
    if !matches {
        return Err(PrecipitationJobValidationError::OutputMismatch { job: job.id() });
    }
    validate_powered_ore_job_replay(registries, job, replay)
        .map_err(PrecipitationJobValidationError::Powered)
}

#[cfg(test)]
#[path = "precipitation_execution_tests.rs"]
mod tests;
