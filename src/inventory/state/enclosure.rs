//! Persistent storage-enclosure profile transitions and preservation-history checkpoints.

use crate::core::time::SimulationTick;
use crate::inventory::StorageDefinitionId;

use super::{InventoryState, StockpileEnclosureRecord, StockpileId, StockpileStorageProfile};

impl InventoryState {
    pub(in crate::inventory) fn apply_storage_enclosure_removal(
        &mut self,
        stockpile: StockpileId,
        expected_profile: StockpileStorageProfile,
        next_profile: StockpileStorageProfile,
        expected_definition: StorageDefinitionId,
        at: SimulationTick,
        next_revision: u64,
    ) {
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "validated storage dismantling must advance inventory revision exactly once after recovered material ingress"
        );
        self.transition_stockpile_preservation(
            stockpile,
            expected_profile.preservation_multiplier_ppm(),
            next_profile.preservation_multiplier_ppm(),
            at,
        );
        let record = self.stockpiles.get_mut(&stockpile).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: stockpile {} disappeared during enclosure dismantling",
                stockpile.value()
            )
        });
        assert_eq!(
            record.storage_profile, expected_profile,
            "validated storage dismantling target profile changed before apply"
        );
        assert_eq!(
            record
                .enclosure
                .as_ref()
                .map(StockpileEnclosureRecord::definition),
            Some(expected_definition),
            "validated storage dismantling target enclosure changed before apply"
        );
        record.storage_profile = next_profile;
        record.enclosure = None;
        self.revision = next_revision;
    }

    pub(in crate::inventory) fn apply_storage_enclosure(
        &mut self,
        stockpile: StockpileId,
        expected_profile: StockpileStorageProfile,
        next_profile: StockpileStorageProfile,
        enclosure: StockpileEnclosureRecord,
        at: SimulationTick,
        next_revision: u64,
    ) {
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "validated storage construction must advance inventory revision exactly once after material egress"
        );
        self.transition_stockpile_preservation(
            stockpile,
            expected_profile.preservation_multiplier_ppm(),
            next_profile.preservation_multiplier_ppm(),
            at,
        );
        let record = self.stockpiles.get_mut(&stockpile).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: stockpile {} disappeared during enclosure construction",
                stockpile.value()
            )
        });
        assert_eq!(
            record.storage_profile, expected_profile,
            "validated storage construction target profile changed before apply"
        );
        assert!(
            record.enclosure.is_none(),
            "validated storage construction target unexpectedly gained an enclosure"
        );
        record.storage_profile = next_profile;
        record.enclosure = Some(enclosure);
        self.revision = next_revision;
    }

    fn transition_stockpile_preservation(
        &mut self,
        stockpile: StockpileId,
        source_preservation_multiplier_ppm: u32,
        destination_preservation_multiplier_ppm: u32,
        at: SimulationTick,
    ) {
        let Some(index) = self.lot_indexes.get(&stockpile) else {
            let record = self.stockpiles.get(&stockpile).unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: stockpile {} disappeared during storage profile transition",
                    stockpile.value()
                )
            });
            assert!(
                record.stored_mass().is_zero(),
                "runtime invariant broken: nonempty stockpile is missing its lot index"
            );
            return;
        };
        for lot_id in index.lot_ids() {
            let lot = self.lots.get_mut(&lot_id).unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: stockpile {} lot index references missing lot {}",
                    stockpile.value(),
                    lot_id.value()
                )
            });
            assert_eq!(
                lot.stockpile, stockpile,
                "runtime invariant broken: stockpile lot index references a lot owned elsewhere"
            );
            lot.storage_history = lot
                .storage_history
                .transition_preservation(
                    at,
                    source_preservation_multiplier_ppm,
                    destination_preservation_multiplier_ppm,
                )
                .unwrap_or_else(|| {
                    panic!("validated storage profile transition overflowed lot age")
                });
        }
    }
}
