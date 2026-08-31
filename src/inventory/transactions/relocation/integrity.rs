//! Release-mode integrity replay for validated material relocation plans.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::lot_identity::LotIdentityPlanner;

use super::ValidatedMaterialRelocation;

impl ValidatedMaterialRelocation {
    pub(super) fn assert_matches_state(&self, state: &AppState) {
        let inventories = state.inventory();
        assert_eq!(
            self.expected_revision.checked_add(1),
            Some(self.next_revision),
            "validated material relocation must advance inventory revision exactly once"
        );
        let source_record = inventories
            .get_stockpile(self.source)
            .unwrap_or_else(|| panic!("validated material relocation source disappeared"));
        let destination_record = inventories
            .get_stockpile(self.destination)
            .unwrap_or_else(|| panic!("validated material relocation destination disappeared"));
        assert_ne!(
            self.source, self.destination,
            "validated material relocation requires distinct stockpiles"
        );

        let mut selected_by_commodity = BTreeMap::new();
        for transfer in &self.transfers {
            let lot = inventories.get_lot(transfer.slice.lot).unwrap_or_else(|| {
                panic!(
                    "validated material relocation references missing lot {}",
                    transfer.slice.lot.value()
                )
            });
            assert_eq!(
                lot.stockpile(),
                self.source,
                "validated material relocation source lot owner changed before commit"
            );
            assert!(
                inventories
                    .lot_ids_for_commodity(self.source, lot.commodity())
                    .any(|indexed| indexed == transfer.slice.lot),
                "validated material relocation source lot disappeared from its commodity index"
            );
            assert!(
                transfer.slice.mass <= lot.mass(),
                "validated material relocation exceeds source lot mass"
            );
            let commodity = lot.commodity();
            let selected = selected_by_commodity
                .get(&commodity)
                .copied()
                .unwrap_or(Mass::ZERO)
                .checked_add(transfer.slice.mass)
                .unwrap_or_else(|| panic!("validated relocation selected mass overflowed"));
            selected_by_commodity.insert(commodity, selected);
        }
        let mut input_by_commodity = BTreeMap::new();
        for input in &self.inputs {
            let current = input_by_commodity
                .get(&input.commodity())
                .copied()
                .unwrap_or(Mass::ZERO);
            let total = current
                .checked_add(input.mass())
                .unwrap_or_else(|| panic!("validated relocation input mass overflowed"));
            input_by_commodity.insert(input.commodity(), total);
        }
        assert_eq!(
            selected_by_commodity, input_by_commodity,
            "validated material relocation lot transfers must equal aggregate inputs"
        );
        for (commodity, requested) in &input_by_commodity {
            assert!(
                source_record.get_mass(*commodity) >= *requested,
                "validated material relocation exceeds source aggregate commodity mass"
            );
            destination_record
                .get_mass(*commodity)
                .checked_add(*requested)
                .unwrap_or_else(|| {
                    panic!("validated relocation destination commodity mass overflowed")
                });
        }

        let total_mass = self.inputs.iter().fold(Mass::ZERO, |total, input| {
            total
                .checked_add(input.mass())
                .unwrap_or_else(|| panic!("validated material relocation mass overflowed"))
        });
        assert!(
            source_record.stored_mass() >= total_mass,
            "validated material relocation exceeds source stored mass"
        );
        let destination_committed = destination_record
            .stored_mass()
            .checked_add(destination_record.reserved_inbound())
            .unwrap_or_else(|| panic!("validated relocation destination mass overflowed"));
        let destination_after = destination_committed
            .checked_add(total_mass)
            .unwrap_or_else(|| panic!("validated relocation destination mass overflowed"));
        assert!(
            destination_after <= destination_record.capacity(),
            "validated material relocation exceeds destination capacity"
        );

        let source_preservation_multiplier_ppm = source_record
            .storage_profile()
            .preservation_multiplier_ppm();
        let destination_preservation_multiplier_ppm = destination_record
            .storage_profile()
            .preservation_multiplier_ppm();
        let mut identity_planner = LotIdentityPlanner::new(inventories, std::iter::empty());
        for transfer in self
            .transfers
            .iter()
            .filter(|transfer| transfer.split_lot_id.is_none())
        {
            let lot = inventories
                .get_lot(transfer.slice.lot)
                .unwrap_or_else(|| unreachable!("relocation source lot was checked above"));
            assert_eq!(
                transfer.slice.mass,
                lot.mass(),
                "full relocation transfer must cover its complete source lot"
            );
            let storage_history = lot
                .storage_history()
                .transition_preservation(
                    state.tick(),
                    source_preservation_multiplier_ppm,
                    destination_preservation_multiplier_ppm,
                )
                .unwrap_or_else(|| panic!("validated full relocation storage history is invalid"));
            identity_planner.note_preserved_arrival(
                lot.id(),
                self.destination,
                &lot.profile,
                storage_history,
            );
        }
        for transfer in &self.transfers {
            let lot = inventories
                .get_lot(transfer.slice.lot)
                .unwrap_or_else(|| unreachable!("relocation source lot was checked above"));
            if transfer.slice.mass == lot.mass() {
                assert!(
                    transfer.split_lot_id.is_none(),
                    "full relocation transfer cannot allocate a split lot identity"
                );
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
                    panic!("validated partial relocation storage history is invalid")
                });
            let replayed = identity_planner
                .plan(
                    self.destination,
                    &lot.profile,
                    storage_history,
                    state.tick(),
                    destination_preservation_multiplier_ppm,
                    transfer.merge_policy,
                )
                .unwrap_or_else(|| {
                    panic!("validated relocation identity replay exhausted lot IDs")
                });
            assert_eq!(
                transfer.split_lot_id,
                Some(replayed),
                "validated relocation split identity changed before commit"
            );
        }
        let replayed_cursor = identity_planner
            .allocated_any()
            .then_some(identity_planner.next_lot_id());
        assert_eq!(
            replayed_cursor, self.next_lot_id_after,
            "validated relocation lot cursor changed before commit"
        );
    }
}
