//! Canonical inventory transactions; sibling state records remain passive and privately mutable.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
#[cfg(test)]
use crate::core::quantity::Temperature;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, CompositionError, FormId, MaterialId, MaterialInputSpec, MaterialLotSpec,
};
#[cfg(test)]
use crate::material::{MaterialComposition, MaterialPhase};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

#[cfg(test)]
use super::lot_mutation::apply_insert_lot;
use super::lot_mutation::{
    LotSlice, apply_aggregate_deposit, apply_aggregate_withdraw, apply_consume_lot_slice,
    apply_insert_or_merge_new_lot, apply_move_full_lot, apply_split_lot,
};
use super::selection::ConsumptionSelection;
#[cfg(test)]
use super::selection::{
    apply_consumption_reservation, apply_reserved_deposit,
    validate_consumption_reservation_from_selection, validate_consumption_selection,
};
use super::state::{
    ConsumedMaterialTrace, InventoryState, MaterialLotId, MaterialLotProfile,
    MaterialLotProvenance, MaterialLotRecord, StockpileId, StockpileRecord,
    StockpileStorageProfile,
};
use super::storage_validation::{StockpileStorageError, validate_stockpile_storage};
use super::{
    StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedStockpileStructuralLoad,
    validate_stockpile_stored_mass_changes,
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
    Storage(StockpileStorageError),
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

/// Revision-bound relocation of one already-resolved exact lot selection between stockpiles.
///
/// Cross-owner systems use this when a physical resolver inspected specific lot slices before
/// deciding a consequence. The relocation preserves selected profiles and provenance exactly and
/// never substitutes equivalent-looking inventory at validation time.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedMaterialRelocation {
    expected_revision: u64,
    next_revision: u64,
    source: StockpileId,
    destination: StockpileId,
    inputs: Vec<MaterialInputSpec>,
    lot_slices: Vec<LotSlice>,
    split_lot_ids: Vec<Option<MaterialLotId>>,
    next_lot_id_after: Option<u64>,
    total_mass: Mass,
    structural: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedMaterialRelocation {
    pub(crate) const fn total_mass(&self) -> Mass {
        self.total_mass
    }

    pub(crate) fn commit(self, state: &mut AppState) -> Result<(), MaterialRelocationCommitError> {
        let actual = state.inventory_state().revision();
        if actual != self.expected_revision {
            return Err(MaterialRelocationCommitError::StaleInventoryRevision {
                expected: self.expected_revision,
                actual,
            });
        }
        if let Some(structural) = self.structural {
            structural
                .commit(state)
                .map_err(MaterialRelocationCommitError::Structure)?;
        }

        let inventories = state.inventory_state_mut();
        for input in &self.inputs {
            apply_aggregate_withdraw(inventories, self.source, input.commodity(), input.mass());
            apply_aggregate_deposit(
                inventories,
                self.destination,
                input.commodity(),
                input.mass(),
            );
        }
        for (slice, split_lot_id) in self.lot_slices.into_iter().zip(self.split_lot_ids) {
            let lot_mass = match inventories.lots.get(&slice.lot) {
                Some(lot) => lot.mass,
                None => panic!(
                    "validated material relocation references missing lot {}",
                    slice.lot.value()
                ),
            };
            if slice.mass == lot_mass {
                debug_assert!(split_lot_id.is_none());
                apply_move_full_lot(inventories, slice.lot, self.source, self.destination);
            } else {
                let split_lot_id = match split_lot_id {
                    Some(split_lot_id) => split_lot_id,
                    None => panic!(
                        "validated partial material relocation is missing an allocated lot id"
                    ),
                };
                apply_split_lot(
                    inventories,
                    slice.lot,
                    split_lot_id,
                    self.destination,
                    slice.mass,
                );
            }
        }
        if let Some(next_lot_id) = self.next_lot_id_after {
            inventories.next_lot_id = next_lot_id;
        }
        inventories.revision = self.next_revision;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialRelocationError {
    StaleSelection {
        expected: u64,
        actual: u64,
    },
    UnknownSource {
        stockpile: StockpileId,
    },
    UnknownDestination {
        stockpile: StockpileId,
    },
    SameStockpile {
        stockpile: StockpileId,
    },
    DestinationStorage(StockpileStorageError),
    DestinationMassOverflow {
        stockpile: StockpileId,
    },
    DestinationCapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    RevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for MaterialRelocationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSelection { expected, actual } => write!(
                formatter,
                "exact material relocation expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "exact material relocation source stockpile {} does not exist",
                stockpile.value()
            ),
            Self::UnknownDestination { stockpile } => write!(
                formatter,
                "exact material relocation destination stockpile {} does not exist",
                stockpile.value()
            ),
            Self::SameStockpile { stockpile } => write!(
                formatter,
                "exact material relocation requires distinct source and destination; both are stockpile {}",
                stockpile.value()
            ),
            Self::DestinationStorage(error) => write!(
                formatter,
                "exact material relocation destination rejects selected matter: {error}"
            ),
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "exact material relocation overflows destination stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "exact material relocation exceeds stockpile {} capacity {} mg: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => formatter
                .write_str("material lot identifier space is exhausted during exact relocation"),
            Self::RevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted during exact relocation")
            }
            Self::StructuralLoad(error) => {
                write!(
                    formatter,
                    "exact material relocation structural load failed: {error}"
                )
            }
        }
    }
}

impl Error for MaterialRelocationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DestinationStorage(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::StaleSelection { .. }
            | Self::UnknownSource { .. }
            | Self::UnknownDestination { .. }
            | Self::SameStockpile { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::LotIdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialRelocationCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for MaterialRelocationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated material relocation expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => {
                write!(
                    formatter,
                    "material relocation structural commit failed: {error}"
                )
            }
        }
    }
}

impl Error for MaterialRelocationCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventoryRevision { .. } => None,
        }
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
        validate_stockpile_storage(
            registries,
            destination_record,
            destination,
            profile.commodity(),
            profile.composition(),
            profile.temperature(),
            profile.particle_size_distribution(),
        )
        .map_err(MaterialBatchIngressError::Storage)?;
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
    Storage(StockpileStorageError),
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
    StructuralLoad(StockpileStructuralLoadError),
    StructuralCommit(StructuralCommitError),
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
            Self::Storage(error) => write!(formatter, "stockpile rejects deposit: {error}"),
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
            Self::StructuralLoad(error) => {
                write!(
                    formatter,
                    "deposit cannot update stored-matter support load: {error}"
                )
            }
            Self::StructuralCommit(error) => write!(
                formatter,
                "deposit could not commit stored-matter structural load: {error}"
            ),
        }
    }
}

impl Error for DepositError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::StructuralCommit(error) => Some(error),
            Self::UnknownStockpile { .. }
            | Self::UnknownMaterial { .. }
            | Self::UnknownForm { .. }
            | Self::ZeroMass
            | Self::MassOverflow { .. }
            | Self::CapacityExceeded { .. }
            | Self::LotIdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}

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
    Storage(StockpileStorageError),
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
            Self::Storage(error) => {
                write!(formatter, "stockpile rejects material ingress: {error}")
            }
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
            Self::Storage(error) => Some(error),
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
    SameStockpile {
        stockpile: StockpileId,
    },
    UnknownMaterial {
        material: MaterialId,
    },
    UnknownForm {
        form: FormId,
    },
    ZeroMass,
    Storage(StockpileStorageError),
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
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for TransferError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
            Self::SameStockpile { stockpile } => write!(
                formatter,
                "inventory transfer requires distinct source and destination; both are stockpile {}",
                stockpile.value()
            ),
            Self::UnknownMaterial { material } => {
                write!(formatter, "unknown material id {}", material.value())
            }
            Self::UnknownForm { form } => write!(formatter, "unknown form id {}", form.value()),
            Self::ZeroMass => formatter.write_str("transfer mass must be nonzero"),
            Self::Storage(error) => write!(formatter, "destination rejects transfer: {error}"),
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
            Self::StructuralLoad(error) => {
                write!(
                    formatter,
                    "transfer cannot update stored-matter support load: {error}"
                )
            }
        }
    }
}

impl Error for TransferError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownStockpile { .. }
            | Self::SameStockpile { .. }
            | Self::UnknownMaterial { .. }
            | Self::UnknownForm { .. }
            | Self::ZeroMass
            | Self::InsufficientMass { .. }
            | Self::MassOverflow { .. }
            | Self::CapacityExceeded { .. }
            | Self::LotIdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}

/// Failure when a previously validated transfer is committed after inventory has changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for TransferCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated transfer expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "validated transfer could not commit stored-matter structural load: {error}"
            ),
        }
    }
}

impl Error for TransferCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventoryRevision { .. } => None,
        }
    }
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
    structural: Option<ValidatedStockpileStructuralLoad>,
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
            structural,
        } = self;

        let actual_inventory_revision = state.inventory_state().revision();
        if actual_inventory_revision != expected_revision {
            return Err(TransferCommitError::StaleInventoryRevision {
                expected: expected_revision,
                actual: actual_inventory_revision,
            });
        }
        if let Some(structural) = structural {
            structural
                .commit(state)
                .map_err(TransferCommitError::Structure)?;
        }

        let inventories = state.inventory_state_mut();

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

/// Adds empty material storage with an explicit phase and temperature containment envelope.
pub fn add_stockpile(
    state: &mut AppState,
    capacity: Mass,
    storage_profile: StockpileStorageProfile,
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
        storage_profile,
        supported_by: None,
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

#[cfg(test)]
pub(crate) fn add_solid_stockpile_for_test(
    state: &mut AppState,
    capacity: Mass,
) -> Result<StockpileId, AddStockpileError> {
    add_stockpile(state, capacity, StockpileStorageProfile::solid_only())
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
    validate_stockpile_storage(
        registries,
        destination_record,
        destination,
        output.commodity(),
        output.composition(),
        output.temperature(),
        output.particle_size_distribution(),
    )
    .map_err(MaterialIngressError::Storage)?;
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
                particle_size: output.particle_size_distribution().cloned(),
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
    if mass.is_zero() {
        return Err(DepositError::ZeroMass);
    }
    let specification =
        match MaterialLotSpec::with_composition(commodity, mass, temperature, composition) {
            Ok(specification) => specification,
            Err(error) => panic!("validated test material lot could not be specified: {error}"),
        };
    deposit_lot_spec_for_test(registries, state, stockpile, specification)
}

/// Seeds one already-validated lot specification through the normal inventory storage checks.
#[cfg(test)]
pub(crate) fn deposit_lot_spec_for_test(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    specification: MaterialLotSpec,
) -> Result<MaterialLotId, DepositError> {
    let commodity = specification.commodity();
    let mass = specification.mass();
    let temperature = specification.temperature();
    let composition = specification.composition().clone();
    let particle_size = specification.particle_size_distribution().cloned();
    validate_commodity(registries, commodity).map_err(|error| match error {
        CommodityReferenceError::UnknownMaterial { material } => {
            DepositError::UnknownMaterial { material }
        }
        CommodityReferenceError::UnknownForm { form } => DepositError::UnknownForm { form },
    })?;
    let inventories = state.inventory_state();
    let Some(record) = inventories.get_stockpile(stockpile) else {
        return Err(DepositError::UnknownStockpile { stockpile });
    };
    validate_stockpile_storage(
        registries,
        record,
        stockpile,
        commodity,
        &composition,
        temperature,
        particle_size.as_ref(),
    )
    .map_err(DepositError::Storage)?;
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
    let stored_after = record
        .stored_mass()
        .checked_add(mass)
        .ok_or(DepositError::MassOverflow { stockpile })?;
    let structural = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(stockpile, stored_after)],
    )
    .map_err(DepositError::StructuralLoad)?;
    if let Some(structural) = structural {
        structural
            .commit(state)
            .map_err(DepositError::StructuralCommit)?;
    }

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
                particle_size,
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

    if source == destination {
        return Err(TransferError::SameStockpile { stockpile: source });
    }

    let slices = select_lot_slices(inventories, source_record, commodity, mass);
    for slice in &slices {
        let lot = match inventories.lots.get(&slice.lot) {
            Some(lot) => lot,
            None => panic!("validated transfer source lot disappeared during validation"),
        };
        validate_stockpile_storage(
            registries,
            destination_record,
            destination,
            lot.commodity(),
            lot.composition(),
            lot.temperature(),
            lot.particle_size_distribution(),
        )
        .map_err(TransferError::Storage)?;
    }
    validate_destination_capacity(destination_record, destination, mass)?;
    destination_record
        .get_mass(commodity)
        .checked_add(mass)
        .ok_or(TransferError::MassOverflow {
            stockpile: destination,
        })?;

    let Some(next_revision) = inventories.revision.checked_add(1) else {
        return Err(TransferError::RevisionExhausted);
    };
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
    let source_after =
        source_record
            .stored_mass()
            .checked_sub(mass)
            .ok_or(TransferError::InsufficientMass {
                stockpile: source,
                commodity,
                available: source_record.stored_mass(),
                requested: mass,
            })?;
    let destination_after =
        destination_record
            .stored_mass()
            .checked_add(mass)
            .ok_or(TransferError::MassOverflow {
                stockpile: destination,
            })?;
    let structural = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [
            StockpileStoredMassChange::new(source, source_after),
            StockpileStoredMassChange::new(destination, destination_after),
        ],
    )
    .map_err(TransferError::StructuralLoad)?;

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
        structural,
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

/// Validates relocation of the exact lot slices already bound by a physical resolver.
pub(crate) fn validate_material_relocation_from_selection(
    registries: &Registries,
    state: &AppState,
    destination: StockpileId,
    selection: ConsumptionSelection,
) -> Result<ValidatedMaterialRelocation, MaterialRelocationError> {
    let ConsumptionSelection {
        expected_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs,
        total_consumed,
    } = selection;
    let inventories = state.inventory_state();
    if inventories.revision != expected_revision {
        return Err(MaterialRelocationError::StaleSelection {
            expected: expected_revision,
            actual: inventories.revision,
        });
    }
    let source_record = inventories
        .get_stockpile(source)
        .ok_or(MaterialRelocationError::UnknownSource { stockpile: source })?;
    let destination_record = inventories.get_stockpile(destination).ok_or(
        MaterialRelocationError::UnknownDestination {
            stockpile: destination,
        },
    )?;
    if source == destination {
        return Err(MaterialRelocationError::SameStockpile { stockpile: source });
    }

    for trace in &consumed_inputs {
        validate_stockpile_storage(
            registries,
            destination_record,
            destination,
            trace.profile().commodity(),
            trace.profile().composition(),
            trace.profile().temperature(),
            trace.profile().particle_size_distribution(),
        )
        .map_err(MaterialRelocationError::DestinationStorage)?;
    }
    let committed = destination_record
        .stored_mass
        .checked_add(destination_record.reserved_inbound)
        .ok_or(MaterialRelocationError::DestinationMassOverflow {
            stockpile: destination,
        })?;
    let capacity_after = committed.checked_add(total_consumed).ok_or(
        MaterialRelocationError::DestinationMassOverflow {
            stockpile: destination,
        },
    )?;
    if capacity_after > destination_record.capacity {
        return Err(MaterialRelocationError::DestinationCapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity,
            committed,
            requested: total_consumed,
        });
    }
    let destination_stored_after = destination_record
        .stored_mass()
        .checked_add(total_consumed)
        .ok_or(MaterialRelocationError::DestinationMassOverflow {
            stockpile: destination,
        })?;
    for input in &inputs {
        destination_record
            .get_mass(input.commodity())
            .checked_add(input.mass())
            .ok_or(MaterialRelocationError::DestinationMassOverflow {
                stockpile: destination,
            })?;
    }

    let source_after = source_record
        .stored_mass()
        .checked_sub(total_consumed)
        .ok_or(MaterialRelocationError::StaleSelection {
            expected: expected_revision,
            actual: inventories.revision,
        })?;
    let structural = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [
            StockpileStoredMassChange::new(source, source_after),
            StockpileStoredMassChange::new(destination, destination_stored_after),
        ],
    )
    .map_err(MaterialRelocationError::StructuralLoad)?;

    let mut split_lot_ids = Vec::with_capacity(lot_slices.len());
    let mut next_lot_id = inventories.next_lot_id;
    let mut allocated_any = false;
    for slice in &lot_slices {
        let lot = match inventories.lots.get(&slice.lot) {
            Some(lot) => lot,
            None => panic!(
                "validated exact selection references missing lot {}",
                slice.lot.value()
            ),
        };
        if slice.mass == lot.mass {
            split_lot_ids.push(None);
        } else {
            split_lot_ids.push(Some(MaterialLotId::new(next_lot_id)));
            next_lot_id = next_lot_id
                .checked_add(1)
                .ok_or(MaterialRelocationError::LotIdExhausted)?;
            allocated_any = true;
        }
    }
    let next_lot_id_after = allocated_any.then_some(next_lot_id);
    let next_revision = inventories
        .revision
        .checked_add(1)
        .ok_or(MaterialRelocationError::RevisionExhausted)?;

    Ok(ValidatedMaterialRelocation {
        expected_revision,
        next_revision,
        source,
        destination,
        inputs,
        lot_slices,
        split_lot_ids,
        next_lot_id_after,
        total_mass: total_consumed,
        structural,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        FORM_LOG, FORM_LUMP, FORM_MOLTEN, FORM_ORE, MATERIAL_CHARCOAL, MATERIAL_COPPER,
        MATERIAL_SLAG, MATERIAL_WOOD, build_registries,
    };
    use crate::core::time::WorldSeed;
    use crate::inventory::{add_solid_stockpile_for_test, validate_loaded_inventory};
    use crate::material::CompositionComponent;
    use crate::matter::calculate_matter_accounting;

    fn wood_log() -> CommodityKey {
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
    }

    #[test]
    fn default_stockpile_rejects_liquid_material_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_1001));
        let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("solid stockpile fixture failed: {error}"),
        };
        let before = state.clone();

        let result = deposit_lot_for_test(
            &registries,
            &mut state,
            stockpile,
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(1_357_770),
        );

        assert_eq!(
            result,
            Err(DepositError::Storage(
                StockpileStorageError::PhaseNotAccepted {
                    stockpile,
                    phase: MaterialPhase::Liquid,
                }
            ))
        );
        assert_eq!(state, before);
    }

    #[test]
    fn liquid_storage_accepts_matching_phase_but_enforces_temperature_limit() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_1002));
        let maximum = Temperature::from_millikelvin(1_400_000);
        let profile = match StockpileStorageProfile::new(false, true, maximum) {
            Ok(profile) => profile,
            Err(error) => panic!("liquid storage profile fixture failed: {error}"),
        };
        let vessel = match add_stockpile(&mut state, Mass::from_milligrams(100), profile) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("liquid storage fixture failed: {error}"),
        };

        if let Err(error) = deposit_lot_for_test(
            &registries,
            &mut state,
            vessel,
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(1_357_770),
        ) {
            panic!("valid molten deposit was rejected: {error}");
        }
        let before_hot_rejection = state.clone();
        let too_hot = Temperature::from_millikelvin(1_500_000);
        assert_eq!(
            deposit_lot_for_test(
                &registries,
                &mut state,
                vessel,
                CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
                Mass::from_milligrams(1),
                too_hot,
            ),
            Err(DepositError::Storage(
                StockpileStorageError::TemperatureExceedsMaximum {
                    stockpile: vessel,
                    temperature: too_hot,
                    maximum,
                }
            ))
        );
        assert_eq!(state, before_hot_rejection);
        assert_eq!(
            validate_loaded_inventory(registries.materials(), state.inventory()),
            Ok(())
        );
    }

    #[test]
    fn transfer_rechecks_destination_containment_for_actual_selected_lots() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_1003));
        let source_profile = match StockpileStorageProfile::new(
            false,
            true,
            Temperature::from_millikelvin(2_000_000),
        ) {
            Ok(profile) => profile,
            Err(error) => panic!("source vessel profile failed: {error}"),
        };
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100), source_profile) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("source vessel failed: {error}"),
        };
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("destination pile failed: {error}"),
        };
        if let Err(error) = deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(1_357_770),
        ) {
            panic!("molten transfer source fixture failed: {error}");
        }
        let before = state.clone();

        assert_eq!(
            validate_transfer_bulk(
                &registries,
                &state,
                source,
                destination,
                CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
                Mass::from_milligrams(5),
            ),
            Err(TransferError::Storage(
                StockpileStorageError::PhaseNotAccepted {
                    stockpile: destination,
                    phase: MaterialPhase::Liquid,
                }
            ))
        );
        assert_eq!(state, before);
    }

    #[test]
    fn failed_transfer_leaves_both_stockpiles_unchanged() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(1));
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture stockpile failed: {error}"),
        };
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5)) {
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
    fn same_stockpile_transfer_is_rejected_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(11));
        let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture stockpile failed: {error}"),
        };
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            stockpile,
            wood_log(),
            Mass::from_milligrams(10),
        ) {
            panic!("fixture deposit failed: {error}");
        }
        let before = state.clone();

        assert_eq!(
            validate_transfer_bulk(
                &registries,
                &state,
                stockpile,
                stockpile,
                wood_log(),
                Mass::from_milligrams(5),
            ),
            Err(TransferError::SameStockpile { stockpile })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn validated_transfer_updates_cached_mass_and_contents_atomically() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(2));
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture stockpile failed: {error}"),
        };
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        {
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
        assert_eq!(
            validate_loaded_inventory(registries.materials(), state.inventory()),
            Ok(())
        );
    }

    #[test]
    fn partial_transfer_splits_lots_without_erasing_thermal_history() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(3));
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture source failed: {error}"),
        };
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        {
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
        assert_eq!(
            validate_loaded_inventory(registries.materials(), state.inventory()),
            Ok(())
        );
    }

    #[test]
    fn stale_transfer_token_is_rejected_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(4));
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture source failed: {error}"),
        };
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        {
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

        if let Err(error) = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
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
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture source failed: {error}"),
        };
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        {
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
        assert_eq!(
            validate_loaded_inventory(registries.materials(), state.inventory()),
            Ok(())
        );
    }

    #[test]
    fn composed_lot_split_preserves_normalized_constituent_profile() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(5));
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture source failed: {error}"),
        };
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        {
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
        assert_eq!(
            validate_loaded_inventory(registries.materials(), state.inventory()),
            Ok(())
        );
    }

    fn stored_lot_total(state: &AppState) -> Mass {
        state.inventory().lots().fold(Mass::ZERO, |acc, lot| {
            acc.checked_add(lot.mass())
                .unwrap_or_else(|| panic!("conservation test overflow"))
        })
    }

    fn stored_aggregate_total(state: &AppState) -> Mass {
        state
            .inventory()
            .stockpiles()
            .fold(Mass::ZERO, |acc, pile| {
                acc.checked_add(pile.stored_mass())
                    .unwrap_or_else(|| panic!("conservation test overflow"))
            })
    }

    fn assert_lot_aggregate_agreement(registries: &Registries, state: &AppState, label: &str) {
        assert_eq!(
            stored_lot_total(state),
            stored_aggregate_total(state),
            "{label}: lot total disagrees with stockpile aggregate total"
        );
        assert_eq!(
            validate_loaded_inventory(registries.materials(), state.inventory()),
            Ok(())
        );
    }

    #[test]
    fn transfer_split_sequence_preserves_inventory_quantity() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_2001));
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("source fixture failed: {error}"),
        };
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        {
            Ok(id) => id,
            Err(error) => panic!("destination fixture failed: {error}"),
        };
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            wood_log(),
            Mass::from_milligrams(10),
        ) {
            panic!("transfer source deposit failed: {error}");
        }
        let before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("initial accounting failed: {error:?}"))
            .total();

        for requested in [
            Mass::from_milligrams(3),
            Mass::from_milligrams(4),
            Mass::from_milligrams(3),
        ] {
            let token = validate_transfer_bulk(
                &registries,
                &state,
                source,
                destination,
                wood_log(),
                requested,
            )
            .unwrap_or_else(|error| panic!("transfer validation failed: {error}"));
            token
                .commit(&mut state)
                .unwrap_or_else(|error| panic!("transfer commit failed: {error}"));
            assert_eq!(
                calculate_matter_accounting(&state)
                    .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
                    .total(),
                before,
                "partial transfer must conserve world matter"
            );
            assert_lot_aggregate_agreement(&registries, &state, "after partial transfer");
        }

        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
                .stored_mass(),
            Mass::ZERO
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
                .stored_mass(),
            Mass::from_milligrams(10)
        );
        assert_lot_aggregate_agreement(&registries, &state, "after transfer sequence");
    }

    #[test]
    fn stale_transfer_commit_leaves_matter_accounting_unchanged() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_2002));
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("source fixture failed: {error}"),
        };
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5)) {
            Ok(id) => id,
            Err(error) => panic!("small destination fixture failed: {error}"),
        };
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            wood_log(),
            Mass::from_milligrams(10),
        ) {
            panic!("transfer source deposit failed: {error}");
        }
        let before = state.clone();
        let before_total = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
            .total();

        assert!(
            validate_transfer_bulk(
                &registries,
                &state,
                source,
                destination,
                wood_log(),
                Mass::from_milligrams(11),
            )
            .is_err(),
            "over-available transfer must fail validation"
        );
        assert!(
            validate_transfer_bulk(
                &registries,
                &state,
                source,
                destination,
                wood_log(),
                Mass::from_milligrams(9),
            )
            .is_err(),
            "over-capacity transfer must fail validation"
        );
        assert_eq!(state, before, "failed validation must not mutate inventory");

        let valid = validate_transfer_bulk(
            &registries,
            &state,
            source,
            destination,
            wood_log(),
            Mass::from_milligrams(4),
        )
        .unwrap_or_else(|error| panic!("valid transfer validation failed: {error}"));
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(50))
            .unwrap_or_else(|error| panic!("revision bump failed: {error}"));
        let result = valid.commit(&mut state);
        assert!(
            matches!(
                result,
                Err(TransferCommitError::StaleInventoryRevision { .. })
            ),
            "stale transfer commit must be rejected: {result:?}"
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
                .total(),
            before_total,
            "stale commit must not change world matter"
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
                .stored_mass(),
            Mass::from_milligrams(10),
            "stale commit must not withdraw from source"
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
                .stored_mass(),
            Mass::ZERO,
            "stale commit must not deposit into destination"
        );
        assert_lot_aggregate_agreement(&registries, &state, "after stale commit");
    }

    #[test]
    fn consumption_reservation_and_reserved_deposit_preserve_final_quantity() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_2003));
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("source fixture failed: {error}"),
        };
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        {
            Ok(id) => id,
            Err(error) => panic!("destination fixture failed: {error}"),
        };
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            wood_log(),
            Mass::from_milligrams(10),
        ) {
            panic!("reservation source deposit failed: {error}");
        }
        let before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
            .total();

        let inputs = vec![MaterialInputSpec::new(
            wood_log(),
            Mass::from_milligrams(10),
        )];
        let selection = validate_consumption_selection(state.inventory_state(), source, &inputs)
            .unwrap_or_else(|error| panic!("selection failed: {error:?}"));
        assert_eq!(
            selection.total_consumed(),
            Mass::from_milligrams(10),
            "selection must bind exactly the requested input mass"
        );
        let mut inbound_by_destination = BTreeMap::new();
        inbound_by_destination.insert(destination, Mass::from_milligrams(10));
        let reservation = validate_consumption_reservation_from_selection(
            state.inventory_state(),
            selection,
            inbound_by_destination,
        )
        .unwrap_or_else(|error| panic!("reservation failed: {error:?}"));
        apply_consumption_reservation(state.inventory_state_mut(), reservation)
            .unwrap_or_else(|error| panic!("reservation commit failed: {error:?}"));
        assert_lot_aggregate_agreement(&registries, &state, "after reservation");
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
                .stored_mass(),
            Mass::ZERO,
            "consumption must drain the source"
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
                .reserved_inbound(),
            Mass::from_milligrams(10),
            "reserved inbound must reflect the incoming output mass"
        );

        let output = MaterialLotSpec::new(
            CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(500_000),
        );
        let lot_id = next_material_lot_id(state.inventory_state());
        let created_at = state.tick();
        apply_reserved_deposit(
            state.inventory_state_mut(),
            destination,
            &[output],
            &[MaterialLotId::new(lot_id)],
            Mass::from_milligrams(10),
            created_at,
        );
        let current_revision = state.inventory_state().revision();
        apply_lot_cursor_and_revision(state.inventory_state_mut(), lot_id + 1, current_revision);
        assert_lot_aggregate_agreement(&registries, &state, "after reserved deposit");
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
                .reserved_inbound(),
            Mass::ZERO,
            "reserved inbound must be consumed by the deposit"
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
                .stored_mass(),
            Mass::from_milligrams(10),
            "deposit must land the output mass in stored inventory"
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
                .total(),
            before,
            "reserved deposit must not change world matter"
        );
    }

    #[test]
    fn egress_and_batch_ingress_round_trip_preserves_exact_quantity() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_2004));
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("source fixture failed: {error}"),
        };
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        {
            Ok(id) => id,
            Err(error) => panic!("destination fixture failed: {error}"),
        };
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            wood_log(),
            Mass::from_milligrams(10),
        ) {
            panic!("egress source deposit failed: {error}");
        }
        let before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
            .total();

        let inputs = vec![MaterialInputSpec::new(wood_log(), Mass::from_milligrams(7))];
        let selection = validate_consumption_selection(state.inventory_state(), source, &inputs)
            .unwrap_or_else(|error| panic!("selection failed: {error:?}"));
        let egress = validate_material_egress_from_selection(state.inventory_state(), selection)
            .unwrap_or_else(|error| panic!("egress failed: {error:?}"));
        assert_eq!(egress.total_consumed(), Mass::from_milligrams(7));
        let traces = egress.consumed_inputs().to_vec();
        apply_material_egress(state.inventory_state_mut(), egress);
        assert_lot_aggregate_agreement(&registries, &state, "after egress");
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
                .stored_mass(),
            Mass::from_milligrams(3),
            "egress must remove exactly the selected mass"
        );

        let ingress = validate_material_batch_ingress(
            &registries,
            state.inventory_state(),
            destination,
            &traces,
            state.tick(),
        )
        .unwrap_or_else(|error| panic!("batch ingress failed: {error:?}"));
        apply_material_batch_ingress(state.inventory_state_mut(), ingress);
        assert_lot_aggregate_agreement(&registries, &state, "after batch ingress");
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
                .stored_mass(),
            Mass::from_milligrams(7),
            "batch ingress must restore exactly the egressed mass"
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
                .total(),
            before,
            "egress plus batch ingress round trip must conserve world matter"
        );
    }

    #[test]
    fn exact_relocation_preserves_inventory_quantity() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_2005));
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("source fixture failed: {error}"),
        };
        let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        {
            Ok(id) => id,
            Err(error) => panic!("destination fixture failed: {error}"),
        };
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            wood_log(),
            Mass::from_milligrams(10),
        ) {
            panic!("relocation source deposit failed: {error}");
        }
        let before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
            .total();

        let inputs = vec![MaterialInputSpec::new(wood_log(), Mass::from_milligrams(6))];
        let selection = validate_consumption_selection(state.inventory_state(), source, &inputs)
            .unwrap_or_else(|error| panic!("selection failed: {error:?}"));
        let relocation = validate_material_relocation_from_selection(
            &registries,
            &state,
            destination,
            selection,
        )
        .unwrap_or_else(|error| panic!("relocation failed: {error:?}"));
        assert_eq!(relocation.total_mass(), Mass::from_milligrams(6));
        relocation
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("relocation commit failed: {error:?}"));
        assert_lot_aggregate_agreement(&registries, &state, "after relocation");
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
                .stored_mass(),
            Mass::from_milligrams(4),
            "relocation must leave the unselected mass in source"
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
                .stored_mass(),
            Mass::from_milligrams(6),
            "relocation must land the selected mass in destination"
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
                .total(),
            before,
            "relocation must conserve world matter"
        );
    }

    #[test]
    fn randomized_complete_transaction_sequence_conserves_inventory_quantity() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_2006));
        let a = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(500))
            .unwrap_or_else(|error| panic!("pile a allocation failed: {error}"));
        let b = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(500))
            .unwrap_or_else(|error| panic!("pile b allocation failed: {error}"));
        let c = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(500))
            .unwrap_or_else(|error| panic!("pile c allocation failed: {error}"));
        for (pile, amount) in [(a, 100), (b, 60), (c, 40)] {
            deposit_bulk_for_test(
                &registries,
                &mut state,
                pile,
                wood_log(),
                Mass::from_milligrams(amount),
            )
            .unwrap_or_else(|error| panic!("seed deposit failed: {error}"));
        }
        let initial = stored_aggregate_total(&state);
        assert_eq!(initial, Mass::from_milligrams(200));

        let mut seed = 0xD00D_2026u64;
        for step in 1..=400 {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let choice = (seed >> 32) % 3;
            let source = [a, b, c][((seed >> 24) % 3) as usize];
            let destination = [a, b, c][((seed >> 16) % 3) as usize];
            let requested = Mass::from_milligrams(1 + ((seed >> 8) % 20));
            let mut moved = false;

            if source == destination {
                continue;
            }

            match choice {
                0 => {
                    if let Ok(validated) = validate_transfer_bulk(
                        &registries,
                        &state,
                        source,
                        destination,
                        wood_log(),
                        requested,
                    ) {
                        validated.commit(&mut state).unwrap_or_else(|error| {
                            panic!("random transfer commit failed: {error}")
                        });
                        moved = true;
                    }
                }
                1 => {
                    let inputs = vec![MaterialInputSpec::new(wood_log(), requested)];
                    if let Ok(selection) =
                        validate_consumption_selection(state.inventory_state(), source, &inputs)
                        && let Ok(relocation) = validate_material_relocation_from_selection(
                            &registries,
                            &state,
                            destination,
                            selection,
                        )
                    {
                        relocation.commit(&mut state).unwrap_or_else(|error| {
                            panic!("random relocation commit failed: {error:?}")
                        });
                        moved = true;
                    }
                }
                2 => {
                    let inputs = vec![MaterialInputSpec::new(wood_log(), requested)];
                    if let Ok(selection) =
                        validate_consumption_selection(state.inventory_state(), source, &inputs)
                    {
                        let egress = validate_material_egress_from_selection(
                            state.inventory_state(),
                            selection,
                        )
                        .unwrap_or_else(|error| {
                            panic!("random egress validation failed: {error:?}")
                        });
                        let traces = egress.consumed_inputs().to_vec();
                        apply_material_egress(state.inventory_state_mut(), egress);
                        let ingress = validate_material_batch_ingress(
                            &registries,
                            state.inventory_state(),
                            destination,
                            &traces,
                            state.tick(),
                        )
                        .unwrap_or_else(|error| {
                            panic!("random ingress validation failed: {error:?}")
                        });
                        apply_material_batch_ingress(state.inventory_state_mut(), ingress);
                        moved = true;
                    }
                }
                _ => unreachable!("three-way randomized transaction choice"),
            }

            if moved || step % 5 == 0 {
                assert_eq!(
                    stored_aggregate_total(&state),
                    initial,
                    "step {step}: complete inventory transaction changed total stored matter"
                );
                assert_lot_aggregate_agreement(&registries, &state, &format!("step {step}"));
            }
        }
        assert_lot_aggregate_agreement(&registries, &state, "randomized sequence end");
    }
}
