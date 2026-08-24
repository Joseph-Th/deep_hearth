//! Exact revision-bound relocation of preselected material between inventory stockpiles.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::material::MaterialInputSpec;
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::super::coalescing::LotMergePolicy;
use super::super::lot_identity::LotIdentityPlanner;
use super::super::selection::ConsumptionSelection;
use super::super::state::{
    LotSlice, LotStorageTransition, MaterialLotId, StockpileId, apply_aggregate_deposit,
    apply_aggregate_withdraw, apply_move_full_lot, apply_split_lot,
};
use super::super::storage_validation::{StockpileStorageError, validate_stockpile_storage};
use super::super::structural_integration::{
    StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedStockpileStructuralLoad,
    validate_stockpile_stored_mass_changes,
};

/// Revision-bound relocation of one already-resolved exact lot selection between stockpiles.
///
/// Cross-owner systems use this when a physical resolver inspected specific lot slices before
/// deciding a consequence. The relocation preserves selected profiles and provenance exactly and
/// never substitutes equivalent-looking inventory at validation time.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedMaterialRelocation {
    expected_revision: u64,
    next_revision: u64,
    source: StockpileId,
    destination: StockpileId,
    inputs: Vec<MaterialInputSpec>,
    lot_slices: Vec<LotSlice>,
    split_lot_ids: Vec<Option<MaterialLotId>>,
    merge_policies: Vec<LotMergePolicy>,
    next_lot_id_after: Option<u64>,
    total_mass: Mass,
    structural: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedMaterialRelocation {
    #[cfg(test)]
    pub(crate) const fn total_mass(&self) -> Mass {
        self.total_mass
    }

    pub(crate) fn commit(self, state: &mut AppState) -> Result<(), MaterialRelocationCommitError> {
        let actual = state.inventory().revision();
        if actual != self.expected_revision {
            return Err(MaterialRelocationCommitError::StaleInventoryRevision {
                expected: self.expected_revision,
                actual,
            });
        }
        let current_tick = state.tick();
        let source_preservation_multiplier_ppm = state
            .inventory()
            .get_stockpile(self.source)
            .unwrap_or_else(|| panic!("validated material relocation source disappeared"))
            .storage_profile()
            .preservation_multiplier_ppm();
        let destination_preservation_multiplier_ppm = state
            .inventory()
            .get_stockpile(self.destination)
            .unwrap_or_else(|| panic!("validated material relocation destination disappeared"))
            .storage_profile()
            .preservation_multiplier_ppm();
        let storage_transition = LotStorageTransition::new(
            current_tick,
            source_preservation_multiplier_ppm,
            destination_preservation_multiplier_ppm,
        );
        if let Some(structural) = self.structural {
            structural
                .commit(state)
                .map_err(MaterialRelocationCommitError::Structure)?;
        }

        let inventories = state.inventory_state_mut();
        for input in &self.inputs {
            apply_aggregate_withdraw(inventories, self.source, input.commodity(), input.mass());
            apply_aggregate_deposit(
                inventories,
                self.destination,
                input.commodity(),
                input.mass(),
            );
        }
        let transfers = self
            .lot_slices
            .into_iter()
            .zip(self.split_lot_ids)
            .zip(self.merge_policies)
            .map(|((slice, split_lot_id), merge_policy)| (slice, split_lot_id, merge_policy))
            .collect::<Vec<_>>();
        for (slice, _split_lot_id, merge_policy) in transfers
            .iter()
            .copied()
            .filter(|(_, split_lot_id, _)| split_lot_id.is_none())
        {
            let lot_mass = match inventories.get_lot(slice.lot) {
                Some(lot) => lot.mass,
                None => panic!(
                    "validated material relocation references missing lot {}",
                    slice.lot.value()
                ),
            };
            assert_eq!(
                slice.mass, lot_mass,
                "validated full material relocation no longer covers its complete lot"
            );
            apply_move_full_lot(
                inventories,
                slice.lot,
                self.source,
                self.destination,
                storage_transition,
                merge_policy,
            );
        }
        for (slice, split_lot_id, merge_policy) in transfers
            .into_iter()
            .filter(|(_, split_lot_id, _)| split_lot_id.is_some())
        {
            let split_lot_id = split_lot_id.unwrap_or_else(|| {
                unreachable!("partial relocation filter requires a planned lot identity")
            });
            apply_split_lot(
                inventories,
                slice.lot,
                split_lot_id,
                self.destination,
                slice.mass,
                storage_transition,
                merge_policy,
            );
        }
        if let Some(next_lot_id) = self.next_lot_id_after {
            inventories.apply_lot_cursor_and_revision(next_lot_id, self.next_revision);
        } else {
            inventories.apply_revision(self.next_revision);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialRelocationError {
    StaleSelection {
        expected: u64,
        actual: u64,
    },
    UnknownSource {
        stockpile: StockpileId,
    },
    UnknownDestination {
        stockpile: StockpileId,
    },
    SameStockpile {
        stockpile: StockpileId,
    },
    DestinationStorage(StockpileStorageError),
    DestinationMassOverflow {
        stockpile: StockpileId,
    },
    DestinationCapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    RevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for MaterialRelocationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSelection { expected, actual } => write!(
                formatter,
                "exact material relocation expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "exact material relocation source stockpile {} does not exist",
                stockpile.value()
            ),
            Self::UnknownDestination { stockpile } => write!(
                formatter,
                "exact material relocation destination stockpile {} does not exist",
                stockpile.value()
            ),
            Self::SameStockpile { stockpile } => write!(
                formatter,
                "exact material relocation requires distinct source and destination; both are stockpile {}",
                stockpile.value()
            ),
            Self::DestinationStorage(error) => write!(
                formatter,
                "exact material relocation destination rejects selected matter: {error}"
            ),
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "exact material relocation overflows destination stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "exact material relocation exceeds stockpile {} capacity {} mg: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => formatter
                .write_str("material lot identifier space is exhausted during exact relocation"),
            Self::RevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted during exact relocation")
            }
            Self::StructuralLoad(error) => write!(
                formatter,
                "exact material relocation structural load failed: {error}"
            ),
        }
    }
}

impl Error for MaterialRelocationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DestinationStorage(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::StaleSelection { .. }
            | Self::UnknownSource { .. }
            | Self::UnknownDestination { .. }
            | Self::SameStockpile { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::LotIdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialRelocationCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for MaterialRelocationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated material relocation expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "material relocation structural commit failed: {error}"
            ),
        }
    }
}

impl Error for MaterialRelocationCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventoryRevision { .. } => None,
        }
    }
}

/// Validates relocation of the exact lot slices already bound by a physical resolver.
pub(crate) fn validate_material_relocation_from_selection(
    registries: &Registries,
    state: &AppState,
    destination: StockpileId,
    selection: ConsumptionSelection,
) -> Result<ValidatedMaterialRelocation, MaterialRelocationError> {
    let ConsumptionSelection {
        expected_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs,
        total_consumed,
    } = selection;
    let inventories = state.inventory();
    if inventories.revision() != expected_revision {
        return Err(MaterialRelocationError::StaleSelection {
            expected: expected_revision,
            actual: inventories.revision(),
        });
    }
    let source_record = inventories
        .get_stockpile(source)
        .ok_or(MaterialRelocationError::UnknownSource { stockpile: source })?;
    let destination_record = inventories.get_stockpile(destination).ok_or(
        MaterialRelocationError::UnknownDestination {
            stockpile: destination,
        },
    )?;
    if source == destination {
        return Err(MaterialRelocationError::SameStockpile { stockpile: source });
    }

    for trace in &consumed_inputs {
        validate_stockpile_storage(
            registries,
            destination_record,
            destination,
            trace.profile().commodity(),
            trace.profile().composition(),
            trace.profile().temperature(),
            trace.profile().particle_size_distribution(),
        )
        .map_err(MaterialRelocationError::DestinationStorage)?;
    }
    let source_preservation_multiplier_ppm = source_record
        .storage_profile()
        .preservation_multiplier_ppm();
    debug_assert!(lot_slices.iter().all(|slice| {
        inventories
            .get_lot(slice.lot)
            .and_then(|lot| {
                lot.storage_history()
                    .rebase(state.tick(), source_preservation_multiplier_ppm)
            })
            .is_some()
    }));
    let committed = destination_record
        .stored_mass
        .checked_add(destination_record.reserved_inbound)
        .ok_or(MaterialRelocationError::DestinationMassOverflow {
            stockpile: destination,
        })?;
    let capacity_after = committed.checked_add(total_consumed).ok_or(
        MaterialRelocationError::DestinationMassOverflow {
            stockpile: destination,
        },
    )?;
    if capacity_after > destination_record.capacity {
        return Err(MaterialRelocationError::DestinationCapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity,
            committed,
            requested: total_consumed,
        });
    }
    let destination_stored_after = destination_record
        .stored_mass()
        .checked_add(total_consumed)
        .ok_or(MaterialRelocationError::DestinationMassOverflow {
            stockpile: destination,
        })?;
    for input in &inputs {
        destination_record
            .get_mass(input.commodity())
            .checked_add(input.mass())
            .ok_or(MaterialRelocationError::DestinationMassOverflow {
                stockpile: destination,
            })?;
    }

    let source_after = source_record
        .stored_mass()
        .checked_sub(total_consumed)
        .ok_or(MaterialRelocationError::StaleSelection {
            expected: expected_revision,
            actual: inventories.revision(),
        })?;
    let structural = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [
            StockpileStoredMassChange::new(source, source_after),
            StockpileStoredMassChange::new(destination, destination_stored_after),
        ],
    )
    .map_err(MaterialRelocationError::StructuralLoad)?;

    let merge_policies = lot_slices
        .iter()
        .map(|slice| {
            let lot = inventories.get_lot(slice.lot).unwrap_or_else(|| {
                panic!(
                    "validated exact selection references missing lot {}",
                    slice.lot.value()
                )
            });
            LotMergePolicy::for_commodity(registries, lot.commodity())
        })
        .collect::<Vec<_>>();
    let destination_preservation_multiplier_ppm = destination_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let mut identity_planner = LotIdentityPlanner::new(inventories, std::iter::empty());
    for slice in &lot_slices {
        let lot = match inventories.get_lot(slice.lot) {
            Some(lot) => lot,
            None => panic!(
                "validated exact selection references missing lot {}",
                slice.lot.value()
            ),
        };
        if slice.mass == lot.mass {
            let storage_history = lot
                .storage_history()
                .rebase(state.tick(), source_preservation_multiplier_ppm)
                .unwrap_or_else(|| {
                    panic!("valid full-lot relocation storage history could not be rebased")
                });
            identity_planner.note_preserved_arrival(
                lot.id(),
                destination,
                &lot.profile,
                storage_history,
            );
        }
    }
    let mut split_lot_ids = Vec::with_capacity(lot_slices.len());
    for (slice, merge_policy) in lot_slices.iter().zip(&merge_policies) {
        let lot = inventories.get_lot(slice.lot).unwrap_or_else(|| {
            panic!(
                "validated exact selection references missing lot {}",
                slice.lot.value()
            )
        });
        if slice.mass == lot.mass() {
            split_lot_ids.push(None);
            continue;
        }
        let storage_history = lot
            .storage_history()
            .rebase(state.tick(), source_preservation_multiplier_ppm)
            .unwrap_or_else(|| {
                panic!("valid partial relocation storage history could not be rebased")
            });
        split_lot_ids.push(Some(
            identity_planner
                .plan(
                    destination,
                    &lot.profile,
                    storage_history,
                    state.tick(),
                    destination_preservation_multiplier_ppm,
                    *merge_policy,
                )
                .ok_or(MaterialRelocationError::LotIdExhausted)?,
        ));
    }
    let next_lot_id_after = identity_planner
        .allocated_any()
        .then_some(identity_planner.next_lot_id());
    let next_revision = inventories
        .revision()
        .checked_add(1)
        .ok_or(MaterialRelocationError::RevisionExhausted)?;

    Ok(ValidatedMaterialRelocation {
        expected_revision,
        next_revision,
        source,
        destination,
        inputs,
        lot_slices,
        split_lot_ids,
        merge_policies,
        next_lot_id_after,
        total_mass: total_consumed,
        structural,
    })
}
