//! Conserved inventory-to-energy-storage construction transactions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass};
use crate::core::state::AppState;
use crate::inventory::{
    StockpileId, StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress, validate_consumption_selection,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::state::EnergyStoreRecord;
use super::{EnergyStoreDefinitionId, EnergyStoreId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnergyStoreAssemblyError {
    UnknownDefinition {
        definition: EnergyStoreDefinitionId,
    },
    NoAssemblyProfile {
        definition: EnergyStoreDefinitionId,
    },
    UnknownSource {
        stockpile: StockpileId,
    },
    InsufficientMaterial {
        stockpile: StockpileId,
        available: Mass,
        required: Mass,
    },
    SourceMassOverflow {
        stockpile: StockpileId,
    },
    StaleInventorySelection {
        expected: u64,
        actual: u64,
    },
    InventoryRevisionExhausted,
    StoreIdExhausted,
    EnergyRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for EnergyStoreAssemblyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDefinition { definition } => write!(
                formatter,
                "unknown energy store definition {}",
                definition.value()
            ),
            Self::NoAssemblyProfile { definition } => write!(
                formatter,
                "energy store definition {} has no authored construction material",
                definition.value()
            ),
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "unknown storage-construction stockpile {}",
                stockpile.value()
            ),
            Self::InsufficientMaterial {
                stockpile,
                available,
                required,
            } => write!(
                formatter,
                "stockpile {} contains {} mg of construction material but {} mg is required",
                stockpile.value(),
                available.milligrams(),
                required.milligrams()
            ),
            Self::SourceMassOverflow { stockpile } => write!(
                formatter,
                "energy-store construction source {} mass accounting overflowed",
                stockpile.value()
            ),
            Self::StaleInventorySelection { expected, actual } => write!(
                formatter,
                "energy-store construction material selection expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::StoreIdExhausted => {
                formatter.write_str("energy store identifier space is exhausted")
            }
            Self::EnergyRevisionExhausted => {
                formatter.write_str("energy state revision space is exhausted")
            }
            Self::StructuralLoad(error) => write!(
                formatter,
                "energy-store construction source load failed: {error}"
            ),
        }
    }
}

impl Error for EnergyStoreAssemblyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownDefinition { .. }
            | Self::NoAssemblyProfile { .. }
            | Self::UnknownSource { .. }
            | Self::InsufficientMaterial { .. }
            | Self::SourceMassOverflow { .. }
            | Self::StaleInventorySelection { .. }
            | Self::InventoryRevisionExhausted
            | Self::StoreIdExhausted
            | Self::EnergyRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnergyStoreAssemblyCommitError {
    StaleInventory { expected: u64, actual: u64 },
    StaleEnergy { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for EnergyStoreAssemblyCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "energy-store construction expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEnergy { expected, actual } => write!(
                formatter,
                "energy-store construction expected energy revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "energy-store construction structure failed: {error}"
            ),
        }
    }
}

impl Error for EnergyStoreAssemblyCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventory { .. } | Self::StaleEnergy { .. } => None,
        }
    }
}

#[must_use]
pub struct ValidatedEnergyStoreAssembly {
    record: EnergyStoreRecord,
    next_store_id: u64,
    expected_energy_revision: u64,
    next_energy_revision: u64,
    egress: ValidatedMaterialEgress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedEnergyStoreAssembly {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<EnergyStoreId, EnergyStoreAssemblyCommitError> {
        if state.inventory().revision() != self.egress.expected_revision() {
            return Err(EnergyStoreAssemblyCommitError::StaleInventory {
                expected: self.egress.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.energy().revision() != self.expected_energy_revision {
            return Err(EnergyStoreAssemblyCommitError::StaleEnergy {
                expected: self.expected_energy_revision,
                actual: state.energy().revision(),
            });
        }
        state.energy().assert_allocation_available(
            &self.record,
            self.next_store_id,
            self.next_energy_revision,
        );
        self.egress.assert_matches_state(state.inventory());
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(EnergyStoreAssemblyCommitError::Structure)?;
        }
        let id = self.record.id();
        apply_material_egress(state.inventory_state_mut(), self.egress);
        state.energy_state_mut().insert_store(
            self.record,
            self.next_store_id,
            self.next_energy_revision,
        );
        Ok(id)
    }
}

/// Validates construction of one authored finite-energy store from exact conserved material.
pub fn validate_assemble_energy_store(
    registries: &Registries,
    state: &AppState,
    definition: EnergyStoreDefinitionId,
    source: StockpileId,
) -> Result<ValidatedEnergyStoreAssembly, EnergyStoreAssemblyError> {
    let definition_record = registries
        .energy()
        .get_store(definition)
        .ok_or(EnergyStoreAssemblyError::UnknownDefinition { definition })?;
    let assembly = definition_record
        .assembly_profile()
        .ok_or(EnergyStoreAssemblyError::NoAssemblyProfile { definition })?;
    let selection = validate_consumption_selection(state.inventory(), source, assembly.inputs())
        .map_err(|error| match error {
            crate::inventory::ConsumptionSelectionError::UnknownStockpile { stockpile } => {
                EnergyStoreAssemblyError::UnknownSource { stockpile }
            }
            crate::inventory::ConsumptionSelectionError::InsufficientMass {
                stockpile,
                available,
                requested,
                ..
            } => EnergyStoreAssemblyError::InsufficientMaterial {
                stockpile,
                available,
                required: requested,
            },
            crate::inventory::ConsumptionSelectionError::MassOverflow { stockpile } => {
                EnergyStoreAssemblyError::SourceMassOverflow { stockpile }
            }
        })?;
    let embodied_material = selection.consumed_inputs().to_vec();
    let egress =
        validate_material_egress_from_selection(state.inventory(), selection).map_err(|error| {
            match error {
                crate::inventory::MaterialEgressError::StaleSelection { expected, actual } => {
                    EnergyStoreAssemblyError::StaleInventorySelection { expected, actual }
                }
                crate::inventory::MaterialEgressError::RevisionExhausted => {
                    EnergyStoreAssemblyError::InventoryRevisionExhausted
                }
            }
        })?;
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .ok_or(EnergyStoreAssemblyError::UnknownSource { stockpile: source })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(egress.total_consumed())
        .ok_or(EnergyStoreAssemblyError::SourceMassOverflow { stockpile: source })?;
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(EnergyStoreAssemblyError::StructuralLoad)?;

    let id_value = state.energy().next_store_id();
    let next_store_id = id_value
        .checked_add(1)
        .ok_or(EnergyStoreAssemblyError::StoreIdExhausted)?;
    let id = EnergyStoreId::new(id_value);
    let expected_energy_revision = state.energy().revision();
    let next_energy_revision = expected_energy_revision
        .checked_add(1)
        .ok_or(EnergyStoreAssemblyError::EnergyRevisionExhausted)?;
    Ok(ValidatedEnergyStoreAssembly {
        record: EnergyStoreRecord {
            id,
            definition,
            stored: Energy::ZERO,
            embodied_material,
            created_at: state.tick(),
        },
        next_store_id,
        expected_energy_revision,
        next_energy_revision,
        egress,
        structural_load,
    })
}

#[cfg(test)]
#[path = "construction_execution_tests.rs"]
mod tests;
