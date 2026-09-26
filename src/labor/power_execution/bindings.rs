//! Shared current-state bindings for direct player-power admission and planning.

use crate::capability::{CapabilityId, CapabilityValue};
use crate::core::quantity::Power;
use crate::core::state::AppState;
use crate::energy::{ValidatedEnergySinkAccess, validate_energy_sink_access};
use crate::equipment::{
    EquipmentId, EquipmentOccupancy, ResolvedEquipmentProvider,
    resolve_equipment_provider_with_occupancy,
};
use crate::logistics::{validate_player_energy_store_access, validate_player_equipment_access};
use crate::registry::Registries;

use super::super::{ManualPowerDefinition, ManualPowerMethodId};
use super::ManualPowerError;

/// Current physical resources shared by exact manual-power admission and read-only planning.
#[derive(Clone, Copy)]
pub(in crate::labor) struct ResolvedManualPowerBindings<'state> {
    definition: ManualPowerDefinition,
    provider: ResolvedEquipmentProvider<'state>,
    sink: ValidatedEnergySinkAccess,
    transfer_power: Power,
}

impl<'state> ResolvedManualPowerBindings<'state> {
    pub(in crate::labor) const fn definition(&self) -> ManualPowerDefinition {
        self.definition
    }

    pub(in crate::labor) const fn provider(&self) -> ResolvedEquipmentProvider<'state> {
        self.provider
    }

    pub(in crate::labor) const fn sink(&self) -> ValidatedEnergySinkAccess {
        self.sink
    }

    pub(in crate::labor) const fn transfer_power(&self) -> Power {
        self.transfer_power
    }
}

fn validate_equipment_occupancy(
    occupancy: Option<EquipmentOccupancy>,
    equipment: EquipmentId,
) -> Result<(), ManualPowerError> {
    match occupancy {
        Some(EquipmentOccupancy::Production { job, release }) => {
            Err(ManualPowerError::EquipmentBusyProduction {
                equipment,
                job,
                release,
            })
        }
        Some(EquipmentOccupancy::Mining { job }) => {
            Err(ManualPowerError::EquipmentBusyMining { equipment, job })
        }
        Some(
            EquipmentOccupancy::ManualPower { .. }
            | EquipmentOccupancy::Prospecting { .. }
            | EquipmentOccupancy::Maintenance { .. },
        )
        | None => Ok(()),
    }
}

fn resolve_equipment_power(
    provider: ResolvedEquipmentProvider<'_>,
    equipment: EquipmentId,
    capability: CapabilityId,
) -> Result<Power, ManualPowerError> {
    let value =
        provider
            .get_capability(capability)
            .ok_or(ManualPowerError::MissingPowerCapability {
                equipment,
                capability,
            })?;
    let CapabilityValue::Power(power) = value else {
        return Err(ManualPowerError::PowerCapabilityKindMismatch {
            equipment,
            capability,
            found: value.kind(),
        });
    };
    if power.is_zero() {
        return Err(ManualPowerError::ZeroEquipmentPower {
            equipment,
            capability,
        });
    }
    Ok(power)
}

pub(in crate::labor) fn resolve_manual_power_bindings<'state>(
    registries: &'state Registries,
    state: &'state AppState,
    method: ManualPowerMethodId,
    equipment: EquipmentId,
    destination: crate::energy::EnergyStoreId,
) -> Result<ResolvedManualPowerBindings<'state>, ManualPowerError> {
    let definition = registries
        .labor()
        .get_manual_power(method)
        .copied()
        .ok_or(ManualPowerError::UnknownMethod { method })?;
    if state
        .equipment()
        .get_equipment(equipment)
        .is_some_and(|record| record.supported_by().is_some())
    {
        return Err(ManualPowerError::EquipmentMounted { equipment });
    }
    validate_player_equipment_access(state, equipment)
        .map_err(ManualPowerError::EquipmentAccess)?;
    let (provider, occupancy) =
        resolve_equipment_provider_with_occupancy(registries, state, equipment)
            .map_err(ManualPowerError::Equipment)?;
    validate_equipment_occupancy(occupancy, equipment)?;
    let equipment_power =
        resolve_equipment_power(provider, equipment, definition.power_capability())?;
    let sink = validate_energy_sink_access(registries, state, destination)
        .map_err(ManualPowerError::EnergySink)?;
    validate_player_energy_store_access(state, destination)
        .map_err(ManualPowerError::DestinationAccess)?;
    if sink.carrier() != definition.carrier() {
        return Err(ManualPowerError::WrongCarrier {
            required: definition.carrier(),
            provided: sink.carrier(),
        });
    }
    let transfer_power = std::cmp::min(equipment_power, sink.max_input_power());
    if transfer_power.is_zero() {
        return Err(ManualPowerError::ZeroTransferPower {
            equipment,
            destination,
        });
    }
    Ok(ResolvedManualPowerBindings {
        definition,
        provider,
        sink,
        transfer_power,
    })
}
