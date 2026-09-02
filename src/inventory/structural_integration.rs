//! Coordinates stockpile support assignments with structure-owned stored-matter loads.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::quantity::{AggregateMass, Force, Mass};
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::structural::{
    StructuralAnalysis, StructuralElementId, StructuralLifecycle, StructuralLoadKind,
    StructuralMutationError, StructuralMutationOutcome, ValidatedStructuralLoadChange,
    validate_owned_structural_load_change,
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

mod errors;
mod projection;

pub use errors::{
    StockpileStructuralLoadError, StockpileSupportCommitError, StockpileSupportError,
};
use projection::{support_force, supported_mass_projection, validate_existing_load};

pub(crate) type ValidatedStockpileStructuralLoad = ValidatedStructuralLoadChange;

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
    validate_owned_structural_load_change(
        registries,
        state,
        StructuralLoadKind::StoredMatter,
        loads,
    )
    .map_err(StockpileStructuralLoadError::Structure)
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
        if state
            .player_work()
            .get_storage_dismantling_stockpile_occupant(self.stockpile)
            .is_some()
        {
            return Err(
                StockpileSupportCommitError::StockpileBusyStorageDismantling {
                    stockpile: self.stockpile,
                },
            );
        }
        state.inventory().assert_support_change_available(
            self.stockpile,
            self.before,
            self.after,
            self.next_inventory_revision,
        );

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
    if state
        .player_work()
        .get_storage_dismantling_stockpile_occupant(stockpile)
        .is_some()
    {
        return Err(StockpileSupportError::StockpileBusyStorageDismantling { stockpile });
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
