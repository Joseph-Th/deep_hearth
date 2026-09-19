//! Derives structure-owned loads from supported fluid stores.

use std::collections::BTreeMap;

#[cfg(test)]
use crate::core::quantity::Force;
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::structural::{
    StructuralAnalysis, StructuralElementId, StructuralLifecycle, StructuralMutationError,
    StructuralMutationOutcome,
};

use super::FluidStoreId;

mod errors;
mod load;

pub use errors::{FluidStructuralLoadError, FluidSupportCommitError, FluidSupportError};
pub(crate) use load::{
    FluidContentsChange, ValidatedFluidStructuralLoad, validate_existing_fluid_load,
    validate_fluid_contents_changes, validate_unreserved_fluid_structural_load_headroom,
};
use load::{
    contents_mass_micrograms, support_force, supported_mass_micrograms, validate_existing_load,
    validate_structural_load_plan,
};

/// Successful fluid-store support change plus any resulting structural damage.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
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
    validate_unreserved_fluid_structural_load_headroom(state, Some(&structural))
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
    validate_unreserved_fluid_structural_load_headroom(state, Some(&structural))
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
