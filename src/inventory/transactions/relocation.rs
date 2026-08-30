//! Exact revision-bound relocation of preselected material between inventory stockpiles.

use std::collections::BTreeMap;
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
    ConsumedMaterialTrace, InventoryState, LotSlice, LotStorageTransition, MaterialLotId,
    StockpileId, StockpileRecord, apply_aggregate_deposit, apply_aggregate_withdraw,
    apply_move_full_lot, apply_split_lot, checked_consumed_material_mass,
};
use super::super::storage_validation::{StockpileStorageError, validate_stockpile_storage};
use super::super::structural_integration::{
    StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedStockpileStructuralLoad,
    validate_stockpile_stored_mass_changes,
};

mod integrity;

/// Revision-bound relocation of one already-resolved exact lot selection between stockpiles.
///
/// Cross-owner systems use this when a physical resolver inspected specific lot slices before
/// deciding a consequence. The relocation preserves selected profiles and provenance exactly and
/// never substitutes equivalent-looking inventory at validation time.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedMaterialRelocation {
    expected_revision: u64,
    next_revision: u64,
    source: StockpileId,
    destination: StockpileId,
    inputs: Vec<MaterialInputSpec>,
    transfers: Vec<RelocationLotTransfer>,
    next_lot_id_after: Option<u64>,
    structural: Option<ValidatedStockpileStructuralLoad>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RelocationLotTransfer {
    slice: LotSlice,
    split_lot_id: Option<MaterialLotId>,
    merge_policy: LotMergePolicy,
}

impl ValidatedMaterialRelocation {
    #[cfg(test)]
    pub(crate) fn total_mass(&self) -> Mass {
        self.inputs.iter().fold(Mass::ZERO, |total, input| {
            total
                .checked_add(input.mass())
                .unwrap_or_else(|| panic!("validated material relocation mass overflowed"))
        })
    }

    pub(crate) fn commit(self, state: &mut AppState) -> Result<(), MaterialRelocationCommitError> {
        let actual = state.inventory().revision();
        if actual != self.expected_revision {
            return Err(MaterialRelocationCommitError::StaleInventoryRevision {
                expected: self.expected_revision,
                actual,
            });
        }
        self.assert_matches_state(state);
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
        apply_lot_transfers(
            inventories,
            self.source,
            self.destination,
            storage_transition,
            self.transfers,
        );
        if let Some(next_lot_id) = self.next_lot_id_after {
            inventories.apply_lot_cursor_and_revision(next_lot_id, self.next_revision);
        } else {
            inventories.apply_revision(self.next_revision);
        }
        Ok(())
    }
}

fn apply_lot_transfers(
    inventories: &mut InventoryState,
    source: StockpileId,
    destination: StockpileId,
    storage_transition: LotStorageTransition,
    transfers: Vec<RelocationLotTransfer>,
) {
    for transfer in transfers
        .iter()
        .copied()
        .filter(|transfer| transfer.split_lot_id.is_none())
    {
        let lot_mass = inventories
            .get_lot(transfer.slice.lot)
            .unwrap_or_else(|| {
                panic!(
                    "validated material relocation references missing lot {}",
                    transfer.slice.lot.value()
                )
            })
            .mass;
        assert_eq!(
            transfer.slice.mass, lot_mass,
            "validated full material relocation no longer covers its complete lot"
        );
        apply_move_full_lot(
            inventories,
            transfer.slice.lot,
            source,
            destination,
            storage_transition,
            transfer.merge_policy,
        );
    }
    for transfer in transfers
        .into_iter()
        .filter(|transfer| transfer.split_lot_id.is_some())
    {
        let split_lot_id = transfer.split_lot_id.unwrap_or_else(|| {
            unreachable!("partial relocation filter requires a planned lot identity")
        });
        apply_split_lot(
            inventories,
            transfer.slice.lot,
            split_lot_id,
            destination,
            transfer.slice.mass,
            storage_transition,
            transfer.merge_policy,
        );
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
    } = selection;
    let total_consumed = checked_consumed_material_mass(&consumed_inputs).unwrap_or_else(|| {
        panic!("validated consumption selection mass overflowed before relocation")
    });
    let inventories = state.inventory();
    let (source_record, destination_record) =
        validate_relocation_endpoints(inventories, expected_revision, source, destination)?;
    validate_destination_storage(
        registries,
        destination_record,
        destination,
        &consumed_inputs,
    )?;
    let destination_stored_after =
        validate_destination_mass(destination_record, destination, &inputs, total_consumed)?;

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
    let (transfers, next_lot_id_after) = plan_lot_transfers(
        registries,
        state,
        destination,
        source_record,
        destination_record,
        &lot_slices,
    )?;
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
        transfers,
        next_lot_id_after,
        structural,
    })
}

fn validate_relocation_endpoints(
    inventories: &InventoryState,
    expected_revision: u64,
    source: StockpileId,
    destination: StockpileId,
) -> Result<(&StockpileRecord, &StockpileRecord), MaterialRelocationError> {
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
    Ok((source_record, destination_record))
}

fn validate_destination_storage(
    registries: &Registries,
    destination_record: &StockpileRecord,
    destination: StockpileId,
    consumed_inputs: &[ConsumedMaterialTrace],
) -> Result<(), MaterialRelocationError> {
    for trace in consumed_inputs {
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
    Ok(())
}

fn validate_destination_mass(
    destination_record: &StockpileRecord,
    destination: StockpileId,
    inputs: &[MaterialInputSpec],
    total_consumed: Mass,
) -> Result<Mass, MaterialRelocationError> {
    let overflow = || MaterialRelocationError::DestinationMassOverflow {
        stockpile: destination,
    };
    let committed = destination_record
        .stored_mass
        .checked_add(destination_record.reserved_inbound)
        .ok_or_else(overflow)?;
    let capacity_after = committed.checked_add(total_consumed).ok_or_else(overflow)?;
    if capacity_after > destination_record.capacity {
        return Err(MaterialRelocationError::DestinationCapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity,
            committed,
            requested: total_consumed,
        });
    }
    for input in inputs {
        destination_record
            .get_mass(input.commodity())
            .checked_add(input.mass())
            .ok_or_else(overflow)?;
    }
    destination_record
        .stored_mass()
        .checked_add(total_consumed)
        .ok_or_else(overflow)
}

fn plan_lot_transfers(
    registries: &Registries,
    state: &AppState,
    destination: StockpileId,
    source_record: &StockpileRecord,
    destination_record: &StockpileRecord,
    lot_slices: &[LotSlice],
) -> Result<(Vec<RelocationLotTransfer>, Option<u64>), MaterialRelocationError> {
    let lot_slices = consolidate_lot_slices(lot_slices);
    let inventories = state.inventory();
    let source_preservation_multiplier_ppm = source_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let destination_preservation_multiplier_ppm = destination_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let mut identity_planner = LotIdentityPlanner::new(inventories, std::iter::empty());

    for slice in &lot_slices {
        let lot = inventories.get_lot(slice.lot).unwrap_or_else(|| {
            panic!(
                "validated exact selection references missing lot {}",
                slice.lot.value()
            )
        });
        if slice.mass != lot.mass() {
            continue;
        }
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

    let mut transfers = Vec::with_capacity(lot_slices.len());
    for slice in &lot_slices {
        let lot = inventories.get_lot(slice.lot).unwrap_or_else(|| {
            panic!(
                "validated exact selection references missing lot {}",
                slice.lot.value()
            )
        });
        let merge_policy = LotMergePolicy::for_commodity(registries, lot.commodity());
        let split_lot_id = if slice.mass == lot.mass() {
            None
        } else {
            let storage_history = lot
                .storage_history()
                .rebase(state.tick(), source_preservation_multiplier_ppm)
                .unwrap_or_else(|| {
                    panic!("valid partial relocation storage history could not be rebased")
                });
            Some(
                identity_planner
                    .plan(
                        destination,
                        &lot.profile,
                        storage_history,
                        state.tick(),
                        destination_preservation_multiplier_ppm,
                        merge_policy,
                    )
                    .ok_or(MaterialRelocationError::LotIdExhausted)?,
            )
        };
        transfers.push(RelocationLotTransfer {
            slice: *slice,
            split_lot_id,
            merge_policy,
        });
    }
    let next_lot_id_after = identity_planner
        .allocated_any()
        .then_some(identity_planner.next_lot_id());
    Ok((transfers, next_lot_id_after))
}

fn consolidate_lot_slices(lot_slices: &[LotSlice]) -> Vec<LotSlice> {
    let mut by_lot = BTreeMap::<MaterialLotId, Mass>::new();
    for slice in lot_slices {
        let current = by_lot.get(&slice.lot).copied().unwrap_or(Mass::ZERO);
        let mass = current
            .checked_add(slice.mass)
            .unwrap_or_else(|| panic!("validated relocation lot-slice mass overflowed"));
        by_lot.insert(slice.lot, mass);
    }
    by_lot
        .into_iter()
        .map(|(lot, mass)| LotSlice { lot, mass })
        .collect()
}
