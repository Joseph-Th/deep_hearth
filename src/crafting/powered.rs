//! Finite-mechanical-work execution for unattended crafting transformations.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityId, CapabilityValue, CapabilityValueKind};
use crate::core::quantity::{Mass, MassFlow};
use crate::core::state::AppState;
use crate::core::throughput::{MassFlowDurationError, calculate_mass_flow_duration_ceiling};
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyCarrier, EnergyStoreId, EnergySupplyError, PowerDurationError,
    assess_energy_supply_access, calculate_mass_specific_energy, calculate_power_duration_ceiling,
    validate_energy_supply_request,
};
use crate::equipment::{
    EquipmentId, EquipmentProviderError, resolve_equipment_capability, resolve_equipment_provider,
};
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::maintenance::{
    ActiveConditionDurationError, Condition, calculate_usable_condition_after_active_ticks,
};
use crate::material::MaterialLotSpecError;
use crate::production::{
    ProcessId, ProcessInputError, ProcessResolution, ProcessResolutionError, StartProcessError,
    ValidatedStartProcess, validate_process_inputs, validate_start_process,
};
use crate::registry::Registries;

use super::batch::{
    ManualCraftBatchError, build_manual_craft_outputs, validate_manual_craft_batch,
};

/// Exact powered crafting request with explicit matter, machine, and finite-work source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoweredCraftRequest {
    process: ProcessId,
    source: StockpileId,
    selections: Vec<MaterialLotSelection>,
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
}

impl PoweredCraftRequest {
    #[must_use]
    pub fn new(
        process: ProcessId,
        source: StockpileId,
        selections: Vec<MaterialLotSelection>,
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

    #[must_use]
    pub fn single(
        process: ProcessId,
        source: StockpileId,
        selection: MaterialLotSelection,
        equipment: EquipmentId,
        energy_store: EnergyStoreId,
    ) -> Self {
        Self::new(process, source, vec![selection], equipment, energy_store)
    }
}

/// Failure while resolving an unattended machine crafting operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoweredCraftError {
    UnknownProcess {
        process: ProcessId,
    },
    MissingTransform {
        process: ProcessId,
    },
    Input(ProcessInputError),
    EmptyInput,
    InputCommodityMismatch,
    InputCompositionMismatch,
    MixedInputTemperature,
    InputMassNotWholeBatches {
        consumed: Mass,
        batch_mass: Mass,
    },
    Equipment(EquipmentProviderError),
    MissingEquipmentCapability {
        equipment: EquipmentId,
        capability: CapabilityId,
    },
    EquipmentCapabilityKindMismatch {
        equipment: EquipmentId,
        capability: CapabilityId,
        found: CapabilityValueKind,
    },
    Energy(EnergySupplyError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    ThroughputDuration(MassFlowDurationError),
    EnergyDuration(PowerDurationError),
    EquipmentCondition(ActiveConditionDurationError),
    OutputMassOverflow,
    Output(MaterialLotSpecError),
    Resolution(ProcessResolutionError),
}

impl Display for PoweredCraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProcess { process } => write!(
                formatter,
                "unknown powered craft process {}",
                process.value()
            ),
            Self::MissingTransform { process } => write!(
                formatter,
                "powered craft process {} lost its material transform",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "powered craft input is invalid: {error}"),
            Self::EmptyInput => formatter.write_str("powered craft selection is empty"),
            Self::InputCommodityMismatch => formatter
                .write_str("powered craft input commodity does not match its authored transform"),
            Self::InputCompositionMismatch => formatter
                .write_str("powered craft input composition does not match its authored transform"),
            Self::MixedInputTemperature => {
                formatter.write_str("powered craft cannot combine mixed input temperatures")
            }
            Self::InputMassNotWholeBatches {
                consumed,
                batch_mass,
            } => write!(
                formatter,
                "powered craft selected {} mg, not a whole number of {} mg transform batches",
                consumed.milligrams(),
                batch_mass.milligrams()
            ),
            Self::Equipment(error) => {
                write!(formatter, "powered craft equipment is unavailable: {error}")
            }
            Self::MissingEquipmentCapability {
                equipment,
                capability,
            } => write!(
                formatter,
                "powered craft equipment {} lacks throughput capability {}",
                equipment.value(),
                capability.value()
            ),
            Self::EquipmentCapabilityKindMismatch {
                equipment,
                capability,
                found,
            } => write!(
                formatter,
                "powered craft equipment {} capability {} has {found:?}, not mass throughput",
                equipment.value(),
                capability.value()
            ),
            Self::Energy(error) => write!(
                formatter,
                "powered craft energy supply is unavailable: {error}"
            ),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "powered craft requires {required:?} energy but selected store supplies {provided:?}"
            ),
            Self::ThroughputDuration(error) => write!(
                formatter,
                "powered craft throughput cannot schedule work: {error}"
            ),
            Self::EnergyDuration(error) => write!(
                formatter,
                "powered craft energy delivery cannot schedule work: {error}"
            ),
            Self::EquipmentCondition(error) => write!(
                formatter,
                "powered craft equipment cannot remain productive: {error}"
            ),
            Self::OutputMassOverflow => formatter.write_str("powered craft output mass overflowed"),
            Self::Output(error) => write!(formatter, "powered craft output is invalid: {error}"),
            Self::Resolution(error) => {
                write!(formatter, "powered craft resolution is invalid: {error}")
            }
        }
    }
}

impl Error for PoweredCraftError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::ThroughputDuration(error) => Some(error),
            Self::EnergyDuration(error) => Some(error),
            Self::EquipmentCondition(error) => Some(error),
            Self::Output(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownProcess { .. }
            | Self::MissingTransform { .. }
            | Self::EmptyInput
            | Self::InputCommodityMismatch
            | Self::InputCompositionMismatch
            | Self::MixedInputTemperature
            | Self::InputMassNotWholeBatches { .. }
            | Self::MissingEquipmentCapability { .. }
            | Self::EquipmentCapabilityKindMismatch { .. }
            | Self::WrongEnergyCarrier { .. }
            | Self::OutputMassOverflow => None,
        }
    }
}

fn batch_error(error: ManualCraftBatchError) -> PoweredCraftError {
    match error {
        ManualCraftBatchError::EmptyInput => PoweredCraftError::EmptyInput,
        ManualCraftBatchError::InputCommodityMismatch => PoweredCraftError::InputCommodityMismatch,
        ManualCraftBatchError::InputCompositionMismatch => {
            PoweredCraftError::InputCompositionMismatch
        }
        ManualCraftBatchError::MixedInputTemperature => PoweredCraftError::MixedInputTemperature,
        ManualCraftBatchError::InputMassNotWholeBatches {
            consumed,
            batch_mass,
        } => PoweredCraftError::InputMassNotWholeBatches {
            consumed,
            batch_mass,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PoweredCraftTimingError {
    ThroughputDuration(MassFlowDurationError),
    EnergyDuration(PowerDurationError),
    EquipmentCondition(ActiveConditionDurationError),
}

pub(crate) fn resolve_powered_craft_timing(
    registries: &Registries,
    rate: MassFlow,
    mass: Mass,
    required_energy: crate::core::quantity::Energy,
    available_power: crate::core::quantity::Power,
    wear_ppm_per_active_tick: u32,
    condition_before: Condition,
) -> Result<(TickSpan, Condition), PoweredCraftTimingError> {
    let throughput = calculate_mass_flow_duration_ceiling(
        rate,
        mass,
        registries.core().physical_tick_duration(),
    )
    .map_err(PoweredCraftTimingError::ThroughputDuration)?;
    let energy = calculate_power_duration_ceiling(
        available_power,
        required_energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(PoweredCraftTimingError::EnergyDuration)?;
    let duration = std::cmp::max(throughput, energy);
    let condition_after = calculate_usable_condition_after_active_ticks(
        wear_ppm_per_active_tick,
        condition_before,
        duration,
    )
    .map_err(PoweredCraftTimingError::EquipmentCondition)?;
    Ok((duration, condition_after))
}

/// Resolves one powered crafting operation without claiming player attention.
pub fn resolve_powered_craft(
    registries: &Registries,
    state: &AppState,
    request: &PoweredCraftRequest,
) -> Result<ProcessResolution, PoweredCraftError> {
    let definition = registries.crafting().get_powered(request.process).ok_or(
        PoweredCraftError::UnknownProcess {
            process: request.process,
        },
    )?;
    let transform = registries
        .crafting()
        .get_manual(definition.transform())
        .ok_or(PoweredCraftError::MissingTransform {
            process: request.process,
        })?;
    let inputs = validate_process_inputs(
        registries,
        state,
        request.process,
        request.source,
        &request.selections,
    )
    .map_err(PoweredCraftError::Input)?;
    let batch =
        validate_manual_craft_batch(transform, inputs.input_mass(), inputs.consumed_inputs())
            .map_err(batch_error)?;
    let outputs = build_manual_craft_outputs(transform, batch.batches(), batch.temperature())
        .map_err(|error| match error {
            super::batch::ManualCraftOutputError::MassOverflow { .. } => {
                PoweredCraftError::OutputMassOverflow
            }
            super::batch::ManualCraftOutputError::Construction { error, .. } => {
                PoweredCraftError::Output(error)
            }
        })?;

    let provider = resolve_equipment_provider(registries, state, request.equipment)
        .map_err(PoweredCraftError::Equipment)?;
    let capability = definition.mass_flow_capability();
    let rate =
        match resolve_equipment_capability(provider.definition(), provider.condition(), capability)
        {
            Some(CapabilityValue::MassFlow(rate)) => rate,
            Some(value) => {
                return Err(PoweredCraftError::EquipmentCapabilityKindMismatch {
                    equipment: request.equipment,
                    capability,
                    found: value.kind(),
                });
            }
            None => {
                return Err(PoweredCraftError::MissingEquipmentCapability {
                    equipment: request.equipment,
                    capability,
                });
            }
        };
    let access = assess_energy_supply_access(registries, state, request.energy_store)
        .map_err(PoweredCraftError::Energy)?;
    if access.carrier() != definition.energy_carrier() {
        return Err(PoweredCraftError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: access.carrier(),
        });
    }
    let required_energy =
        calculate_mass_specific_energy(inputs.input_mass(), definition.specific_energy());
    let energy_supply = validate_energy_supply_request(access, required_energy)
        .map_err(PoweredCraftError::Energy)?;
    let (duration, condition_after) = resolve_powered_craft_timing(
        registries,
        rate,
        inputs.input_mass(),
        required_energy,
        energy_supply.max_output_power(),
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    )
    .map_err(|error| match error {
        PoweredCraftTimingError::ThroughputDuration(error) => {
            PoweredCraftError::ThroughputDuration(error)
        }
        PoweredCraftTimingError::EnergyDuration(error) => PoweredCraftError::EnergyDuration(error),
        PoweredCraftTimingError::EquipmentCondition(error) => {
            PoweredCraftError::EquipmentCondition(error)
        }
    })?;
    inputs
        .resolve_with_energy_and_equipment(
            duration,
            vec![crate::production::ProcessOutputStream::new(
                crate::production::ProcessOutputStreamId::PRIMARY,
                outputs,
            )],
            energy_supply,
            provider.validated_use(),
            condition_after,
        )
        .map_err(PoweredCraftError::Resolution)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartPoweredCraftError {
    Resolution(PoweredCraftError),
    Process(StartProcessError),
}

impl Display for StartPoweredCraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolution(error) => {
                write!(formatter, "powered craft resolution failed: {error}")
            }
            Self::Process(error) => write!(formatter, "powered craft start failed: {error}"),
        }
    }
}

impl Error for StartPoweredCraftError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Resolution(error) => Some(error),
            Self::Process(error) => Some(error),
        }
    }
}

/// Resolves and admits one unattended powered craft through canonical production ownership.
pub fn validate_start_powered_craft(
    registries: &Registries,
    state: &AppState,
    request: PoweredCraftRequest,
    destination: StockpileId,
) -> Result<ValidatedStartProcess, StartPoweredCraftError> {
    let source = request.source;
    let resolution = resolve_powered_craft(registries, state, &request)
        .map_err(StartPoweredCraftError::Resolution)?;
    validate_start_process(registries, state, &resolution, source, destination)
        .map_err(StartPoweredCraftError::Process)
}
