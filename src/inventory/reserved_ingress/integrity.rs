//! Release-mode integrity replay for reserved material ingress plans.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::inventory::lot_identity::LotIdentityPlanner;
use crate::inventory::state::{
    InventoryState, MaterialLotProfile, MaterialStorageHistory, StockpileId,
};

use super::ReservedDepositPlan;

impl ReservedDepositPlan {
    fn planned_mass_by_destination(&self) -> BTreeMap<StockpileId, Mass> {
        let mut planned = BTreeMap::<StockpileId, Mass>::new();
        for entry in &self.entries {
            let entry_mass = entry.outputs.iter().fold(Mass::ZERO, |total, output| {
                total
                    .checked_add(output.mass())
                    .unwrap_or_else(|| panic!("reserved deposit output mass overflowed"))
            });
            let current = planned
                .get(&entry.destination)
                .copied()
                .unwrap_or(Mass::ZERO);
            let combined = current
                .checked_add(entry_mass)
                .unwrap_or_else(|| panic!("reserved deposit destination mass overflowed"));
            planned.insert(entry.destination, combined);
        }
        planned
    }

    /// Returns authoritative post-deposit stored mass for every destination touched by this plan.
    ///
    /// Reserved deposit plans already own exact destination and mass. Cross-owner structural
    /// planning consumes this projection instead of maintaining parallel deposit-mass ledgers.
    pub(crate) fn stored_mass_after_by_destination(
        &self,
        state: &InventoryState,
    ) -> BTreeMap<StockpileId, Mass> {
        assert_eq!(
            state.revision(),
            self.expected_revision,
            "reserved deposit projection must use its planned inventory revision"
        );
        self.planned_mass_by_destination()
            .into_iter()
            .map(|(destination, deposited)| {
                let record = state.get_stockpile(destination).unwrap_or_else(|| {
                    panic!(
                        "reserved deposit destination {} disappeared before projection",
                        destination.value()
                    )
                });
                let stored_after =
                    record
                        .stored_mass()
                        .checked_add(deposited)
                        .unwrap_or_else(|| {
                            panic!(
                                "validated reserved deposit overflows destination {} stored mass",
                                destination.value()
                            )
                        });
                (destination, stored_after)
            })
            .collect()
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

        for (destination, planned_mass) in self.planned_mass_by_destination() {
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
