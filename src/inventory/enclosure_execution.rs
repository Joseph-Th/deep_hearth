//! Material-backed construction of preservation enclosures around existing stockpiles.

use crate::core::state::AppState;
use crate::registry::Registries;

use super::storage_validation::validate_stockpile_storage_profile;
use super::{
    ConsumedMaterialTrace, ConsumptionSelection, ConsumptionSelectionError, MaterialEgressError,
    StockpileEnclosureRecord, StockpileId, StockpileRecord, StockpileStorageProfile,
    StockpileStoredMassChange, StorageDefinition, StorageDefinitionId, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress, validate_consumption_selection,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
};

mod errors;

pub use errors::{StorageEnclosureCommitError, StorageEnclosureConstructionError};

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

struct EnclosureMaterialPlan {
    embodied_material: Vec<ConsumedMaterialTrace>,
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
        self.egress.assert_matches_state(state.inventory());
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
    if state
        .player_work()
        .get_storage_dismantling_stockpile_occupant(target)
        .is_some()
    {
        return Err(
            StorageEnclosureConstructionError::TargetBusyStorageDismantling { stockpile: target },
        );
    }
    let required_profile = StockpileStorageProfile::unbounded_solid_only();
    validate_enclosure_target(definition_record, target_record, target, required_profile)?;
    let selection = select_enclosure_material(state, definition_record, source)?;
    let next_profile = definition_record.storage_profile();
    validate_enclosure_contents(registries, state, target_record, &selection, next_profile)?;
    let material_plan = plan_enclosure_materials(registries, state, source, selection)?;
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
        enclosure: StockpileEnclosureRecord::new(
            definition,
            material_plan.embodied_material,
            state.tick(),
        ),
        egress: material_plan.egress,
        structural_load: material_plan.structural_load,
    })
}

fn validate_enclosure_target(
    definition: &StorageDefinition,
    target_record: &StockpileRecord,
    target: StockpileId,
    required_profile: StockpileStorageProfile,
) -> Result<(), StorageEnclosureConstructionError> {
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
    if target_record.capacity() > definition.maximum_stockpile_capacity() {
        return Err(StorageEnclosureConstructionError::TargetCapacityTooLarge {
            stockpile: target,
            capacity: target_record.capacity(),
            maximum: definition.maximum_stockpile_capacity(),
        });
    }
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
    Ok(())
}

fn select_enclosure_material(
    state: &AppState,
    definition: &StorageDefinition,
    source: StockpileId,
) -> Result<ConsumptionSelection, StorageEnclosureConstructionError> {
    validate_consumption_selection(
        state.inventory(),
        source,
        definition.assembly_profile().inputs(),
    )
    .map_err(map_selection_error)
}

fn map_selection_error(error: ConsumptionSelectionError) -> StorageEnclosureConstructionError {
    match error {
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
    }
}

fn validate_enclosure_contents(
    registries: &Registries,
    state: &AppState,
    target_record: &StockpileRecord,
    selection: &ConsumptionSelection,
    next_profile: StockpileStorageProfile,
) -> Result<(), StorageEnclosureConstructionError> {
    let target = target_record.id();
    let source = selection.source();
    let source_preservation = target_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let destination_preservation = next_profile.preservation_multiplier_ppm();
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
            .transition_preservation(state.tick(), source_preservation, destination_preservation)
            .is_none()
        {
            return Err(StorageEnclosureConstructionError::StorageHistoryOverflow { lot });
        }
    }
    Ok(())
}

fn plan_enclosure_materials(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    selection: ConsumptionSelection,
) -> Result<EnclosureMaterialPlan, StorageEnclosureConstructionError> {
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
    Ok(EnclosureMaterialPlan {
        embodied_material,
        egress,
        structural_load,
    })
}

#[cfg(test)]
#[path = "enclosure_execution_tests.rs"]
mod tests;
