//! Selected-batch sensible-heating resolution against exact matter, equipment, and finite energy.

use crate::capability::evaluate_capabilities;
use crate::core::quantity::{Energy, Power, Temperature};
use crate::core::state::AppState;
use crate::energy::{EnergyStoreId, validate_energy_supply};
use crate::equipment::{EquipmentId, resolve_equipment_provider};
use crate::inventory::{MaterialLotSelection, StockpileId};
use crate::production::{
    ProcessId, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    validate_selected_process_inputs,
};
use crate::registry::Registries;

use super::super::equipment_physics::{
    ThermalBatchLimitError, ThermalPowerTemperatureError, ThermalTransferTimingError,
    resolve_thermal_power_temperature_limits, resolve_thermal_transfer_timing,
    validate_thermal_batch_mass,
};
use super::sensible_batch::{SensibleHeatingBatchError, resolve_sensible_heating_batch};

mod errors;

pub use errors::SensibleHeatingResolutionError;

/// Observable physically resolved sensible-heating operation before production start.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedSensibleHeating {
    resolution: ProcessResolution,
    equipment: EquipmentId,
    target: Temperature,
    required_energy: Energy,
    transfer_power: Power,
}

impl ResolvedSensibleHeating {
    pub const fn process_resolution(&self) -> &ProcessResolution {
        &self.resolution
    }

    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn target(&self) -> Temperature {
        self.target
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

fn map_sensible_heating_batch_error(
    error: SensibleHeatingBatchError,
) -> SensibleHeatingResolutionError {
    match error {
        SensibleHeatingBatchError::TargetBelowInputTemperature { current, target } => {
            SensibleHeatingResolutionError::TargetBelowInputTemperature { current, target }
        }
        SensibleHeatingBatchError::Heat(error) => SensibleHeatingResolutionError::Heat(error),
        SensibleHeatingBatchError::ArithmeticOverflow => {
            SensibleHeatingResolutionError::RequiredEnergyOverflow
        }
        SensibleHeatingBatchError::Output(error) => SensibleHeatingResolutionError::Output(error),
    }
}

/// Exact runtime selection and providers requested for one sensible-heating operation.
#[derive(Clone, Copy, Debug)]
pub struct SensibleHeatingRequest<'selection> {
    process: ProcessId,
    source: StockpileId,
    selections: &'selection [MaterialLotSelection],
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
    target: Temperature,
}

impl<'selection> SensibleHeatingRequest<'selection> {
    #[must_use]
    pub const fn new(
        process: ProcessId,
        source: StockpileId,
        selections: &'selection [MaterialLotSelection],
        equipment: EquipmentId,
        energy_store: EnergyStoreId,
        target: Temperature,
    ) -> Self {
        Self {
            process,
            source,
            selections,
            equipment,
            energy_store,
            target,
        }
    }
}

/// Resolves exact sensible heating from selected material state, equipment throughput, and a
/// finite energy store. The ideal transfer is 100% into sensible material heat; losses are not
/// invented until a thermal-environment owner exists to receive them.
pub fn resolve_sensible_heating_process(
    registries: &Registries,
    state: &AppState,
    request: SensibleHeatingRequest<'_>,
) -> Result<ResolvedSensibleHeating, SensibleHeatingResolutionError> {
    let SensibleHeatingRequest {
        process,
        source,
        selections,
        equipment,
        energy_store,
        target,
    } = request;
    let definition = registries
        .thermal()
        .get_sensible_heating(process)
        .ok_or(SensibleHeatingResolutionError::UnknownThermalProcess { process })?;
    let inputs = validate_selected_process_inputs(registries, state, process, source, selections)
        .map_err(SensibleHeatingResolutionError::Input)?;
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(SensibleHeatingResolutionError::Equipment)?;
    let equipment_use = provider.validated_use();
    let process_definition = match registries.production().get_process(process) {
        Some(process_definition) => process_definition,
        None => return Err(SensibleHeatingResolutionError::UnknownThermalProcess { process }),
    };
    evaluate_capabilities(
        registries.capabilities(),
        &provider,
        process_definition.capability_requirements(),
    )
    .map_err(SensibleHeatingResolutionError::Capability)?;

    let limits = resolve_thermal_power_temperature_limits(
        provider.definition(),
        provider.condition(),
        definition.heating_power_capability(),
        definition.max_temperature_capability(),
    )
    .map_err(|error| match error {
        ThermalPowerTemperatureError::MissingTransferPower => {
            SensibleHeatingResolutionError::MissingHeatingPower {
                capability: definition.heating_power_capability(),
            }
        }
        ThermalPowerTemperatureError::MissingMaximumTemperature => {
            SensibleHeatingResolutionError::MissingMaximumTemperature {
                capability: definition.max_temperature_capability(),
            }
        }
    })?;
    let maximum_temperature = limits.maximum_temperature();
    if target > maximum_temperature {
        return Err(
            SensibleHeatingResolutionError::TargetExceedsEquipmentMaximum {
                target,
                maximum: maximum_temperature,
            },
        );
    }
    validate_thermal_batch_mass(
        provider.definition(),
        provider.condition(),
        definition.max_batch_mass_capability(),
        inputs.input_mass(),
    )
    .map_err(|error| match error {
        ThermalBatchLimitError::MissingMaximumBatchMass => {
            SensibleHeatingResolutionError::MissingMaximumBatchMass {
                capability: definition.max_batch_mass_capability(),
            }
        }
        ThermalBatchLimitError::BatchMassExceeded { selected, maximum } => {
            SensibleHeatingResolutionError::BatchMassExceedsEquipmentCapacity { selected, maximum }
        }
    })?;

    let batch =
        resolve_sensible_heating_batch(registries.materials(), inputs.consumed_inputs(), target)
            .map_err(map_sensible_heating_batch_error)?;
    let required_energy = batch.required_energy();
    if required_energy.is_zero() {
        return Err(SensibleHeatingResolutionError::NoHeatingRequired);
    }

    let energy_supply = validate_energy_supply(registries, state, energy_store, required_energy)
        .map_err(SensibleHeatingResolutionError::Energy)?;
    let provided_carrier = energy_supply.trace().carrier();
    if provided_carrier != definition.energy_carrier() {
        return Err(SensibleHeatingResolutionError::WrongEnergyCarrier {
            required: definition.energy_carrier(),
            provided: provided_carrier,
        });
    }
    let timing = resolve_thermal_transfer_timing(
        registries,
        limits.transfer_power(),
        energy_supply.max_output_power(),
        required_energy,
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    )
    .map_err(|error| match error {
        ThermalTransferTimingError::Duration(error) => {
            SensibleHeatingResolutionError::Duration(error)
        }
        ThermalTransferTimingError::ConditionDuration(error) => {
            SensibleHeatingResolutionError::ConditionDuration(error)
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
                batch.into_outputs(),
            )],
            energy_supply,
            equipment_use,
            equipment_condition_after,
        )
        .map_err(SensibleHeatingResolutionError::Resolution)?;
    Ok(ResolvedSensibleHeating {
        resolution,
        equipment,
        target,
        required_energy,
        transfer_power,
    })
}
