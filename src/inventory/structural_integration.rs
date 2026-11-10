//! Coordinates stockpile support assignments with structure-owned stored-matter loads.

use std::collections::BTreeMap;

use crate::core::state::AppState;
use crate::logistics::validate_player_stockpile_access;
use crate::player::{PlayerActionGuard, validate_optional_player_action};
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
use projection::{
    aggregate_stockpile_mass, support_force, supported_mass_projection, validate_existing_load,
};

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

fn production_support_revision_adjustment(
    state: &AppState,
    stockpile: StockpileId,
    supported_after: bool,
) -> (u64, u64) {
    state
        .production()
        .future_supported_output_revision_adjustment_for_stockpile(
            state.inventory(),
            stockpile,
            supported_after,
        )
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
    player_action: PlayerActionGuard,
    stockpile: StockpileId,
    before: Option<StructuralElementId>,
    after: Option<StructuralElementId>,
    expected_inventory_revision: u64,
    next_inventory_revision: u64,
    expected_logistics_revision: u64,
    expected_player_work_revision: u64,
    expected_combat_revision: u64,
    structural: ValidatedStockpileStructuralLoad,
}

impl ValidatedStockpileSupportChange {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StockpileSupportOutcome, StockpileSupportCommitError> {
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(StockpileSupportCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics_revision,
            });
        }
        self.player_action.precheck(state).map_err(|stale| {
            StockpileSupportCommitError::StaleSurvivalRevision {
                expected: stale.expected,
                actual: stale.actual,
            }
        })?;
        let actual_player_work = state.player_work().revision();
        if actual_player_work != self.expected_player_work_revision {
            return Err(StockpileSupportCommitError::StalePlayerWorkRevision {
                expected: self.expected_player_work_revision,
                actual: actual_player_work,
            });
        }
        let actual_combat = state.combat().revision();
        if actual_combat != self.expected_combat_revision {
            return Err(StockpileSupportCommitError::StaleCombatRevision {
                expected: self.expected_combat_revision,
                actual: actual_combat,
            });
        }
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

fn validate_direct_action(state: &AppState) -> Result<PlayerActionGuard, StockpileSupportError> {
    let action =
        validate_optional_player_action(state).map_err(|_| StockpileSupportError::PlayerDead)?;
    // Eating and drinking occupy the actor's hands. Other active work may
    // legitimately coexist with changes to independent structural support.
    if state.logistics().player().is_some()
        && matches!(
            state.player_work().active(),
            Some(
                crate::labor::PlayerWork::Eating { .. } | crate::labor::PlayerWork::Drinking { .. }
            )
        )
    {
        return Err(StockpileSupportError::PlayerBusy);
    }
    if state.logistics().player().is_some() && state.combat().has_prepared_strike() {
        return Err(StockpileSupportError::PreparedStrikeActive);
    }
    Ok(action)
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
    let player_action = validate_direct_action(state)?;
    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .ok_or(StockpileSupportError::UnknownStockpile { stockpile })?;
    validate_player_stockpile_access(state, stockpile).map_err(StockpileSupportError::Access)?;
    if state
        .logistics()
        .player()
        .is_some_and(|player| player.carried_stockpile() == stockpile)
    {
        return Err(StockpileSupportError::PlayerCarried { stockpile });
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
    if let Some(position) = state.logistics().stationary_stockpile_position(stockpile)
        && !target.bounds().has_voxel(position)
    {
        return Err(StockpileSupportError::StockpileNotOnTarget {
            stockpile,
            position,
            element,
        });
    }
    let mass = supported_mass_projection(state, element, &BTreeMap::new(), None)
        .map_err(StockpileSupportError::Load)?;
    validate_existing_load(registries, state, element, mass.current)
        .map_err(StockpileSupportError::Load)?;
    let stockpile_mass = aggregate_stockpile_mass(record.stored_mass(), record.embodied_mass());
    let next_mass = mass
        .current
        .checked_add(stockpile_mass)
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
    let (production_additional, production_released) =
        production_support_revision_adjustment(state, stockpile, true);
    let additional_future_demand = state
        .retained_mining_settlement_count_for_stockpile(stockpile)
        .checked_add(production_additional)
        .unwrap_or_else(|| unreachable!("resident retained structural demand fits u64"));
    validate_support_change_structural_headroom(
        state,
        &structural,
        additional_future_demand,
        production_released,
    )?;
    let (expected_inventory_revision, next_inventory_revision) = next_inventory_revision(state)?;
    Ok(ValidatedStockpileSupportChange {
        player_action,
        stockpile,
        before: None,
        after: Some(element),
        expected_inventory_revision,
        next_inventory_revision,
        expected_logistics_revision: state.logistics().revision(),
        expected_player_work_revision: state.player_work().revision(),
        expected_combat_revision: state.combat().revision(),
        structural,
    })
}

/// Validates removing a stockpile support assignment. Failed structural debris may be unloaded.
pub fn validate_unmount_stockpile(
    registries: &Registries,
    state: &AppState,
    stockpile: StockpileId,
) -> Result<ValidatedStockpileSupportChange, StockpileSupportError> {
    let player_action = validate_direct_action(state)?;
    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .ok_or(StockpileSupportError::UnknownStockpile { stockpile })?;
    validate_player_stockpile_access(state, stockpile).map_err(StockpileSupportError::Access)?;
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
    let (production_additional, production_released) =
        production_support_revision_adjustment(state, stockpile, false);
    let released_future_demand = state
        .retained_mining_settlement_count_for_stockpile(stockpile)
        .checked_add(production_released)
        .unwrap_or_else(|| unreachable!("resident released structural demand fits u64"));
    validate_support_change_structural_headroom(
        state,
        &structural,
        production_additional,
        released_future_demand,
    )?;
    let (expected_inventory_revision, next_inventory_revision) = next_inventory_revision(state)?;
    Ok(ValidatedStockpileSupportChange {
        player_action,
        stockpile,
        before: Some(element),
        after: None,
        expected_inventory_revision,
        next_inventory_revision,
        expected_logistics_revision: state.logistics().revision(),
        expected_player_work_revision: state.player_work().revision(),
        expected_combat_revision: state.combat().revision(),
        structural,
    })
}

#[cfg(test)]
#[path = "structural_integration_tests.rs"]
mod tests;
