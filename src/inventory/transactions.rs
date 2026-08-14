//! Canonical inventory transactions; sibling state records remain passive and privately mutable.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
#[cfg(test)]
use crate::core::quantity::Temperature;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
#[cfg(test)]
use crate::material::MaterialComposition;
use crate::material::{
    CommodityKey, CompositionError, FormId, MaterialId, MaterialInputSpec, MaterialLotSpec,
};
use crate::registry::Registries;

use super::state::{
    ConsumedMaterialTrace, InventoryState, MaterialLotId, MaterialLotProfile,
    MaterialLotProvenance, MaterialLotRecord, StockpileId, StockpileRecord,
};

#[cfg(test)]
const TEST_REFERENCE_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

/// Failure while allocating a new stockpile record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddStockpileError {
    ZeroCapacity,
    IdExhausted,
    RevisionExhausted,
}

/// Failure while validating multiple conserved material traces entering one stockpile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialBatchIngressError {
    EmptyBatch,
    UnknownStockpile {
        stockpile: StockpileId,
    },
    UnknownMaterial {
        material: MaterialId,
    },
    UnknownForm {
        form: FormId,
    },
    UnknownCompositionMaterial {
        material: MaterialId,
    },
    ZeroMass,
    InvalidComposition {
        error: CompositionError,
    },
    CompositionMissingHost {
        host: MaterialId,
    },
    InvalidProvenance,
    ProvenanceInFuture {
        latest: SimulationTick,
        current: SimulationTick,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
    CapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    RevisionExhausted,
}

/// Consumed proof that a complete source-owned trace batch can enter one stockpile atomically.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedMaterialBatchIngress {
    expected_revision: u64,
    next_revision: u64,
    destination: StockpileId,
    traces: Vec<ConsumedMaterialTrace>,
    allocated_lot_ids: Vec<MaterialLotId>,
    next_lot_id: u64,
}

impl ValidatedMaterialBatchIngress {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }
}

/// Revision-bound withdrawal of exact material slices into another authoritative owner.
///
/// The destination owner is deliberately absent. This token proves only that the selected matter
/// can leave inventory exactly once; the cross-subsystem transaction that holds it is responsible
/// for establishing the new owner before exposing a successful commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedMaterialEgress {
    expected_revision: u64,
    next_revision: u64,
    source: StockpileId,
    inputs: Vec<MaterialInputSpec>,
    lot_slices: Vec<LotSlice>,
    consumed_inputs: Vec<ConsumedMaterialTrace>,
    total_consumed: Mass,
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

    pub(crate) const fn total_consumed(&self) -> Mass {
        self.total_consumed
    }

    pub(crate) fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        &self.consumed_inputs
    }
}

impl Display for AddStockpileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCapacity => formatter.write_str("stockpile capacity must be nonzero"),
            Self::IdExhausted => formatter.write_str("stockpile identifier space is exhausted"),
            Self::RevisionExhausted => formatter.write_str("inventory revision space is exhausted"),
        }
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
        total_consumed,
    } = selection;
    if state.revision != expected_revision {
        return Err(MaterialEgressError::StaleSelection {
            expected: expected_revision,
            actual: state.revision,
        });
    }
    let Some(next_revision) = state.revision.checked_add(1) else {
        return Err(MaterialEgressError::RevisionExhausted);
    };
    Ok(ValidatedMaterialEgress {
        expected_revision,
        next_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs,
        total_consumed,
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
        total_consumed: _,
    } = egress;
    assert_eq!(
        state.revision, expected_revision,
        "material egress commit requires its validated inventory revision"
    );
    for input in &inputs {
        apply_aggregate_withdraw(state, source, input.commodity(), input.mass());
    }
    for slice in lot_slices {
        apply_consume_lot_slice(state, slice);
    }
    state.revision = next_revision;
}

/// Validates exact source-owned traces entering inventory while retaining their physical history.
pub(crate) fn validate_material_batch_ingress(
    registries: &Registries,
    state: &InventoryState,
    destination: StockpileId,
    traces: &[ConsumedMaterialTrace],
    current_tick: SimulationTick,
) -> Result<ValidatedMaterialBatchIngress, MaterialBatchIngressError> {
    if traces.is_empty() {
        return Err(MaterialBatchIngressError::EmptyBatch);
    }
    let Some(destination_record) = state.get_stockpile(destination) else {
        return Err(MaterialBatchIngressError::UnknownStockpile {
            stockpile: destination,
        });
    };

    let mut total = Mass::ZERO;
    let mut by_commodity = BTreeMap::<CommodityKey, Mass>::new();
    for trace in traces {
        if trace.mass().is_zero() {
            return Err(MaterialBatchIngressError::ZeroMass);
        }
        let profile = trace.profile();
        profile
            .composition()
            .validate()
            .map_err(|error| MaterialBatchIngressError::InvalidComposition { error })?;
        if profile
            .composition()
            .parts_per_million(profile.commodity().material())
            == 0
        {
            return Err(MaterialBatchIngressError::CompositionMissingHost {
                host: profile.commodity().material(),
            });
        }
        validate_commodity(registries, profile.commodity()).map_err(|error| match error {
            CommodityReferenceError::UnknownMaterial { material } => {
                MaterialBatchIngressError::UnknownMaterial { material }
            }
            CommodityReferenceError::UnknownForm { form } => {
                MaterialBatchIngressError::UnknownForm { form }
            }
        })?;
        for component in profile.composition().components() {
            if registries
                .materials()
                .get_material(component.material())
                .is_none()
            {
                return Err(MaterialBatchIngressError::UnknownCompositionMaterial {
                    material: component.material(),
                });
            }
        }
        let provenance = trace.provenance();
        if provenance.latest_created_at() < provenance.earliest_created_at() {
            return Err(MaterialBatchIngressError::InvalidProvenance);
        }
        if provenance.latest_created_at() > current_tick {
            return Err(MaterialBatchIngressError::ProvenanceInFuture {
                latest: provenance.latest_created_at(),
                current: current_tick,
            });
        }
        total = total
            .checked_add(trace.mass())
            .ok_or(MaterialBatchIngressError::MassOverflow {
                stockpile: destination,
            })?;
        let existing = by_commodity
            .get(&profile.commodity())
            .copied()
            .unwrap_or(Mass::ZERO);
        by_commodity.insert(
            profile.commodity(),
            existing
                .checked_add(trace.mass())
                .ok_or(MaterialBatchIngressError::MassOverflow {
                    stockpile: destination,
                })?,
        );
    }

    let committed = destination_record
        .stored_mass
        .checked_add(destination_record.reserved_inbound)
        .ok_or(MaterialBatchIngressError::MassOverflow {
            stockpile: destination,
        })?;
    let after = committed
        .checked_add(total)
        .ok_or(MaterialBatchIngressError::MassOverflow {
            stockpile: destination,
        })?;
    if after > destination_record.capacity {
        return Err(MaterialBatchIngressError::CapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity,
            committed,
            requested: total,
        });
    }
    for (commodity, incoming) in by_commodity {
        destination_record
            .get_mass(commodity)
            .checked_add(incoming)
            .ok_or(MaterialBatchIngressError::MassOverflow {
                stockpile: destination,
            })?;
    }

    let mut allocated_lot_ids = Vec::with_capacity(traces.len());
    let mut cursor = state.next_lot_id;
    for _ in traces {
        allocated_lot_ids.push(MaterialLotId::new(cursor));
        cursor = cursor
            .checked_add(1)
            .ok_or(MaterialBatchIngressError::LotIdExhausted)?;
    }
    let Some(next_revision) = state.revision.checked_add(1) else {
        return Err(MaterialBatchIngressError::RevisionExhausted);
    };

    Ok(ValidatedMaterialBatchIngress {
        expected_revision: state.revision,
        next_revision,
        destination,
        traces: traces.to_vec(),
        allocated_lot_ids,
        next_lot_id: cursor,
    })
}

/// Applies a validated trace batch after its cross-owner transaction rechecks inventory revision.
pub(crate) fn apply_material_batch_ingress(
    state: &mut InventoryState,
    ingress: ValidatedMaterialBatchIngress,
) -> Vec<MaterialLotId> {
    let ValidatedMaterialBatchIngress {
        expected_revision,
        next_revision,
        destination,
        traces,
        allocated_lot_ids,
        next_lot_id,
    } = ingress;
    assert_eq!(
        state.revision, expected_revision,
        "material batch ingress commit requires its validated inventory revision"
    );

    let mut resulting_lots = Vec::with_capacity(traces.len());
    for (trace, allocated_lot_id) in traces.into_iter().zip(allocated_lot_ids) {
        let profile = trace.profile().clone();
        let provenance = trace.provenance();
        let resulting = apply_insert_or_merge_new_lot(
            state,
            MaterialLotRecord {
                id: allocated_lot_id,
                stockpile: destination,
                mass: trace.mass(),
                profile,
                provenance,
            },
        );
        resulting_lots.push(resulting);
    }
    state.next_lot_id = next_lot_id;
    state.revision = next_revision;
    resulting_lots
}

impl Error for AddStockpileError {}

/// Failure while depositing matter from an explicit source into a stockpile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepositError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    UnknownMaterial {
        material: MaterialId,
    },
    UnknownForm {
        form: FormId,
    },
    ZeroMass,
    MassOverflow {
        stockpile: StockpileId,
    },
    CapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    RevisionExhausted,
}

impl Display for DepositError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
            Self::UnknownMaterial { material } => {
                write!(formatter, "unknown material id {}", material.value())
            }
            Self::UnknownForm { form } => write!(formatter, "unknown form id {}", form.value()),
            Self::ZeroMass => formatter.write_str("deposit mass must be nonzero"),
            Self::MassOverflow { stockpile } => write!(
                formatter,
                "mass accounting overflow in stockpile {}",
                stockpile.value()
            ),
            Self::CapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg exceeded: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted")
            }
            Self::RevisionExhausted => formatter.write_str("inventory revision space is exhausted"),
        }
    }
}

impl Error for DepositError {}

/// Crate-internal failure while validating matter entering inventory from an explicit owner.
///
/// This boundary is intentionally not public. Source systems such as geology must prove ownership
/// and conservation before invoking it; callers cannot use it as an arbitrary matter-spawn API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialIngressError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    UnknownMaterial {
        material: MaterialId,
    },
    UnknownForm {
        form: FormId,
    },
    UnknownCompositionMaterial {
        material: MaterialId,
    },
    ZeroMass,
    InvalidComposition {
        error: CompositionError,
    },
    CompositionMissingHost {
        host: MaterialId,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
    CapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    RevisionExhausted,
}

impl Display for MaterialIngressError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
            Self::UnknownMaterial { material } => {
                write!(formatter, "unknown material id {}", material.value())
            }
            Self::UnknownForm { form } => write!(formatter, "unknown form id {}", form.value()),
            Self::UnknownCompositionMaterial { material } => write!(
                formatter,
                "material ingress composition references unknown material {}",
                material.value()
            ),
            Self::ZeroMass => formatter.write_str("material ingress mass must be nonzero"),
            Self::InvalidComposition { error } => {
                write!(
                    formatter,
                    "material ingress has invalid composition: {error}"
                )
            }
            Self::CompositionMissingHost { host } => write!(
                formatter,
                "material ingress composition omits host material {}",
                host.value()
            ),
            Self::MassOverflow { stockpile } => write!(
                formatter,
                "material ingress overflows mass accounting in stockpile {}",
                stockpile.value()
            ),
            Self::CapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg exceeded: {} mg committed, {} mg ingress requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted")
            }
            Self::RevisionExhausted => formatter.write_str("inventory revision space is exhausted"),
        }
    }
}

impl Error for MaterialIngressError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComposition { error } => Some(error),
            Self::UnknownStockpile { .. }
            | Self::UnknownMaterial { .. }
            | Self::UnknownForm { .. }
            | Self::UnknownCompositionMaterial { .. }
            | Self::ZeroMass
            | Self::CompositionMissingHost { .. }
            | Self::MassOverflow { .. }
            | Self::CapacityExceeded { .. }
            | Self::LotIdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}

/// Consumed proof that one source-owned material lot may enter a destination stockpile atomically.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedMaterialIngress {
    expected_revision: u64,
    next_revision: u64,
    destination: StockpileId,
    output: MaterialLotSpec,
    allocated_lot_id: MaterialLotId,
    next_lot_id: u64,
    created_at: SimulationTick,
}

impl ValidatedMaterialIngress {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }
}

/// Failure while validating an atomic stockpile-to-stockpile transfer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    UnknownMaterial {
        material: MaterialId,
    },
    UnknownForm {
        form: FormId,
    },
    ZeroMass,
    InsufficientMass {
        stockpile: StockpileId,
        commodity: CommodityKey,
        available: Mass,
        requested: Mass,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
    CapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    RevisionExhausted,
}

impl Display for TransferError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
            Self::UnknownMaterial { material } => {
                write!(formatter, "unknown material id {}", material.value())
            }
            Self::UnknownForm { form } => write!(formatter, "unknown form id {}", form.value()),
            Self::ZeroMass => formatter.write_str("transfer mass must be nonzero"),
            Self::InsufficientMass {
                stockpile,
                available,
                requested,
                ..
            } => write!(
                formatter,
                "stockpile {} has {} mg available but {} mg was requested",
                stockpile.value(),
                available.milligrams(),
                requested.milligrams()
            ),
            Self::MassOverflow { stockpile } => write!(
                formatter,
                "mass accounting overflow in stockpile {}",
                stockpile.value()
            ),
            Self::CapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg exceeded: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted")
            }
            Self::RevisionExhausted => formatter.write_str("inventory revision space is exhausted"),
        }
    }
}

impl Error for TransferError {}

/// Failure when a previously validated transfer is committed after inventory has changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
}

impl Display for TransferCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated transfer expected inventory revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for TransferCommitError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LotSlice {
    lot: MaterialLotId,
    mass: Mass,
}

/// Consumed proof that all preconditions for a two-stockpile transfer have been checked.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedTransferBulk {
    expected_revision: u64,
    next_revision: u64,
    source: StockpileId,
    destination: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    slices: Vec<LotSlice>,
    split_lot_id: Option<MaterialLotId>,
    next_lot_id_after: Option<u64>,
}

impl ValidatedTransferBulk {
    /// Atomically commits this already validated transfer and consumes the proof token.
    pub fn commit(self, state: &mut AppState) -> Result<(), TransferCommitError> {
        let Self {
            expected_revision,
            next_revision,
            source,
            destination,
            commodity,
            mass,
            slices,
            split_lot_id,
            next_lot_id_after,
        } = self;

        if source == destination {
            return Ok(());
        }

        let inventories = state.inventory_state_mut();
        if inventories.revision != expected_revision {
            return Err(TransferCommitError::StaleInventoryRevision {
                expected: expected_revision,
                actual: inventories.revision,
            });
        }

        apply_aggregate_withdraw(inventories, source, commodity, mass);
        apply_aggregate_deposit(inventories, destination, commodity, mass);

        let mut split_id = split_lot_id;
        for slice in slices {
            let lot_mass = match inventories.lots.get(&slice.lot) {
                Some(lot) => lot.mass,
                None => panic!(
                    "validated transfer references missing material lot {}",
                    slice.lot.value()
                ),
            };
            if slice.mass == lot_mass {
                apply_move_full_lot(inventories, slice.lot, source, destination);
            } else {
                let allocated = match split_id.take() {
                    Some(id) => id,
                    None => panic!("validated partial transfer is missing its allocated lot ID"),
                };
                apply_split_lot(inventories, slice.lot, allocated, destination, slice.mass);
            }
        }

        if split_id.is_some() {
            panic!("validated transfer allocated an unused split lot ID");
        }
        if let Some(next_lot_id) = next_lot_id_after {
            inventories.next_lot_id = next_lot_id;
        }
        inventories.revision = next_revision;
        Ok(())
    }
}

/// Adds an empty stockpile through the canonical inventory mutation path.
pub fn add_stockpile(
    state: &mut AppState,
    capacity: Mass,
) -> Result<StockpileId, AddStockpileError> {
    if capacity.is_zero() {
        return Err(AddStockpileError::ZeroCapacity);
    }

    let inventories = state.inventory_state_mut();
    let id = StockpileId::new(inventories.next_stockpile_id);
    let Some(next_id) = inventories.next_stockpile_id.checked_add(1) else {
        return Err(AddStockpileError::IdExhausted);
    };
    let Some(next_revision) = inventories.revision.checked_add(1) else {
        return Err(AddStockpileError::RevisionExhausted);
    };

    let record = StockpileRecord {
        id,
        capacity,
        stored_mass: Mass::ZERO,
        reserved_inbound: Mass::ZERO,
        lot_ids: std::collections::BTreeSet::new(),
        contents: BTreeMap::new(),
    };

    inventories.next_stockpile_id = next_id;
    inventories.revision = next_revision;
    let replaced = inventories.stockpiles.insert(id, record);
    debug_assert!(replaced.is_none(), "stockpile ID allocation must be unique");
    Ok(id)
}

/// Validates inventory admission for matter already owned and conserved by another subsystem.
pub(crate) fn validate_material_ingress(
    registries: &Registries,
    state: &InventoryState,
    destination: StockpileId,
    output: MaterialLotSpec,
    created_at: SimulationTick,
) -> Result<ValidatedMaterialIngress, MaterialIngressError> {
    if output.mass().is_zero() {
        return Err(MaterialIngressError::ZeroMass);
    }
    output
        .composition()
        .validate()
        .map_err(|error| MaterialIngressError::InvalidComposition { error })?;
    if output
        .composition()
        .parts_per_million(output.commodity().material())
        == 0
    {
        return Err(MaterialIngressError::CompositionMissingHost {
            host: output.commodity().material(),
        });
    }
    validate_commodity(registries, output.commodity()).map_err(|error| match error {
        CommodityReferenceError::UnknownMaterial { material } => {
            MaterialIngressError::UnknownMaterial { material }
        }
        CommodityReferenceError::UnknownForm { form } => MaterialIngressError::UnknownForm { form },
    })?;
    for component in output.composition().components() {
        if registries
            .materials()
            .get_material(component.material())
            .is_none()
        {
            return Err(MaterialIngressError::UnknownCompositionMaterial {
                material: component.material(),
            });
        }
    }

    let Some(destination_record) = state.get_stockpile(destination) else {
        return Err(MaterialIngressError::UnknownStockpile {
            stockpile: destination,
        });
    };
    let committed = destination_record
        .stored_mass
        .checked_add(destination_record.reserved_inbound)
        .ok_or(MaterialIngressError::MassOverflow {
            stockpile: destination,
        })?;
    let after = committed
        .checked_add(output.mass())
        .ok_or(MaterialIngressError::MassOverflow {
            stockpile: destination,
        })?;
    if after > destination_record.capacity {
        return Err(MaterialIngressError::CapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity,
            committed,
            requested: output.mass(),
        });
    }
    destination_record
        .get_mass(output.commodity())
        .checked_add(output.mass())
        .ok_or(MaterialIngressError::MassOverflow {
            stockpile: destination,
        })?;

    let allocated_lot_id = MaterialLotId::new(state.next_lot_id);
    let Some(next_lot_id) = state.next_lot_id.checked_add(1) else {
        return Err(MaterialIngressError::LotIdExhausted);
    };
    let Some(next_revision) = state.revision.checked_add(1) else {
        return Err(MaterialIngressError::RevisionExhausted);
    };

    Ok(ValidatedMaterialIngress {
        expected_revision: state.revision,
        next_revision,
        destination,
        output,
        allocated_lot_id,
        next_lot_id,
        created_at,
    })
}

/// Applies a previously validated source ingress after the owning cross-system transaction has
/// rechecked the inventory revision.
pub(crate) fn apply_material_ingress(
    state: &mut InventoryState,
    ingress: ValidatedMaterialIngress,
) -> MaterialLotId {
    let ValidatedMaterialIngress {
        expected_revision,
        next_revision,
        destination,
        output,
        allocated_lot_id,
        next_lot_id,
        created_at,
    } = ingress;
    assert_eq!(
        state.revision, expected_revision,
        "material ingress commit requires its validated inventory revision"
    );

    let resulting_lot = apply_insert_or_merge_new_lot(
        state,
        MaterialLotRecord {
            id: allocated_lot_id,
            stockpile: destination,
            mass: output.mass(),
            profile: MaterialLotProfile {
                commodity: output.commodity(),
                temperature: output.temperature(),
                composition: output.composition().clone(),
            },
            provenance: MaterialLotProvenance {
                earliest_created_at: created_at,
                latest_created_at: created_at,
            },
        },
    );
    state.next_lot_id = next_lot_id;
    state.revision = next_revision;
    resulting_lot
}

/// Deposits explicitly sourced matter after validating references and capacity.
#[cfg(test)]
pub(crate) fn deposit_bulk_for_test(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) -> Result<(), DepositError> {
    deposit_lot_for_test(
        registries,
        state,
        stockpile,
        commodity,
        mass,
        TEST_REFERENCE_TEMPERATURE,
    )
    .map(|_| ())
}

/// Seeds one explicit homogeneous lot for behavioral tests that need controlled thermal state.
#[cfg(test)]
pub(crate) fn deposit_lot_for_test(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
) -> Result<MaterialLotId, DepositError> {
    deposit_composed_lot_for_test(
        registries,
        state,
        stockpile,
        commodity,
        mass,
        temperature,
        MaterialComposition::pure(commodity.material()),
    )
}

#[cfg(test)]
pub(crate) fn deposit_composed_lot_for_test(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
) -> Result<MaterialLotId, DepositError> {
    if let Err(error) = composition.validate() {
        panic!("test fixture provided invalid material composition: {error}");
    }
    assert!(
        composition.parts_per_million(commodity.material()) > 0,
        "test fixture composition must contain its host material"
    );
    validate_commodity(registries, commodity).map_err(|error| match error {
        CommodityReferenceError::UnknownMaterial { material } => {
            DepositError::UnknownMaterial { material }
        }
        CommodityReferenceError::UnknownForm { form } => DepositError::UnknownForm { form },
    })?;
    if mass.is_zero() {
        return Err(DepositError::ZeroMass);
    }

    let inventories = state.inventory_state();
    let Some(record) = inventories.get_stockpile(stockpile) else {
        return Err(DepositError::UnknownStockpile { stockpile });
    };
    let committed = record
        .stored_mass
        .checked_add(record.reserved_inbound)
        .ok_or(DepositError::MassOverflow { stockpile })?;
    let after = committed
        .checked_add(mass)
        .ok_or(DepositError::MassOverflow { stockpile })?;
    if after > record.capacity {
        return Err(DepositError::CapacityExceeded {
            stockpile,
            capacity: record.capacity,
            committed,
            requested: mass,
        });
    }
    record
        .get_mass(commodity)
        .checked_add(mass)
        .ok_or(DepositError::MassOverflow { stockpile })?;
    let lot_id = MaterialLotId::new(inventories.next_lot_id);
    let Some(next_lot_id) = inventories.next_lot_id.checked_add(1) else {
        return Err(DepositError::LotIdExhausted);
    };
    let Some(next_revision) = inventories.revision.checked_add(1) else {
        return Err(DepositError::RevisionExhausted);
    };
    let created_at = state.tick();

    let inventories = state.inventory_state_mut();
    apply_insert_lot(
        inventories,
        MaterialLotRecord {
            id: lot_id,
            stockpile,
            mass,
            profile: MaterialLotProfile {
                commodity,
                temperature,
                composition,
            },
            provenance: MaterialLotProvenance {
                earliest_created_at: created_at,
                latest_created_at: created_at,
            },
        },
    );
    inventories.next_lot_id = next_lot_id;
    inventories.revision = next_revision;
    Ok(lot_id)
}

/// Validates a multi-record transfer without mutating either stockpile.
pub fn validate_transfer_bulk(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    destination: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) -> Result<ValidatedTransferBulk, TransferError> {
    validate_commodity(registries, commodity).map_err(|error| match error {
        CommodityReferenceError::UnknownMaterial { material } => {
            TransferError::UnknownMaterial { material }
        }
        CommodityReferenceError::UnknownForm { form } => TransferError::UnknownForm { form },
    })?;
    if mass.is_zero() {
        return Err(TransferError::ZeroMass);
    }

    let inventories = state.inventory_state();
    let Some(source_record) = inventories.get_stockpile(source) else {
        return Err(TransferError::UnknownStockpile { stockpile: source });
    };
    let available = source_record.get_mass(commodity);
    if available < mass {
        return Err(TransferError::InsufficientMass {
            stockpile: source,
            commodity,
            available,
            requested: mass,
        });
    }

    let Some(destination_record) = inventories.get_stockpile(destination) else {
        return Err(TransferError::UnknownStockpile {
            stockpile: destination,
        });
    };

    if source != destination {
        validate_destination_capacity(destination_record, destination, mass)?;
        destination_record
            .get_mass(commodity)
            .checked_add(mass)
            .ok_or(TransferError::MassOverflow {
                stockpile: destination,
            })?;
    }

    if source == destination {
        return Ok(ValidatedTransferBulk {
            expected_revision: inventories.revision,
            next_revision: inventories.revision,
            source,
            destination,
            commodity,
            mass,
            slices: Vec::new(),
            split_lot_id: None,
            next_lot_id_after: None,
        });
    }

    let Some(next_revision) = inventories.revision.checked_add(1) else {
        return Err(TransferError::RevisionExhausted);
    };
    let slices = select_lot_slices(inventories, source_record, commodity, mass);
    let needs_split = slices.last().is_some_and(|slice| {
        inventories
            .lots
            .get(&slice.lot)
            .is_some_and(|lot| slice.mass < lot.mass)
    });
    let (split_lot_id, next_lot_id_after) = if needs_split {
        let id = MaterialLotId::new(inventories.next_lot_id);
        let Some(next_id) = inventories.next_lot_id.checked_add(1) else {
            return Err(TransferError::LotIdExhausted);
        };
        (Some(id), Some(next_id))
    } else {
        (None, None)
    };

    Ok(ValidatedTransferBulk {
        expected_revision: inventories.revision,
        next_revision,
        source,
        destination,
        commodity,
        mass,
        slices,
        split_lot_id,
        next_lot_id_after,
    })
}

fn select_lot_slices(
    inventories: &InventoryState,
    source: &StockpileRecord,
    commodity: CommodityKey,
    requested: Mass,
) -> Vec<LotSlice> {
    let mut remaining = requested;
    let mut slices = Vec::new();
    for lot_id in &source.lot_ids {
        if remaining.is_zero() {
            break;
        }
        let lot = match inventories.lots.get(lot_id) {
            Some(lot) => lot,
            None => panic!(
                "runtime invariant broken: stockpile {} indexes missing lot {}",
                source.id.value(),
                lot_id.value()
            ),
        };
        if lot.commodity() != commodity {
            continue;
        }
        let take = if lot.mass <= remaining {
            lot.mass
        } else {
            remaining
        };
        slices.push(LotSlice {
            lot: *lot_id,
            mass: take,
        });
        remaining = match remaining.checked_sub(take) {
            Some(value) => value,
            None => panic!("lot selection underflow after validated availability check"),
        };
    }
    assert!(
        remaining.is_zero(),
        "inventory aggregate/index invariant broken during lot selection"
    );
    slices
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
        if lot.commodity() != input.commodity() || !input.matches_composition(lot.composition()) {
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

fn validate_destination_capacity(
    record: &StockpileRecord,
    stockpile: StockpileId,
    requested: Mass,
) -> Result<(), TransferError> {
    let committed = record
        .stored_mass
        .checked_add(record.reserved_inbound)
        .ok_or(TransferError::MassOverflow { stockpile })?;
    let after = committed
        .checked_add(requested)
        .ok_or(TransferError::MassOverflow { stockpile })?;
    if after > record.capacity {
        return Err(TransferError::CapacityExceeded {
            stockpile,
            capacity: record.capacity,
            committed,
            requested,
        });
    }
    Ok(())
}

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
    destination: StockpileId,
    inputs: Vec<MaterialInputSpec>,
    lot_slices: Vec<LotSlice>,
    consumed_inputs: Vec<ConsumedMaterialTrace>,
    inbound_mass: Mass,
}

/// Deterministic read-only material selection for physical process resolution.
///
/// The selection owns the exact lot slices and physical/provenance traces chosen from one
/// inventory revision. A later reservation consumes this same selection rather than selecting
/// equivalent-looking matter a second time after a resolver has already calculated an outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConsumptionSelection {
    expected_revision: u64,
    source: StockpileId,
    inputs: Vec<MaterialInputSpec>,
    lot_slices: Vec<LotSlice>,
    consumed_inputs: Vec<ConsumedMaterialTrace>,
    total_consumed: Mass,
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
    destination: StockpileId,
    selection: ConsumptionSelection,
    inbound_mass: Mass,
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
    let Some(destination_record) = state.get_stockpile(destination) else {
        return Err(ReservationError::UnknownStockpile {
            stockpile: destination,
        });
    };
    let Some(next_revision) = state.revision.checked_add(1) else {
        return Err(ReservationError::RevisionExhausted);
    };

    let destination_stored_after_consumption = if source == destination {
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
            stockpile: destination,
        })?;
    let after_reservation = committed_after_consumption
        .checked_add(inbound_mass)
        .ok_or(ReservationError::MassOverflow {
            stockpile: destination,
        })?;
    if after_reservation > destination_record.capacity {
        return Err(ReservationError::CapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity,
            committed_after_consumption,
            requested_inbound: inbound_mass,
        });
    }

    Ok(ConsumptionReservation {
        expected_revision,
        next_revision,
        source,
        destination,
        inputs,
        lot_slices,
        consumed_inputs,
        inbound_mass,
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
        destination,
        inputs,
        lot_slices,
        consumed_inputs: _consumed_inputs,
        inbound_mass,
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
                },
                provenance: MaterialLotProvenance {
                    earliest_created_at: created_at,
                    latest_created_at: created_at,
                },
            },
        );
    }
}

pub(crate) fn next_material_lot_id(state: &InventoryState) -> u64 {
    state.next_lot_id
}

pub(crate) fn apply_lot_cursor_and_revision(
    state: &mut InventoryState,
    next_lot_id: u64,
    next_revision: u64,
) {
    state.next_lot_id = next_lot_id;
    state.revision = next_revision;
}

fn apply_aggregate_deposit(
    state: &mut InventoryState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) {
    let record = get_stockpile_mut_or_panic(state, stockpile);
    let current = record.get_mass(commodity);
    let next = match current.checked_add(mass) {
        Some(value) => value,
        None => panic!(
            "validated commodity mass overflow in stockpile {}",
            stockpile.value()
        ),
    };
    record.contents.insert(commodity, next);
    record.stored_mass = match record.stored_mass.checked_add(mass) {
        Some(value) => value,
        None => panic!(
            "validated stored mass overflow in stockpile {}",
            stockpile.value()
        ),
    };
}

fn apply_aggregate_withdraw(
    state: &mut InventoryState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) {
    let record = get_stockpile_mut_or_panic(state, stockpile);
    let current = record.get_mass(commodity);
    let remaining = match current.checked_sub(mass) {
        Some(value) => value,
        None => panic!(
            "validated commodity mass underflow in stockpile {}",
            stockpile.value()
        ),
    };
    if remaining.is_zero() {
        record.contents.remove(&commodity);
    } else {
        record.contents.insert(commodity, remaining);
    }
    record.stored_mass = match record.stored_mass.checked_sub(mass) {
        Some(value) => value,
        None => panic!(
            "validated stored mass underflow in stockpile {}",
            stockpile.value()
        ),
    };
}

fn apply_insert_lot(state: &mut InventoryState, lot: MaterialLotRecord) {
    apply_aggregate_deposit(state, lot.stockpile, lot.commodity(), lot.mass);
    apply_insert_lot_record(state, lot);
}

fn apply_insert_lot_record(state: &mut InventoryState, lot: MaterialLotRecord) {
    let id = lot.id;
    let stockpile = lot.stockpile;
    let inserted = get_stockpile_mut_or_panic(state, stockpile)
        .lot_ids
        .insert(id);
    assert!(
        inserted,
        "validated material lot ID must be unique in owner index"
    );
    let replaced = state.lots.insert(id, lot);
    assert!(
        replaced.is_none(),
        "validated material lot ID must be globally unique"
    );
}

fn apply_insert_or_merge_new_lot(
    state: &mut InventoryState,
    lot: MaterialLotRecord,
) -> MaterialLotId {
    let compatible = find_compatible_lot(state, lot.stockpile, &lot.profile);

    let Some(existing_id) = compatible else {
        let id = lot.id;
        apply_insert_lot(state, lot);
        return id;
    };

    apply_aggregate_deposit(state, lot.stockpile, lot.commodity(), lot.mass);
    apply_merge_lot_record(state, existing_id, lot);
    existing_id
}

fn find_compatible_lot(
    state: &InventoryState,
    stockpile: StockpileId,
    profile: &MaterialLotProfile,
) -> Option<MaterialLotId> {
    let owner = match state.stockpiles.get(&stockpile) {
        Some(owner) => owner,
        None => panic!(
            "runtime invariant broken: missing destination stockpile {}",
            stockpile.value()
        ),
    };
    owner.lot_ids.iter().copied().find(|id| {
        state
            .lots
            .get(id)
            .is_some_and(|existing| &existing.profile == profile)
    })
}

fn apply_merge_lot_record(
    state: &mut InventoryState,
    existing_id: MaterialLotId,
    lot: MaterialLotRecord,
) {
    let existing = match state.lots.get_mut(&existing_id) {
        Some(existing) => existing,
        None => panic!(
            "runtime invariant broken: compatible lot {} disappeared during merge",
            existing_id.value()
        ),
    };
    existing.mass = match existing.mass.checked_add(lot.mass) {
        Some(value) => value,
        None => panic!("validated compatible lot merge overflowed authoritative mass"),
    };
    existing.provenance.earliest_created_at = std::cmp::min(
        existing.provenance.earliest_created_at,
        lot.provenance.earliest_created_at,
    );
    existing.provenance.latest_created_at = std::cmp::max(
        existing.provenance.latest_created_at,
        lot.provenance.latest_created_at,
    );
}

fn apply_move_full_lot(
    state: &mut InventoryState,
    lot: MaterialLotId,
    source: StockpileId,
    destination: StockpileId,
) {
    let removed = get_stockpile_mut_or_panic(state, source)
        .lot_ids
        .remove(&lot);
    assert!(
        removed,
        "validated source stockpile must index moved material lot"
    );
    let inserted = get_stockpile_mut_or_panic(state, destination)
        .lot_ids
        .insert(lot);
    assert!(
        inserted,
        "destination stockpile must not already index moved material lot"
    );
    let record = match state.lots.get_mut(&lot) {
        Some(record) => record,
        None => panic!(
            "validated transfer references missing material lot {}",
            lot.value()
        ),
    };
    assert_eq!(
        record.stockpile, source,
        "validated lot owner changed before commit"
    );
    record.stockpile = destination;
}

fn apply_split_lot(
    state: &mut InventoryState,
    source_lot: MaterialLotId,
    new_lot: MaterialLotId,
    destination: StockpileId,
    transferred: Mass,
) {
    let source_snapshot = match state.lots.get(&source_lot) {
        Some(lot) => lot.clone(),
        None => panic!(
            "validated partial transfer references missing material lot {}",
            source_lot.value()
        ),
    };
    assert!(
        transferred < source_snapshot.mass,
        "partial transfer must leave positive mass in its source lot"
    );

    let source_record = match state.lots.get_mut(&source_lot) {
        Some(lot) => lot,
        None => panic!("validated partial transfer source disappeared"),
    };
    source_record.mass = match source_record.mass.checked_sub(transferred) {
        Some(value) => value,
        None => panic!("validated partial transfer underflowed source lot mass"),
    };

    let split = MaterialLotRecord {
        id: new_lot,
        stockpile: destination,
        mass: transferred,
        profile: source_snapshot.profile.clone(),
        provenance: source_snapshot.provenance,
    };
    if let Some(existing_id) = find_compatible_lot(state, destination, &split.profile) {
        apply_merge_lot_record(state, existing_id, split);
    } else {
        apply_insert_lot_record(state, split);
    }
}

fn apply_consume_lot_slice(state: &mut InventoryState, slice: LotSlice) {
    let snapshot = match state.lots.get(&slice.lot) {
        Some(lot) => lot.clone(),
        None => panic!(
            "validated consumption references missing material lot {}",
            slice.lot.value()
        ),
    };
    if slice.mass == snapshot.mass {
        let removed = get_stockpile_mut_or_panic(state, snapshot.stockpile)
            .lot_ids
            .remove(&slice.lot);
        assert!(removed, "consumed full lot must exist in owner index");
        let removed = state.lots.remove(&slice.lot);
        assert!(
            removed.is_some(),
            "consumed full lot must exist in lot owner"
        );
    } else {
        let lot = match state.lots.get_mut(&slice.lot) {
            Some(lot) => lot,
            None => panic!("validated partial consumption source disappeared"),
        };
        lot.mass = match lot.mass.checked_sub(slice.mass) {
            Some(value) if !value.is_zero() => value,
            Some(_) => panic!("partial consumption unexpectedly reduced lot to zero"),
            None => panic!("validated partial consumption underflowed lot mass"),
        };
    }
}

fn get_stockpile_mut_or_panic(
    state: &mut InventoryState,
    stockpile: StockpileId,
) -> &mut StockpileRecord {
    match state.stockpiles.get_mut(&stockpile) {
        Some(record) => record,
        None => panic!(
            "runtime invariant broken: missing stockpile {}",
            stockpile.value()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        FORM_LOG, FORM_ORE, MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_WOOD, build_registries,
    };
    use crate::core::time::WorldSeed;
    use crate::inventory::validate_loaded_inventory;
    use crate::material::CompositionComponent;

    fn wood_log() -> CommodityKey {
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
    }

    #[test]
    fn failed_transfer_leaves_both_stockpiles_unchanged() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(1));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture stockpile failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(5)) {
            Ok(id) => id,
            Err(error) => panic!("fixture stockpile failed: {error}"),
        };
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            wood_log(),
            Mass::from_milligrams(10),
        ) {
            panic!("fixture deposit failed: {error}");
        }
        let before = state.clone();

        let result = validate_transfer_bulk(
            &registries,
            &state,
            source,
            destination,
            wood_log(),
            Mass::from_milligrams(10),
        );

        assert!(matches!(
            result,
            Err(TransferError::CapacityExceeded { .. })
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn validated_transfer_updates_cached_mass_and_contents_atomically() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(2));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture stockpile failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture stockpile failed: {error}"),
        };
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            wood_log(),
            Mass::from_milligrams(30),
        ) {
            panic!("fixture deposit failed: {error}");
        }

        let token = match validate_transfer_bulk(
            &registries,
            &state,
            source,
            destination,
            wood_log(),
            Mass::from_milligrams(12),
        ) {
            Ok(token) => token,
            Err(error) => panic!("transfer validation failed: {error}"),
        };
        if let Err(error) = token.commit(&mut state) {
            panic!("transfer commit failed: {error}");
        }

        let source_record = match state.inventory().get_stockpile(source) {
            Some(record) => record,
            None => panic!("source disappeared"),
        };
        let destination_record = match state.inventory().get_stockpile(destination) {
            Some(record) => record,
            None => panic!("destination disappeared"),
        };
        assert_eq!(source_record.stored_mass(), Mass::from_milligrams(18));
        assert_eq!(
            source_record.get_mass(wood_log()),
            Mass::from_milligrams(18)
        );
        assert_eq!(destination_record.stored_mass(), Mass::from_milligrams(12));
        assert_eq!(
            destination_record.get_mass(wood_log()),
            Mass::from_milligrams(12)
        );
        assert_eq!(validate_loaded_inventory(state.inventory()), Ok(()));
    }

    #[test]
    fn partial_transfer_splits_lots_without_erasing_thermal_history() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(3));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture source failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture destination failed: {error}"),
        };
        let cool = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            wood_log(),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(300_000),
        ) {
            Ok(id) => id,
            Err(error) => panic!("cool lot fixture failed: {error}"),
        };
        let hot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            wood_log(),
            Mass::from_milligrams(20),
            Temperature::from_millikelvin(800_000),
        ) {
            Ok(id) => id,
            Err(error) => panic!("hot lot fixture failed: {error}"),
        };

        let token = match validate_transfer_bulk(
            &registries,
            &state,
            source,
            destination,
            wood_log(),
            Mass::from_milligrams(15),
        ) {
            Ok(token) => token,
            Err(error) => panic!("split transfer validation failed: {error}"),
        };
        if let Err(error) = token.commit(&mut state) {
            panic!("split transfer commit failed: {error}");
        }

        let cool_lot = match state.inventory().get_lot(cool) {
            Some(lot) => lot,
            None => panic!("full moved cool lot disappeared"),
        };
        assert_eq!(cool_lot.stockpile(), destination);
        assert_eq!(cool_lot.mass(), Mass::from_milligrams(10));
        assert_eq!(
            cool_lot.temperature(),
            Temperature::from_millikelvin(300_000)
        );

        let hot_lot = match state.inventory().get_lot(hot) {
            Some(lot) => lot,
            None => panic!("hot source lot disappeared"),
        };
        assert_eq!(hot_lot.stockpile(), source);
        assert_eq!(hot_lot.mass(), Mass::from_milligrams(15));
        assert_eq!(
            hot_lot.temperature(),
            Temperature::from_millikelvin(800_000)
        );

        let destination_record = match state.inventory().get_stockpile(destination) {
            Some(record) => record,
            None => panic!("destination disappeared"),
        };
        let destination_lots: Vec<_> = destination_record.lot_ids().collect();
        assert_eq!(destination_lots.len(), 2);
        let split = match destination_lots.into_iter().find(|id| *id != cool) {
            Some(id) => id,
            None => panic!("split lot missing"),
        };
        let split_lot = match state.inventory().get_lot(split) {
            Some(lot) => lot,
            None => panic!("split lot record missing"),
        };
        assert_eq!(split_lot.mass(), Mass::from_milligrams(5));
        assert_eq!(
            split_lot.temperature(),
            Temperature::from_millikelvin(800_000)
        );
        assert_eq!(validate_loaded_inventory(state.inventory()), Ok(()));
    }

    #[test]
    fn stale_transfer_token_is_rejected_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(4));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture source failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture destination failed: {error}"),
        };
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            wood_log(),
            Mass::from_milligrams(20),
        ) {
            panic!("fixture deposit failed: {error}");
        }
        let token = match validate_transfer_bulk(
            &registries,
            &state,
            source,
            destination,
            wood_log(),
            Mass::from_milligrams(10),
        ) {
            Ok(token) => token,
            Err(error) => panic!("transfer validation failed: {error}"),
        };

        if let Err(error) = add_stockpile(&mut state, Mass::from_milligrams(1)) {
            panic!("intervening stockpile mutation failed: {error}");
        }
        let before_commit = state.clone();
        let result = token.commit(&mut state);

        assert!(matches!(
            result,
            Err(TransferCommitError::StaleInventoryRevision { .. })
        ));
        assert_eq!(state, before_commit);
    }

    #[test]
    fn repeated_partial_transfers_coalesce_new_fragments_in_destination() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(41));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture source failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture destination failed: {error}"),
        };
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            wood_log(),
            Mass::from_milligrams(10),
        ) {
            panic!("fixture deposit failed: {error}");
        }

        for _ in 0..2 {
            let token = match validate_transfer_bulk(
                &registries,
                &state,
                source,
                destination,
                wood_log(),
                Mass::from_milligrams(3),
            ) {
                Ok(token) => token,
                Err(error) => panic!("fragment transfer validation failed: {error}"),
            };
            if let Err(error) = token.commit(&mut state) {
                panic!("fragment transfer commit failed: {error}");
            }
        }

        let source_record = match state.inventory().get_stockpile(source) {
            Some(record) => record,
            None => panic!("source disappeared"),
        };
        let destination_record = match state.inventory().get_stockpile(destination) {
            Some(record) => record,
            None => panic!("destination disappeared"),
        };
        assert_eq!(source_record.get_mass(wood_log()), Mass::from_milligrams(4));
        assert_eq!(
            destination_record.get_mass(wood_log()),
            Mass::from_milligrams(6)
        );
        assert_eq!(destination_record.lot_ids().count(), 1);
        assert_eq!(state.inventory().lots().count(), 2);
        assert_eq!(validate_loaded_inventory(state.inventory()), Ok(()));
    }

    #[test]
    fn composed_lot_split_preserves_normalized_constituent_profile() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(5));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture source failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture destination failed: {error}"),
        };
        let composition = match MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_COPPER, 700_000),
            CompositionComponent::new(MATERIAL_SLAG, 300_000),
        ]) {
            Ok(composition) => composition,
            Err(error) => panic!("composition fixture failed: {error}"),
        };
        let commodity = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
        let original = match deposit_composed_lot_for_test(
            &registries,
            &mut state,
            source,
            commodity,
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(400_000),
            composition.clone(),
        ) {
            Ok(id) => id,
            Err(error) => panic!("composed lot fixture failed: {error}"),
        };

        let token = match validate_transfer_bulk(
            &registries,
            &state,
            source,
            destination,
            commodity,
            Mass::from_milligrams(4),
        ) {
            Ok(token) => token,
            Err(error) => panic!("composed split validation failed: {error}"),
        };
        if let Err(error) = token.commit(&mut state) {
            panic!("composed split commit failed: {error}");
        }

        let source_lot = match state.inventory().get_lot(original) {
            Some(lot) => lot,
            None => panic!("source composition lot disappeared"),
        };
        assert_eq!(source_lot.mass(), Mass::from_milligrams(6));
        assert_eq!(source_lot.composition(), &composition);
        let destination_record = match state.inventory().get_stockpile(destination) {
            Some(record) => record,
            None => panic!("destination disappeared"),
        };
        let split_id = match destination_record.lot_ids().next() {
            Some(id) => id,
            None => panic!("destination split lot missing"),
        };
        let split = match state.inventory().get_lot(split_id) {
            Some(lot) => lot,
            None => panic!("destination split lot record missing"),
        };
        assert_eq!(split.mass(), Mass::from_milligrams(4));
        assert_eq!(split.composition(), &composition);
        assert_eq!(
            split.composition().parts_per_million(MATERIAL_COPPER),
            700_000
        );
        assert_eq!(
            split.composition().parts_per_million(MATERIAL_SLAG),
            300_000
        );
        assert_eq!(validate_loaded_inventory(state.inventory()), Ok(()));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommodityReferenceError {
    UnknownMaterial { material: MaterialId },
    UnknownForm { form: FormId },
}

fn validate_commodity(
    registries: &Registries,
    commodity: CommodityKey,
) -> Result<(), CommodityReferenceError> {
    if registries
        .materials()
        .get_material(commodity.material())
        .is_none()
    {
        return Err(CommodityReferenceError::UnknownMaterial {
            material: commodity.material(),
        });
    }
    if registries.materials().get_form(commodity.form()).is_none() {
        return Err(CommodityReferenceError::UnknownForm {
            form: commodity.form(),
        });
    }
    Ok(())
}
