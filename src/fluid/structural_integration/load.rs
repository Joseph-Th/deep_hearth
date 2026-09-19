//! Fluid-owned structural-load reconstruction, projection, and consistency checks.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::quantity::Force;
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::structural::{
    StructuralElementId, StructuralLifecycle, StructuralLoadKind, StructuralMutationError,
    ValidatedStructuralLoadChange, calculate_fractional_milligram_weight_force_ceiling,
    validate_owned_structural_load_change,
};

use crate::fluid::{
    FluidContents, FluidMassProjectionError, FluidStoreId, project_fluid_material_mass,
};

use super::FluidStructuralLoadError;

const MICROGRAMS_PER_MILLIGRAM: u32 = 1_000;

/// Final contents of one store after a validated fluid-owner mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FluidContentsChange {
    store: FluidStoreId,
    contents_after: Option<FluidContents>,
}

impl FluidContentsChange {
    #[must_use]
    pub(crate) const fn new(store: FluidStoreId, contents_after: Option<FluidContents>) -> Self {
        Self {
            store,
            contents_after,
        }
    }
}

pub(super) fn contents_mass_micrograms(
    registries: &Registries,
    store: FluidStoreId,
    contents: Option<FluidContents>,
) -> Result<u128, FluidStructuralLoadError> {
    let Some(contents) = contents else {
        return Ok(0);
    };
    project_fluid_material_mass(registries, store, contents)
        .map(|mass| mass.micrograms())
        .map_err(|error| match error {
            FluidMassProjectionError::UnknownDefinition { store, definition } => {
                FluidStructuralLoadError::UnknownFluidDefinition { store, definition }
            }
        })
}

pub(super) fn supported_mass_micrograms(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    overrides: &BTreeMap<FluidStoreId, Option<FluidContents>>,
    excluded: Option<FluidStoreId>,
) -> Result<u128, FluidStructuralLoadError> {
    let mut total = 0_u128;
    for store in state.fluid().supported_stores(element) {
        if excluded == Some(store) {
            continue;
        }
        let record = state
            .fluid()
            .get_store(store)
            .ok_or(FluidStructuralLoadError::UnknownStore { store })?;
        let contents = overrides.get(&store).copied().unwrap_or(record.contents());
        let micrograms = contents_mass_micrograms(registries, store, contents)?;
        total = total
            .checked_add(micrograms)
            .ok_or(FluidStructuralLoadError::AggregateMassOverflow { element })?;
    }
    Ok(total)
}

pub(super) fn support_force(
    registries: &Registries,
    element: StructuralElementId,
    mass_micrograms: u128,
) -> Result<Force, FluidStructuralLoadError> {
    calculate_fractional_milligram_weight_force_ceiling(
        mass_micrograms,
        MICROGRAMS_PER_MILLIGRAM,
        registries.core().gravity(),
    )
    .ok_or(FluidStructuralLoadError::WeightForceOverflow { element })
}

pub(super) fn validate_existing_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<u128, FluidStructuralLoadError> {
    let micrograms = supported_mass_micrograms(registries, state, element, &BTreeMap::new(), None)?;
    let expected = support_force(registries, element, micrograms)?;
    let stored = state
        .structures()
        .get_element(element)
        .ok_or(FluidStructuralLoadError::Structure(
            StructuralMutationError::UnknownElement { element },
        ))?
        .load(StructuralLoadKind::Fluid);
    if stored != expected {
        return Err(FluidStructuralLoadError::ExistingLoadMismatch {
            element,
            stored,
            expected,
        });
    }
    Ok(micrograms)
}

/// Exhaustively rechecks one structure-owned Fluid contribution from authoritative fluid stores.
pub(crate) fn validate_existing_fluid_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<(), FluidStructuralLoadError> {
    validate_existing_load(registries, state, element).map(|_| ())
}

/// Resolves exact final Fluid loads for stores whose contents change together.
pub(crate) fn resolve_fluid_structural_loads(
    registries: &Registries,
    state: &AppState,
    changes: impl IntoIterator<Item = FluidContentsChange>,
) -> Result<BTreeMap<StructuralElementId, Force>, FluidStructuralLoadError> {
    let mut overrides = BTreeMap::new();
    let mut affected_supports = BTreeSet::new();
    for change in changes {
        let record = state.fluid().get_store(change.store).ok_or(
            FluidStructuralLoadError::UnknownStore {
                store: change.store,
            },
        )?;
        if overrides
            .insert(change.store, change.contents_after)
            .is_some()
        {
            panic!(
                "fluid contents change set contains duplicate store {}",
                change.store.value()
            );
        }
        let Some(element) = record.supported_by() else {
            continue;
        };
        state.structures().get_element(element).ok_or(
            FluidStructuralLoadError::UnknownSupport {
                store: change.store,
                element,
            },
        )?;
        affected_supports.insert(element);
    }

    let mut loads = BTreeMap::new();
    for element in affected_supports {
        let before = validate_existing_load(registries, state, element)?;
        let after = supported_mass_micrograms(registries, state, element, &overrides, None)?;
        let support = match state.structures().get_element(element) {
            Some(support) => support,
            None => unreachable!("affected fluid support existence was prevalidated"),
        };
        if after > before && support.lifecycle() != StructuralLifecycle::Active {
            return Err(FluidStructuralLoadError::SupportNotActiveForIncrease {
                element,
                lifecycle: support.lifecycle(),
            });
        }
        loads.insert(element, support_force(registries, element, after)?);
    }
    Ok(loads)
}

pub(crate) type ValidatedFluidStructuralLoad = ValidatedStructuralLoadChange;

pub(super) fn validate_structural_load_plan(
    registries: &Registries,
    state: &AppState,
    loads: BTreeMap<StructuralElementId, Force>,
) -> Result<ValidatedFluidStructuralLoad, FluidStructuralLoadError> {
    validate_owned_structural_load_change(registries, state, StructuralLoadKind::Fluid, loads)
        .map_err(FluidStructuralLoadError::Structure)
}

/// Validates all structure-owned fluid weight changes implied by final store contents.
pub(crate) fn validate_fluid_contents_changes(
    registries: &Registries,
    state: &AppState,
    changes: impl IntoIterator<Item = FluidContentsChange>,
) -> Result<Option<ValidatedFluidStructuralLoad>, FluidStructuralLoadError> {
    let loads = resolve_fluid_structural_loads(registries, state, changes)?;
    if loads.is_empty() {
        return Ok(None);
    }
    validate_structural_load_plan(registries, state, loads).map(Some)
}

pub(crate) fn validate_unreserved_fluid_structural_load_headroom(
    state: &AppState,
    structural: Option<&ValidatedFluidStructuralLoad>,
) -> Result<(), FluidStructuralLoadError> {
    let immediate_steps = structural.map_or(0, ValidatedFluidStructuralLoad::revision_delta);
    if state.can_spend_structure_revisions(immediate_steps) {
        Ok(())
    } else {
        Err(FluidStructuralLoadError::Structure(
            StructuralMutationError::RevisionExhausted,
        ))
    }
}
