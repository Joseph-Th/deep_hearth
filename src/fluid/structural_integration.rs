//! Derives structure-owned loads from supported fluid stores.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::quantity::Force;
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::structural::{
    StructuralAnalysis, StructuralElementId, StructuralLifecycle, StructuralLoadKind,
    StructuralMutationError, StructuralMutationOutcome, ValidatedStructuralLoadChange,
    calculate_fractional_milligram_weight_force_ceiling, validate_owned_structural_load_change,
};

use super::{FluidContents, FluidMassProjectionError, FluidStoreId, project_fluid_material_mass};

mod errors;

pub use errors::{FluidStructuralLoadError, FluidSupportCommitError, FluidSupportError};

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

fn contents_mass_micrograms(
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

fn supported_mass_micrograms(
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

fn support_force(
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

fn validate_existing_load(
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

/// Exhaustively rechecks one structure-owned `Fluid` contribution from authoritative fluid stores.
pub(crate) fn validate_existing_fluid_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<(), FluidStructuralLoadError> {
    validate_existing_load(registries, state, element).map(|_| ())
}

/// Resolves exact final `Fluid` loads for stores whose contents change together.
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

fn validate_structural_load_plan(
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

/// Successful fluid-store support change plus any resulting structural damage.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FluidSupportOutcome {
    structural: Option<StructuralMutationOutcome>,
}

impl FluidSupportOutcome {
    #[must_use]
    pub fn structural_analysis(&self) -> Option<&StructuralAnalysis> {
        self.structural
            .as_ref()
            .map(StructuralMutationOutcome::analysis)
    }
}

/// Consumed proof that fluid ownership and the corresponding aggregate structural load agree.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedFluidSupportChange {
    store: FluidStoreId,
    before: Option<StructuralElementId>,
    after: Option<StructuralElementId>,
    expected_fluid_revision: u64,
    next_fluid_revision: u64,
    structural: ValidatedFluidStructuralLoad,
}

impl ValidatedFluidSupportChange {
    /// Returns the precomputed structural consequence of this support change when load changes.
    #[must_use]
    pub fn structural_analysis(&self) -> Option<&StructuralAnalysis> {
        self.structural.analysis()
    }

    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<FluidSupportOutcome, FluidSupportCommitError> {
        let actual_revision = state.fluid().revision();
        if actual_revision != self.expected_fluid_revision {
            return Err(FluidSupportCommitError::StaleFluidRevision {
                expected: self.expected_fluid_revision,
                actual: actual_revision,
            });
        }
        let Some(record) = state.fluid().get_store(self.store) else {
            return Err(FluidSupportCommitError::UnknownStore { store: self.store });
        };
        if record.supported_by() != self.before {
            return Err(FluidSupportCommitError::SupportChanged {
                store: self.store,
                expected: self.before,
                actual: record.supported_by(),
            });
        }
        state.fluid().assert_support_change_available(
            self.store,
            self.before,
            self.after,
            self.next_fluid_revision,
        );
        let structural = self
            .structural
            .commit(state)
            .map_err(FluidSupportCommitError::Structure)?;
        state.fluid_state_mut().apply_support_change(
            self.store,
            self.before,
            self.after,
            self.next_fluid_revision,
        );
        Ok(FluidSupportOutcome { structural })
    }
}

fn next_fluid_revision(state: &AppState) -> Result<(u64, u64), FluidSupportError> {
    let current = state.fluid().revision();
    let next = current
        .checked_add(1)
        .ok_or(FluidSupportError::FluidRevisionExhausted)?;
    Ok((current, next))
}

/// Validates placing one finite fluid store on an active structural member.
pub fn validate_mount_fluid_store(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
    element: StructuralElementId,
) -> Result<ValidatedFluidSupportChange, FluidSupportError> {
    let record = state
        .fluid()
        .get_store(store)
        .ok_or(FluidSupportError::UnknownStore { store })?;
    if let Some(existing) = record.supported_by() {
        return Err(FluidSupportError::AlreadyMounted {
            store,
            element: existing,
        });
    }
    let target = state
        .structures()
        .get_element(element)
        .ok_or(FluidSupportError::Load(
            FluidStructuralLoadError::Structure(StructuralMutationError::UnknownElement {
                element,
            }),
        ))?;
    if target.lifecycle() != StructuralLifecycle::Active {
        return Err(FluidSupportError::TargetNotActive {
            element,
            lifecycle: target.lifecycle(),
        });
    }
    let current =
        validate_existing_load(registries, state, element).map_err(FluidSupportError::Load)?;
    let added = contents_mass_micrograms(registries, store, record.contents())
        .map_err(FluidSupportError::Load)?;
    let next = current.checked_add(added).ok_or(FluidSupportError::Load(
        FluidStructuralLoadError::AggregateMassOverflow { element },
    ))?;
    let load = support_force(registries, element, next).map_err(FluidSupportError::Load)?;
    let structural =
        validate_structural_load_plan(registries, state, BTreeMap::from([(element, load)]))
            .map_err(FluidSupportError::Load)?;
    let (expected_fluid_revision, next_fluid_revision) = next_fluid_revision(state)?;
    Ok(ValidatedFluidSupportChange {
        store,
        before: None,
        after: Some(element),
        expected_fluid_revision,
        next_fluid_revision,
        structural,
    })
}

/// Validates removing one fluid store from structural support. Failed debris may be drained and unloaded.
pub fn validate_unmount_fluid_store(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
) -> Result<ValidatedFluidSupportChange, FluidSupportError> {
    let record = state
        .fluid()
        .get_store(store)
        .ok_or(FluidSupportError::UnknownStore { store })?;
    let element = record
        .supported_by()
        .ok_or(FluidSupportError::NotMounted { store })?;
    if state.structures().get_element(element).is_none() {
        return Err(FluidSupportError::Load(
            FluidStructuralLoadError::UnknownSupport { store, element },
        ));
    }
    validate_existing_load(registries, state, element).map_err(FluidSupportError::Load)?;
    let remaining =
        supported_mass_micrograms(registries, state, element, &BTreeMap::new(), Some(store))
            .map_err(FluidSupportError::Load)?;
    let load = support_force(registries, element, remaining).map_err(FluidSupportError::Load)?;
    let structural =
        validate_structural_load_plan(registries, state, BTreeMap::from([(element, load)]))
            .map_err(FluidSupportError::Load)?;
    let (expected_fluid_revision, next_fluid_revision) = next_fluid_revision(state)?;
    Ok(ValidatedFluidSupportChange {
        store,
        before: Some(element),
        after: None,
        expected_fluid_revision,
        next_fluid_revision,
        structural,
    })
}

#[cfg(test)]
#[path = "structural_integration_tests.rs"]
mod tests;
