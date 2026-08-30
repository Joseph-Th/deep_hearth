//! Allocates and commits inventory-owned reserved output matter.

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::material::MaterialLotSpec;
use crate::registry::Registries;

use super::coalescing::LotMergePolicy;
use super::lot_identity::LotIdentityPlanner;
use super::state::{
    InventoryState, MaterialLotId, MaterialLotProfile, MaterialLotProvenance, MaterialLotRecord,
    MaterialStorageHistory, StockpileId, apply_insert_or_merge_new_lot, get_stockpile_mut_or_panic,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReservedDepositPlanError {
    LotIdExhausted,
    RevisionExhausted,
}

/// One already-reserved output stream awaiting authoritative inventory ingress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReservedDepositRequest {
    destination: StockpileId,
    outputs: Vec<MaterialLotSpec>,
    storage_age_parts: u128,
}

impl ReservedDepositRequest {
    pub(crate) fn new(
        destination: StockpileId,
        outputs: Vec<MaterialLotSpec>,
        storage_age_parts: u128,
    ) -> Self {
        Self {
            destination,
            outputs,
            storage_age_parts,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReservedDepositPlanEntry {
    destination: StockpileId,
    outputs: Vec<MaterialLotSpec>,
    lot_ids: Vec<MaterialLotId>,
    merge_policies: Vec<LotMergePolicy>,
    storage_age_parts: u128,
}

/// Inventory-owned allocation and revision plan for already-reserved material outputs.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ReservedDepositPlan {
    expected_revision: u64,
    next_revision: u64,
    next_lot_id: u64,
    provenance_created_at: SimulationTick,
    admitted_at: SimulationTick,
    entries: Vec<ReservedDepositPlanEntry>,
}

impl ReservedDepositPlan {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }
}

/// Decides all material-lot identities and the single inventory revision used by one admission.
pub(crate) fn decide_reserved_deposits(
    registries: &Registries,
    state: &InventoryState,
    provenance_created_at: SimulationTick,
    admitted_at: SimulationTick,
    requests: Vec<ReservedDepositRequest>,
) -> Result<ReservedDepositPlan, ReservedDepositPlanError> {
    assert!(
        provenance_created_at <= admitted_at,
        "reserved output provenance cannot postdate inventory admission"
    );
    let expected_revision = state.revision();
    if requests.is_empty() {
        return Ok(ReservedDepositPlan {
            expected_revision,
            next_revision: expected_revision,
            next_lot_id: state.next_lot_id(),
            provenance_created_at,
            admitted_at,
            entries: Vec::new(),
        });
    }

    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(ReservedDepositPlanError::RevisionExhausted)?;
    let mut identity_planner = LotIdentityPlanner::new(state, std::iter::empty());
    let mut entries = Vec::with_capacity(requests.len());
    for request in requests {
        let ReservedDepositRequest {
            destination,
            outputs,
            storage_age_parts,
        } = request;
        let mut lot_ids = Vec::with_capacity(outputs.len());
        let merge_policies = outputs
            .iter()
            .map(|output| LotMergePolicy::for_commodity(registries, output.commodity()))
            .collect::<Vec<_>>();
        let preservation_multiplier_ppm = state
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("reserved output destination disappeared during planning"))
            .storage_profile()
            .preservation_multiplier_ppm();
        let storage_history =
            MaterialStorageHistory::with_ambient_age_parts(storage_age_parts, admitted_at);
        for (output, merge_policy) in outputs.iter().zip(&merge_policies) {
            let profile = MaterialLotProfile {
                commodity: output.commodity(),
                temperature: output.temperature(),
                composition: output.composition().clone(),
                particle_size: output.particle_size_distribution().cloned(),
            };
            lot_ids.push(
                identity_planner
                    .plan(
                        destination,
                        &profile,
                        storage_history,
                        admitted_at,
                        preservation_multiplier_ppm,
                        *merge_policy,
                    )
                    .ok_or(ReservedDepositPlanError::LotIdExhausted)?,
            );
        }
        entries.push(ReservedDepositPlanEntry {
            destination,
            outputs,
            lot_ids,
            merge_policies,
            storage_age_parts,
        });
    }

    Ok(ReservedDepositPlan {
        expected_revision,
        next_revision,
        next_lot_id: identity_planner.next_lot_id(),
        provenance_created_at,
        admitted_at,
        entries,
    })
}

/// Applies an inventory-owned reserved-output plan after its producing owner is prechecked.
pub(crate) fn apply_reserved_deposits(state: &mut InventoryState, plan: ReservedDepositPlan) {
    let ReservedDepositPlan {
        expected_revision,
        next_revision,
        next_lot_id,
        provenance_created_at,
        admitted_at,
        entries,
    } = plan;
    assert_eq!(
        state.revision(),
        expected_revision,
        "reserved deposit application requires its planned inventory revision"
    );
    if entries.is_empty() {
        debug_assert_eq!(next_revision, expected_revision);
        debug_assert_eq!(next_lot_id, state.next_lot_id());
        return;
    }

    for entry in entries {
        let ReservedDepositPlanEntry {
            destination,
            outputs,
            lot_ids,
            merge_policies,
            storage_age_parts,
        } = entry;
        debug_assert_eq!(outputs.len(), lot_ids.len());
        debug_assert_eq!(outputs.len(), merge_policies.len());
        let reserved_mass = outputs.iter().fold(Mass::ZERO, |total, output| {
            total.checked_add(output.mass()).unwrap_or_else(|| {
                panic!(
                    "validated reserved output mass overflowed for stockpile {}",
                    destination.value()
                )
            })
        });
        {
            let record = get_stockpile_mut_or_panic(state, destination);
            record.reserved_inbound = match record.reserved_inbound.checked_sub(reserved_mass) {
                Some(value) => value,
                None => panic!(
                    "reserved output mass underflow in stockpile {}",
                    destination.value()
                ),
            };
        }

        let preservation_multiplier_ppm = state
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("reserved output destination disappeared"))
            .storage_profile()
            .preservation_multiplier_ppm();
        let storage_history =
            MaterialStorageHistory::with_ambient_age_parts(storage_age_parts, admitted_at);

        for ((output, lot_id), merge_policy) in outputs.into_iter().zip(lot_ids).zip(merge_policies)
        {
            apply_insert_or_merge_new_lot(
                state,
                MaterialLotRecord {
                    id: lot_id,
                    stockpile: destination,
                    mass: output.mass(),
                    profile: MaterialLotProfile {
                        commodity: output.commodity(),
                        temperature: output.temperature(),
                        composition: output.composition().clone(),
                        particle_size: output.particle_size_distribution().cloned(),
                    },
                    provenance: MaterialLotProvenance {
                        earliest_created_at: provenance_created_at,
                        latest_created_at: provenance_created_at,
                    },
                    storage_history,
                },
                merge_policy,
                admitted_at,
                preservation_multiplier_ppm,
            );
        }
    }
    state.apply_lot_cursor_and_revision(next_lot_id, next_revision);
}

#[cfg(test)]
#[path = "reserved_ingress_tests.rs"]
mod tests;
