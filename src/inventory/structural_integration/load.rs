//! Stored-mass projection into the structure-owned stockpile load channel.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::quantity::{Force, Mass};
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::structural::{
    StructuralElementId, StructuralLifecycle, StructuralLoadKind, StructuralMutationError,
    ValidatedStructuralLoadChange, validate_owned_structural_load_change,
};

use super::projection::{support_force, supported_mass_projection, validate_existing_load};
use super::{StockpileId, StockpileStructuralLoadError};

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

pub(crate) type ValidatedStockpileStructuralLoad = ValidatedStructuralLoadChange;

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

pub(super) fn validate_stockpile_structural_load_plan(
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

/// Preserves structural revision space already owed to admitted future work before an unrelated
/// immediate stockpile-load mutation is accepted.
pub(crate) fn validate_unreserved_stockpile_structural_load_headroom(
    state: &AppState,
    structural: Option<&ValidatedStockpileStructuralLoad>,
) -> Result<(), StockpileStructuralLoadError> {
    let immediate_steps = structural.map_or(0, ValidatedStockpileStructuralLoad::revision_delta);
    if state.can_spend_structure_revisions(immediate_steps) {
        Ok(())
    } else {
        Err(StockpileStructuralLoadError::Structure(
            StructuralMutationError::RevisionExhausted,
        ))
    }
}
