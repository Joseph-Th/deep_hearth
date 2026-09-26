//! Deterministic planning for persistent lot identities across coalescing inventory ingress.

use std::collections::BTreeSet;

use crate::core::time::SimulationTick;

use super::coalescing::{LotMergePolicy, lots_are_merge_compatible};
use super::state::{
    InventoryState, MaterialLotId, MaterialLotProfile, MaterialStorageHistory, StockpileId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlannedAvailableLot {
    id: MaterialLotId,
    destination: StockpileId,
    profile: MaterialLotProfile,
    storage_history: MaterialStorageHistory,
}

/// Plans candidate identities against both current lots and earlier arrivals in the same atomic
/// transaction. A cursor value is consumed only when no compatible lot will exist at insertion.
pub(in crate::inventory) struct LotIdentityPlanner<'a> {
    state: &'a InventoryState,
    excluded_existing: BTreeSet<MaterialLotId>,
    available_arrivals: Vec<PlannedAvailableLot>,
    initial_next_lot_id: u64,
    next_lot_id: u64,
}

impl<'a> LotIdentityPlanner<'a> {
    pub(in crate::inventory) fn new(
        state: &'a InventoryState,
        excluded_existing: impl IntoIterator<Item = MaterialLotId>,
    ) -> Self {
        let next_lot_id = state.next_lot_id();
        Self {
            state,
            excluded_existing: excluded_existing.into_iter().collect(),
            available_arrivals: Vec::new(),
            initial_next_lot_id: next_lot_id,
            next_lot_id,
        }
    }

    /// Adds a lot whose existing identity will arrive before subsequently planned new parcels.
    pub(in crate::inventory) fn note_preserved_arrival(
        &mut self,
        id: MaterialLotId,
        destination: StockpileId,
        profile: &MaterialLotProfile,
        storage_history: MaterialStorageHistory,
    ) {
        self.available_arrivals.push(PlannedAvailableLot {
            id,
            destination,
            profile: profile.clone(),
            storage_history,
        });
    }

    /// Returns an identity safe to place on the incoming record. If a compatible destination lot
    /// already exists, its identity is returned and the cursor is untouched. Otherwise a new
    /// identity is allocated and becomes available to later parcels in this same plan.
    pub(in crate::inventory) fn plan(
        &mut self,
        destination: StockpileId,
        profile: &MaterialLotProfile,
        storage_history: MaterialStorageHistory,
        at: SimulationTick,
        preservation_multiplier_ppm: u32,
        merge_policy: LotMergePolicy,
    ) -> Option<MaterialLotId> {
        if let Some(existing) = self
            .state
            .lot_ids_for_commodity(destination, profile.commodity())
            .filter(|id| !self.excluded_existing.contains(id))
            .find(|id| {
                self.state.get_lot(*id).is_some_and(|lot| {
                    lots_are_merge_compatible(
                        &lot.profile,
                        lot.storage_history,
                        profile,
                        storage_history,
                        at,
                        preservation_multiplier_ppm,
                        merge_policy,
                    )
                })
            })
        {
            return Some(existing);
        }

        if let Some(arrival) = self.available_arrivals.iter().find(|arrival| {
            arrival.destination == destination
                && lots_are_merge_compatible(
                    &arrival.profile,
                    arrival.storage_history,
                    profile,
                    storage_history,
                    at,
                    preservation_multiplier_ppm,
                    merge_policy,
                )
        }) {
            return Some(arrival.id);
        }

        let id = MaterialLotId::new(self.next_lot_id);
        self.next_lot_id = self.next_lot_id.checked_add(1)?;
        self.available_arrivals.push(PlannedAvailableLot {
            id,
            destination,
            profile: profile.clone(),
            storage_history,
        });
        Some(id)
    }

    pub(in crate::inventory) const fn next_lot_id(&self) -> u64 {
        self.next_lot_id
    }

    pub(in crate::inventory) const fn allocated_any(&self) -> bool {
        self.next_lot_id != self.initial_next_lot_id
    }
}
