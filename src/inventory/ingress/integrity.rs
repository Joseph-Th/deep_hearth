//! Release-mode integrity replay for validated material ingress plans.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::inventory::state::InventoryState;

use super::{
    IngressMassSummary, MaterialIngressEntry, ValidatedMaterialIngress,
    replay_ingress_identity_plan, validate_ingress_capacity,
};

impl ValidatedMaterialIngress {
    /// Fails closed if an internally produced ingress token no longer binds exactly one identity
    /// and merge policy to every parcel. Cross-owner commits call this before any mutation.
    pub(crate) fn assert_well_formed(&self) {
        assert!(
            !self.entries.is_empty(),
            "validated material ingress must own at least one parcel"
        );
        assert_eq!(
            self.expected_revision.checked_add(1),
            Some(self.next_revision),
            "validated material ingress must advance inventory revision exactly once"
        );
        assert_eq!(
            self.entries.len(),
            self.lot_ids.len(),
            "validated material ingress must bind one lot identity per parcel"
        );
        assert_eq!(
            self.entries.len(),
            self.merge_policies.len(),
            "validated material ingress must bind one lot merge policy per parcel"
        );
    }

    /// Replays lot identity allocation against the inventory snapshot without mutating it.
    pub(crate) fn assert_identity_plan_matches_state(&self, state: &InventoryState) {
        self.assert_well_formed();
        let destination_record = state.get_stockpile(self.destination).unwrap_or_else(|| {
            panic!(
                "validated material ingress destination {} disappeared",
                self.destination.value()
            )
        });
        let replayed = replay_ingress_identity_plan(
            state,
            destination_record,
            self.destination,
            &self.entries,
            self.current_tick,
            self.excluded_existing.clone(),
            self.merge_policies.clone(),
        )
        .unwrap_or_else(|_| panic!("validated material ingress identity replay exhausted lot IDs"));
        assert_eq!(
            replayed.lot_ids, self.lot_ids,
            "validated material ingress lot identities changed before commit"
        );
        assert_eq!(
            replayed.next_lot_id, self.next_lot_id,
            "validated material ingress lot cursor changed before commit"
        );
    }

    pub(crate) fn assert_matches_state(&self, state: &InventoryState) {
        assert_eq!(
            state.revision(),
            self.expected_revision,
            "validated material ingress must match its inventory revision"
        );
        let destination_record = state.get_stockpile(self.destination).unwrap_or_else(|| {
            panic!(
                "validated material ingress destination {} disappeared",
                self.destination.value()
            )
        });
        let summary = summarize_planned_ingress_mass(&self.entries, self.current_tick);
        validate_ingress_capacity(destination_record, self.destination, &summary).unwrap_or_else(
            |error| panic!("validated material ingress capacity changed: {error:?}"),
        );
        self.assert_identity_plan_matches_state(state);
    }
}

pub(super) fn summarize_planned_ingress_mass(
    entries: &[MaterialIngressEntry],
    current_tick: SimulationTick,
) -> IngressMassSummary {
    let mut total = Mass::ZERO;
    let mut by_commodity = BTreeMap::new();
    for entry in entries {
        assert!(
            !entry.mass.is_zero(),
            "validated material ingress cannot contain zero mass"
        );
        assert!(
            entry.profile.composition().validate().is_ok(),
            "validated material ingress composition became invalid"
        );
        assert!(
            entry
                .profile
                .composition()
                .parts_per_million(entry.profile.commodity().material())
                != 0,
            "validated material ingress composition lost its commodity host"
        );
        assert!(
            entry.provenance.earliest_created_at() <= entry.provenance.latest_created_at(),
            "validated material ingress provenance became invalid"
        );
        assert!(
            entry.provenance.latest_created_at() <= current_tick,
            "validated material ingress provenance moved into the future"
        );
        total = total
            .checked_add(entry.mass)
            .unwrap_or_else(|| panic!("validated material ingress mass overflowed"));
        let commodity = entry.profile.commodity();
        let current = by_commodity.get(&commodity).copied().unwrap_or(Mass::ZERO);
        let combined = current
            .checked_add(entry.mass)
            .unwrap_or_else(|| panic!("validated material ingress commodity mass overflowed"));
        by_commodity.insert(commodity, combined);
    }
    IngressMassSummary {
        total,
        by_commodity,
    }
}
