//! Deterministic lot selection and revision-bound material-consumption reservations.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, MaterialInputSpec};

use super::state::{
    ConsumedMaterialTrace, InventoryState, LotSlice, MaterialLotId, MaterialLotRecord,
    MaterialStorageHistory, StockpileId, StockpileRecord, apply_aggregate_withdraw,
    apply_consume_lot_slice, checked_consumed_material_mass, get_stockpile_mut_or_panic,
};

mod integrity;

pub(in crate::inventory) use integrity::{
    assert_consumption_parts_match_state, assert_consumption_parts_well_formed,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConsumptionSelectionError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    InsufficientMass {
        stockpile: StockpileId,
        commodity: CommodityKey,
        available: Mass,
        requested: Mass,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExplicitConsumptionSelectionError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    EmptySelection,
    ZeroMass {
        lot: MaterialLotId,
    },
    DuplicateLot {
        lot: MaterialLotId,
    },
    UnknownLot {
        lot: MaterialLotId,
    },
    LotOwnedElsewhere {
        lot: MaterialLotId,
        requested_source: StockpileId,
        actual_source: StockpileId,
    },
    InsufficientLotMass {
        lot: MaterialLotId,
        available: Mass,
        requested: Mass,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReservationError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
    CapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed_after_consumption: Mass,
        requested_inbound: Mass,
    },
    RevisionExhausted,
    StaleSelection {
        expected: u64,
        actual: u64,
    },
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReservationCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ConsumptionReservation {
    expected_revision: u64,
    next_revision: u64,
    source: StockpileId,
    inputs: Vec<MaterialInputSpec>,
    lot_slices: Vec<LotSlice>,
    consumed_inputs: Vec<ConsumedMaterialTrace>,
    inbound_by_destination: BTreeMap<StockpileId, Mass>,
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

impl ConsumptionReservation {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    pub(crate) fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        &self.consumed_inputs
    }

    /// Captures the oldest ambient-equivalent storage exposure represented by this exact
    /// reservation at `at`. Production uses the conservative oldest cohort because generic
    /// process streams do not retain a one-to-one lineage from every selected input lot to every
    /// output lot.
    pub(crate) fn oldest_storage_history_at(
        &self,
        state: &InventoryState,
        at: SimulationTick,
    ) -> Option<MaterialStorageHistory> {
        assert_eq!(
            state.revision(),
            self.expected_revision,
            "storage exposure must be captured from the reservation's validated inventory revision"
        );
        let source = state.get_stockpile(self.source).unwrap_or_else(|| {
            panic!(
                "validated reservation source stockpile {} disappeared",
                self.source.value()
            )
        });
        let preservation_multiplier_ppm = source.storage_profile().preservation_multiplier_ppm();
        let mut oldest_ambient_age_parts = 0_u128;
        for slice in &self.lot_slices {
            let lot = state.get_lot(slice.lot).unwrap_or_else(|| {
                panic!(
                    "validated reservation references missing material lot {}",
                    slice.lot.value()
                )
            });
            let age = lot
                .storage_history()
                .project(at, preservation_multiplier_ppm)?;
            oldest_ambient_age_parts = oldest_ambient_age_parts.max(age);
        }
        Some(MaterialStorageHistory::with_ambient_age_parts(
            oldest_ambient_age_parts,
            at,
        ))
    }
}

pub(crate) fn validate_consumption_selection(
    state: &InventoryState,
    source: StockpileId,
    inputs: &[MaterialInputSpec],
) -> Result<ConsumptionSelection, ConsumptionSelectionError> {
    let Some(source_record) = state.get_stockpile(source) else {
        return Err(ConsumptionSelectionError::UnknownStockpile { stockpile: source });
    };

    let mut total_consumed = Mass::ZERO;
    let mut lot_slices = Vec::new();
    let mut selected_by_lot = BTreeMap::<MaterialLotId, Mass>::new();
    for input in inputs {
        let selected = select_input_lot_slices(state, source_record, input, &mut selected_by_lot)
            .map_err(|available| ConsumptionSelectionError::InsufficientMass {
            stockpile: source,
            commodity: input.commodity(),
            available,
            requested: input.mass(),
        })?;
        total_consumed = total_consumed
            .checked_add(input.mass())
            .ok_or(ConsumptionSelectionError::MassOverflow { stockpile: source })?;
        lot_slices.extend(selected);
    }

    let consumed_inputs = lot_slices
        .iter()
        .map(|slice| {
            let lot = match state.get_lot(slice.lot) {
                Some(lot) => lot,
                None => panic!(
                    "validated input slice references missing material lot {}",
                    slice.lot.value()
                ),
            };
            ConsumedMaterialTrace {
                mass: slice.mass,
                profile: lot.profile.clone(),
                provenance: lot.provenance,
            }
        })
        .collect();

    Ok(ConsumptionSelection {
        expected_revision: state.revision(),
        source,
        inputs: inputs.to_vec(),
        lot_slices,
        consumed_inputs,
    })
}

pub(crate) fn validate_explicit_consumption_selection(
    state: &InventoryState,
    source: StockpileId,
    selections: &[MaterialLotSelection],
) -> Result<ConsumptionSelection, ExplicitConsumptionSelectionError> {
    if state.get_stockpile(source).is_none() {
        return Err(ExplicitConsumptionSelectionError::UnknownStockpile { stockpile: source });
    }
    let ordered = order_explicit_selections(selections)?;

    let mut total_consumed = Mass::ZERO;
    let mut lot_slices = Vec::with_capacity(ordered.len());
    let mut consumed_inputs = Vec::with_capacity(ordered.len());
    let mut aggregate_inputs = BTreeMap::<CommodityKey, Mass>::new();
    for selection in ordered {
        let lot = validate_explicit_lot_selection(state, source, selection)?;
        total_consumed = total_consumed
            .checked_add(selection.mass)
            .ok_or(ExplicitConsumptionSelectionError::MassOverflow { stockpile: source })?;
        let current = aggregate_inputs
            .get(&lot.commodity())
            .copied()
            .unwrap_or(Mass::ZERO);
        aggregate_inputs.insert(
            lot.commodity(),
            current
                .checked_add(selection.mass)
                .ok_or(ExplicitConsumptionSelectionError::MassOverflow { stockpile: source })?,
        );
        lot_slices.push(LotSlice {
            lot: selection.lot,
            mass: selection.mass,
        });
        consumed_inputs.push(ConsumedMaterialTrace {
            mass: selection.mass,
            profile: lot.profile.clone(),
            provenance: lot.provenance,
        });
    }

    let inputs = aggregate_inputs
        .into_iter()
        .map(|(commodity, mass)| MaterialInputSpec::new(commodity, mass))
        .collect();
    Ok(ConsumptionSelection {
        expected_revision: state.revision(),
        source,
        inputs,
        lot_slices,
        consumed_inputs,
    })
}

fn order_explicit_selections(
    selections: &[MaterialLotSelection],
) -> Result<Vec<MaterialLotSelection>, ExplicitConsumptionSelectionError> {
    if selections.is_empty() {
        return Err(ExplicitConsumptionSelectionError::EmptySelection);
    }
    let mut ordered = selections.to_vec();
    ordered.sort();
    if let Some(pair) = ordered.windows(2).find(|pair| pair[0].lot == pair[1].lot) {
        return Err(ExplicitConsumptionSelectionError::DuplicateLot { lot: pair[0].lot });
    }
    Ok(ordered)
}

fn validate_explicit_lot_selection(
    state: &InventoryState,
    source: StockpileId,
    selection: MaterialLotSelection,
) -> Result<&MaterialLotRecord, ExplicitConsumptionSelectionError> {
    if selection.mass.is_zero() {
        return Err(ExplicitConsumptionSelectionError::ZeroMass { lot: selection.lot });
    }
    let lot = state
        .get_lot(selection.lot)
        .ok_or(ExplicitConsumptionSelectionError::UnknownLot { lot: selection.lot })?;
    if lot.stockpile() != source {
        return Err(ExplicitConsumptionSelectionError::LotOwnedElsewhere {
            lot: selection.lot,
            requested_source: source,
            actual_source: lot.stockpile(),
        });
    }
    if lot.mass() < selection.mass {
        return Err(ExplicitConsumptionSelectionError::InsufficientLotMass {
            lot: selection.lot,
            available: lot.mass(),
            requested: selection.mass,
        });
    }
    Ok(lot)
}

pub(crate) fn validate_consumption_reservation_from_selection(
    state: &InventoryState,
    selection: ConsumptionSelection,
    inbound_by_destination: BTreeMap<StockpileId, Mass>,
) -> Result<ConsumptionReservation, ReservationError> {
    let ConsumptionSelection {
        expected_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs,
    } = selection;
    if state.revision() != expected_revision {
        return Err(ReservationError::StaleSelection {
            expected: expected_revision,
            actual: state.revision(),
        });
    }
    let Some(next_revision) = state.revision().checked_add(1) else {
        return Err(ReservationError::RevisionExhausted);
    };
    let total_consumed = checked_consumed_material_mass(&consumed_inputs)
        .ok_or(ReservationError::MassOverflow { stockpile: source })?;

    for (destination, inbound_mass) in &inbound_by_destination {
        let Some(destination_record) = state.get_stockpile(*destination) else {
            return Err(ReservationError::UnknownStockpile {
                stockpile: *destination,
            });
        };
        let destination_stored_after_consumption = if source == *destination {
            destination_record
                .stored_mass
                .checked_sub(total_consumed)
                .ok_or(ReservationError::MassOverflow { stockpile: source })?
        } else {
            destination_record.stored_mass
        };
        let committed_after_consumption = destination_stored_after_consumption
            .checked_add(destination_record.reserved_inbound)
            .ok_or(ReservationError::MassOverflow {
                stockpile: *destination,
            })?;
        let after_reservation = committed_after_consumption
            .checked_add(*inbound_mass)
            .ok_or(ReservationError::MassOverflow {
                stockpile: *destination,
            })?;
        if after_reservation > destination_record.capacity {
            return Err(ReservationError::CapacityExceeded {
                stockpile: *destination,
                capacity: destination_record.capacity,
                committed_after_consumption,
                requested_inbound: *inbound_mass,
            });
        }
    }

    Ok(ConsumptionReservation {
        expected_revision,
        next_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs,
        inbound_by_destination,
    })
}

#[cfg(test)]
pub(crate) fn apply_consumption_reservation(
    state: &mut InventoryState,
    reservation: ConsumptionReservation,
) -> Result<(), ReservationCommitError> {
    if state.revision() != reservation.expected_revision {
        return Err(ReservationCommitError::StaleInventoryRevision {
            expected: reservation.expected_revision,
            actual: state.revision(),
        });
    }
    apply_prechecked_consumption_reservation(state, reservation);
    Ok(())
}

/// Applies a reservation after a surrounding multi-owner transaction has checked its revision.
///
/// This form is intentionally infallible: callers must perform every recoverable stale-state check
/// before mutating any other owner. The assertion protects that ordering contract from internal
/// misuse without introducing a post-mutation error path.
pub(crate) fn apply_prechecked_consumption_reservation(
    state: &mut InventoryState,
    reservation: ConsumptionReservation,
) {
    reservation.assert_matches_state(state);
    let ConsumptionReservation {
        expected_revision,
        next_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs: _consumed_inputs,
        inbound_by_destination,
    } = reservation;
    assert_eq!(
        state.revision(),
        expected_revision,
        "prechecked consumption reservation requires its validated inventory revision"
    );

    for input in &inputs {
        apply_aggregate_withdraw(state, source, input.commodity(), input.mass());
    }
    for slice in lot_slices {
        apply_consume_lot_slice(state, slice);
    }

    for (destination, inbound_mass) in inbound_by_destination {
        let destination_record = get_stockpile_mut_or_panic(state, destination);
        destination_record.reserved_inbound = match destination_record
            .reserved_inbound
            .checked_add(inbound_mass)
        {
            Some(value) => value,
            None => panic!(
                "validated reservation overflowed stockpile {} inbound mass",
                destination.value()
            ),
        };
    }
    state.apply_revision(next_revision);
}

fn select_input_lot_slices(
    inventories: &InventoryState,
    source: &StockpileRecord,
    input: &MaterialInputSpec,
    selected_by_lot: &mut BTreeMap<MaterialLotId, Mass>,
) -> Result<Vec<LotSlice>, Mass> {
    let mut remaining = input.mass();
    let mut available = Mass::ZERO;
    let mut slices = Vec::new();

    // Fixed-input recipes intentionally use stable persistent identity as their generic allocation
    // order. This is not a FIFO/FEFO policy: owners whose outcome depends on age or another local
    // lot property must require explicit lot selection, as direct food consumption does.
    for lot_id in inventories.lot_ids_for_commodity(source.id, input.commodity()) {
        let lot = inventories.get_lot(lot_id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: stockpile {} indexes missing lot {}",
                source.id.value(),
                lot_id.value()
            )
        });
        if !input.is_satisfied_by(lot.composition()) {
            continue;
        }

        let already_selected = selected_by_lot.get(&lot_id).copied().unwrap_or(Mass::ZERO);
        let free = lot.mass.checked_sub(already_selected).unwrap_or_else(|| {
            panic!("input allocator selected more mass than material lot contains")
        });
        available = available
            .checked_add(free)
            .unwrap_or_else(|| panic!("eligible input mass overflowed stockpile mass accounting"));
        if free.is_zero() {
            continue;
        }

        let take = free.min(remaining);
        slices.push(LotSlice {
            lot: lot_id,
            mass: take,
        });
        let selected_after = already_selected
            .checked_add(take)
            .unwrap_or_else(|| panic!("input allocator selection overflowed material lot mass"));
        selected_by_lot.insert(lot_id, selected_after);
        remaining = remaining
            .checked_sub(take)
            .unwrap_or_else(|| panic!("input allocator underflowed remaining requested mass"));
        if remaining.is_zero() {
            return Ok(slices);
        }
    }

    Err(available)
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
