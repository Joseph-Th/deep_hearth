//! Allocates and commits inventory-owned reserved output matter.

use std::collections::BTreeMap;

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

/// Inventory-owned receipt for one reserved deposit request after authoritative admission.
///
/// `lot_ids` preserves one contribution-to-surviving-lot identity per requested output parcel.
/// Compatible parcels may therefore name an already-existing lot, and multiple parcels may name the
/// same surviving identity when coalescing is canonical.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReservedDepositReceipt {
    destination: StockpileId,
    lot_ids: Vec<MaterialLotId>,
}

impl ReservedDepositReceipt {
    pub(crate) const fn destination(&self) -> StockpileId {
        self.destination
    }

    #[cfg(test)]
    pub(crate) fn lot_ids(&self) -> &[MaterialLotId] {
        &self.lot_ids
    }

    pub(crate) fn into_lot_ids(self) -> Vec<MaterialLotId> {
        self.lot_ids
    }
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
        assert!(
            !outputs.is_empty(),
            "reserved deposit request must own at least one material output"
        );
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
#[derive(Clone, Debug, PartialEq, Eq)]
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

    /// Fails closed if an internally produced deposit plan no longer has one identity and merge
    /// policy for every material output. Cross-owner transactions call this before any mutation.
    pub(crate) fn assert_well_formed(&self) {
        assert!(
            self.provenance_created_at <= self.admitted_at,
            "reserved deposit provenance cannot postdate inventory admission"
        );
        if self.entries.is_empty() {
            assert_eq!(
                self.next_revision, self.expected_revision,
                "empty reserved deposit plan cannot advance inventory revision"
            );
            return;
        }
        assert_eq!(
            self.expected_revision.checked_add(1),
            Some(self.next_revision),
            "nonempty reserved deposit plan must advance inventory revision exactly once"
        );
        for entry in &self.entries {
            assert!(
                !entry.outputs.is_empty(),
                "reserved deposit plan entry must own at least one material output"
            );
            assert_eq!(
                entry.outputs.len(),
                entry.lot_ids.len(),
                "reserved deposit plan must bind one material lot identity per output"
            );
            assert_eq!(
                entry.outputs.len(),
                entry.merge_policies.len(),
                "reserved deposit plan must bind one merge policy per output"
            );
        }
    }

    /// Replays reserved-output identity allocation and reserved-mass ownership against state.
    pub(crate) fn assert_matches_state(&self, state: &InventoryState) {
        self.assert_well_formed();
        assert_eq!(
            state.revision(),
            self.expected_revision,
            "reserved deposit plan must match its planned inventory revision"
        );
        if self.entries.is_empty() {
            assert_eq!(
                self.next_lot_id,
                state.next_lot_id(),
                "empty reserved deposit plan cannot advance material lot identity"
            );
            return;
        }

        let mut reserved_by_destination = BTreeMap::<StockpileId, Mass>::new();
        for entry in &self.entries {
            let entry_mass = entry.outputs.iter().fold(Mass::ZERO, |total, output| {
                total
                    .checked_add(output.mass())
                    .unwrap_or_else(|| panic!("reserved deposit output mass overflowed"))
            });
            let current = reserved_by_destination
                .get(&entry.destination)
                .copied()
                .unwrap_or(Mass::ZERO);
            let combined = current
                .checked_add(entry_mass)
                .unwrap_or_else(|| panic!("reserved deposit destination mass overflowed"));
            reserved_by_destination.insert(entry.destination, combined);
        }
        for (destination, planned_mass) in reserved_by_destination {
            let destination_record = state.get_stockpile(destination).unwrap_or_else(|| {
                panic!(
                    "reserved deposit destination {} disappeared before commit",
                    destination.value()
                )
            });
            assert!(
                destination_record.reserved_inbound() >= planned_mass,
                "reserved deposit plan exceeds destination reserved inbound mass"
            );
        }

        let mut identity_planner = LotIdentityPlanner::new(state, std::iter::empty());
        for entry in &self.entries {
            let destination_record = state.get_stockpile(entry.destination).unwrap_or_else(|| {
                panic!(
                    "reserved deposit destination {} disappeared before commit",
                    entry.destination.value()
                )
            });
            let preservation_multiplier_ppm = destination_record
                .storage_profile()
                .preservation_multiplier_ppm();
            let storage_history = MaterialStorageHistory::with_ambient_age_parts(
                entry.storage_age_parts,
                self.admitted_at,
            );
            for ((output, merge_policy), planned_lot) in entry
                .outputs
                .iter()
                .zip(&entry.merge_policies)
                .zip(&entry.lot_ids)
            {
                let profile = MaterialLotProfile {
                    commodity: output.commodity(),
                    temperature: output.temperature(),
                    composition: output.composition().clone(),
                    particle_size: output.particle_size_distribution().cloned(),
                };
                let replayed = identity_planner
                    .plan(
                        entry.destination,
                        &profile,
                        storage_history,
                        self.admitted_at,
                        preservation_multiplier_ppm,
                        *merge_policy,
                    )
                    .unwrap_or_else(|| {
                        panic!("reserved deposit identity replay exhausted lot IDs")
                    });
                assert_eq!(
                    replayed, *planned_lot,
                    "reserved deposit lot identity changed before commit"
                );
            }
        }
        assert_eq!(
            identity_planner.next_lot_id(),
            self.next_lot_id,
            "reserved deposit lot cursor changed before commit"
        );
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
///
/// Returns one merge-aware landing receipt per reserved request in request order.
pub(crate) fn apply_reserved_deposits(
    state: &mut InventoryState,
    plan: ReservedDepositPlan,
) -> Vec<ReservedDepositReceipt> {
    plan.assert_matches_state(state);
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
        assert_eq!(
            next_lot_id,
            state.next_lot_id(),
            "empty reserved deposit plan cannot advance material lot identity"
        );
        return Vec::new();
    }

    let mut receipts = Vec::with_capacity(entries.len());
    for entry in entries {
        let ReservedDepositPlanEntry {
            destination,
            outputs,
            lot_ids,
            merge_policies,
            storage_age_parts,
        } = entry;
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

        let mut resulting_lots = Vec::with_capacity(outputs.len());
        for ((output, lot_id), merge_policy) in outputs.into_iter().zip(lot_ids).zip(merge_policies)
        {
            let resulting = apply_insert_or_merge_new_lot(
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
            resulting_lots.push(resulting);
        }
        receipts.push(ReservedDepositReceipt {
            destination,
            lot_ids: resulting_lots,
        });
    }
    state.apply_lot_cursor_and_revision(next_lot_id, next_revision);
    receipts
}

#[cfg(test)]
#[path = "reserved_ingress_tests.rs"]
mod tests;
