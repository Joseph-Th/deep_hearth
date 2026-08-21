//! Runtime-only material-lot routing indexes derived from authoritative inventory records.

use std::collections::{BTreeMap, BTreeSet};

use crate::material::CommodityKey;

use super::MaterialLotId;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct StockpileLotIndex {
    all: BTreeSet<MaterialLotId>,
    by_commodity: BTreeMap<CommodityKey, BTreeSet<MaterialLotId>>,
}

impl StockpileLotIndex {
    pub(super) fn insert(&mut self, lot: MaterialLotId, commodity: CommodityKey) {
        assert!(
            !self.all.contains(&lot),
            "material lot index must not contain duplicate ids"
        );
        assert!(
            self.by_commodity
                .values()
                .all(|indexed| !indexed.contains(&lot)),
            "commodity lot index must not contain a lot under any commodity before insertion"
        );
        let inserted = self.all.insert(lot);
        assert!(inserted, "prechecked material lot index insertion failed");
        let inserted = self.by_commodity.entry(commodity).or_default().insert(lot);
        assert!(inserted, "prechecked commodity lot index insertion failed");
    }

    pub(super) fn remove(&mut self, lot: MaterialLotId, commodity: CommodityKey) {
        assert!(
            self.all.contains(&lot),
            "material lot index must contain removed ids"
        );
        assert!(
            self.by_commodity
                .get(&commodity)
                .is_some_and(|indexed| indexed.contains(&lot)),
            "commodity lot index must contain removed ids"
        );
        let removed = self.all.remove(&lot);
        assert!(removed, "prechecked material lot index removal failed");
        let remove_entry = {
            let indexed = match self.by_commodity.get_mut(&commodity) {
                Some(indexed) => indexed,
                None => unreachable!("commodity lot index presence was prechecked"),
            };
            let removed = indexed.remove(&lot);
            assert!(removed, "prechecked commodity lot index removal failed");
            indexed.is_empty()
        };
        if remove_entry {
            self.by_commodity.remove(&commodity);
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    pub(super) fn lot_ids(&self) -> impl Iterator<Item = MaterialLotId> + '_ {
        self.all.iter().copied()
    }

    pub(super) fn lot_ids_for_commodity(
        &self,
        commodity: CommodityKey,
    ) -> impl Iterator<Item = MaterialLotId> + '_ {
        self.by_commodity
            .get(&commodity)
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
    }
}
