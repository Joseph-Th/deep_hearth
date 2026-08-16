//! Deterministic lot selection and revision-bound material-consumption reservations.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, MaterialInputSpec, MaterialLotSpec};

use super::lot_mutation::{
    LotSlice, apply_aggregate_withdraw, apply_consume_lot_slice, apply_insert_or_merge_new_lot,
    get_stockpile_mut_or_panic,
};
use super::state::{
    ConsumedMaterialTrace, InventoryState, MaterialLotId, MaterialLotProfile,
    MaterialLotProvenance, MaterialLotRecord, StockpileId, StockpileRecord,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReservationCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConsumptionSelection {
    pub(super) expected_revision: u64,
    pub(super) source: StockpileId,
    pub(super) inputs: Vec<MaterialInputSpec>,
    pub(super) lot_slices: Vec<LotSlice>,
    pub(super) consumed_inputs: Vec<ConsumedMaterialTrace>,
    pub(super) total_consumed: Mass,
}

impl ConsumptionSelection {
    pub(crate) const fn source(&self) -> StockpileId {
        self.source
    }

    pub(crate) fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        &self.consumed_inputs
    }

    pub(crate) const fn total_consumed(&self) -> Mass {
        self.total_consumed
    }
}

impl ConsumptionReservation {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    pub(crate) fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        &self.consumed_inputs
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
        let (selected, available) =
            select_input_lot_slices(state, source_record, input, &mut selected_by_lot);
        let Some(selected) = selected else {
            return Err(ConsumptionSelectionError::InsufficientMass {
                stockpile: source,
                commodity: input.commodity(),
                available,
                requested: input.mass(),
            });
        };
        total_consumed = total_consumed
            .checked_add(input.mass())
            .ok_or(ConsumptionSelectionError::MassOverflow { stockpile: source })?;
        lot_slices.extend(selected);
    }

    let consumed_inputs = lot_slices
        .iter()
        .map(|slice| {
            let lot = match state.lots.get(&slice.lot) {
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
        expected_revision: state.revision,
        source,
        inputs: inputs.to_vec(),
        lot_slices,
        consumed_inputs,
        total_consumed,
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
    if selections.is_empty() {
        return Err(ExplicitConsumptionSelectionError::EmptySelection);
    }

    let mut ordered = selections.to_vec();
    ordered.sort();
    for pair in ordered.windows(2) {
        if pair[0].lot == pair[1].lot {
            return Err(ExplicitConsumptionSelectionError::DuplicateLot { lot: pair[0].lot });
        }
    }

    let mut total_consumed = Mass::ZERO;
    let mut lot_slices = Vec::with_capacity(ordered.len());
    let mut consumed_inputs = Vec::with_capacity(ordered.len());
    let mut aggregate_inputs = BTreeMap::<CommodityKey, Mass>::new();
    for selection in ordered {
        if selection.mass.is_zero() {
            return Err(ExplicitConsumptionSelectionError::ZeroMass { lot: selection.lot });
        }
        let Some(lot) = state.get_lot(selection.lot) else {
            return Err(ExplicitConsumptionSelectionError::UnknownLot { lot: selection.lot });
        };
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
        expected_revision: state.revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs,
        total_consumed,
    })
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
        total_consumed,
    } = selection;
    if state.revision != expected_revision {
        return Err(ReservationError::StaleSelection {
            expected: expected_revision,
            actual: state.revision,
        });
    }
    let Some(next_revision) = state.revision.checked_add(1) else {
        return Err(ReservationError::RevisionExhausted);
    };

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

pub(crate) fn apply_consumption_reservation(
    state: &mut InventoryState,
    reservation: ConsumptionReservation,
) -> Result<(), ReservationCommitError> {
    let ConsumptionReservation {
        expected_revision,
        next_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs: _consumed_inputs,
        inbound_by_destination,
    } = reservation;

    if state.revision != expected_revision {
        return Err(ReservationCommitError::StaleInventoryRevision {
            expected: expected_revision,
            actual: state.revision,
        });
    }

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
    state.revision = next_revision;
    Ok(())
}

pub(crate) fn apply_reserved_deposit(
    state: &mut InventoryState,
    destination: StockpileId,
    outputs: &[MaterialLotSpec],
    lot_ids: &[MaterialLotId],
    reserved_mass: Mass,
    created_at: SimulationTick,
) {
    assert_eq!(
        outputs.len(),
        lot_ids.len(),
        "completion plan must allocate exactly one ID per output lot"
    );
    {
        let record = get_stockpile_mut_or_panic(state, destination);
        record.reserved_inbound = match record.reserved_inbound.checked_sub(reserved_mass) {
            Some(value) => value,
            None => panic!(
                "reserved output mass underflow in stockpile {}",
                destination.value()
            ),
        };
    }

    for (output, lot_id) in outputs.iter().zip(lot_ids) {
        apply_insert_or_merge_new_lot(
            state,
            MaterialLotRecord {
                id: *lot_id,
                stockpile: destination,
                mass: output.mass(),
                profile: MaterialLotProfile {
                    commodity: output.commodity(),
                    temperature: output.temperature(),
                    composition: output.composition().clone(),
                    particle_size: output.particle_size_distribution().cloned(),
                },
                provenance: MaterialLotProvenance {
                    earliest_created_at: created_at,
                    latest_created_at: created_at,
                },
            },
        );
    }
}

fn select_input_lot_slices(
    inventories: &InventoryState,
    source: &StockpileRecord,
    input: &MaterialInputSpec,
    selected_by_lot: &mut BTreeMap<MaterialLotId, Mass>,
) -> (Option<Vec<LotSlice>>, Mass) {
    let mut remaining = input.mass();
    let mut available = Mass::ZERO;
    let mut slices = Vec::new();

    for lot_id in &source.lot_ids {
        let lot = match inventories.lots.get(lot_id) {
            Some(lot) => lot,
            None => panic!(
                "runtime invariant broken: stockpile {} indexes missing lot {}",
                source.id.value(),
                lot_id.value()
            ),
        };
        if lot.commodity() != input.commodity() || !input.is_satisfied_by(lot.composition()) {
            continue;
        }

        let already_selected = selected_by_lot.get(lot_id).copied().unwrap_or(Mass::ZERO);
        let free = match lot.mass.checked_sub(already_selected) {
            Some(value) => value,
            None => panic!("input allocator selected more mass than material lot contains"),
        };
        available = match available.checked_add(free) {
            Some(value) => value,
            None => panic!("eligible input mass overflowed stockpile mass accounting"),
        };
        if remaining.is_zero() || free.is_zero() {
            continue;
        }

        let take = if free <= remaining { free } else { remaining };
        slices.push(LotSlice {
            lot: *lot_id,
            mass: take,
        });
        let selected_after = match already_selected.checked_add(take) {
            Some(value) => value,
            None => panic!("input allocator selection overflowed material lot mass"),
        };
        selected_by_lot.insert(*lot_id, selected_after);
        remaining = match remaining.checked_sub(take) {
            Some(value) => value,
            None => panic!("input allocator underflowed remaining requested mass"),
        };
    }

    if remaining.is_zero() {
        (Some(slices), available)
    } else {
        (None, available)
    }
}

#[cfg(test)]
mod tests {
    use super::super::transactions::{add_solid_stockpile_for_test, deposit_lot_for_test};
    use super::*;
    use crate::content::{FORM_LOG, MATERIAL_WOOD, build_registries};
    use crate::core::quantity::Temperature;
    use crate::core::state::AppState;
    use crate::core::time::WorldSeed;

    #[test]
    fn explicit_selection_binds_partial_lot_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_0001));
        let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
            .unwrap_or_else(|error| panic!("explicit selection source fixture failed: {error}"));
        let lot = deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(20),
            Temperature::from_millikelvin(300_000),
        )
        .unwrap_or_else(|error| panic!("explicit selection lot fixture failed: {error}"));
        let before = state.clone();

        let selection = validate_explicit_consumption_selection(
            state.inventory(),
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(7))],
        )
        .unwrap_or_else(|error| panic!("explicit selection validation failed: {error:?}"));

        assert_eq!(selection.total_consumed(), Mass::from_milligrams(7));
        assert_eq!(selection.consumed_inputs().len(), 1);
        assert_eq!(
            selection.consumed_inputs()[0].mass(),
            Mass::from_milligrams(7)
        );
        assert_eq!(
            selection.consumed_inputs()[0].profile().temperature(),
            Temperature::from_millikelvin(300_000)
        );
        assert_eq!(state, before);
    }

    #[test]
    fn explicit_selection_rejects_duplicate_lot_and_wrong_source() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_0002));
        let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
            .unwrap_or_else(|error| panic!("explicit selection source fixture failed: {error}"));
        let other = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
            .unwrap_or_else(|error| panic!("explicit selection secondary fixture failed: {error}"));
        let lot = deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(20),
            Temperature::from_millikelvin(300_000),
        )
        .unwrap_or_else(|error| panic!("explicit selection lot fixture failed: {error}"));
        let before = state.clone();
        let slice = MaterialLotSelection::new(lot, Mass::from_milligrams(5));

        assert_eq!(
            validate_explicit_consumption_selection(state.inventory(), source, &[slice, slice],),
            Err(ExplicitConsumptionSelectionError::DuplicateLot { lot })
        );
        assert_eq!(
            validate_explicit_consumption_selection(state.inventory(), other, &[slice]),
            Err(ExplicitConsumptionSelectionError::LotOwnedElsewhere {
                lot,
                requested_source: other,
                actual_source: source,
            })
        );
        assert_eq!(state, before);
    }
}
