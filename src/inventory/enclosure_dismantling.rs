//! Exact recovery of material-backed storage enclosures into ordinary inventory custody.

use crate::core::state::AppState;
use crate::registry::Registries;

use super::storage_validation::validate_stockpile_storage_profile;
use super::{
    MaterialIngressEntry, MaterialIngressError, MaterialLotId, StockpileId,
    StockpileStorageProfile, StockpileStoredMassChange, StorageDefinitionId,
    ValidatedMaterialIngress, ValidatedStockpileStructuralLoad, apply_material_ingress,
    validate_material_ingress, validate_stockpile_stored_mass_changes,
};

mod errors;

pub use errors::{StorageEnclosureDismantleCommitError, StorageEnclosureDismantleError};

fn map_recovery_ingress_error(error: MaterialIngressError) -> StorageEnclosureDismantleError {
    match error {
        MaterialIngressError::UnknownStockpile { stockpile } => {
            StorageEnclosureDismantleError::UnknownRecoveryDestination { stockpile }
        }
        MaterialIngressError::Storage(error) => {
            StorageEnclosureDismantleError::RecoveryDestinationStorage(error)
        }
        MaterialIngressError::MassOverflow { stockpile } => {
            StorageEnclosureDismantleError::RecoveryMassOverflow { stockpile }
        }
        MaterialIngressError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => StorageEnclosureDismantleError::RecoveryCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialIngressError::LotIdExhausted => {
            StorageEnclosureDismantleError::RecoveryLotIdExhausted
        }
        MaterialIngressError::RevisionExhausted => {
            StorageEnclosureDismantleError::InventoryRevisionExhausted
        }
        MaterialIngressError::Empty
        | MaterialIngressError::UnknownMaterial { .. }
        | MaterialIngressError::UnknownForm { .. }
        | MaterialIngressError::UnknownCompositionMaterial { .. }
        | MaterialIngressError::ZeroMass
        | MaterialIngressError::InvalidComposition { .. }
        | MaterialIngressError::CompositionMissingHost { .. }
        | MaterialIngressError::InvalidProvenance
        | MaterialIngressError::ProvenanceInFuture { .. } => unreachable!(
            "validated enclosure embodiment must remain valid material ingress at the current tick"
        ),
    }
}

/// Observable result of returning one enclosure body to inventory custody.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageEnclosureDismantleOutcome {
    definition: StorageDefinitionId,
    recovered_lots: Vec<MaterialLotId>,
}

impl StorageEnclosureDismantleOutcome {
    #[must_use]
    pub const fn definition(&self) -> StorageDefinitionId {
        self.definition
    }

    #[must_use]
    pub fn recovered_lots(&self) -> &[MaterialLotId] {
        &self.recovered_lots
    }
}

/// Revision-bound proof that one enclosure can be removed without losing matter or storage history.
#[must_use]
pub struct ValidatedStorageEnclosureDismantling {
    target: StockpileId,
    definition: StorageDefinitionId,
    expected_inventory_revision: u64,
    next_inventory_revision: u64,
    expected_profile: StockpileStorageProfile,
    next_profile: StockpileStorageProfile,
    ingress: ValidatedMaterialIngress,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
    at: crate::core::time::SimulationTick,
}

impl ValidatedStorageEnclosureDismantling {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StorageEnclosureDismantleOutcome, StorageEnclosureDismantleCommitError> {
        let actual_revision = state.inventory().revision();
        if actual_revision != self.expected_inventory_revision {
            return Err(
                StorageEnclosureDismantleCommitError::StaleInventoryRevision {
                    expected: self.expected_inventory_revision,
                    actual: actual_revision,
                },
            );
        }
        let target = state.inventory().get_stockpile(self.target).ok_or(
            StorageEnclosureDismantleCommitError::UnknownTarget {
                stockpile: self.target,
            },
        )?;
        if target.storage_profile() != self.expected_profile {
            return Err(StorageEnclosureDismantleCommitError::TargetProfileChanged {
                stockpile: self.target,
            });
        }
        if target.enclosure().map(|record| record.definition()) != Some(self.definition) {
            return Err(
                StorageEnclosureDismantleCommitError::TargetEnclosureChanged {
                    stockpile: self.target,
                },
            );
        }
        self.ingress.assert_matches_state(state.inventory());
        if let Some(structural_load) = self.structural_load {
            structural_load
                .commit(state)
                .map_err(StorageEnclosureDismantleCommitError::Structure)?;
        }
        let recovered_lots = apply_material_ingress(state.inventory_state_mut(), self.ingress);
        state.inventory_state_mut().apply_storage_enclosure_removal(
            self.target,
            self.expected_profile,
            self.next_profile,
            self.definition,
            self.at,
            self.next_inventory_revision,
        );
        Ok(StorageEnclosureDismantleOutcome {
            definition: self.definition,
            recovered_lots,
        })
    }
}

/// Validates dismantling one material-backed enclosure into a distinct recovery stockpile.
///
/// Dismantling checkpoints every retained lot under the enclosure's current preservation multiplier
/// before reverting the target to ambient storage. Recovered enclosure matter retains exact
/// temperature, composition, and provenance; only custody changes.
pub fn validate_dismantle_storage_enclosure(
    registries: &Registries,
    state: &AppState,
    target: StockpileId,
    recovery_destination: StockpileId,
) -> Result<ValidatedStorageEnclosureDismantling, StorageEnclosureDismantleError> {
    let target_record = state
        .inventory()
        .get_stockpile(target)
        .ok_or(StorageEnclosureDismantleError::UnknownTarget { stockpile: target })?;
    let enclosure = target_record
        .enclosure()
        .ok_or(StorageEnclosureDismantleError::NotEnclosed { stockpile: target })?;
    if let Some(element) = target_record.supported_by() {
        return Err(StorageEnclosureDismantleError::TargetMounted {
            stockpile: target,
            element,
        });
    }
    if !target_record.reserved_inbound().is_zero() {
        return Err(StorageEnclosureDismantleError::TargetHasReservedInbound {
            stockpile: target,
            reserved: target_record.reserved_inbound(),
        });
    }
    if recovery_destination == target {
        return Err(
            StorageEnclosureDismantleError::RecoveryDestinationIsTarget { stockpile: target },
        );
    }
    let recovery_record = state
        .inventory()
        .get_stockpile(recovery_destination)
        .ok_or(StorageEnclosureDismantleError::UnknownRecoveryDestination {
            stockpile: recovery_destination,
        })?;
    let definition = enclosure.definition();
    let next_profile = StockpileStorageProfile::unbounded_solid_only();
    let source_preservation = target_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let destination_preservation = next_profile.preservation_multiplier_ppm();
    for lot in state.inventory().lot_ids(target) {
        let record = state
            .inventory()
            .get_lot(lot)
            .unwrap_or_else(|| unreachable!("stockpile lot index references a live lot"));
        validate_stockpile_storage_profile(
            registries,
            next_profile,
            target,
            record.commodity(),
            record.composition(),
            record.temperature(),
            record.particle_size_distribution(),
        )
        .map_err(
            |error| StorageEnclosureDismantleError::TargetContentsIncompatible { lot, error },
        )?;
        if record
            .storage_history()
            .transition_preservation(state.tick(), source_preservation, destination_preservation)
            .is_none()
        {
            return Err(StorageEnclosureDismantleError::StorageHistoryOverflow { lot });
        }
    }
    let entries = enclosure
        .embodied_material()
        .iter()
        .map(MaterialIngressEntry::from_consumed_trace)
        .collect::<Vec<_>>();
    let ingress = validate_material_ingress(
        registries,
        state.inventory(),
        recovery_destination,
        entries,
        state.tick(),
    )
    .map_err(map_recovery_ingress_error)?;
    let recovered_mass = enclosure.embodied_mass();
    let recovery_after = recovery_record
        .stored_mass()
        .checked_add(recovered_mass)
        .ok_or(StorageEnclosureDismantleError::RecoveryMassOverflow {
            stockpile: recovery_destination,
        })?;
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(
            recovery_destination,
            recovery_after,
        )],
    )
    .map_err(StorageEnclosureDismantleError::StructuralLoad)?;
    let expected_inventory_revision = state.inventory().revision();
    let next_inventory_revision = expected_inventory_revision
        .checked_add(2)
        .ok_or(StorageEnclosureDismantleError::InventoryRevisionExhausted)?;
    assert_eq!(
        ingress.expected_revision(),
        expected_inventory_revision,
        "enclosure recovery ingress must bind the dismantling inventory revision"
    );
    Ok(ValidatedStorageEnclosureDismantling {
        target,
        definition,
        expected_inventory_revision,
        next_inventory_revision,
        expected_profile: target_record.storage_profile(),
        next_profile,
        ingress,
        structural_load,
        at: state.tick(),
    })
}

#[cfg(test)]
#[path = "enclosure_dismantling_tests.rs"]
mod tests;
