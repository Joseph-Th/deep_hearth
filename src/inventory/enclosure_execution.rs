//! Material-backed construction of preservation enclosures around existing stockpiles.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::structural::{StructuralCommitError, StructuralElementId};

use super::storage_validation::validate_stockpile_storage_profile;
use super::{
    ConsumptionSelectionError, MaterialEgressError, MaterialLotId, StockpileEnclosureRecord,
    StockpileId, StockpileStorageError, StockpileStorageProfile, StockpileStoredMassChange,
    StockpileStructuralLoadError, StorageDefinitionId, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress, validate_consumption_selection,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
};

/// Failure while validating construction of one authored storage enclosure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageEnclosureConstructionError {
    UnknownDefinition {
        definition: StorageDefinitionId,
    },
    UnknownTarget {
        stockpile: StockpileId,
    },
    UnknownSource {
        stockpile: StockpileId,
    },
    AlreadyEnclosed {
        stockpile: StockpileId,
        definition: StorageDefinitionId,
    },
    TargetMounted {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    TargetCapacityTooLarge {
        stockpile: StockpileId,
        capacity: Mass,
        maximum: Mass,
    },
    TargetStorageProfileMismatch {
        stockpile: StockpileId,
        current: StockpileStorageProfile,
        required: StockpileStorageProfile,
    },
    TargetHasReservedInbound {
        stockpile: StockpileId,
        reserved: Mass,
    },
    TargetContentsIncompatible {
        lot: MaterialLotId,
        error: StockpileStorageError,
    },
    StorageHistoryOverflow {
        lot: MaterialLotId,
    },
    InsufficientMaterial {
        stockpile: StockpileId,
        commodity: CommodityKey,
        available: Mass,
        required: Mass,
    },
    SourceMassOverflow {
        stockpile: StockpileId,
    },
    InventoryRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for StorageEnclosureConstructionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDefinition { definition } => write!(
                formatter,
                "unknown storage enclosure definition {}",
                definition.value()
            ),
            Self::UnknownTarget { stockpile } => {
                write!(
                    formatter,
                    "unknown storage target stockpile {}",
                    stockpile.value()
                )
            }
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "unknown storage-construction material stockpile {}",
                stockpile.value()
            ),
            Self::AlreadyEnclosed {
                stockpile,
                definition,
            } => write!(
                formatter,
                "stockpile {} already has storage enclosure {}",
                stockpile.value(),
                definition.value()
            ),
            Self::TargetMounted { stockpile, element } => write!(
                formatter,
                "stockpile {} must be unmounted before constructing an enclosure around it; current support is {}",
                stockpile.value(),
                element.value()
            ),
            Self::TargetCapacityTooLarge {
                stockpile,
                capacity,
                maximum,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg exceeds enclosure maximum {} mg",
                stockpile.value(),
                capacity.milligrams(),
                maximum.milligrams()
            ),
            Self::TargetStorageProfileMismatch {
                stockpile,
                current: _,
                required: _,
            } => write!(
                formatter,
                "stockpile {} does not have the ambient solid-storage profile required for this enclosure",
                stockpile.value()
            ),
            Self::TargetHasReservedInbound {
                stockpile,
                reserved,
            } => write!(
                formatter,
                "stockpile {} cannot change storage enclosure while {} mg of inbound matter is reserved",
                stockpile.value(),
                reserved.milligrams()
            ),
            Self::TargetContentsIncompatible { lot, error } => write!(
                formatter,
                "material lot {} is incompatible with the completed storage enclosure: {error}",
                lot.value()
            ),
            Self::StorageHistoryOverflow { lot } => write!(
                formatter,
                "material lot {} cannot checkpoint its existing storage exposure at construction time",
                lot.value()
            ),
            Self::InsufficientMaterial {
                stockpile,
                commodity,
                available,
                required,
            } => write!(
                formatter,
                "storage construction stockpile {} has {} mg of commodity {} but requires {} mg",
                stockpile.value(),
                available.milligrams(),
                commodity.value(),
                required.milligrams()
            ),
            Self::SourceMassOverflow { stockpile } => write!(
                formatter,
                "storage construction source stockpile {} mass accounting overflowed",
                stockpile.value()
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::StructuralLoad(error) => {
                write!(
                    formatter,
                    "storage construction source-load update failed: {error}"
                )
            }
        }
    }
}

impl Error for StorageEnclosureConstructionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::TargetContentsIncompatible { error, .. } => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownDefinition { .. }
            | Self::UnknownTarget { .. }
            | Self::UnknownSource { .. }
            | Self::AlreadyEnclosed { .. }
            | Self::TargetMounted { .. }
            | Self::TargetCapacityTooLarge { .. }
            | Self::TargetStorageProfileMismatch { .. }
            | Self::TargetHasReservedInbound { .. }
            | Self::StorageHistoryOverflow { .. }
            | Self::InsufficientMaterial { .. }
            | Self::SourceMassOverflow { .. }
            | Self::InventoryRevisionExhausted => None,
        }
    }
}

/// Failure to commit a validated storage-enclosure construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageEnclosureCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    UnknownTarget { stockpile: StockpileId },
    TargetProfileChanged { stockpile: StockpileId },
    TargetEnclosureChanged { stockpile: StockpileId },
    Structure(StructuralCommitError),
}

impl Display for StorageEnclosureCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "storage construction expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::UnknownTarget { stockpile } => write!(
                formatter,
                "storage construction target stockpile {} disappeared before commit",
                stockpile.value()
            ),
            Self::TargetProfileChanged { stockpile } => write!(
                formatter,
                "storage construction target stockpile {} changed storage profile before commit",
                stockpile.value()
            ),
            Self::TargetEnclosureChanged { stockpile } => write!(
                formatter,
                "storage construction target stockpile {} gained an enclosure before commit",
                stockpile.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "storage construction structural-load commit failed: {error}"
            ),
        }
    }
}

impl Error for StorageEnclosureCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventoryRevision { .. }
            | Self::UnknownTarget { .. }
            | Self::TargetProfileChanged { .. }
            | Self::TargetEnclosureChanged { .. } => None,
        }
    }
}

/// Revision-bound proof that exact construction matter can become one stockpile enclosure.
#[must_use]
pub struct ValidatedStorageEnclosureConstruction {
    target: StockpileId,
    expected_inventory_revision: u64,
    next_inventory_revision: u64,
    expected_profile: StockpileStorageProfile,
    next_profile: StockpileStorageProfile,
    enclosure: StockpileEnclosureRecord,
    egress: ValidatedMaterialEgress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedStorageEnclosureConstruction {
    /// Transfers the exact selected construction matter into persistent enclosure ownership.
    pub fn commit(self, state: &mut AppState) -> Result<(), StorageEnclosureCommitError> {
        let actual_revision = state.inventory().revision();
        if actual_revision != self.expected_inventory_revision {
            return Err(StorageEnclosureCommitError::StaleInventoryRevision {
                expected: self.expected_inventory_revision,
                actual: actual_revision,
            });
        }
        let target = state.inventory().get_stockpile(self.target).ok_or(
            StorageEnclosureCommitError::UnknownTarget {
                stockpile: self.target,
            },
        )?;
        if target.storage_profile() != self.expected_profile {
            return Err(StorageEnclosureCommitError::TargetProfileChanged {
                stockpile: self.target,
            });
        }
        if target.enclosure().is_some() {
            return Err(StorageEnclosureCommitError::TargetEnclosureChanged {
                stockpile: self.target,
            });
        }
        if let Some(structural_load) = self.structural_load {
            structural_load
                .commit(state)
                .map_err(StorageEnclosureCommitError::Structure)?;
        }
        apply_material_egress(state.inventory_state_mut(), self.egress);
        let at = self.enclosure.created_at();
        state.inventory_state_mut().apply_storage_enclosure(
            self.target,
            self.expected_profile,
            self.next_profile,
            self.enclosure,
            at,
            self.next_inventory_revision,
        );
        Ok(())
    }
}

/// Validates enclosing one existing ambient solid stockpile with an authored material-backed store.
///
/// Construction is intentionally in-place because general world-space haulage is not implemented.
/// Existing lot exposure is checkpointed at the construction tick before the improved preservation
/// multiplier begins, so infrastructure never retroactively restores freshness.
pub fn validate_build_storage_enclosure(
    registries: &Registries,
    state: &AppState,
    definition: StorageDefinitionId,
    target: StockpileId,
    source: StockpileId,
) -> Result<ValidatedStorageEnclosureConstruction, StorageEnclosureConstructionError> {
    let definition_record = registries
        .storage()
        .get(definition)
        .ok_or(StorageEnclosureConstructionError::UnknownDefinition { definition })?;
    let target_record = state
        .inventory()
        .get_stockpile(target)
        .ok_or(StorageEnclosureConstructionError::UnknownTarget { stockpile: target })?;
    if let Some(enclosure) = target_record.enclosure() {
        return Err(StorageEnclosureConstructionError::AlreadyEnclosed {
            stockpile: target,
            definition: enclosure.definition(),
        });
    }
    if let Some(element) = target_record.supported_by() {
        return Err(StorageEnclosureConstructionError::TargetMounted {
            stockpile: target,
            element,
        });
    }
    if target_record.capacity() > definition_record.maximum_stockpile_capacity() {
        return Err(StorageEnclosureConstructionError::TargetCapacityTooLarge {
            stockpile: target,
            capacity: target_record.capacity(),
            maximum: definition_record.maximum_stockpile_capacity(),
        });
    }
    let required_profile = StockpileStorageProfile::unbounded_solid_only();
    if target_record.storage_profile() != required_profile {
        return Err(
            StorageEnclosureConstructionError::TargetStorageProfileMismatch {
                stockpile: target,
                current: target_record.storage_profile(),
                required: required_profile,
            },
        );
    }
    if !target_record.reserved_inbound().is_zero() {
        return Err(
            StorageEnclosureConstructionError::TargetHasReservedInbound {
                stockpile: target,
                reserved: target_record.reserved_inbound(),
            },
        );
    }
    let selection = validate_consumption_selection(
        state.inventory(),
        source,
        definition_record.assembly_profile().inputs(),
    )
    .map_err(|error| match error {
        ConsumptionSelectionError::UnknownStockpile { stockpile } => {
            StorageEnclosureConstructionError::UnknownSource { stockpile }
        }
        ConsumptionSelectionError::InsufficientMass {
            stockpile,
            commodity,
            available,
            requested,
        } => StorageEnclosureConstructionError::InsufficientMaterial {
            stockpile,
            commodity,
            available,
            required: requested,
        },
        ConsumptionSelectionError::MassOverflow { stockpile } => {
            StorageEnclosureConstructionError::SourceMassOverflow { stockpile }
        }
    })?;
    let next_profile = definition_record.storage_profile();
    let source_preservation = required_profile.preservation_multiplier_ppm();
    for lot in state.inventory().lot_ids(target) {
        let record = state
            .inventory()
            .get_lot(lot)
            .unwrap_or_else(|| unreachable!("stockpile lot index references a live lot"));
        if source == target && selection.selected_mass_for_lot(lot) == record.mass() {
            continue;
        }
        validate_stockpile_storage_profile(
            registries,
            next_profile,
            target,
            record.commodity(),
            record.composition(),
            record.temperature(),
            record.particle_size_distribution(),
        )
        .map_err(|error| {
            StorageEnclosureConstructionError::TargetContentsIncompatible { lot, error }
        })?;
        if record
            .storage_history()
            .rebase(state.tick(), source_preservation)
            .is_none()
        {
            return Err(StorageEnclosureConstructionError::StorageHistoryOverflow { lot });
        }
    }
    let embodied_material = selection.consumed_inputs().to_vec();
    let egress =
        validate_material_egress_from_selection(state.inventory(), selection).map_err(|error| {
            match error {
                MaterialEgressError::StaleSelection { .. } => {
                    unreachable!(
                        "storage construction selection was derived from the current revision"
                    )
                }
                MaterialEgressError::RevisionExhausted => {
                    StorageEnclosureConstructionError::InventoryRevisionExhausted
                }
            }
        })?;
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .ok_or(StorageEnclosureConstructionError::UnknownSource { stockpile: source })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(egress.total_consumed())
        .ok_or(StorageEnclosureConstructionError::SourceMassOverflow { stockpile: source })?;
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(StorageEnclosureConstructionError::StructuralLoad)?;
    let expected_inventory_revision = state.inventory().revision();
    let next_inventory_revision = expected_inventory_revision
        .checked_add(2)
        .ok_or(StorageEnclosureConstructionError::InventoryRevisionExhausted)?;
    Ok(ValidatedStorageEnclosureConstruction {
        target,
        expected_inventory_revision,
        next_inventory_revision,
        expected_profile: required_profile,
        next_profile,
        enclosure: StockpileEnclosureRecord::new(definition, embodied_material, state.tick()),
        egress,
        structural_load,
    })
}

#[cfg(test)]
#[path = "enclosure_execution_tests.rs"]
mod tests;
