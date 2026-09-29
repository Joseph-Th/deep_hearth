//! Controlled energy-store fixture allocation; runtime construction remains material-conserving assembly.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Energy;
use crate::core::state::AppState;
use crate::registry::Registries;

use super::definitions::EnergyStoreDefinitionId;
use super::state::{EnergyStoreId, EnergyStoreRecord};

/// Failure while allocating an authoritative finite energy store for controlled fixtures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AddEnergyStoreError {
    UnknownDefinition { definition: EnergyStoreDefinitionId },
    RequiresAssembly { definition: EnergyStoreDefinitionId },
    InitialEnergyExceedsCapacity { initial: Energy, capacity: Energy },
    IdExhausted,
    RevisionExhausted,
}

impl Display for AddEnergyStoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDefinition { definition } => write!(
                formatter,
                "unknown energy store definition {}",
                definition.value()
            ),
            Self::RequiresAssembly { definition } => write!(
                formatter,
                "energy store definition {} requires conserved material construction",
                definition.value()
            ),
            Self::InitialEnergyExceedsCapacity { initial, capacity } => write!(
                formatter,
                "initial energy {} nJ exceeds store capacity {} nJ",
                initial.nanojoules(),
                capacity.nanojoules()
            ),
            Self::IdExhausted => formatter.write_str("energy store identifier space is exhausted"),
            Self::RevisionExhausted => {
                formatter.write_str("energy state revision space is exhausted")
            }
        }
    }
}

impl Error for AddEnergyStoreError {}

/// Allocates one empty finite energy store for unit tests only.
#[cfg(test)]
pub(crate) fn add_energy_store(
    registries: &Registries,
    state: &mut AppState,
    definition: EnergyStoreDefinitionId,
) -> Result<EnergyStoreId, AddEnergyStoreError> {
    allocate_energy_store(registries, state, definition, Energy::ZERO)
}

fn allocate_energy_store(
    registries: &Registries,
    state: &mut AppState,
    definition: EnergyStoreDefinitionId,
    initial: Energy,
) -> Result<EnergyStoreId, AddEnergyStoreError> {
    let Some(authored) = registries.energy().get_store(definition) else {
        return Err(AddEnergyStoreError::UnknownDefinition { definition });
    };
    if authored.assembly_profile().is_some() {
        return Err(AddEnergyStoreError::RequiresAssembly { definition });
    }
    if initial > authored.capacity() {
        return Err(AddEnergyStoreError::InitialEnergyExceedsCapacity {
            initial,
            capacity: authored.capacity(),
        });
    }
    let energy = state.energy();
    let id = EnergyStoreId::new(energy.next_store_id());
    let next_store_id = energy
        .next_store_id()
        .checked_add(1)
        .ok_or(AddEnergyStoreError::IdExhausted)?;
    let next_revision = energy
        .revision()
        .checked_add(1)
        .ok_or(AddEnergyStoreError::RevisionExhausted)?;
    let record = EnergyStoreRecord {
        id,
        definition,
        stored: initial,
        embodied_material: Vec::new(),
        created_at: state.tick(),
    };

    state
        .energy_state_mut()
        .insert_store(record, next_store_id, next_revision);
    Ok(id)
}

pub(crate) fn add_energy_store_with_initial_for_fixture(
    registries: &Registries,
    state: &mut AppState,
    definition: EnergyStoreDefinitionId,
    initial: Energy,
) -> Result<EnergyStoreId, AddEnergyStoreError> {
    allocate_energy_store(registries, state, definition, initial)
}

#[cfg(test)]
#[path = "fixture_execution_tests.rs"]
mod tests;
