//! Deterministic lot selection and revision-bound material-consumption reservations.

use crate::core::quantity::Mass;
use crate::material::MaterialInputSpec;

use super::state::{
    ConsumedMaterialTrace, LotSlice, MaterialLotId, StockpileId, checked_consumed_material_mass,
};

mod explicit;
mod implicit;
mod integrity;
mod reservation;

pub(crate) use explicit::{
    ExplicitConsumptionSelectionError, validate_explicit_consumption_selection,
};
pub(crate) use implicit::{ConsumptionSelectionError, validate_consumption_selection};
pub(in crate::inventory) use integrity::{
    assert_consumption_parts_match_state, assert_consumption_parts_match_state_iter,
    assert_consumption_parts_well_formed,
};
#[cfg(test)]
pub(crate) use reservation::apply_consumption_reservation;
pub(crate) use reservation::{
    ConsumptionReservation, ReservationError, apply_prechecked_consumption_reservation,
    validate_consumption_reservation_from_selection,
};

/// Explicit runtime selection of conserved matter from one homogeneous lot.
///
/// Physical operation resolvers use these selections when input quantity and material identity are
/// properties of the chosen batch rather than static recipe requirements.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MaterialLotSelection {
    lot: MaterialLotId,
    mass: Mass,
}

impl MaterialLotSelection {
    #[must_use]
    pub const fn new(lot: MaterialLotId, mass: Mass) -> Self {
        Self { lot, mass }
    }

    #[must_use]
    pub const fn lot(self) -> MaterialLotId {
        self.lot
    }

    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
    }
}

/// Deterministic read-only material selection for physical process resolution.
///
/// The selection owns the exact lot slices and physical/provenance traces chosen from one
/// inventory revision. A later reservation consumes this same selection rather than selecting
/// equivalent-looking matter a second time after a resolver has already calculated an outcome.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConsumptionSelection {
    pub(super) expected_revision: u64,
    pub(super) source: StockpileId,
    pub(super) inputs: Vec<MaterialInputSpec>,
    pub(super) lot_slices: Vec<LotSlice>,
    pub(super) consumed_inputs: Vec<ConsumedMaterialTrace>,
}

impl ConsumptionSelection {
    pub(crate) const fn source(&self) -> StockpileId {
        self.source
    }

    pub(crate) fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        &self.consumed_inputs
    }

    /// Iterates already-validated lot slices in the selection owner's deterministic order.
    pub(crate) fn lot_selections(
        &self,
    ) -> impl ExactSizeIterator<Item = MaterialLotSelection> + '_ {
        self.lot_slices
            .iter()
            .map(|slice| MaterialLotSelection::new(slice.lot, slice.mass))
    }

    pub(crate) fn total_consumed(&self) -> Mass {
        checked_consumed_material_mass(&self.consumed_inputs)
            .unwrap_or_else(|| panic!("validated consumption selection mass overflowed"))
    }

    pub(crate) fn selected_mass_for_lot(&self, lot: MaterialLotId) -> Mass {
        self.lot_slices
            .iter()
            .filter(|slice| slice.lot == lot)
            .fold(Mass::ZERO, |total, slice| {
                total.checked_add(slice.mass).unwrap_or_else(|| {
                    panic!(
                        "validated consumption selection overflowed selected mass for lot {}",
                        lot.value()
                    )
                })
            })
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
