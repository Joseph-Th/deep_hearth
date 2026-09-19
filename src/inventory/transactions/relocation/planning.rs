//! Read-only endpoint, storage, mass, and lot-identity planning for material relocation.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::coalescing::LotMergePolicy;
use crate::inventory::lot_identity::LotIdentityPlanner;
use crate::inventory::state::{
    ConsumedMaterialTrace, InventoryState, LotSlice, MaterialLotId, StockpileId, StockpileRecord,
};
use crate::inventory::storage_validation::validate_stockpile_storage;
use crate::material::MaterialInputSpec;
use crate::registry::Registries;

use super::RelocationLotTransfer;
use super::errors::MaterialRelocationError;

pub(super) fn validate_relocation_endpoints(
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
        .unwrap_or_else(|| panic!("validated material relocation source disappeared"));
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

pub(super) fn validate_destination_storage(
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

pub(super) fn validate_destination_mass(
    destination_record: &StockpileRecord,
    destination: StockpileId,
    inputs: &[MaterialInputSpec],
    total_consumed: Mass,
) -> Result<Mass, MaterialRelocationError> {
    let overflow = || MaterialRelocationError::DestinationMassOverflow {
        stockpile: destination,
    };
    let projection = destination_record
        .project_mass_exchange(Mass::ZERO, total_consumed)
        .ok_or_else(overflow)?;
    if projection.after_incoming > destination_record.capacity() {
        return Err(MaterialRelocationError::DestinationCapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity(),
            committed: projection.committed_before_incoming,
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

pub(super) fn plan_lot_transfers(
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
            .transition_preservation(
                state.tick(),
                source_preservation_multiplier_ppm,
                destination_preservation_multiplier_ppm,
            )
            .unwrap_or_else(|| {
                panic!("valid full-lot relocation storage history could not be transitioned")
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
                .transition_preservation(
                    state.tick(),
                    source_preservation_multiplier_ppm,
                    destination_preservation_multiplier_ppm,
                )
                .unwrap_or_else(|| {
                    panic!("valid partial relocation storage history could not be transitioned")
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
