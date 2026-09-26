//! Coordinates stockpile support assignments with structure-owned stored-matter loads.

use std::collections::BTreeMap;

use crate::core::quantity::AggregateMass;
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::structural::{
    StructuralAnalysis, StructuralElementId, StructuralLifecycle, StructuralMutationError,
    StructuralMutationOutcome,
};

use super::StockpileId;

mod availability;
mod errors;
mod load;
mod projection;

use availability::{support_commit_error, support_validation_error};
pub use errors::{
    StockpileStructuralLoadError, StockpileSupportCommitError, StockpileSupportError,
};
use load::validate_stockpile_structural_load_plan;
pub(crate) use load::{
    StockpileStoredMassChange, ValidatedStockpileStructuralLoad,
    validate_reserved_stockpile_structural_load_headroom, validate_stockpile_stored_mass_changes,
    validate_unreserved_stockpile_structural_load_headroom,
};
pub(crate) use projection::{
    StockpileStructuralLoadConsistencyError, validate_existing_stockpile_structural_load,
};
use projection::{support_force, supported_mass_projection, validate_existing_load};

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

/// Successful support assignment change plus any resulting structural damage.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
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
        if self.after.is_some()
            && let Some(position) = state.logistics().ground_stockpile_position(self.stockpile)
        {
            return Err(StockpileSupportCommitError::GroundLocated {
                stockpile: self.stockpile,
                position,
            });
        }
        if let Some(error) = support_commit_error(state, self.stockpile) {
            return Err(error);
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
    if !state.can_spend_inventory_revisions(1) {
        return Err(StockpileSupportError::InventoryRevisionExhausted);
    }
    let next = current
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("inventory headroom check includes support revision"));
    Ok((current, next))
}

fn validate_not_busy(
    state: &AppState,
    stockpile: StockpileId,
) -> Result<(), StockpileSupportError> {
    support_validation_error(state, stockpile).map_or(Ok(()), Err)
}

fn validate_support_change_structural_headroom(
    state: &AppState,
    structural: &ValidatedStockpileStructuralLoad,
    additional_future_demand: u64,
    released_future_demand: u64,
) -> Result<(), StockpileSupportError> {
    if state.can_spend_structure_revisions_after_adjusting(
        structural.revision_delta(),
        additional_future_demand,
        released_future_demand,
    ) {
        Ok(())
    } else {
        Err(StockpileSupportError::Load(
            StockpileStructuralLoadError::Structure(StructuralMutationError::RevisionExhausted),
        ))
    }
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
    if state
        .logistics()
        .player()
        .is_some_and(|player| player.carried_stockpile() == stockpile)
    {
        return Err(StockpileSupportError::PlayerCarried { stockpile });
    }
    if let Some(position) = state.logistics().ground_stockpile_position(stockpile) {
        return Err(StockpileSupportError::GroundLocated {
            stockpile,
            position,
        });
    }
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
    validate_support_change_structural_headroom(
        state,
        &structural,
        state.retained_mining_claim_count_for_stockpile(stockpile),
        0,
    )?;
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
    validate_support_change_structural_headroom(
        state,
        &structural,
        0,
        state.retained_mining_claim_count_for_stockpile(stockpile),
    )?;
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
