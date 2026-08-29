//! Coordinates stockpile support assignments with structure-owned stored-matter loads.

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
}

impl StockpileStoredMassChange {
    #[must_use]
    pub(crate) const fn new(stockpile: StockpileId, stored_after: Mass) -> Self {
        Self {
            stockpile,
            stored_after,
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
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedStockpileStructuralLoad {
    expected_revision: u64,
    structural: Option<ValidatedStructuralLoadBatch>,
}

impl ValidatedStockpileStructuralLoad {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    #[cfg(any(test, feature = "test-gameplay"))]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SupportedMassProjection {
    current: AggregateMass,
    projected: AggregateMass,
}

fn supported_mass_projection(
    state: &AppState,
    element: StructuralElementId,
    overrides: &BTreeMap<StockpileId, Mass>,
    excluded: Option<StockpileId>,
) -> Result<SupportedMassProjection, StockpileStructuralLoadError> {
    let mut current = AggregateMass::ZERO;
    let mut projected = AggregateMass::ZERO;
    for stockpile in state.inventory().supported_stockpiles(element) {
        let record = state
            .inventory()
            .get_stockpile(stockpile)
            .ok_or(StockpileStructuralLoadError::UnknownStockpile { stockpile })?;
        let current_mass = record
            .stored_mass()
            .checked_add(record.embodied_mass())
            .ok_or(StockpileStructuralLoadError::AggregateMassOverflow { element })?;
        current = current
            .checked_add(AggregateMass::from_mass(current_mass))
            .ok_or(StockpileStructuralLoadError::AggregateMassOverflow { element })?;

        if excluded == Some(stockpile) {
            continue;
        }
        let projected_stored_mass = overrides
            .get(&stockpile)
            .copied()
            .unwrap_or_else(|| record.stored_mass());
        let projected_mass = projected_stored_mass
            .checked_add(record.embodied_mass())
            .ok_or(StockpileStructuralLoadError::AggregateMassOverflow { element })?;
        projected = projected
            .checked_add(AggregateMass::from_mass(projected_mass))
            .ok_or(StockpileStructuralLoadError::AggregateMassOverflow { element })?;
    }
    Ok(SupportedMassProjection { current, projected })
}

fn validate_existing_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    current_mass: AggregateMass,
) -> Result<(), StockpileStructuralLoadError> {
    let expected = support_force(registries, element, current_mass)?;
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

fn collect_stored_mass_changes(
    state: &AppState,
    changes: impl IntoIterator<Item = StockpileStoredMassChange>,
) -> Result<
    (BTreeMap<StockpileId, Mass>, BTreeSet<StructuralElementId>),
    StockpileStructuralLoadError,
> {
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
        let Some(support) = record.supported_by() else {
            continue;
        };
        let support_record = state.structures().get_element(support).ok_or(
            StockpileStructuralLoadError::UnknownSupport {
                stockpile: change.stockpile,
                element: support,
            },
        )?;
        if change.stored_after > record.stored_mass()
            && support_record.lifecycle() != StructuralLifecycle::Active
        {
            return Err(StockpileStructuralLoadError::SupportNotActiveForIncrease {
                stockpile: change.stockpile,
                element: support,
                lifecycle: support_record.lifecycle(),
            });
        }
        affected_supports.insert(support);
    }
    Ok((overrides, affected_supports))
}

/// Resolves the exact final structure-owned loads implied by final stockpile masses.
pub(crate) fn resolve_stockpile_stored_loads(
    registries: &Registries,
    state: &AppState,
    changes: impl IntoIterator<Item = StockpileStoredMassChange>,
) -> Result<BTreeMap<StructuralElementId, Force>, StockpileStructuralLoadError> {
    let (overrides, affected_supports) = collect_stored_mass_changes(state, changes)?;

    let mut loads = BTreeMap::new();
    for element in affected_supports {
        let mass = supported_mass_projection(state, element, &overrides, None)?;
        validate_existing_load(registries, state, element, mass.current)?;
        loads.insert(element, support_force(registries, element, mass.projected)?);
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
                "stockpile {} is an in-flight output destination for production job {} {release} and cannot be moved",
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
                "stockpile {} became an in-flight output destination for production job {} {release} before support commit",
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
#[must_use]
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
#[derive(Debug, PartialEq, Eq)]
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
        if let Some(job) = state
            .production()
            .get_running_output_stockpile_occupant(self.stockpile)
        {
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
    if let Some(job) = state
        .production()
        .get_running_output_stockpile_occupant(stockpile)
    {
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
    let mass = supported_mass_projection(state, element, &BTreeMap::new(), None)
        .map_err(StockpileSupportError::Load)?;
    validate_existing_load(registries, state, element, mass.current)
        .map_err(StockpileSupportError::Load)?;
    let stockpile_mass = record
        .stored_mass()
        .checked_add(record.embodied_mass())
        .ok_or(StockpileSupportError::Load(
            StockpileStructuralLoadError::AggregateMassOverflow { element },
        ))?;
    let next_mass = mass
        .current
        .checked_add(AggregateMass::from_mass(stockpile_mass))
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
    let mass = supported_mass_projection(state, element, &BTreeMap::new(), Some(stockpile))
        .map_err(StockpileSupportError::Load)?;
    validate_existing_load(registries, state, element, mass.current)
        .map_err(StockpileSupportError::Load)?;
    let next_load =
        support_force(registries, element, mass.projected).map_err(StockpileSupportError::Load)?;
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
#[path = "structural_integration_tests.rs"]
mod tests;
