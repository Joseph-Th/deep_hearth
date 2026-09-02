//! Deterministic lot-identity and merge-policy planning for canonical ingress.

use std::collections::BTreeSet;

use crate::core::time::SimulationTick;
use crate::registry::Registries;

use super::{MaterialIngressEntry, MaterialIngressError};
use crate::inventory::coalescing::LotMergePolicy;
use crate::inventory::lot_identity::LotIdentityPlanner;
use crate::inventory::state::{
    InventoryState, MaterialLotId, MaterialStorageHistory, StockpileId, StockpileRecord,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct IngressIdentityPlan {
    pub(super) lot_ids: Vec<MaterialLotId>,
    pub(super) merge_policies: Vec<LotMergePolicy>,
    pub(super) excluded_existing: BTreeSet<MaterialLotId>,
    pub(super) next_lot_id: u64,
}

pub(super) fn plan_ingress_identities(
    registries: &Registries,
    state: &InventoryState,
    destination_record: &StockpileRecord,
    destination: StockpileId,
    entries: &[MaterialIngressEntry],
    current_tick: SimulationTick,
    excluded_existing: BTreeSet<MaterialLotId>,
) -> Result<IngressIdentityPlan, MaterialIngressError> {
    let merge_policies = entries
        .iter()
        .map(|entry| LotMergePolicy::for_commodity(registries, entry.profile.commodity()))
        .collect::<Vec<_>>();
    replay_ingress_identity_plan(
        state,
        destination_record,
        destination,
        entries,
        current_tick,
        excluded_existing,
        merge_policies,
    )
}

pub(super) fn replay_ingress_identity_plan(
    state: &InventoryState,
    destination_record: &StockpileRecord,
    destination: StockpileId,
    entries: &[MaterialIngressEntry],
    current_tick: SimulationTick,
    excluded_existing: BTreeSet<MaterialLotId>,
    merge_policies: Vec<LotMergePolicy>,
) -> Result<IngressIdentityPlan, MaterialIngressError> {
    assert_eq!(
        entries.len(),
        merge_policies.len(),
        "ingress identity planning requires one merge policy per parcel"
    );
    let preservation_multiplier_ppm = destination_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let storage_history = MaterialStorageHistory::new(current_tick);
    let mut identity_planner = LotIdentityPlanner::new(state, excluded_existing.iter().copied());
    let mut lot_ids = Vec::with_capacity(entries.len());
    for (entry, merge_policy) in entries.iter().zip(&merge_policies) {
        lot_ids.push(
            identity_planner
                .plan(
                    destination,
                    &entry.profile,
                    storage_history,
                    current_tick,
                    preservation_multiplier_ppm,
                    *merge_policy,
                )
                .ok_or(MaterialIngressError::LotIdExhausted)?,
        );
    }
    Ok(IngressIdentityPlan {
        lot_ids,
        merge_policies,
        excluded_existing,
        next_lot_id: identity_planner.next_lot_id(),
    })
}
