//! Canonical inventory transactions; sibling state records remain passive and privately mutable.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
#[cfg(test)]
use crate::core::quantity::Temperature;
use crate::core::state::AppState;
#[cfg(test)]
use crate::material::MaterialComposition;
#[cfg(test)]
use crate::material::MaterialLotSpec;
#[cfg(test)]
use crate::material::MaterialPhase;
use crate::material::{CommodityKey, FormId, MaterialId, MaterialInputSpec};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::coalescing::LotMergePolicy;
#[cfg(test)]
use super::ingress::{
    MaterialIngressEntry, MaterialIngressError, apply_material_ingress, validate_material_ingress,
};
#[cfg(test)]
use super::reserved_ingress::{
    ReservedDepositRequest, apply_reserved_deposits, decide_reserved_deposits,
};
use super::selection::{
    ConsumptionSelection, ConsumptionSelectionError, validate_consumption_selection,
};
#[cfg(test)]
use super::selection::{
    apply_consumption_reservation, validate_consumption_reservation_from_selection,
};
use super::state::{
    ConsumedMaterialTrace, InventoryState, LotSlice, LotStorageTransition, MaterialLotId,
    MaterialLotProfile, MaterialLotRecord, MaterialStorageHistory, StockpileId, StockpileRecord,
    StockpileStorageProfile, apply_aggregate_deposit, apply_aggregate_withdraw,
    apply_consume_lot_slice, apply_insert_or_merge_new_lot, apply_move_full_lot, apply_split_lot,
};
use super::storage_validation::{
    CommodityReferenceError, StockpileStorageError, validate_commodity_reference,
    validate_stockpile_storage,
};
use super::{
    StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedStockpileStructuralLoad,
    validate_stockpile_stored_mass_changes,
};

/// Failure while allocating a new stockpile record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddStockpileError {
    ZeroCapacity,
    IdExhausted,
    RevisionExhausted,
}

/// Revision-bound reforming of exact selected matter into another physical form of the same material.
///
/// The caller owns the physical reason for the form change. Inventory owns only exact withdrawal,
/// destination storage admission, conserved mass, lot identity/provenance, and structural-load updates.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedMaterialReform {
    expected_revision: u64,
    next_revision: u64,
    source: StockpileId,
    destination: StockpileId,
    source_inputs: Vec<MaterialInputSpec>,
    lot_slices: Vec<LotSlice>,
    outputs: Vec<ConsumedMaterialTrace>,
    target: CommodityKey,
    total_mass: Mass,
    allocated_lot_ids: Vec<MaterialLotId>,
    merge_policy: LotMergePolicy,
    next_lot_id: u64,
    structural: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedMaterialReform {
    pub(crate) const fn total_mass(&self) -> Mass {
        self.total_mass
    }

    pub(crate) fn commit(self, state: &mut AppState) -> Result<(), MaterialReformCommitError> {
        let actual = state.inventory().revision();
        if actual != self.expected_revision {
            return Err(MaterialReformCommitError::StaleInventoryRevision {
                expected: self.expected_revision,
                actual,
            });
        }
        if let Some(structural) = self.structural {
            structural
                .commit(state)
                .map_err(MaterialReformCommitError::Structure)?;
        }

        let current_tick = state.tick();
        let inventories = state.inventory_state_mut();
        let destination_preservation_multiplier_ppm = inventories
            .get_stockpile(self.destination)
            .unwrap_or_else(|| panic!("validated material reform destination disappeared"))
            .storage_profile()
            .preservation_multiplier_ppm();
        for input in &self.source_inputs {
            apply_aggregate_withdraw(inventories, self.source, input.commodity(), input.mass());
        }
        for slice in self.lot_slices {
            apply_consume_lot_slice(inventories, slice);
        }
        for (trace, lot_id) in self.outputs.into_iter().zip(self.allocated_lot_ids) {
            let mut profile: MaterialLotProfile = trace.profile().clone();
            profile.commodity = self.target;
            apply_insert_or_merge_new_lot(
                inventories,
                MaterialLotRecord {
                    id: lot_id,
                    stockpile: self.destination,
                    mass: trace.mass(),
                    profile,
                    provenance: trace.provenance(),
                    storage_history: MaterialStorageHistory::new(current_tick),
                },
                self.merge_policy,
                current_tick,
                destination_preservation_multiplier_ppm,
            );
        }
        inventories.apply_lot_cursor_and_revision(self.next_lot_id, self.next_revision);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialReformError {
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
    UnknownTargetMaterial {
        material: MaterialId,
    },
    UnknownTargetForm {
        form: FormId,
    },
    MaterialChanged {
        source: MaterialId,
        target: MaterialId,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialReformCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
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

/// Validates a same-material physical-form change for one exact preselected quantity.
pub(crate) fn validate_material_reform_from_selection(
    registries: &Registries,
    state: &AppState,
    destination: StockpileId,
    target: CommodityKey,
    selection: ConsumptionSelection,
) -> Result<ValidatedMaterialReform, MaterialReformError> {
    let ConsumptionSelection {
        expected_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs,
        total_consumed,
    } = selection;
    let inventories = state.inventory();
    if inventories.revision() != expected_revision {
        return Err(MaterialReformError::StaleSelection {
            expected: expected_revision,
            actual: inventories.revision(),
        });
    }
    let source_record = inventories
        .get_stockpile(source)
        .ok_or(MaterialReformError::UnknownSource { stockpile: source })?;
    let destination_record =
        inventories
            .get_stockpile(destination)
            .ok_or(MaterialReformError::UnknownDestination {
                stockpile: destination,
            })?;
    if source == destination {
        return Err(MaterialReformError::SameStockpile { stockpile: source });
    }
    validate_commodity_reference(registries, target).map_err(|error| match error {
        CommodityReferenceError::UnknownMaterial { material } => {
            MaterialReformError::UnknownTargetMaterial { material }
        }
        CommodityReferenceError::UnknownForm { form } => {
            MaterialReformError::UnknownTargetForm { form }
        }
    })?;

    for trace in &consumed_inputs {
        let source_material = trace.profile().commodity().material();
        if source_material != target.material() {
            return Err(MaterialReformError::MaterialChanged {
                source: source_material,
                target: target.material(),
            });
        }
        validate_stockpile_storage(
            registries,
            destination_record,
            destination,
            target,
            trace.profile().composition(),
            trace.profile().temperature(),
            trace.profile().particle_size_distribution(),
        )
        .map_err(MaterialReformError::DestinationStorage)?;
    }

    let committed = destination_record
        .stored_mass()
        .checked_add(destination_record.reserved_inbound())
        .ok_or(MaterialReformError::DestinationMassOverflow {
            stockpile: destination,
        })?;
    let destination_after = destination_record
        .stored_mass()
        .checked_add(total_consumed)
        .ok_or(MaterialReformError::DestinationMassOverflow {
            stockpile: destination,
        })?;
    let after_with_reserved = committed.checked_add(total_consumed).ok_or(
        MaterialReformError::DestinationMassOverflow {
            stockpile: destination,
        },
    )?;
    if after_with_reserved > destination_record.capacity() {
        return Err(MaterialReformError::DestinationCapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity(),
            committed,
            requested: total_consumed,
        });
    }
    destination_record
        .get_mass(target)
        .checked_add(total_consumed)
        .ok_or(MaterialReformError::DestinationMassOverflow {
            stockpile: destination,
        })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(total_consumed)
        .ok_or(MaterialReformError::StaleSelection {
            expected: expected_revision,
            actual: inventories.revision(),
        })?;
    let structural = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [
            StockpileStoredMassChange::new(source, source_after),
            StockpileStoredMassChange::new(destination, destination_after),
        ],
    )
    .map_err(MaterialReformError::StructuralLoad)?;

    let mut allocated_lot_ids = Vec::with_capacity(consumed_inputs.len());
    let mut next_lot_id = inventories.next_lot_id();
    for _ in &consumed_inputs {
        allocated_lot_ids.push(MaterialLotId::new(next_lot_id));
        next_lot_id = next_lot_id
            .checked_add(1)
            .ok_or(MaterialReformError::LotIdExhausted)?;
    }
    let next_revision = inventories
        .revision()
        .checked_add(1)
        .ok_or(MaterialReformError::RevisionExhausted)?;

    Ok(ValidatedMaterialReform {
        expected_revision,
        next_revision,
        source,
        destination,
        source_inputs: inputs,
        lot_slices,
        outputs: consumed_inputs,
        target,
        total_mass: total_consumed,
        allocated_lot_ids,
        merge_policy: LotMergePolicy::for_commodity(registries, target),
        next_lot_id,
        structural,
    })
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

    pub(crate) const fn source(&self) -> StockpileId {
        self.source
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
    merge_policies: Vec<LotMergePolicy>,
    next_lot_id_after: Option<u64>,
    total_mass: Mass,
    structural: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedMaterialRelocation {
    #[cfg(test)]
    pub(crate) const fn total_mass(&self) -> Mass {
        self.total_mass
    }

    pub(crate) fn commit(self, state: &mut AppState) -> Result<(), MaterialRelocationCommitError> {
        let actual = state.inventory().revision();
        if actual != self.expected_revision {
            return Err(MaterialRelocationCommitError::StaleInventoryRevision {
                expected: self.expected_revision,
                actual,
            });
        }
        let current_tick = state.tick();
        let source_preservation_multiplier_ppm = state
            .inventory()
            .get_stockpile(self.source)
            .unwrap_or_else(|| panic!("validated material relocation source disappeared"))
            .storage_profile()
            .preservation_multiplier_ppm();
        let destination_preservation_multiplier_ppm = state
            .inventory()
            .get_stockpile(self.destination)
            .unwrap_or_else(|| panic!("validated material relocation destination disappeared"))
            .storage_profile()
            .preservation_multiplier_ppm();
        let storage_transition = LotStorageTransition::new(
            current_tick,
            source_preservation_multiplier_ppm,
            destination_preservation_multiplier_ppm,
        );
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
        for ((slice, split_lot_id), merge_policy) in self
            .lot_slices
            .into_iter()
            .zip(self.split_lot_ids)
            .zip(self.merge_policies)
        {
            let lot_mass = match inventories.get_lot(slice.lot) {
                Some(lot) => lot.mass,
                None => panic!(
                    "validated material relocation references missing lot {}",
                    slice.lot.value()
                ),
            };
            if slice.mass == lot_mass {
                debug_assert!(split_lot_id.is_none());
                apply_move_full_lot(
                    inventories,
                    slice.lot,
                    self.source,
                    self.destination,
                    storage_transition,
                );
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
                    storage_transition,
                    merge_policy,
                );
            }
        }
        if let Some(next_lot_id) = self.next_lot_id_after {
            inventories.apply_lot_cursor_and_revision(next_lot_id, self.next_revision);
        } else {
            inventories.apply_revision(self.next_revision);
        }
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
            Self::StaleSelection {
                expected: _expected,
                actual: _actual,
            } => None,
            Self::UnknownSource {
                stockpile: _stockpile,
            }
            | Self::UnknownDestination {
                stockpile: _stockpile,
            }
            | Self::SameStockpile {
                stockpile: _stockpile,
            }
            | Self::DestinationMassOverflow {
                stockpile: _stockpile,
            } => None,
            Self::DestinationCapacityExceeded {
                stockpile: _stockpile,
                capacity: _capacity,
                committed: _committed,
                requested: _requested,
            } => None,
            Self::LotIdExhausted | Self::RevisionExhausted => None,
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
            Self::StaleInventoryRevision {
                expected: _expected,
                actual: _actual,
            } => None,
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

impl Error for AddStockpileError {}

/// Opaque authorization for one already physically resolved stockpile-to-stockpile movement.
///
/// Inventory validates and commits storage ownership but does not decide how matter travels through
/// the world. Physical/logistics owners construct this token after resolving path, timing, and any
/// transport-specific constraints; external callers cannot manufacture one directly.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct MaterialTransferResolution {
    source: StockpileId,
    destination: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
}

impl MaterialTransferResolution {
    #[cfg(any(test, feature = "test-gameplay"))]
    pub(crate) const fn new(
        source: StockpileId,
        destination: StockpileId,
        commodity: CommodityKey,
        mass: Mass,
    ) -> Self {
        Self {
            source,
            destination,
            commodity,
            mass,
        }
    }

    #[must_use]
    pub const fn source(&self) -> StockpileId {
        self.source
    }

    #[must_use]
    pub const fn destination(&self) -> StockpileId {
        self.destination
    }

    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }
}

/// Failure while validating an atomic stockpile-to-stockpile transfer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialTransferError {
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

impl Display for MaterialTransferError {
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
                commodity: _commodity,
                available,
                requested,
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

impl Error for MaterialTransferError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownStockpile {
                stockpile: _stockpile,
            }
            | Self::SameStockpile {
                stockpile: _stockpile,
            }
            | Self::MassOverflow {
                stockpile: _stockpile,
            } => None,
            Self::UnknownMaterial {
                material: _material,
            } => None,
            Self::UnknownForm { form: _form } => None,
            Self::InsufficientMass {
                stockpile: _stockpile,
                commodity: _commodity,
                available: _available,
                requested: _requested,
            } => None,
            Self::CapacityExceeded {
                stockpile: _stockpile,
                capacity: _capacity,
                committed: _committed,
                requested: _requested,
            } => None,
            Self::ZeroMass | Self::LotIdExhausted | Self::RevisionExhausted => None,
        }
    }
}

/// Failure when a previously validated transfer is committed after inventory has changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialTransferCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for MaterialTransferCommitError {
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

impl Error for MaterialTransferCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventoryRevision {
                expected: _expected,
                actual: _actual,
            } => None,
        }
    }
}

/// Consumed proof that all preconditions for a two-stockpile transfer have been checked.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedMaterialTransfer {
    relocation: ValidatedMaterialRelocation,
}

impl ValidatedMaterialTransfer {
    /// Atomically commits this already validated transfer and consumes the proof token.
    pub fn commit(self, state: &mut AppState) -> Result<(), MaterialTransferCommitError> {
        self.relocation.commit(state).map_err(|error| match error {
            MaterialRelocationCommitError::StaleInventoryRevision { expected, actual } => {
                MaterialTransferCommitError::StaleInventoryRevision { expected, actual }
            }
            MaterialRelocationCommitError::Structure(error) => {
                MaterialTransferCommitError::Structure(error)
            }
        })
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
    let id = StockpileId::new(inventories.next_stockpile_id());
    let Some(next_id) = inventories.next_stockpile_id().checked_add(1) else {
        return Err(AddStockpileError::IdExhausted);
    };
    let Some(next_revision) = inventories.revision().checked_add(1) else {
        return Err(AddStockpileError::RevisionExhausted);
    };

    let record = StockpileRecord {
        id,
        capacity,
        storage_profile,
        supported_by: None,
        stored_mass: Mass::ZERO,
        reserved_inbound: Mass::ZERO,
        contents: BTreeMap::new(),
    };

    inventories.insert_stockpile(record, next_id, next_revision);
    Ok(id)
}

/// Validates one already physically resolved material transfer without mutating either stockpile.
pub fn validate_material_transfer(
    registries: &Registries,
    state: &AppState,
    resolution: MaterialTransferResolution,
) -> Result<ValidatedMaterialTransfer, MaterialTransferError> {
    let MaterialTransferResolution {
        source,
        destination,
        commodity,
        mass,
    } = resolution;
    validate_commodity_reference(registries, commodity).map_err(|error| match error {
        CommodityReferenceError::UnknownMaterial { material } => {
            MaterialTransferError::UnknownMaterial { material }
        }
        CommodityReferenceError::UnknownForm { form } => {
            MaterialTransferError::UnknownForm { form }
        }
    })?;
    if mass.is_zero() {
        return Err(MaterialTransferError::ZeroMass);
    }
    let input = MaterialInputSpec::new(commodity, mass);
    let selection = validate_consumption_selection(state.inventory(), source, &[input])
        .map_err(map_transfer_selection_error)?;
    let relocation =
        validate_material_relocation_from_selection(registries, state, destination, selection)
            .map_err(map_transfer_relocation_error)?;
    Ok(ValidatedMaterialTransfer { relocation })
}

#[cfg(test)]
pub(crate) fn validate_material_transfer_for_test(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    destination: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) -> Result<ValidatedMaterialTransfer, MaterialTransferError> {
    validate_material_transfer(
        registries,
        state,
        MaterialTransferResolution::new(source, destination, commodity, mass),
    )
}

fn map_transfer_selection_error(error: ConsumptionSelectionError) -> MaterialTransferError {
    match error {
        ConsumptionSelectionError::UnknownStockpile { stockpile } => {
            MaterialTransferError::UnknownStockpile { stockpile }
        }
        ConsumptionSelectionError::InsufficientMass {
            stockpile,
            commodity,
            available,
            requested,
        } => MaterialTransferError::InsufficientMass {
            stockpile,
            commodity,
            available,
            requested,
        },
        ConsumptionSelectionError::MassOverflow { stockpile } => {
            MaterialTransferError::MassOverflow { stockpile }
        }
    }
}

fn map_transfer_relocation_error(error: MaterialRelocationError) -> MaterialTransferError {
    match error {
        MaterialRelocationError::StaleSelection { expected, actual } => {
            unreachable!(
                "material transfer selection revision {expected} cannot become stale at revision {actual} between synchronous selection and relocation validation"
            )
        }
        MaterialRelocationError::UnknownSource { stockpile }
        | MaterialRelocationError::UnknownDestination { stockpile } => {
            MaterialTransferError::UnknownStockpile { stockpile }
        }
        MaterialRelocationError::SameStockpile { stockpile } => {
            MaterialTransferError::SameStockpile { stockpile }
        }
        MaterialRelocationError::DestinationStorage(error) => MaterialTransferError::Storage(error),
        MaterialRelocationError::DestinationMassOverflow { stockpile } => {
            MaterialTransferError::MassOverflow { stockpile }
        }
        MaterialRelocationError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => MaterialTransferError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialRelocationError::LotIdExhausted => MaterialTransferError::LotIdExhausted,
        MaterialRelocationError::RevisionExhausted => MaterialTransferError::RevisionExhausted,
        MaterialRelocationError::StructuralLoad(error) => {
            MaterialTransferError::StructuralLoad(error)
        }
    }
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
    let inventories = state.inventory();
    if inventories.revision() != expected_revision {
        return Err(MaterialRelocationError::StaleSelection {
            expected: expected_revision,
            actual: inventories.revision(),
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
    let source_preservation_multiplier_ppm = source_record
        .storage_profile()
        .preservation_multiplier_ppm();
    debug_assert!(lot_slices.iter().all(|slice| {
        inventories
            .get_lot(slice.lot)
            .and_then(|lot| {
                lot.storage_history()
                    .rebase(state.tick(), source_preservation_multiplier_ppm)
            })
            .is_some()
    }));
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
            actual: inventories.revision(),
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
    let mut merge_policies = Vec::with_capacity(lot_slices.len());
    let mut next_lot_id = inventories.next_lot_id();
    let mut allocated_any = false;
    for slice in &lot_slices {
        let lot = match inventories.get_lot(slice.lot) {
            Some(lot) => lot,
            None => panic!(
                "validated exact selection references missing lot {}",
                slice.lot.value()
            ),
        };
        merge_policies.push(LotMergePolicy::for_commodity(registries, lot.commodity()));
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
        .revision()
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
        merge_policies,
        next_lot_id_after,
        total_mass: total_consumed,
        structural,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        FORM_CHIP, FORM_LOG, FORM_LUMP, FORM_MOLTEN, FORM_ORE, MATERIAL_CHARCOAL, MATERIAL_COPPER,
        MATERIAL_SLAG, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
    };
    use crate::core::time::WorldSeed;
    use crate::inventory::{
        MaterialFixtureError, add_solid_stockpile_for_test, deposit_bulk_for_test,
        deposit_composed_lot_for_test, deposit_lot_for_test, validate_loaded_inventory,
    };
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
            Err(MaterialFixtureError::Ingress(
                MaterialIngressError::Storage(StockpileStorageError::PhaseNotAccepted {
                    stockpile,
                    phase: MaterialPhase::Liquid,
                })
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
            Err(MaterialFixtureError::Ingress(
                MaterialIngressError::Storage(StockpileStorageError::TemperatureExceedsMaximum {
                    stockpile: vessel,
                    temperature: too_hot,
                    maximum,
                })
            ))
        );
        assert_eq!(state, before_hot_rejection);
        assert_eq!(
            validate_loaded_inventory(registries.materials(), state.inventory(), state.tick()),
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
            validate_material_transfer_for_test(
                &registries,
                &state,
                source,
                destination,
                CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
                Mass::from_milligrams(5),
            ),
            Err(MaterialTransferError::Storage(
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

        let result = validate_material_transfer_for_test(
            &registries,
            &state,
            source,
            destination,
            wood_log(),
            Mass::from_milligrams(10),
        );

        assert!(matches!(
            result,
            Err(MaterialTransferError::CapacityExceeded {
                stockpile: _stockpile,
                capacity: _capacity,
                committed: _committed,
                requested: _requested,
            })
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
            validate_material_transfer_for_test(
                &registries,
                &state,
                stockpile,
                stockpile,
                wood_log(),
                Mass::from_milligrams(5),
            ),
            Err(MaterialTransferError::SameStockpile { stockpile })
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

        let token = match validate_material_transfer_for_test(
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
            validate_loaded_inventory(registries.materials(), state.inventory(), state.tick()),
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

        let token = match validate_material_transfer_for_test(
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

        let destination_lots: Vec<_> = state.inventory().lot_ids(destination).collect();
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
            validate_loaded_inventory(registries.materials(), state.inventory(), state.tick()),
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
        let token = match validate_material_transfer_for_test(
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
            Err(MaterialTransferCommitError::StaleInventoryRevision {
                expected: _expected,
                actual: _actual,
            })
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
            let token = match validate_material_transfer_for_test(
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
        assert_eq!(state.inventory().lot_ids(destination).count(), 1);
        assert_eq!(state.inventory().lots().count(), 2);
        assert_eq!(
            validate_loaded_inventory(registries.materials(), state.inventory(), state.tick()),
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

        let token = match validate_material_transfer_for_test(
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
        let split_id = match state.inventory().lot_ids(destination).next() {
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
            validate_loaded_inventory(registries.materials(), state.inventory(), state.tick()),
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
            validate_loaded_inventory(registries.materials(), state.inventory(), state.tick()),
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
            let token = validate_material_transfer_for_test(
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

        assert_eq!(
            validate_material_transfer_for_test(
                &registries,
                &state,
                source,
                destination,
                wood_log(),
                Mass::from_milligrams(11),
            ),
            Err(MaterialTransferError::InsufficientMass {
                stockpile: source,
                commodity: wood_log(),
                available: Mass::from_milligrams(10),
                requested: Mass::from_milligrams(11),
            })
        );
        assert_eq!(
            validate_material_transfer_for_test(
                &registries,
                &state,
                source,
                destination,
                wood_log(),
                Mass::from_milligrams(9),
            ),
            Err(MaterialTransferError::CapacityExceeded {
                stockpile: destination,
                capacity: Mass::from_milligrams(5),
                committed: Mass::ZERO,
                requested: Mass::from_milligrams(9),
            })
        );
        assert_eq!(state, before, "failed validation must not mutate inventory");

        let valid = validate_material_transfer_for_test(
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
                Err(MaterialTransferCommitError::StaleInventoryRevision {
                    expected: _expected,
                    actual: _actual,
                })
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
        let selection = validate_consumption_selection(state.inventory(), source, &inputs)
            .unwrap_or_else(|error| panic!("selection failed: {error:?}"));
        assert_eq!(
            selection.total_consumed(),
            Mass::from_milligrams(10),
            "selection must bind exactly the requested input mass"
        );
        let mut inbound_by_destination = BTreeMap::new();
        inbound_by_destination.insert(destination, Mass::from_milligrams(10));
        let reservation = validate_consumption_reservation_from_selection(
            state.inventory(),
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
        let created_at = state.tick();
        let deposit_plan = decide_reserved_deposits(
            &registries,
            state.inventory(),
            created_at,
            vec![ReservedDepositRequest::new(
                destination,
                vec![output],
                Mass::from_milligrams(10),
            )],
        )
        .unwrap_or_else(|error| panic!("reserved deposit planning failed: {error:?}"));
        apply_reserved_deposits(state.inventory_state_mut(), deposit_plan);
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
    fn egress_and_ingress_round_trip_preserves_exact_quantity() {
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
        let selection = validate_consumption_selection(state.inventory(), source, &inputs)
            .unwrap_or_else(|error| panic!("selection failed: {error:?}"));
        let egress = validate_material_egress_from_selection(state.inventory(), selection)
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

        let ingress = validate_material_ingress(
            &registries,
            state.inventory(),
            destination,
            traces.iter().map(MaterialIngressEntry::from_consumed_trace),
            state.tick(),
        )
        .unwrap_or_else(|error| panic!("ingress failed: {error:?}"));
        apply_material_ingress(state.inventory_state_mut(), ingress);
        assert_lot_aggregate_agreement(&registries, &state, "after ingress");
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .unwrap_or_else(|| panic!("conservation stockpile disappeared"))
                .stored_mass(),
            Mass::from_milligrams(7),
            "ingress must restore exactly the egressed mass"
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("accounting failed: {error:?}"))
                .total(),
            before,
            "egress plus ingress round trip must conserve world matter"
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
        let selection = validate_consumption_selection(state.inventory(), source, &inputs)
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
    fn exact_reform_changes_only_physical_form_and_conserves_matter() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_2007));
        let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
            .unwrap_or_else(|error| panic!("reform source fixture failed: {error}"));
        let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
            .unwrap_or_else(|error| panic!("reform destination fixture failed: {error}"));
        deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            wood_log(),
            Mass::from_milligrams(10),
        )
        .unwrap_or_else(|error| panic!("reform source deposit failed: {error}"));
        let before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("reform accounting failed: {error:?}"))
            .total();

        let inputs = [MaterialInputSpec::new(wood_log(), Mass::from_milligrams(6))];
        let invalid_selection = validate_consumption_selection(state.inventory(), source, &inputs)
            .unwrap_or_else(|error| panic!("reform selection failed: {error:?}"));
        assert_eq!(
            validate_material_reform_from_selection(
                &registries,
                &state,
                destination,
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                invalid_selection,
            ),
            Err(MaterialReformError::MaterialChanged {
                source: MATERIAL_WOOD,
                target: MATERIAL_STONE,
            })
        );

        let selection = validate_consumption_selection(state.inventory(), source, &inputs)
            .unwrap_or_else(|error| panic!("reform selection failed: {error:?}"));
        let target = CommodityKey::new(MATERIAL_WOOD, FORM_CHIP);
        let reform = validate_material_reform_from_selection(
            &registries,
            &state,
            destination,
            target,
            selection,
        )
        .unwrap_or_else(|error| panic!("reform validation failed: {error:?}"));
        assert_eq!(reform.total_mass(), Mass::from_milligrams(6));
        reform
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("reform commit failed: {error:?}"));

        assert_lot_aggregate_agreement(&registries, &state, "after form reform");
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .map(|stockpile| stockpile.get_mass(wood_log())),
            Some(Mass::from_milligrams(4))
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .map(|stockpile| stockpile.get_mass(target)),
            Some(Mass::from_milligrams(6))
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("reform accounting failed: {error:?}"))
                .total(),
            before,
            "same-material form reform must conserve world matter"
        );
        validate_loaded_inventory(registries.materials(), state.inventory(), state.tick())
            .unwrap_or_else(|error| panic!("reformed inventory failed validation: {error}"));
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
                    if let Ok(validated) = validate_material_transfer_for_test(
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
                        validate_consumption_selection(state.inventory(), source, &inputs)
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
                        validate_consumption_selection(state.inventory(), source, &inputs)
                    {
                        let egress =
                            validate_material_egress_from_selection(state.inventory(), selection)
                                .unwrap_or_else(|error| {
                                    panic!("random egress validation failed: {error:?}")
                                });
                        let traces = egress.consumed_inputs().to_vec();
                        apply_material_egress(state.inventory_state_mut(), egress);
                        let ingress = validate_material_ingress(
                            &registries,
                            state.inventory(),
                            destination,
                            traces.iter().map(MaterialIngressEntry::from_consumed_trace),
                            state.tick(),
                        )
                        .unwrap_or_else(|error| {
                            panic!("random ingress validation failed: {error:?}")
                        });
                        apply_material_ingress(state.inventory_state_mut(), ingress);
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
