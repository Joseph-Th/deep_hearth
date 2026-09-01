//! Release-mode integrity replay for validated inventory consumption plans.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::material::{CommodityKey, MaterialInputSpec};

use super::reservation::ConsumptionReservation;
use crate::inventory::state::{
    ConsumedMaterialTrace, InventoryState, LotSlice, MaterialLotId, StockpileId,
    checked_consumed_material_mass,
};

impl ConsumptionReservation {
    /// Fails closed if aggregate input accounting, lot slices, and physical traces diverge inside
    /// a reservation. Cross-owner commits call this before mutating structure or another owner.
    pub(crate) fn assert_well_formed(&self) {
        assert_consumption_parts_well_formed(&self.inputs, &self.lot_slices, &self.consumed_inputs);
        assert_eq!(
            self.expected_revision.checked_add(1),
            Some(self.next_revision),
            "consumption reservation must advance inventory revision exactly once"
        );
    }

    /// Replays the reservation's exact lot/traces against the inventory snapshot it was bound to.
    pub(crate) fn assert_matches_state(&self, state: &InventoryState) {
        self.assert_well_formed();
        assert_eq!(
            state.revision(),
            self.expected_revision,
            "consumption reservation must match its validated inventory revision"
        );
        assert_consumption_parts_match_state(
            state,
            self.source,
            &self.inputs,
            &self.lot_slices,
            &self.consumed_inputs,
        );

        let total_consumed = checked_consumed_material_mass(&self.consumed_inputs)
            .unwrap_or_else(|| panic!("validated consumption reservation mass overflowed"));
        for (destination, inbound_mass) in &self.inbound_by_destination {
            let destination_record = state.get_stockpile(*destination).unwrap_or_else(|| {
                panic!(
                    "validated consumption reservation destination {} disappeared",
                    destination.value()
                )
            });
            let outgoing = if self.source == *destination {
                total_consumed
            } else {
                Mass::ZERO
            };
            let projection = destination_record
                .project_mass_exchange(outgoing, *inbound_mass)
                .unwrap_or_else(|| {
                    panic!(
                        "validated consumption reservation mass projection failed for stockpile {}",
                        destination.value()
                    )
                });
            assert!(
                projection.after_incoming <= destination_record.capacity(),
                "validated consumption reservation exceeds destination capacity"
            );
        }
    }
}

pub(in crate::inventory) fn assert_consumption_parts_well_formed(
    inputs: &[MaterialInputSpec],
    lot_slices: &[LotSlice],
    consumed_inputs: &[ConsumedMaterialTrace],
) {
    assert_eq!(
        lot_slices.len(),
        consumed_inputs.len(),
        "consumption plan must retain one physical trace per selected lot slice"
    );
    for (slice, trace) in lot_slices.iter().zip(consumed_inputs) {
        assert_eq!(
            slice.mass,
            trace.mass(),
            "consumption plan lot-slice mass must match its physical trace"
        );
    }

    let mut expected = BTreeMap::<CommodityKey, Mass>::new();
    for input in inputs {
        let current = expected
            .get(&input.commodity())
            .copied()
            .unwrap_or(Mass::ZERO);
        let next = current
            .checked_add(input.mass())
            .unwrap_or_else(|| panic!("consumption plan aggregate input mass overflowed"));
        expected.insert(input.commodity(), next);
    }
    let mut traced = BTreeMap::<CommodityKey, Mass>::new();
    for trace in consumed_inputs {
        let commodity = trace.profile().commodity();
        let current = traced.get(&commodity).copied().unwrap_or(Mass::ZERO);
        let next = current
            .checked_add(trace.mass())
            .unwrap_or_else(|| panic!("consumption plan traced material mass overflowed"));
        traced.insert(commodity, next);
    }
    assert_eq!(
        expected, traced,
        "consumption plan aggregate inputs must equal traced material by commodity"
    );
}

pub(in crate::inventory) fn assert_consumption_parts_match_state(
    state: &InventoryState,
    source: StockpileId,
    inputs: &[MaterialInputSpec],
    lot_slices: &[LotSlice],
    consumed_inputs: &[ConsumedMaterialTrace],
) {
    assert_consumption_parts_well_formed(inputs, lot_slices, consumed_inputs);
    let source_record = state
        .get_stockpile(source)
        .unwrap_or_else(|| panic!("consumption plan source stockpile disappeared"));
    let mut requested_by_commodity = BTreeMap::<CommodityKey, Mass>::new();
    let mut requested_total = Mass::ZERO;
    for input in inputs {
        let current = requested_by_commodity
            .get(&input.commodity())
            .copied()
            .unwrap_or(Mass::ZERO);
        let requested = current
            .checked_add(input.mass())
            .unwrap_or_else(|| panic!("consumption plan source commodity mass overflowed"));
        requested_by_commodity.insert(input.commodity(), requested);
        requested_total = requested_total
            .checked_add(input.mass())
            .unwrap_or_else(|| panic!("consumption plan source stored mass overflowed"));
    }
    for (commodity, requested) in requested_by_commodity {
        assert!(
            source_record.get_mass(commodity) >= requested,
            "consumption plan exceeds source aggregate commodity mass"
        );
    }
    assert!(
        source_record.stored_mass() >= requested_total,
        "consumption plan exceeds source aggregate stored mass"
    );
    let mut selected_by_lot = BTreeMap::<MaterialLotId, Mass>::new();
    for (slice, trace) in lot_slices.iter().zip(consumed_inputs) {
        let lot = state.get_lot(slice.lot).unwrap_or_else(|| {
            panic!(
                "consumption plan references missing material lot {}",
                slice.lot.value()
            )
        });
        assert_eq!(
            lot.stockpile(),
            source,
            "consumption plan lot owner changed before commit"
        );
        assert!(
            state
                .lot_ids_for_commodity(source, lot.commodity())
                .any(|indexed| indexed == slice.lot),
            "consumption plan source lot disappeared from its commodity index"
        );
        assert_eq!(
            &lot.profile,
            trace.profile(),
            "consumption plan physical trace no longer matches its selected lot"
        );
        assert_eq!(
            lot.provenance,
            trace.provenance(),
            "consumption plan provenance no longer matches its selected lot"
        );
        let selected = selected_by_lot
            .get(&slice.lot)
            .copied()
            .unwrap_or(Mass::ZERO)
            .checked_add(slice.mass)
            .unwrap_or_else(|| panic!("consumption plan selected lot mass overflowed"));
        assert!(
            selected <= lot.mass(),
            "consumption plan selects more mass than its source lot contains"
        );
        selected_by_lot.insert(slice.lot, selected);
    }
}
