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
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
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
    SourceBusy {
        stockpile: StockpileId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
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
            Self::SourceBusy {
                stockpile,
                job,
                release,
            } => write!(
                formatter,
                "assembly source {} is occupied by production job {} {release}",
                stockpile.value(),
                job.value()
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
            | Self::SourceBusy {
                stockpile: _,
                job: _,
                release: _,
            }
            | Self::InventoryRevisionExhausted
            | Self::EquipmentIdExhausted
            | Self::EquipmentRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentAssemblyCommitError {
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    StaleEquipment {
        expected: u64,
        actual: u64,
    },
    SourceBusy {
        stockpile: StockpileId,
        job: ProductionJobId,
    },
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
            Self::SourceBusy { stockpile, job } => write!(
                formatter,
                "equipment assembly source {} became occupied by production job {}",
                stockpile.value(),
                job.value()
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
            }
            | Self::SourceBusy {
                stockpile: _,
                job: _,
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
        if let Some(job) = state
            .production()
            .get_stockpile_occupant(self.egress.source())
        {
            return Err(EquipmentAssemblyCommitError::SourceBusy {
                stockpile: self.egress.source(),
                job: job.id(),
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
    if let Some(job) = state.production().get_stockpile_occupant(source) {
        return Err(EquipmentAssemblyError::SourceBusy {
            stockpile: source,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
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
mod tests {
    use super::*;
    use crate::content::{
        EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD,
        build_registries,
    };
    use crate::core::quantity::Temperature;
    use crate::core::state::StateValidationError;
    use crate::core::time::WorldSeed;
    use crate::equipment::EquipmentValidationError;
    use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
    use crate::material::CommodityKey;
    use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};

    #[test]
    fn composite_pick_requires_both_authored_inputs_and_rejects_forged_embodiment() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0xA55E_0001));
        let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
            .unwrap_or_else(|error| panic!("assembly source fixture failed: {error}"));
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("assembly stone fixture failed: {error}"));

        assert_eq!(
            validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, source).err(),
            Some(EquipmentAssemblyError::InsufficientMaterial {
                stockpile: source,
                available: Mass::ZERO,
                required: Mass::from_milligrams(200_000),
            })
        );
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("assembly handle fixture failed: {error}"));
        let equipment =
            validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, source)
                .unwrap_or_else(|error| panic!("composite pick validation failed: {error}"))
                .commit(&mut state)
                .unwrap_or_else(|error| panic!("composite pick commit failed: {error}"));

        let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("composite pick serialization failed: {error}"));
        encoded["state"]["systems"]["equipment"]["records"][equipment.value().to_string()]["embodied_material"]
            [0]["mass"] = serde_json::json!(200_001_u64);
        let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
            .unwrap_or_else(|error| panic!("composite pick tamper decode failed: {error}"));

        assert_eq!(
            tampered.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::Equipment(
                EquipmentValidationError::EmbodiedTraceMassMismatch {
                    equipment,
                    stored: Mass::from_milligrams(1_000_000),
                    traced: Mass::from_milligrams(1_000_001),
                }
            )))
        );
    }
}
