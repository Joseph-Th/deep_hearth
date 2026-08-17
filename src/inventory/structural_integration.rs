//! Stockpile-to-structure support integration; inventory owns support assignment while structural state owns the resulting aggregate stored-matter load.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{AggregateMass, Force, Mass};
use crate::core::state::AppState;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;
use crate::structural::{
    StructuralAnalysis, StructuralCommitError, StructuralElementId, StructuralLifecycle,
    StructuralLoadKind, StructuralMutationError, StructuralMutationOutcome,
    ValidatedStructuralLoadBatch, calculate_aggregate_weight_force_ceiling,
    validate_set_owned_structural_loads,
};

use super::StockpileId;

/// Final stored mass of one stockpile after a validated inventory mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StockpileStoredMassChange {
    stockpile: StockpileId,
    stored_after: Mass,
    increase_policy: StockpileMassIncreasePolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StockpileMassIncreasePolicy {
    ActiveSupportRequired,
    CommittedInboundMayComplete,
}

impl StockpileStoredMassChange {
    #[must_use]
    pub(crate) const fn new(stockpile: StockpileId, stored_after: Mass) -> Self {
        Self {
            stockpile,
            stored_after,
            increase_policy: StockpileMassIncreasePolicy::ActiveSupportRequired,
        }
    }

    /// Final mass from inbound matter that was already durably reserved while support was valid.
    #[must_use]
    pub(crate) const fn new_committed_inbound(stockpile: StockpileId, stored_after: Mass) -> Self {
        Self {
            stockpile,
            stored_after,
            increase_policy: StockpileMassIncreasePolicy::CommittedInboundMayComplete,
        }
    }
}

/// Failure while deriving structure-owned load from stockpile matter ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StockpileStructuralLoadError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    UnknownSupport {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    SupportNotActiveForIncrease {
        stockpile: StockpileId,
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    AggregateMassOverflow {
        element: StructuralElementId,
    },
    WeightForceOverflow {
        element: StructuralElementId,
    },
    ExistingLoadMismatch {
        element: StructuralElementId,
        stored: Force,
        expected: Force,
    },
    Structure(StructuralMutationError),
}

impl Display for StockpileStructuralLoadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
            Self::UnknownSupport { stockpile, element } => write!(
                formatter,
                "stockpile {} references missing structural support {}",
                stockpile.value(),
                element.value()
            ),
            Self::SupportNotActiveForIncrease {
                stockpile,
                element,
                lifecycle,
            } => write!(
                formatter,
                "stockpile {} cannot add stored matter while structural support {} is {lifecycle:?}",
                stockpile.value(),
                element.value()
            ),
            Self::AggregateMassOverflow { element } => write!(
                formatter,
                "stored matter mass overflows aggregate accounting on structural element {}",
                element.value()
            ),
            Self::WeightForceOverflow { element } => write!(
                formatter,
                "stored matter weight exceeds structural force range on element {}",
                element.value()
            ),
            Self::ExistingLoadMismatch {
                element,
                stored,
                expected,
            } => write!(
                formatter,
                "structural element {} stores {} mN stored-matter load but inventory ownership requires {} mN",
                element.value(),
                stored.millinewtons(),
                expected.millinewtons()
            ),
            Self::Structure(error) => {
                write!(formatter, "stored-matter structural load failed: {error}")
            }
        }
    }
}

impl Error for StockpileStructuralLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::UnknownStockpile {
                stockpile: _stockpile,
            } => None,
            Self::UnknownSupport {
                stockpile: _stockpile,
                element: _element,
            } => None,
            Self::SupportNotActiveForIncrease {
                stockpile: _stockpile,
                element: _element,
                lifecycle: _lifecycle,
            } => None,
            Self::AggregateMassOverflow { element: _element }
            | Self::WeightForceOverflow { element: _element } => None,
            Self::ExistingLoadMismatch {
                element: _element,
                stored: _stored,
                expected: _expected,
            } => None,
        }
    }
}

/// Revision guard plus any actual stored-matter structural mutation required by inventory.
///
/// A supported stockpile operation binds the structural owner even when conservative force rounding
/// leaves the numeric load unchanged. This prevents a zero-delta operation from committing after its
/// support has failed or otherwise changed since validation.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedStockpileStructuralLoad {
    expected_revision: u64,
    structural: Option<ValidatedStructuralLoadBatch>,
}

impl ValidatedStockpileStructuralLoad {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    pub(crate) const fn revision_delta(&self) -> u64 {
        if self.structural.is_some() { 1 } else { 0 }
    }

    pub(crate) fn commit(
        self,
        state: &mut AppState,
    ) -> Result<Option<StructuralMutationOutcome>, StructuralCommitError> {
        let actual = state.structures().revision();
        if actual != self.expected_revision {
            return Err(StructuralCommitError::StaleRevision {
                expected: self.expected_revision,
                actual,
            });
        }
        match self.structural {
            Some(structural) => structural.commit(state).map(Some),
            None => Ok(None),
        }
    }
}

fn support_force(
    registries: &Registries,
    element: StructuralElementId,
    mass: AggregateMass,
) -> Result<Force, StockpileStructuralLoadError> {
    calculate_aggregate_weight_force_ceiling(mass, registries.core().gravity())
        .ok_or(StockpileStructuralLoadError::WeightForceOverflow { element })
}

fn supported_mass(
    state: &AppState,
    element: StructuralElementId,
    overrides: &BTreeMap<StockpileId, Mass>,
    excluded: Option<StockpileId>,
) -> Result<AggregateMass, StockpileStructuralLoadError> {
    let mut total = AggregateMass::ZERO;
    for stockpile in state.inventory().supported_stockpiles(element) {
        if excluded == Some(stockpile) {
            continue;
        }
        let record = state
            .inventory()
            .get_stockpile(stockpile)
            .ok_or(StockpileStructuralLoadError::UnknownStockpile { stockpile })?;
        let mass = overrides
            .get(&stockpile)
            .copied()
            .unwrap_or_else(|| record.stored_mass());
        total = total
            .checked_add(AggregateMass::from_mass(mass))
            .ok_or(StockpileStructuralLoadError::AggregateMassOverflow { element })?;
    }
    Ok(total)
}

fn validate_existing_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<(), StockpileStructuralLoadError> {
    let mass = supported_mass(state, element, &BTreeMap::new(), None)?;
    let expected = support_force(registries, element, mass)?;
    let stored = state
        .structures()
        .get_element(element)
        .ok_or(StockpileStructuralLoadError::Structure(
            StructuralMutationError::UnknownElement { element },
        ))?
        .load(StructuralLoadKind::StoredMatter);
    if stored != expected {
        return Err(StockpileStructuralLoadError::ExistingLoadMismatch {
            element,
            stored,
            expected,
        });
    }
    Ok(())
}

/// Requires a stockpile's current support, if any, to be active before new inbound work is accepted.
pub(crate) fn validate_stockpile_support_for_new_inbound(
    state: &AppState,
    stockpile: StockpileId,
) -> Result<Option<u64>, StockpileStructuralLoadError> {
    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .ok_or(StockpileStructuralLoadError::UnknownStockpile { stockpile })?;
    let Some(element) = record.supported_by() else {
        return Ok(None);
    };
    let support = state
        .structures()
        .get_element(element)
        .ok_or(StockpileStructuralLoadError::UnknownSupport { stockpile, element })?;
    if support.lifecycle() != StructuralLifecycle::Active {
        return Err(StockpileStructuralLoadError::SupportNotActiveForIncrease {
            stockpile,
            element,
            lifecycle: support.lifecycle(),
        });
    }
    Ok(Some(state.structures().revision()))
}

/// Resolves the exact final structure-owned loads implied by final stockpile masses.
pub(crate) fn resolve_stockpile_stored_loads(
    registries: &Registries,
    state: &AppState,
    changes: impl IntoIterator<Item = StockpileStoredMassChange>,
) -> Result<BTreeMap<StructuralElementId, Force>, StockpileStructuralLoadError> {
    let mut overrides = BTreeMap::new();
    let mut affected_supports = BTreeSet::new();
    for change in changes {
        let record = state.inventory().get_stockpile(change.stockpile).ok_or(
            StockpileStructuralLoadError::UnknownStockpile {
                stockpile: change.stockpile,
            },
        )?;
        if overrides
            .insert(change.stockpile, change.stored_after)
            .is_some()
        {
            panic!(
                "stockpile stored-mass change set contains duplicate stockpile {}",
                change.stockpile.value()
            );
        }
        if let Some(support) = record.supported_by() {
            let support_record = state.structures().get_element(support).ok_or(
                StockpileStructuralLoadError::UnknownSupport {
                    stockpile: change.stockpile,
                    element: support,
                },
            )?;
            if change.stored_after > record.stored_mass() {
                match change.increase_policy {
                    StockpileMassIncreasePolicy::ActiveSupportRequired => {
                        if support_record.lifecycle() != StructuralLifecycle::Active {
                            return Err(
                                StockpileStructuralLoadError::SupportNotActiveForIncrease {
                                    stockpile: change.stockpile,
                                    element: support,
                                    lifecycle: support_record.lifecycle(),
                                },
                            );
                        }
                    }
                    StockpileMassIncreasePolicy::CommittedInboundMayComplete => {}
                }
            }
            affected_supports.insert(support);
        }
    }

    let mut loads = BTreeMap::new();
    for element in affected_supports {
        validate_existing_load(registries, state, element)?;
        let mass = supported_mass(state, element, &overrides, None)?;
        loads.insert(element, support_force(registries, element, mass)?);
    }
    Ok(loads)
}

fn validate_stockpile_structural_load_plan(
    registries: &Registries,
    state: &AppState,
    loads: BTreeMap<StructuralElementId, Force>,
) -> Result<ValidatedStockpileStructuralLoad, StockpileStructuralLoadError> {
    debug_assert!(!loads.is_empty());
    let expected_revision = state.structures().revision();
    let structural = validate_set_owned_structural_loads(
        registries,
        state,
        StructuralLoadKind::StoredMatter,
        loads,
    )
    .map_err(StockpileStructuralLoadError::Structure)?;
    Ok(ValidatedStockpileStructuralLoad {
        expected_revision,
        structural,
    })
}

/// Validates all structure-owned weight changes implied by final stockpile masses.
pub(crate) fn validate_stockpile_stored_mass_changes(
    registries: &Registries,
    state: &AppState,
    changes: impl IntoIterator<Item = StockpileStoredMassChange>,
) -> Result<Option<ValidatedStockpileStructuralLoad>, StockpileStructuralLoadError> {
    let loads = resolve_stockpile_stored_loads(registries, state, changes)?;
    if loads.is_empty() {
        return Ok(None);
    }
    validate_stockpile_structural_load_plan(registries, state, loads).map(Some)
}

/// Failure while assigning or removing a stockpile's structural support.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StockpileSupportError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    AlreadyMounted {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    NotMounted {
        stockpile: StockpileId,
    },
    TargetNotActive {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    StockpileBusy {
        stockpile: StockpileId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    InventoryRevisionExhausted,
    Load(StockpileStructuralLoadError),
}

impl Display for StockpileSupportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
            Self::AlreadyMounted { stockpile, element } => write!(
                formatter,
                "stockpile {} is already supported by structural element {}",
                stockpile.value(),
                element.value()
            ),
            Self::NotMounted { stockpile } => write!(
                formatter,
                "stockpile {} has no structural support assignment to remove",
                stockpile.value()
            ),
            Self::TargetNotActive { element, lifecycle } => write!(
                formatter,
                "structural element {} is {lifecycle:?} and cannot receive a stockpile",
                element.value()
            ),
            Self::StockpileBusy {
                stockpile,
                job,
                release,
            } => write!(
                formatter,
                "stockpile {} participates in production job {} {release} and cannot be moved",
                stockpile.value(),
                job.value()
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::Load(error) => write!(formatter, "stockpile support load failed: {error}"),
        }
    }
}

impl Error for StockpileSupportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Load(error) => Some(error),
            Self::UnknownStockpile {
                stockpile: _stockpile,
            }
            | Self::NotMounted {
                stockpile: _stockpile,
            } => None,
            Self::AlreadyMounted {
                stockpile: _stockpile,
                element: _element,
            } => None,
            Self::TargetNotActive {
                element: _element,
                lifecycle: _lifecycle,
            } => None,
            Self::StockpileBusy {
                stockpile: _stockpile,
                job: _job,
                release: _release,
            } => None,
            Self::InventoryRevisionExhausted => None,
        }
    }
}

/// Failure to commit a revision-bound stockpile support change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StockpileSupportCommitError {
    StaleInventoryRevision {
        expected: u64,
        actual: u64,
    },
    UnknownStockpile {
        stockpile: StockpileId,
    },
    SupportChanged {
        stockpile: StockpileId,
        expected: Option<StructuralElementId>,
        actual: Option<StructuralElementId>,
    },
    StockpileBusy {
        stockpile: StockpileId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    Structure(StructuralCommitError),
}

impl Display for StockpileSupportCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated stockpile support change expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::UnknownStockpile { stockpile } => write!(
                formatter,
                "stockpile {} disappeared before support commit",
                stockpile.value()
            ),
            Self::SupportChanged {
                stockpile,
                expected,
                actual,
            } => write!(
                formatter,
                "stockpile {} support changed from expected {expected:?} to {actual:?} before commit",
                stockpile.value()
            ),
            Self::StockpileBusy {
                stockpile,
                job,
                release,
            } => write!(
                formatter,
                "stockpile {} became occupied by production job {} {release} before support commit",
                stockpile.value(),
                job.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "stockpile support structural commit failed: {error}"
            ),
        }
    }
}

impl Error for StockpileSupportCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventoryRevision {
                expected: _expected,
                actual: _actual,
            } => None,
            Self::UnknownStockpile {
                stockpile: _stockpile,
            } => None,
            Self::SupportChanged {
                stockpile: _stockpile,
                expected: _expected,
                actual: _actual,
            } => None,
            Self::StockpileBusy {
                stockpile: _stockpile,
                job: _job,
                release: _release,
            } => None,
        }
    }
}

/// Successful support assignment change plus any resulting structural damage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StockpileSupportOutcome {
    structural: Option<StructuralMutationOutcome>,
}

impl StockpileSupportOutcome {
    #[must_use]
    pub fn structural_analysis(&self) -> Option<&StructuralAnalysis> {
        self.structural
            .as_ref()
            .map(StructuralMutationOutcome::analysis)
    }
}

/// Consumed proof that inventory ownership and structure-owned stored-matter load agree.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedStockpileSupportChange {
    stockpile: StockpileId,
    before: Option<StructuralElementId>,
    after: Option<StructuralElementId>,
    expected_inventory_revision: u64,
    next_inventory_revision: u64,
    structural: ValidatedStockpileStructuralLoad,
}

impl ValidatedStockpileSupportChange {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StockpileSupportOutcome, StockpileSupportCommitError> {
        let actual_revision = state.inventory().revision();
        if actual_revision != self.expected_inventory_revision {
            return Err(StockpileSupportCommitError::StaleInventoryRevision {
                expected: self.expected_inventory_revision,
                actual: actual_revision,
            });
        }
        let Some(record) = state.inventory().get_stockpile(self.stockpile) else {
            return Err(StockpileSupportCommitError::UnknownStockpile {
                stockpile: self.stockpile,
            });
        };
        if record.supported_by() != self.before {
            return Err(StockpileSupportCommitError::SupportChanged {
                stockpile: self.stockpile,
                expected: self.before,
                actual: record.supported_by(),
            });
        }
        if let Some(job) = state.production().get_stockpile_occupant(self.stockpile) {
            return Err(StockpileSupportCommitError::StockpileBusy {
                stockpile: self.stockpile,
                job: job.id(),
                release: job.occupancy_release(),
            });
        }

        let structural = self
            .structural
            .commit(state)
            .map_err(StockpileSupportCommitError::Structure)?;
        state.inventory_state_mut().apply_support_change(
            self.stockpile,
            self.before,
            self.after,
            self.next_inventory_revision,
        );
        Ok(StockpileSupportOutcome { structural })
    }
}

fn next_inventory_revision(state: &AppState) -> Result<(u64, u64), StockpileSupportError> {
    let current = state.inventory().revision();
    let next = current
        .checked_add(1)
        .ok_or(StockpileSupportError::InventoryRevisionExhausted)?;
    Ok((current, next))
}

fn validate_not_busy(
    state: &AppState,
    stockpile: StockpileId,
) -> Result<(), StockpileSupportError> {
    if let Some(job) = state.production().get_stockpile_occupant(stockpile) {
        return Err(StockpileSupportError::StockpileBusy {
            stockpile,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    Ok(())
}

/// Validates placing an existing stockpile on one active structural member.
pub fn validate_mount_stockpile(
    registries: &Registries,
    state: &AppState,
    stockpile: StockpileId,
    element: StructuralElementId,
) -> Result<ValidatedStockpileSupportChange, StockpileSupportError> {
    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .ok_or(StockpileSupportError::UnknownStockpile { stockpile })?;
    if let Some(existing) = record.supported_by() {
        return Err(StockpileSupportError::AlreadyMounted {
            stockpile,
            element: existing,
        });
    }
    validate_not_busy(state, stockpile)?;
    let target = state
        .structures()
        .get_element(element)
        .ok_or(StockpileSupportError::Load(
            StockpileStructuralLoadError::Structure(StructuralMutationError::UnknownElement {
                element,
            }),
        ))?;
    if target.lifecycle() != StructuralLifecycle::Active {
        return Err(StockpileSupportError::TargetNotActive {
            element,
            lifecycle: target.lifecycle(),
        });
    }
    validate_existing_load(registries, state, element).map_err(StockpileSupportError::Load)?;
    let current_mass = supported_mass(state, element, &BTreeMap::new(), None)
        .map_err(StockpileSupportError::Load)?;
    let next_mass = current_mass
        .checked_add(AggregateMass::from_mass(record.stored_mass()))
        .ok_or(StockpileSupportError::Load(
            StockpileStructuralLoadError::AggregateMassOverflow { element },
        ))?;
    let next_load =
        support_force(registries, element, next_mass).map_err(StockpileSupportError::Load)?;
    let structural = validate_stockpile_structural_load_plan(
        registries,
        state,
        BTreeMap::from([(element, next_load)]),
    )
    .map_err(StockpileSupportError::Load)?;
    let (expected_inventory_revision, next_inventory_revision) = next_inventory_revision(state)?;
    Ok(ValidatedStockpileSupportChange {
        stockpile,
        before: None,
        after: Some(element),
        expected_inventory_revision,
        next_inventory_revision,
        structural,
    })
}

/// Validates removing a stockpile support assignment. Failed structural debris may be unloaded.
pub fn validate_unmount_stockpile(
    registries: &Registries,
    state: &AppState,
    stockpile: StockpileId,
) -> Result<ValidatedStockpileSupportChange, StockpileSupportError> {
    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .ok_or(StockpileSupportError::UnknownStockpile { stockpile })?;
    let element = record
        .supported_by()
        .ok_or(StockpileSupportError::NotMounted { stockpile })?;
    validate_not_busy(state, stockpile)?;
    if state.structures().get_element(element).is_none() {
        return Err(StockpileSupportError::Load(
            StockpileStructuralLoadError::UnknownSupport { stockpile, element },
        ));
    }
    validate_existing_load(registries, state, element).map_err(StockpileSupportError::Load)?;
    let remaining_mass = supported_mass(state, element, &BTreeMap::new(), Some(stockpile))
        .map_err(StockpileSupportError::Load)?;
    let next_load =
        support_force(registries, element, remaining_mass).map_err(StockpileSupportError::Load)?;
    let structural = validate_stockpile_structural_load_plan(
        registries,
        state,
        BTreeMap::from([(element, next_load)]),
    )
    .map_err(StockpileSupportError::Load)?;
    let (expected_inventory_revision, next_inventory_revision) = next_inventory_revision(state)?;
    Ok(ValidatedStockpileSupportChange {
        stockpile,
        before: Some(element),
        after: None,
        expected_inventory_revision,
        next_inventory_revision,
        structural,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        FORM_LOG, FORM_LUMP, MATERIAL_CHARCOAL, MATERIAL_WOOD,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries, make_test_registries_with_process,
    };
    use crate::core::quantity::{Area, Length, Temperature};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::inventory::{
        TransferCommitError, TransferError, add_solid_stockpile_for_test, deposit_lot_for_test,
        validate_transfer_bulk,
    };
    use crate::material::{CommodityKey, MaterialInputSpec, MaterialLotSpec};
    use crate::production::{
        ProcessDefinition, ProcessId, StartProcessError, make_test_process_resolution,
        validate_process_inputs, validate_start_process,
    };
    use crate::simulation::advance_tick;
    use crate::spatial::{VoxelBounds, VoxelCoord};
    use crate::structural::{
        StructuralCommitError, StructuralLifecycle, StructuralMutationError,
        add_structural_element, materialize_structural_element_for_test,
        validate_activate_structural_element, validate_remove_structural_element,
        validate_set_structural_load,
    };

    fn active_support(
        registries: &Registries,
        state: &mut AppState,
        x: i64,
    ) -> StructuralElementId {
        let bounds = match VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1))
        {
            Ok(bounds) => bounds,
            Err(error) => panic!("stockpile support bounds fixture failed: {error}"),
        };
        let element = match add_structural_element(
            registries,
            state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            crate::structural::make_test_structural_geometry(
                bounds,
                Length::from_micrometers(1),
                Area::from_square_millimeters(1_000),
            ),
            true,
        ) {
            Ok(element) => element,
            Err(error) => panic!("stockpile support element fixture failed: {error}"),
        };
        materialize_structural_element_for_test(registries, state, element, FORM_LOG);
        let activation = match validate_activate_structural_element(registries, state, element) {
            Ok(activation) => activation,
            Err(error) => panic!("stockpile support activation fixture failed: {error}"),
        };
        if let Err(error) = activation.commit(state) {
            panic!("stockpile support activation commit failed: {error}");
        }
        element
    }

    fn seeded_stockpile(
        registries: &Registries,
        state: &mut AppState,
        capacity: Mass,
        mass: Mass,
    ) -> StockpileId {
        let stockpile = match add_solid_stockpile_for_test(state, capacity) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("stockpile support storage fixture failed: {error}"),
        };
        if !mass.is_zero()
            && let Err(error) = deposit_lot_for_test(
                registries,
                state,
                stockpile,
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                mass,
                Temperature::from_millikelvin(293_150),
            )
        {
            panic!("stockpile support material fixture failed: {error}");
        }
        stockpile
    }

    fn mount(
        registries: &Registries,
        state: &mut AppState,
        stockpile: StockpileId,
        element: StructuralElementId,
    ) -> StockpileSupportOutcome {
        let token = match validate_mount_stockpile(registries, state, stockpile, element) {
            Ok(token) => token,
            Err(error) => panic!("stockpile mount validation failed: {error}"),
        };
        match token.commit(state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("stockpile mount commit failed: {error}"),
        }
    }

    fn expected_weight(registries: &Registries, mass: Mass) -> Force {
        match calculate_aggregate_weight_force_ceiling(
            AggregateMass::from_mass(mass),
            registries.core().gravity(),
        ) {
            Some(force) => force,
            None => panic!("stockpile support expected weight overflowed"),
        }
    }

    #[test]
    fn multiple_stockpiles_aggregate_mass_before_rounding_weight() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A71_0001));
        let support = active_support(&registries, &mut state, 0);
        let first = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(10),
            Mass::from_milligrams(1),
        );
        let second = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(10),
            Mass::from_milligrams(1),
        );

        mount(&registries, &mut state, first, support);
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(Force::from_millinewtons(1))
        );
        mount(&registries, &mut state, second, support);

        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(Force::from_millinewtons(1))
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn new_production_rejects_failed_destination_support() {
        let process = ProcessDefinition::new(
            ProcessId::new(971_002),
            "failed destination production fixture",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(10),
            )],
            Vec::new(),
        );
        let registries = make_test_registries_with_process(process);
        let mut state = AppState::new(WorldSeed::new(0x1A71_0009));
        let support = active_support(&registries, &mut state, 0);
        let source = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(20),
            Mass::from_milligrams(10),
        );
        let destination = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(20),
            Mass::ZERO,
        );
        mount(&registries, &mut state, destination, support);
        let overload = match validate_set_structural_load(
            &registries,
            &state,
            support,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(50_000_000),
        ) {
            Ok(overload) => overload,
            Err(error) => panic!("failed destination overload validation failed: {error}"),
        };
        if let Err(error) = overload.commit(&mut state) {
            panic!("failed destination overload commit failed: {error}");
        }
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Failed)
        );
        let inputs =
            match validate_process_inputs(&registries, &state, ProcessId::new(971_002), source) {
                Ok(inputs) => inputs,
                Err(error) => panic!("failed destination inputs failed: {error}"),
            };
        let resolution = make_test_process_resolution(
            inputs,
            1,
            vec![MaterialLotSpec::new(
                CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
                Mass::from_milligrams(10),
                Temperature::from_millikelvin(500_000),
            )],
        );

        assert!(matches!(
            validate_start_process(&registries, &state, &resolution, source, destination),
            Err(StartProcessError::StructuralLoad(
                StockpileStructuralLoadError::SupportNotActiveForIncrease {
                    stockpile,
                    element,
                    lifecycle: StructuralLifecycle::Failed,
                }
            )) if stockpile == destination && element == support
        ));
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .map(|record| record.stored_mass()),
            Some(Mass::from_milligrams(10))
        );
        assert_eq!(state.production().jobs().count(), 0);
    }

    #[test]
    fn validated_production_start_rejects_destination_support_collapse_before_commit() {
        let process = ProcessDefinition::new(
            ProcessId::new(971_004),
            "stale destination support fixture",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(10),
            )],
            Vec::new(),
        );
        let registries = make_test_registries_with_process(process);
        let mut state = AppState::new(WorldSeed::new(0x1A71_0011));
        let support = active_support(&registries, &mut state, 0);
        let source = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(20),
            Mass::from_milligrams(10),
        );
        let destination = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(20),
            Mass::ZERO,
        );
        mount(&registries, &mut state, destination, support);
        let inputs =
            match validate_process_inputs(&registries, &state, ProcessId::new(971_004), source) {
                Ok(inputs) => inputs,
                Err(error) => panic!("stale destination support inputs failed: {error}"),
            };
        let resolution = make_test_process_resolution(
            inputs,
            1,
            vec![MaterialLotSpec::new(
                CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
                Mass::from_milligrams(10),
                Temperature::from_millikelvin(500_000),
            )],
        );
        let start =
            match validate_start_process(&registries, &state, &resolution, source, destination) {
                Ok(start) => start,
                Err(error) => panic!("stale destination support start validation failed: {error}"),
            };

        let overload = match validate_set_structural_load(
            &registries,
            &state,
            support,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(50_000_000),
        ) {
            Ok(overload) => overload,
            Err(error) => panic!("stale destination support overload validation failed: {error}"),
        };
        if let Err(error) = overload.commit(&mut state) {
            panic!("stale destination support overload commit failed: {error}");
        }
        let source_before_commit = state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass());

        assert!(matches!(
            start.commit(&mut state),
            Err(
                crate::production::StartProcessCommitError::StaleStructureRevision {
                    expected: _expected,
                    actual: _actual,
                }
            )
        ));
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .map(|record| record.stored_mass()),
            source_before_commit
        );
        assert_eq!(state.production().jobs().count(), 0);
    }

    #[test]
    fn committed_production_completes_after_destination_support_fails() {
        let process = ProcessDefinition::new(
            ProcessId::new(971_003),
            "committed failed destination fixture",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(10),
            )],
            Vec::new(),
        );
        let registries = make_test_registries_with_process(process);
        let mut state = AppState::new(WorldSeed::new(0x1A71_0010));
        let support = active_support(&registries, &mut state, 0);
        let source = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(20),
            Mass::from_milligrams(10),
        );
        let destination = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(20),
            Mass::ZERO,
        );
        mount(&registries, &mut state, destination, support);
        let inputs =
            match validate_process_inputs(&registries, &state, ProcessId::new(971_003), source) {
                Ok(inputs) => inputs,
                Err(error) => panic!("committed destination inputs failed: {error}"),
            };
        let resolution = make_test_process_resolution(
            inputs,
            1,
            vec![MaterialLotSpec::new(
                CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
                Mass::from_milligrams(10),
                Temperature::from_millikelvin(500_000),
            )],
        );
        let start =
            match validate_start_process(&registries, &state, &resolution, source, destination) {
                Ok(start) => start,
                Err(error) => panic!("committed destination start validation failed: {error}"),
            };
        if let Err(error) = start.commit(&mut state) {
            panic!("committed destination start commit failed: {error}");
        }

        let overload = match validate_set_structural_load(
            &registries,
            &state,
            support,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(50_000_000),
        ) {
            Ok(overload) => overload,
            Err(error) => panic!("committed destination overload validation failed: {error}"),
        };
        if let Err(error) = overload.commit(&mut state) {
            panic!("committed destination overload commit failed: {error}");
        }
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Failed)
        );

        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("committed output did not complete onto failed destination support: {error}");
        }
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .map(|record| record.stored_mass()),
            Some(Mass::from_milligrams(10))
        );
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(expected_weight(&registries, Mass::from_milligrams(10)))
        );
        assert_eq!(state.production().jobs().count(), 0);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn production_moves_supported_weight_with_authoritative_matter_ownership() {
        let process = ProcessDefinition::new(
            ProcessId::new(971_001),
            "supported stockpile production fixture",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(10),
            )],
            Vec::new(),
        );
        let registries = make_test_registries_with_process(process);
        let mut state = AppState::new(WorldSeed::new(0x1A71_0006));
        let source_support = active_support(&registries, &mut state, 0);
        let destination_support = active_support(&registries, &mut state, 2);
        let source = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(20),
            Mass::from_milligrams(10),
        );
        let destination = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(20),
            Mass::ZERO,
        );
        mount(&registries, &mut state, source, source_support);
        mount(&registries, &mut state, destination, destination_support);
        let inputs =
            match validate_process_inputs(&registries, &state, ProcessId::new(971_001), source) {
                Ok(inputs) => inputs,
                Err(error) => panic!("supported production inputs failed: {error}"),
            };
        let resolution = make_test_process_resolution(
            inputs,
            1,
            vec![MaterialLotSpec::new(
                CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
                Mass::from_milligrams(10),
                Temperature::from_millikelvin(500_000),
            )],
        );
        let start =
            match validate_start_process(&registries, &state, &resolution, source, destination) {
                Ok(start) => start,
                Err(error) => panic!("supported production start validation failed: {error}"),
            };
        if let Err(error) = start.commit(&mut state) {
            panic!("supported production start commit failed: {error}");
        }

        assert_eq!(
            state
                .structures()
                .get_element(source_support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(Force::ZERO)
        );
        assert_eq!(
            state
                .structures()
                .get_element(destination_support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(Force::ZERO)
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("supported production completion failed: {error}");
        }
        assert_eq!(
            state
                .structures()
                .get_element(destination_support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(expected_weight(&registries, Mass::from_milligrams(10)))
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn transfer_between_supported_stockpiles_updates_both_loads_atomically() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A71_0002));
        let source_support = active_support(&registries, &mut state, 0);
        let destination_support = active_support(&registries, &mut state, 2);
        let source = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(300_000),
            Mass::from_milligrams(200_000),
        );
        let destination = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(300_000),
            Mass::ZERO,
        );
        mount(&registries, &mut state, source, source_support);
        mount(&registries, &mut state, destination, destination_support);

        let transfer = match validate_transfer_bulk(
            &registries,
            &state,
            source,
            destination,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(50_000),
        ) {
            Ok(transfer) => transfer,
            Err(error) => panic!("supported transfer validation failed: {error}"),
        };
        if let Err(error) = transfer.commit(&mut state) {
            panic!("supported transfer commit failed: {error}");
        }

        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .map(|record| record.stored_mass()),
            Some(Mass::from_milligrams(150_000))
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .map(|record| record.stored_mass()),
            Some(Mass::from_milligrams(50_000))
        );
        assert_eq!(
            state
                .structures()
                .get_element(source_support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(expected_weight(&registries, Mass::from_milligrams(150_000)))
        );
        assert_eq!(
            state
                .structures()
                .get_element(destination_support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(expected_weight(&registries, Mass::from_milligrams(50_000)))
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn supported_transfer_rejects_stale_structure_before_moving_matter() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A71_0003));
        let source_support = active_support(&registries, &mut state, 0);
        let destination_support = active_support(&registries, &mut state, 2);
        let source = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(300_000),
            Mass::from_milligrams(200_000),
        );
        let destination = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(300_000),
            Mass::ZERO,
        );
        mount(&registries, &mut state, source, source_support);
        mount(&registries, &mut state, destination, destination_support);
        let transfer = match validate_transfer_bulk(
            &registries,
            &state,
            source,
            destination,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(50_000),
        ) {
            Ok(transfer) => transfer,
            Err(error) => panic!("stale supported transfer validation failed: {error}"),
        };
        let source_before = state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass());
        let destination_before = state
            .inventory()
            .get_stockpile(destination)
            .map(|record| record.stored_mass());

        let snow = match validate_set_structural_load(
            &registries,
            &state,
            source_support,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(1),
        ) {
            Ok(snow) => snow,
            Err(error) => panic!("stale supported transfer mutation failed: {error}"),
        };
        if let Err(error) = snow.commit(&mut state) {
            panic!("stale supported transfer mutation commit failed: {error}");
        }

        assert!(matches!(
            transfer.commit(&mut state),
            Err(TransferCommitError::Structure(
                StructuralCommitError::StaleRevision {
                    expected: _expected,
                    actual: _actual,
                }
            ))
        ));
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .map(|record| record.stored_mass()),
            source_before
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .map(|record| record.stored_mass()),
            destination_before
        );
    }

    #[test]
    fn empty_stockpile_mount_rejects_stale_structure_without_a_load_delta() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A71_0007));
        let support = active_support(&registries, &mut state, 0);
        let stockpile = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(10),
            Mass::ZERO,
        );
        let mount = match validate_mount_stockpile(&registries, &state, stockpile, support) {
            Ok(mount) => mount,
            Err(error) => panic!("empty stale mount validation failed: {error}"),
        };

        let snow = match validate_set_structural_load(
            &registries,
            &state,
            support,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(1),
        ) {
            Ok(snow) => snow,
            Err(error) => panic!("empty stale mount structural mutation failed: {error}"),
        };
        if let Err(error) = snow.commit(&mut state) {
            panic!("empty stale mount structural mutation commit failed: {error}");
        }

        assert!(matches!(
            mount.commit(&mut state),
            Err(StockpileSupportCommitError::Structure(
                StructuralCommitError::StaleRevision {
                    expected: _expected,
                    actual: _actual,
                }
            ))
        ));
        assert_eq!(
            state
                .inventory()
                .get_stockpile(stockpile)
                .and_then(|record| record.supported_by()),
            None
        );
    }

    #[test]
    fn same_support_transfer_binds_structure_even_when_aggregate_weight_is_unchanged() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A71_0008));
        let support = active_support(&registries, &mut state, 0);
        let source = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(10),
            Mass::from_milligrams(2),
        );
        let destination = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(10),
            Mass::ZERO,
        );
        mount(&registries, &mut state, source, support);
        mount(&registries, &mut state, destination, support);
        let transfer = match validate_transfer_bulk(
            &registries,
            &state,
            source,
            destination,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1),
        ) {
            Ok(transfer) => transfer,
            Err(error) => panic!("same-support transfer validation failed: {error}"),
        };
        let before_source = state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass());
        let before_destination = state
            .inventory()
            .get_stockpile(destination)
            .map(|record| record.stored_mass());

        let snow = match validate_set_structural_load(
            &registries,
            &state,
            support,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(1),
        ) {
            Ok(snow) => snow,
            Err(error) => panic!("same-support stale mutation failed: {error}"),
        };
        if let Err(error) = snow.commit(&mut state) {
            panic!("same-support stale mutation commit failed: {error}");
        }

        assert!(matches!(
            transfer.commit(&mut state),
            Err(TransferCommitError::Structure(
                StructuralCommitError::StaleRevision {
                    expected: _expected,
                    actual: _actual,
                }
            ))
        ));
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .map(|record| record.stored_mass()),
            before_source
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .map(|record| record.stored_mass()),
            before_destination
        );
    }

    #[test]
    fn stored_matter_load_is_inventory_owned_and_blocks_support_removal() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A71_0004));
        let support = active_support(&registries, &mut state, 0);
        let stockpile = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(100),
            Mass::from_milligrams(10),
        );
        mount(&registries, &mut state, stockpile, support);

        assert_eq!(
            validate_set_structural_load(
                &registries,
                &state,
                support,
                StructuralLoadKind::StoredMatter,
                Force::ZERO,
            ),
            Err(StructuralMutationError::LoadOwnedBySubsystem {
                kind: StructuralLoadKind::StoredMatter,
            })
        );
        assert_eq!(
            validate_remove_structural_element(&registries, &state, support),
            Err(StructuralMutationError::ElementSupportsStockpile {
                element: support,
                stockpile,
            })
        );
    }

    #[test]
    fn overload_from_stored_matter_can_fail_support_and_failed_debris_can_be_unloaded() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A71_0005));
        let support = active_support(&registries, &mut state, 0);
        let mass = Mass::from_milligrams(5_000_000_000);
        let stockpile = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(6_000_000_000),
            mass,
        );

        let outcome = mount(&registries, &mut state, stockpile, support);
        assert!(outcome.structural_analysis().is_some());
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Failed)
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(stockpile)
                .and_then(|record| record.supported_by()),
            Some(support)
        );

        let source = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(1),
            Mass::from_milligrams(1),
        );
        let before_rejected_transfer = state.clone();
        assert!(matches!(
            validate_transfer_bulk(
                &registries,
                &state,
                source,
                stockpile,
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(1),
            ),
            Err(TransferError::StructuralLoad(
                StockpileStructuralLoadError::SupportNotActiveForIncrease {
                    stockpile: rejected_stockpile,
                    element,
                    lifecycle: StructuralLifecycle::Failed,
                }
            )) if rejected_stockpile == stockpile && element == support
        ));
        assert_eq!(state, before_rejected_transfer);

        let unmount = match validate_unmount_stockpile(&registries, &state, stockpile) {
            Ok(unmount) => unmount,
            Err(error) => panic!("failed-support unmount validation failed: {error}"),
        };
        if let Err(error) = unmount.commit(&mut state) {
            panic!("failed-support unmount commit failed: {error}");
        }
        assert_eq!(
            state
                .inventory()
                .get_stockpile(stockpile)
                .and_then(|record| record.supported_by()),
            None
        );
        assert_eq!(
            state.structures().get_element(support).map(|record| (
                record.lifecycle(),
                record.load(StructuralLoadKind::StoredMatter),
            )),
            Some((StructuralLifecycle::Failed, Force::ZERO))
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    fn run_supported_transfer_soak(seed: WorldSeed) -> AppState {
        let registries = build_registries();
        let mut state = AppState::new(seed);
        let left_support = active_support(&registries, &mut state, 0);
        let right_support = active_support(&registries, &mut state, 2);
        let left = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(10),
            Mass::from_milligrams(1),
        );
        let right = seeded_stockpile(
            &registries,
            &mut state,
            Mass::from_milligrams(10),
            Mass::ZERO,
        );
        mount(&registries, &mut state, left, left_support);
        mount(&registries, &mut state, right, right_support);

        for step in 0..1_000_u64 {
            let (source, destination) = if step.is_multiple_of(2) {
                (left, right)
            } else {
                (right, left)
            };
            let transfer = match validate_transfer_bulk(
                &registries,
                &state,
                source,
                destination,
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(1),
            ) {
                Ok(transfer) => transfer,
                Err(error) => {
                    panic!("supported transfer soak validation failed at {step}: {error}")
                }
            };
            if let Err(error) = transfer.commit(&mut state) {
                panic!("supported transfer soak commit failed at {step}: {error}");
            }
            if step.is_multiple_of(113) {
                assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
            }
        }
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        state
    }

    #[test]
    #[ignore = "long-horizon soak"]
    fn supported_transfer_soak_preserves_invariants_and_deterministic_replay() {
        let seed = WorldSeed::new(0x1A71_5000);
        let first = run_supported_transfer_soak(seed);
        let second = run_supported_transfer_soak(seed);
        assert_eq!(first, second);
    }
}
