//! Exact inventory egress tokens for transferring selected matter into another authoritative owner.

use crate::core::quantity::Mass;
use crate::material::MaterialInputSpec;

use super::super::selection::ConsumptionSelection;
use super::super::state::{
    ConsumedMaterialTrace, InventoryState, LotSlice, StockpileId, apply_aggregate_withdraw,
    apply_consume_lot_slice, checked_consumed_material_mass,
};

/// Revision-bound withdrawal of exact material slices into another authoritative owner.
///
/// The destination owner is deliberately absent. This token proves only that the selected matter
/// can leave inventory exactly once; the cross-subsystem transaction that holds it is responsible
/// for establishing the new owner before exposing a successful commit.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedMaterialEgress {
    expected_revision: u64,
    next_revision: u64,
    source: StockpileId,
    inputs: Vec<MaterialInputSpec>,
    lot_slices: Vec<LotSlice>,
    consumed_inputs: Vec<ConsumedMaterialTrace>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaterialEgressError {
    StaleSelection { expected: u64, actual: u64 },
    RevisionExhausted,
}

impl ValidatedMaterialEgress {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    pub(in crate::inventory) const fn next_revision(&self) -> u64 {
        self.next_revision
    }

    pub(in crate::inventory) const fn source(&self) -> StockpileId {
        self.source
    }

    pub(in crate::inventory) fn inputs(&self) -> &[MaterialInputSpec] {
        &self.inputs
    }

    pub(in crate::inventory) fn lot_slices(&self) -> &[LotSlice] {
        &self.lot_slices
    }

    pub(crate) fn total_consumed(&self) -> Mass {
        checked_consumed_material_mass(&self.consumed_inputs)
            .unwrap_or_else(|| panic!("validated material egress mass overflowed"))
    }

    pub(crate) fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        &self.consumed_inputs
    }
}

/// Converts an exact read-only selection into a one-shot inventory withdrawal for another owner.
pub(crate) fn validate_material_egress_from_selection(
    state: &InventoryState,
    selection: ConsumptionSelection,
) -> Result<ValidatedMaterialEgress, MaterialEgressError> {
    let ConsumptionSelection {
        expected_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs,
    } = selection;
    if state.revision() != expected_revision {
        return Err(MaterialEgressError::StaleSelection {
            expected: expected_revision,
            actual: state.revision(),
        });
    }
    let Some(next_revision) = state.revision().checked_add(1) else {
        return Err(MaterialEgressError::RevisionExhausted);
    };
    Ok(ValidatedMaterialEgress {
        expected_revision,
        next_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs,
    })
}

/// Applies exact validated withdrawal after a cross-owner transaction has prechecked all owners.
pub(crate) fn apply_material_egress(state: &mut InventoryState, egress: ValidatedMaterialEgress) {
    let ValidatedMaterialEgress {
        expected_revision,
        next_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs: _,
    } = egress;
    assert_eq!(
        state.revision(),
        expected_revision,
        "material egress commit requires its validated inventory revision"
    );
    for input in &inputs {
        apply_aggregate_withdraw(state, source, input.commodity(), input.mass());
    }
    for slice in lot_slices {
        apply_consume_lot_slice(state, slice);
    }
    state.apply_revision(next_revision);
}
