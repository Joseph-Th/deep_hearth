//! Release-mode integrity replay for validated material reform plans.

use crate::core::state::AppState;
use crate::inventory::lot_identity::LotIdentityPlanner;
use crate::inventory::selection::assert_consumption_parts_match_state;
use crate::inventory::state::MaterialLotProfile;

use super::ValidatedMaterialReform;

impl ValidatedMaterialReform {
    pub(super) fn assert_matches_state(&self, state: &AppState) {
        let inventories = state.inventory();
        assert_eq!(
            self.expected_revision.checked_add(1),
            Some(self.next_revision),
            "validated material reform must advance inventory revision exactly once"
        );
        assert_eq!(
            self.lot_slices.len(),
            self.outputs.len(),
            "validated material reform must retain one conserved output per selected lot slice"
        );
        assert_eq!(
            self.outputs.len(),
            self.lot_ids.len(),
            "validated material reform must bind one lot identity per conserved output"
        );

        let consumed_inputs = self
            .outputs
            .iter()
            .map(|(trace, _)| trace.clone())
            .collect::<Vec<_>>();
        assert_consumption_parts_match_state(
            inventories,
            self.source,
            &self.source_inputs,
            &self.lot_slices,
            &consumed_inputs,
        );
        let source_record = inventories.get_stockpile(self.source).unwrap_or_else(|| {
            panic!("validated material reform source disappeared before commit")
        });
        let destination_record = inventories
            .get_stockpile(self.destination)
            .unwrap_or_else(|| {
                panic!("validated material reform destination disappeared before commit")
            });
        let source_preservation_multiplier_ppm = source_record
            .storage_profile()
            .preservation_multiplier_ppm();
        for (slice, (_, planned_history)) in self.lot_slices.iter().zip(&self.outputs) {
            let lot = inventories.get_lot(slice.lot).unwrap_or_else(|| {
                panic!(
                    "validated material reform references missing lot {}",
                    slice.lot.value()
                )
            });
            let replayed_history = lot
                .storage_history()
                .rebase(state.tick(), source_preservation_multiplier_ppm)
                .unwrap_or_else(|| {
                    panic!("validated material reform storage history no longer rebases")
                });
            assert_eq!(
                replayed_history, *planned_history,
                "validated material reform storage history changed before commit"
            );
        }

        let excluded_existing = self.lot_slices.iter().filter_map(|slice| {
            inventories
                .get_lot(slice.lot)
                .and_then(|lot| (slice.mass == lot.mass()).then_some(slice.lot))
        });
        let destination_preservation_multiplier_ppm = destination_record
            .storage_profile()
            .preservation_multiplier_ppm();
        let mut identity_planner = LotIdentityPlanner::new(inventories, excluded_existing);
        for ((trace, storage_history), planned_lot) in self.outputs.iter().zip(&self.lot_ids) {
            let mut profile: MaterialLotProfile = trace.profile().clone();
            profile.commodity = self.target;
            let replayed = identity_planner
                .plan(
                    self.destination,
                    &profile,
                    *storage_history,
                    state.tick(),
                    destination_preservation_multiplier_ppm,
                    self.merge_policy,
                )
                .unwrap_or_else(|| panic!("validated material reform identity replay exhausted"));
            assert_eq!(
                replayed, *planned_lot,
                "validated material reform output identity changed before commit"
            );
        }
        assert_eq!(
            identity_planner.next_lot_id(),
            self.next_lot_id,
            "validated material reform lot cursor changed before commit"
        );
    }
}
