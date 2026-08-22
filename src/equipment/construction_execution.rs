//! Conserved inventory-to-equipment assembly transactions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    StockpileId, StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress, validate_consumption_selection,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
};
use crate::maintenance::Condition;
use crate::material::MaterialComposition;
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::EquipmentDefinitionId;
use super::state::{EquipmentId, EquipmentRecord};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentAssemblyError {
    UnknownDefinition {
        definition: EquipmentDefinitionId,
    },
    NoAssemblyProfile {
        definition: EquipmentDefinitionId,
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
    ImpureAssemblyMaterial,
    StaleInventorySelection {
        expected: u64,
        actual: u64,
    },
    InventoryRevisionExhausted,
    EquipmentIdExhausted,
    EquipmentRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for EquipmentAssemblyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDefinition { definition } => write!(
                formatter,
                "unknown equipment definition {}",
                definition.value()
            ),
            Self::NoAssemblyProfile { definition } => write!(
                formatter,
                "equipment definition {} has no authored assembly material",
                definition.value()
            ),
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "unknown assembly stockpile {}",
                stockpile.value()
            ),
            Self::InsufficientMaterial {
                stockpile,
                available,
                required,
            } => write!(
                formatter,
                "stockpile {} contains {} mg of assembly material but {} mg is required",
                stockpile.value(),
                available.milligrams(),
                required.milligrams()
            ),
            Self::SourceMassOverflow { stockpile } => write!(
                formatter,
                "assembly source {} mass accounting overflowed",
                stockpile.value()
            ),
            Self::ImpureAssemblyMaterial => formatter.write_str(
                "equipment assembly requires pure matter matching the authored input material",
            ),
            Self::StaleInventorySelection { expected, actual } => write!(
                formatter,
                "equipment assembly material selection expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::EquipmentIdExhausted => {
                formatter.write_str("equipment identifier space is exhausted")
            }
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted")
            }
            Self::StructuralLoad(error) => {
                write!(formatter, "assembly source load failed: {error}")
            }
        }
    }
}

impl Error for EquipmentAssemblyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownDefinition { definition: _ }
            | Self::NoAssemblyProfile { definition: _ }
            | Self::UnknownSource { stockpile: _ }
            | Self::InsufficientMaterial {
                stockpile: _,
                available: _,
                required: _,
            }
            | Self::SourceMassOverflow { stockpile: _ }
            | Self::ImpureAssemblyMaterial
            | Self::StaleInventorySelection {
                expected: _,
                actual: _,
            }
            | Self::InventoryRevisionExhausted
            | Self::EquipmentIdExhausted
            | Self::EquipmentRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentAssemblyCommitError {
    StaleInventory { expected: u64, actual: u64 },
    StaleEquipment { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for EquipmentAssemblyCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "equipment assembly expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipment { expected, actual } => write!(
                formatter,
                "equipment assembly expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => {
                write!(formatter, "equipment assembly structure failed: {error}")
            }
        }
    }
}

impl Error for EquipmentAssemblyCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventory {
                expected: _,
                actual: _,
            }
            | Self::StaleEquipment {
                expected: _,
                actual: _,
            } => None,
        }
    }
}

#[must_use]
pub struct ValidatedEquipmentAssembly {
    record: EquipmentRecord,
    next_equipment_id: u32,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    egress: ValidatedMaterialEgress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedEquipmentAssembly {
    pub fn commit(self, state: &mut AppState) -> Result<EquipmentId, EquipmentAssemblyCommitError> {
        if state.inventory().revision() != self.egress.expected_revision() {
            return Err(EquipmentAssemblyCommitError::StaleInventory {
                expected: self.egress.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.equipment().revision() != self.expected_equipment_revision {
            return Err(EquipmentAssemblyCommitError::StaleEquipment {
                expected: self.expected_equipment_revision,
                actual: state.equipment().revision(),
            });
        }
        if let Some(load) = self.structural_load {
            load.commit(state)
                .map_err(EquipmentAssemblyCommitError::Structure)?;
        }
        let id = self.record.id();
        apply_material_egress(state.inventory_state_mut(), self.egress);
        state.equipment_state_mut().insert_equipment(
            self.record,
            self.next_equipment_id,
            self.next_equipment_revision,
        );
        Ok(id)
    }
}

/// Validates construction of one authored equipment instance from its exact conserved assembly stock.
pub fn validate_assemble_equipment(
    registries: &Registries,
    state: &AppState,
    definition: EquipmentDefinitionId,
    source: StockpileId,
) -> Result<ValidatedEquipmentAssembly, EquipmentAssemblyError> {
    let definition_record = registries
        .equipment()
        .get_equipment(definition)
        .ok_or(EquipmentAssemblyError::UnknownDefinition { definition })?;
    let assembly = definition_record
        .assembly_profile()
        .ok_or(EquipmentAssemblyError::NoAssemblyProfile { definition })?;
    let selection = validate_consumption_selection(state.inventory(), source, assembly.inputs())
        .map_err(|error| match error {
            crate::inventory::ConsumptionSelectionError::UnknownStockpile { stockpile } => {
                EquipmentAssemblyError::UnknownSource { stockpile }
            }
            crate::inventory::ConsumptionSelectionError::InsufficientMass {
                stockpile,
                available,
                requested,
                ..
            } => EquipmentAssemblyError::InsufficientMaterial {
                stockpile,
                available,
                required: requested,
            },
            crate::inventory::ConsumptionSelectionError::MassOverflow { stockpile } => {
                EquipmentAssemblyError::SourceMassOverflow { stockpile }
            }
        })?;
    if selection.consumed_inputs().iter().any(|trace| {
        trace.profile().composition()
            != &MaterialComposition::pure(trace.profile().commodity().material())
    }) {
        return Err(EquipmentAssemblyError::ImpureAssemblyMaterial);
    }
    let embodied_material = selection.consumed_inputs().to_vec();
    let egress =
        validate_material_egress_from_selection(state.inventory(), selection).map_err(|error| {
            match error {
                crate::inventory::MaterialEgressError::StaleSelection { expected, actual } => {
                    EquipmentAssemblyError::StaleInventorySelection { expected, actual }
                }
                crate::inventory::MaterialEgressError::RevisionExhausted => {
                    EquipmentAssemblyError::InventoryRevisionExhausted
                }
            }
        })?;
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .ok_or(EquipmentAssemblyError::UnknownSource { stockpile: source })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(egress.total_consumed())
        .ok_or(EquipmentAssemblyError::SourceMassOverflow { stockpile: source })?;
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(EquipmentAssemblyError::StructuralLoad)?;

    let id_value = state.equipment().next_equipment_id();
    let next_equipment_id = id_value
        .checked_add(1)
        .ok_or(EquipmentAssemblyError::EquipmentIdExhausted)?;
    let id = EquipmentId::new(id_value);
    let expected_equipment_revision = state.equipment().revision();
    let next_equipment_revision = expected_equipment_revision
        .checked_add(1)
        .ok_or(EquipmentAssemblyError::EquipmentRevisionExhausted)?;
    Ok(ValidatedEquipmentAssembly {
        record: EquipmentRecord {
            id,
            definition,
            condition: Condition::PRISTINE,
            embodied_mass: definition_record.mass(),
            embodied_material,
            supported_by: None,
            created_at: state.tick(),
        },
        next_equipment_id,
        expected_equipment_revision,
        next_equipment_revision,
        egress,
        structural_load,
    })
}

#[cfg(test)]
#[path = "construction_execution_tests.rs"]
mod tests;
