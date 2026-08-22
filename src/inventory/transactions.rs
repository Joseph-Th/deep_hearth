//! Canonical inventory transactions; sibling state records remain passive and privately mutable.

#[cfg(any(test, feature = "test-gameplay"))]
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
    MaterialLotProfile, MaterialLotRecord, MaterialStorageHistory, StockpileId,
    apply_aggregate_deposit, apply_aggregate_withdraw, apply_consume_lot_slice,
    apply_insert_or_merge_new_lot, apply_move_full_lot, apply_split_lot,
};
#[cfg(any(test, feature = "test-gameplay"))]
use super::state::{StockpileRecord, StockpileStorageProfile};
use super::storage_validation::{
    CommodityReferenceError, StockpileStorageError, validate_commodity_reference,
    validate_stockpile_storage,
};
use super::{
    StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedStockpileStructuralLoad,
    validate_stockpile_stored_mass_changes,
};

/// Failure while allocating a new stockpile record for controlled fixtures.
#[cfg(any(test, feature = "test-gameplay"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AddStockpileError {
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
    outputs: Vec<(ConsumedMaterialTrace, MaterialStorageHistory)>,
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
        for ((trace, storage_history), lot_id) in
            self.outputs.into_iter().zip(self.allocated_lot_ids)
        {
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
                    storage_history,
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

    let source_after = source_record
        .stored_mass()
        .checked_sub(total_consumed)
        .ok_or(MaterialReformError::StaleSelection {
            expected: expected_revision,
            actual: inventories.revision(),
        })?;
    let destination_after = if source == destination {
        source_record.stored_mass()
    } else {
        destination_record
            .stored_mass()
            .checked_add(total_consumed)
            .ok_or(MaterialReformError::DestinationMassOverflow {
                stockpile: destination,
            })?
    };
    let committed_before_output = if source == destination {
        source_after
            .checked_add(destination_record.reserved_inbound())
            .ok_or(MaterialReformError::DestinationMassOverflow {
                stockpile: destination,
            })?
    } else {
        destination_record
            .stored_mass()
            .checked_add(destination_record.reserved_inbound())
            .ok_or(MaterialReformError::DestinationMassOverflow {
                stockpile: destination,
            })?
    };
    let after_with_reserved = committed_before_output.checked_add(total_consumed).ok_or(
        MaterialReformError::DestinationMassOverflow {
            stockpile: destination,
        },
    )?;
    if after_with_reserved > destination_record.capacity() {
        return Err(MaterialReformError::DestinationCapacityExceeded {
            stockpile: destination,
            capacity: destination_record.capacity(),
            committed: committed_before_output,
            requested: total_consumed,
        });
    }
    if source != destination {
        destination_record
            .get_mass(target)
            .checked_add(total_consumed)
            .ok_or(MaterialReformError::DestinationMassOverflow {
                stockpile: destination,
            })?;
    }
    let structural = if source == destination {
        None
    } else {
        validate_stockpile_stored_mass_changes(
            registries,
            state,
            [
                StockpileStoredMassChange::new(source, source_after),
                StockpileStoredMassChange::new(destination, destination_after),
            ],
        )
        .map_err(MaterialReformError::StructuralLoad)?
    };

    let source_preservation_multiplier_ppm = source_record
        .storage_profile()
        .preservation_multiplier_ppm();
    let output_storage_histories = lot_slices
        .iter()
        .map(|slice| {
            inventories
                .get_lot(slice.lot)
                .unwrap_or_else(|| {
                    panic!(
                        "validated material reform references missing lot {}",
                        slice.lot.value()
                    )
                })
                .storage_history()
                .rebase(state.tick(), source_preservation_multiplier_ppm)
                .unwrap_or_else(|| {
                    panic!("valid inventory lot storage history could not be rebased for reform")
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        output_storage_histories.len(),
        consumed_inputs.len(),
        "consumption selection trace count must match selected lot slices"
    );
    let outputs: Vec<(ConsumedMaterialTrace, MaterialStorageHistory)> = consumed_inputs
        .into_iter()
        .zip(output_storage_histories)
        .collect();

    let mut allocated_lot_ids = Vec::with_capacity(outputs.len());
    let mut next_lot_id = inventories.next_lot_id();
    for _ in &outputs {
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
        outputs,
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

#[cfg(any(test, feature = "test-gameplay"))]
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

#[cfg(any(test, feature = "test-gameplay"))]
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

/// Adds empty material storage for tests and controlled gameplay bootstrap only.
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) fn add_stockpile(
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
#[path = "transactions_tests.rs"]
mod tests;
