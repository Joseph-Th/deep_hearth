//! Exact current-state mass planning for homogeneous pure phase-change lots.

mod casting;
mod melting;

pub use casting::{
    CastingLotMassConstraint, CastingLotMassEnvelope, CastingLotMassRequest,
    assess_casting_lot_mass_envelope,
};
pub use melting::{
    MeltingLotMassConstraint, MeltingLotMassEnvelope, MeltingLotMassRequest,
    assess_melting_lot_mass_envelope,
};

use crate::capability::{CapabilityEvaluationError, CapabilityId, evaluate_capabilities};
use crate::core::quantity::{Energy, Mass, Power, Temperature};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{PowerIntegrationError, PowerRemainder, integrate_power};
use crate::equipment::{EquipmentId, EquipmentProviderError, resolve_equipment_provider};
use crate::inventory::{ConsumedMaterialTrace, MaterialLotSelection, StockpileId};
use crate::maintenance::Condition;
use crate::production::{ProcessId, ProcessInputError, validate_selected_process_inputs};
use crate::registry::Registries;

use super::equipment_physics::{
    ThermalBatchLimitError, ThermalPowerTemperatureError, resolve_thermal_batch_mass_limit,
    resolve_thermal_power_temperature_limits,
};

#[derive(Clone, Debug)]
struct PhaseChangeLotOffer {
    mass: Mass,
    trace: ConsumedMaterialTrace,
}

fn resolve_phase_change_lot_offer(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
    selection: MaterialLotSelection,
) -> Result<PhaseChangeLotOffer, ProcessInputError> {
    let inputs =
        validate_selected_process_inputs(registries, state, process, source, &[selection])?;
    let trace = inputs
        .consumed_inputs()
        .first()
        .cloned()
        .unwrap_or_else(|| unreachable!("validated nonzero single-lot offer has one trace"));
    Ok(PhaseChangeLotOffer {
        mass: inputs.input_mass(),
        trace,
    })
}

#[derive(Clone, Copy)]
struct ThermalPlanningCapabilities {
    transfer_power: CapabilityId,
    maximum_temperature: CapabilityId,
    maximum_batch_mass: CapabilityId,
}

impl ThermalPlanningCapabilities {
    const fn new(
        transfer_power: CapabilityId,
        maximum_temperature: CapabilityId,
        maximum_batch_mass: CapabilityId,
    ) -> Self {
        Self {
            transfer_power,
            maximum_temperature,
            maximum_batch_mass,
        }
    }
}

#[derive(Clone, Copy)]
struct ThermalEquipmentEnvelope {
    condition: Condition,
    transfer_power: Power,
    maximum_temperature: Temperature,
    batch_mass_capacity: Mass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ThermalEquipmentEnvelopeError {
    UnknownProcess,
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingTransferPower { capability: CapabilityId },
    MissingMaximumTemperature { capability: CapabilityId },
    MissingMaximumBatchMass { capability: CapabilityId },
}

fn resolve_thermal_equipment_envelope(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    equipment: EquipmentId,
    capabilities: ThermalPlanningCapabilities,
) -> Result<ThermalEquipmentEnvelope, ThermalEquipmentEnvelopeError> {
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(ThermalEquipmentEnvelopeError::Equipment)?;
    let process_definition = registries
        .production()
        .get_process(process)
        .ok_or(ThermalEquipmentEnvelopeError::UnknownProcess)?;
    evaluate_capabilities(
        registries.capabilities(),
        &provider,
        process_definition.capability_requirements(),
    )
    .map_err(ThermalEquipmentEnvelopeError::Capability)?;
    let limits = resolve_thermal_power_temperature_limits(
        provider.definition(),
        provider.condition(),
        capabilities.transfer_power,
        capabilities.maximum_temperature,
    )
    .map_err(|error| match error {
        ThermalPowerTemperatureError::MissingTransferPower => {
            ThermalEquipmentEnvelopeError::MissingTransferPower {
                capability: capabilities.transfer_power,
            }
        }
        ThermalPowerTemperatureError::MissingMaximumTemperature => {
            ThermalEquipmentEnvelopeError::MissingMaximumTemperature {
                capability: capabilities.maximum_temperature,
            }
        }
    })?;
    let batch_mass_capacity = resolve_thermal_batch_mass_limit(
        provider.definition(),
        provider.condition(),
        capabilities.maximum_batch_mass,
    )
    .map_err(|error| match error {
        ThermalBatchLimitError::MissingMaximumBatchMass => {
            ThermalEquipmentEnvelopeError::MissingMaximumBatchMass {
                capability: capabilities.maximum_batch_mass,
            }
        }
        ThermalBatchLimitError::BatchMassExceeded { .. } => {
            unreachable!("planning resolves thermal batch limit without selected mass")
        }
    })?;
    Ok(ThermalEquipmentEnvelope {
        condition: provider.condition(),
        transfer_power: limits.transfer_power(),
        maximum_temperature: limits.maximum_temperature(),
        batch_mass_capacity,
    })
}

fn mass_capacity_from_energy(available: Energy, unit_energy: Energy) -> Mass {
    debug_assert!(!unit_energy.is_zero());
    let milligrams = available.nanojoules() / unit_energy.nanojoules();
    Mass::from_milligrams(u64::try_from(milligrams).unwrap_or(u64::MAX))
}

fn mass_capacity_from_integrated_power(
    power: Power,
    duration: TickSpan,
    registries: &Registries,
    unit_energy: Energy,
) -> Mass {
    mass_capacity_from_energy(integrated_energy(power, duration, registries), unit_energy)
}

fn integrated_energy(power: Power, duration: TickSpan, registries: &Registries) -> Energy {
    match integrate_power(
        power,
        duration,
        registries.core().physical_tick_duration(),
        PowerRemainder::ZERO,
    ) {
        Ok(integration) => integration.energy(),
        Err(PowerIntegrationError::ArithmeticOverflow) => Energy::from_nanojoules(u128::MAX),
        Err(PowerIntegrationError::InvalidRemainder { .. }) => {
            unreachable!("zero thermal planning remainder is always valid")
        }
    }
}
