//! Release-mode integrity replay for validated material relocation plans.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::lot_identity::LotIdentityPlanner;
use crate::inventory::{InventoryState, StockpileRecord};
use crate::material::CommodityKey;

use super::ValidatedMaterialRelocation;

impl ValidatedMaterialRelocation {
    pub(super) fn assert_matches_state(&self, state: &AppState) {
        self.assert_revision_progression();
        let inventories = state.inventory();
        let (source_record, destination_record) = self.assert_endpoints(inventories);
        self.assert_material_plan(inventories, source_record, destination_record);
        self.assert_identity_plan(state, inventories, source_record, destination_record);
    }

    fn assert_revision_progression(&self) {
        assert_eq!(
            self.expected_revision.checked_add(1),
            Some(self.next_revision),
            "validated material relocation must advance inventory revision exactly once"
        );
    }

    fn assert_endpoints<'a>(
        &self,
        inventories: &'a InventoryState,
    ) -> (&'a StockpileRecord, &'a StockpileRecord) {
        assert_ne!(
            self.source, self.destination,
            "validated material relocation requires distinct stockpiles"
        );
        let source_record = inventories
            .get_stockpile(self.source)
            .unwrap_or_else(|| panic!("validated material relocation source disappeared"));
        let destination_record = inventories
            .get_stockpile(self.destination)
            .unwrap_or_else(|| panic!("validated material relocation destination disappeared"));
        (source_record, destination_record)
    }

    fn assert_material_plan(
        &self,
        inventories: &InventoryState,
        source_record: &StockpileRecord,
        destination_record: &StockpileRecord,
    ) {
        let selected_by_commodity = self.selected_mass_by_commodity(inventories);
        let (input_by_commodity, total_mass) = self.input_mass_summary();
        assert_eq!(
            selected_by_commodity, input_by_commodity,
            "validated material relocation lot transfers must equal aggregate inputs"
        );

        for (commodity, requested) in input_by_commodity {
            assert!(
                source_record.get_mass(commodity) >= requested,
                "validated material relocation exceeds source aggregate commodity mass"
            );
            destination_record
                .get_mass(commodity)
                .checked_add(requested)
                .unwrap_or_else(|| {
                    panic!("validated relocation destination commodity mass overflowed")
                });
        }
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
    }

    fn selected_mass_by_commodity(
        &self,
        inventories: &InventoryState,
    ) -> BTreeMap<CommodityKey, Mass> {
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
        selected_by_commodity
    }

    fn input_mass_summary(&self) -> (BTreeMap<CommodityKey, Mass>, Mass) {
        let mut by_commodity = BTreeMap::new();
        let mut total_mass = Mass::ZERO;
        for input in &self.inputs {
            let current = by_commodity
                .get(&input.commodity())
                .copied()
                .unwrap_or(Mass::ZERO);
            let commodity_total = current
                .checked_add(input.mass())
                .unwrap_or_else(|| panic!("validated relocation input mass overflowed"));
            by_commodity.insert(input.commodity(), commodity_total);
            total_mass = total_mass
                .checked_add(input.mass())
                .unwrap_or_else(|| panic!("validated material relocation mass overflowed"));
        }
        (by_commodity, total_mass)
    }

    fn assert_identity_plan(
        &self,
        state: &AppState,
        inventories: &InventoryState,
        source_record: &StockpileRecord,
        destination_record: &StockpileRecord,
    ) {
        let source_preservation_multiplier_ppm = source_record
            .storage_profile()
            .preservation_multiplier_ppm();
        let destination_preservation_multiplier_ppm = destination_record
            .storage_profile()
            .preservation_multiplier_ppm();
        let mut identity_planner = LotIdentityPlanner::new(inventories, std::iter::empty());

        self.note_full_lot_arrivals(
            state,
            inventories,
            &mut identity_planner,
            source_preservation_multiplier_ppm,
            destination_preservation_multiplier_ppm,
        );
        self.assert_partial_lot_identities(
            state,
            inventories,
            &mut identity_planner,
            source_preservation_multiplier_ppm,
            destination_preservation_multiplier_ppm,
        );

        let replayed_cursor = identity_planner
            .allocated_any()
            .then_some(identity_planner.next_lot_id());
        assert_eq!(
            replayed_cursor, self.next_lot_id_after,
            "validated relocation lot cursor changed before commit"
        );
    }

    fn note_full_lot_arrivals(
        &self,
        state: &AppState,
        inventories: &InventoryState,
        identity_planner: &mut LotIdentityPlanner<'_>,
        source_preservation_multiplier_ppm: u32,
        destination_preservation_multiplier_ppm: u32,
    ) {
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
    }

    fn assert_partial_lot_identities(
        &self,
        state: &AppState,
        inventories: &InventoryState,
        identity_planner: &mut LotIdentityPlanner<'_>,
        source_preservation_multiplier_ppm: u32,
        destination_preservation_multiplier_ppm: u32,
    ) {
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
    }
}
