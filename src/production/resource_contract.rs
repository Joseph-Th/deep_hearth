//! Canonical execution-family resource-shape validation for resolved and durable processes.

use crate::energy::{EnergyCarrier, EnergyStoreDefinitionId};
use crate::equipment::EquipmentDefinitionId;
use crate::registry::{ProcessEnergyRole, ProcessEquipmentRole, ProcessTopology};

/// Resource facts carried by one resolved or durable production operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProcessResourceSnapshot {
    equipment: Option<EquipmentDefinitionId>,
    consumed_energy: Option<(EnergyCarrier, EnergyStoreDefinitionId)>,
    released_energy: Option<(EnergyCarrier, EnergyStoreDefinitionId)>,
}

impl ProcessResourceSnapshot {
    pub(crate) const fn new(
        equipment: Option<EquipmentDefinitionId>,
        consumed_energy: Option<(EnergyCarrier, EnergyStoreDefinitionId)>,
        released_energy: Option<(EnergyCarrier, EnergyStoreDefinitionId)>,
    ) -> Self {
        Self {
            equipment,
            consumed_energy,
            released_energy,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProcessResourceContractError {
    Equipment,
    Energy,
}

/// Verifies that operation resources match the unique resolver family registered for the process.
pub(crate) fn validate_process_resource_contract(
    topology: &ProcessTopology,
    snapshot: ProcessResourceSnapshot,
) -> Result<(), ProcessResourceContractError> {
    let energy_matches = match topology.energy_role() {
        ProcessEnergyRole::None => {
            snapshot.consumed_energy.is_none() && snapshot.released_energy.is_none()
        }
        ProcessEnergyRole::Supply(carrier) => {
            snapshot.released_energy.is_none()
                && snapshot
                    .consumed_energy
                    .is_some_and(|(actual, definition)| {
                        actual == carrier
                            && topology.compatible_energy_stores().contains(&definition)
                    })
        }
        ProcessEnergyRole::Sink(carrier) => {
            snapshot.consumed_energy.is_none()
                && snapshot
                    .released_energy
                    .is_some_and(|(actual, definition)| {
                        actual == carrier
                            && topology.compatible_energy_stores().contains(&definition)
                    })
        }
    };
    if !energy_matches {
        return Err(ProcessResourceContractError::Energy);
    }

    let equipment_matches = match topology.equipment_role() {
        ProcessEquipmentRole::None => snapshot.equipment.is_none(),
        ProcessEquipmentRole::Optional => snapshot
            .equipment
            .is_none_or(|definition| topology.nominal_providers().contains(&definition)),
        ProcessEquipmentRole::Required => snapshot
            .equipment
            .is_some_and(|definition| topology.nominal_providers().contains(&definition)),
    };
    if !equipment_matches {
        return Err(ProcessResourceContractError::Equipment);
    }
    Ok(())
}
