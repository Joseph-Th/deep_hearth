//! Fluid-store structural support integration; fluid ownership derives one aggregate structural load per support.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{AggregateMass, Force};
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::structural::{
    StructuralAnalysis, StructuralCommitError, StructuralElementId, StructuralLifecycle,
    StructuralLoadKind, StructuralMutationError, StructuralMutationOutcome,
    ValidatedStructuralLoadBatch, calculate_aggregate_weight_force_ceiling,
    validate_set_owned_structural_loads,
};

use super::{FluidContents, FluidDefinitionId, FluidStoreId};

const MICROLITERS_DENSITY_PER_MILLIGRAM: u128 = 1_000;

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

/// Failure while deriving structure-owned load from supported finite fluid ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FluidStructuralLoadError {
    UnknownStore {
        store: FluidStoreId,
    },
    UnknownSupport {
        store: FluidStoreId,
        element: StructuralElementId,
    },
    UnknownFluidDefinition {
        store: FluidStoreId,
        definition: FluidDefinitionId,
    },
    SupportNotActiveForIncrease {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    StoreMassNumeratorOverflow {
        store: FluidStoreId,
    },
    AggregateMassNumeratorOverflow {
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

impl Display for FluidStructuralLoadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStore { store } => {
                write!(formatter, "unknown fluid store {}", store.value())
            }
            Self::UnknownSupport { store, element } => write!(
                formatter,
                "fluid store {} references missing structural support {}",
                store.value(),
                element.value()
            ),
            Self::UnknownFluidDefinition { store, definition } => write!(
                formatter,
                "fluid store {} references missing fluid definition {} while deriving structural load",
                store.value(),
                definition.value()
            ),
            Self::SupportNotActiveForIncrease { element, lifecycle } => write!(
                formatter,
                "aggregate fluid weight cannot increase while structural support {} is {lifecycle:?}",
                element.value()
            ),
            Self::StoreMassNumeratorOverflow { store } => write!(
                formatter,
                "fluid store {} volume-density product overflowed mass accounting",
                store.value()
            ),
            Self::AggregateMassNumeratorOverflow { element } => write!(
                formatter,
                "supported fluid mass calculation overflowed on structural element {}",
                element.value()
            ),
            Self::WeightForceOverflow { element } => write!(
                formatter,
                "supported fluid weight exceeds structural force range on element {}",
                element.value()
            ),
            Self::ExistingLoadMismatch {
                element,
                stored,
                expected,
            } => write!(
                formatter,
                "structural element {} stores {} mN fluid load but supported fluid ownership requires {} mN",
                element.value(),
                stored.millinewtons(),
                expected.millinewtons()
            ),
            Self::Structure(error) => write!(formatter, "fluid structural load failed: {error}"),
        }
    }
}

impl Error for FluidStructuralLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::UnknownStore { store: _store }
            | Self::StoreMassNumeratorOverflow { store: _store } => None,
            Self::UnknownSupport {
                store: _store,
                element: _element,
            } => None,
            Self::UnknownFluidDefinition {
                store: _store,
                definition: _definition,
            } => None,
            Self::SupportNotActiveForIncrease {
                element: _element,
                lifecycle: _lifecycle,
            } => None,
            Self::AggregateMassNumeratorOverflow { element: _element }
            | Self::WeightForceOverflow { element: _element } => None,
            Self::ExistingLoadMismatch {
                element: _element,
                stored: _stored,
                expected: _expected,
            } => None,
        }
    }
}

fn contents_mass_numerator(
    registries: &Registries,
    store: FluidStoreId,
    contents: Option<FluidContents>,
) -> Result<u128, FluidStructuralLoadError> {
    let Some(contents) = contents else {
        return Ok(0);
    };
    let definition = registries.fluid().get_fluid(contents.fluid()).ok_or(
        FluidStructuralLoadError::UnknownFluidDefinition {
            store,
            definition: contents.fluid(),
        },
    )?;
    u128::from(contents.volume().microliters())
        .checked_mul(u128::from(definition.density_kg_per_m3()))
        .ok_or(FluidStructuralLoadError::StoreMassNumeratorOverflow { store })
}

fn supported_mass_numerator(
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
        let numerator = contents_mass_numerator(registries, store, contents)?;
        total = total
            .checked_add(numerator)
            .ok_or(FluidStructuralLoadError::AggregateMassNumeratorOverflow { element })?;
    }
    Ok(total)
}

fn numerator_to_mass(numerator: u128) -> AggregateMass {
    let milligrams = if numerator == 0 {
        0
    } else {
        1 + (numerator - 1) / MICROLITERS_DENSITY_PER_MILLIGRAM
    };
    AggregateMass::from_milligrams(milligrams)
}

fn support_force(
    registries: &Registries,
    element: StructuralElementId,
    mass_numerator: u128,
) -> Result<Force, FluidStructuralLoadError> {
    calculate_aggregate_weight_force_ceiling(
        numerator_to_mass(mass_numerator),
        registries.core().gravity(),
    )
    .ok_or(FluidStructuralLoadError::WeightForceOverflow { element })
}

fn validate_existing_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<u128, FluidStructuralLoadError> {
    let numerator = supported_mass_numerator(registries, state, element, &BTreeMap::new(), None)?;
    let expected = support_force(registries, element, numerator)?;
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
    Ok(numerator)
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
        let after = supported_mass_numerator(registries, state, element, &overrides, None)?;
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

/// Revision guard plus any actual structural load mutation required by a fluid-owner transaction.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedFluidStructuralLoad {
    expected_revision: u64,
    structural: Option<ValidatedStructuralLoadBatch>,
}

impl ValidatedFluidStructuralLoad {
    #[must_use]
    pub(crate) fn analysis(&self) -> Option<&StructuralAnalysis> {
        self.structural
            .as_ref()
            .map(ValidatedStructuralLoadBatch::analysis)
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

fn validate_structural_load_plan(
    registries: &Registries,
    state: &AppState,
    loads: BTreeMap<StructuralElementId, Force>,
) -> Result<ValidatedFluidStructuralLoad, FluidStructuralLoadError> {
    let expected_revision = state.structures().revision();
    let structural =
        validate_set_owned_structural_loads(registries, state, StructuralLoadKind::Fluid, loads)
            .map_err(FluidStructuralLoadError::Structure)?;
    Ok(ValidatedFluidStructuralLoad {
        expected_revision,
        structural,
    })
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

/// Failure while assigning or removing a fluid store's structural support.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FluidSupportError {
    UnknownStore {
        store: FluidStoreId,
    },
    AlreadyMounted {
        store: FluidStoreId,
        element: StructuralElementId,
    },
    NotMounted {
        store: FluidStoreId,
    },
    TargetNotActive {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    FluidRevisionExhausted,
    Load(FluidStructuralLoadError),
}

impl Display for FluidSupportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStore { store } => {
                write!(formatter, "unknown fluid store {}", store.value())
            }
            Self::AlreadyMounted { store, element } => write!(
                formatter,
                "fluid store {} is already supported by structural element {}",
                store.value(),
                element.value()
            ),
            Self::NotMounted { store } => write!(
                formatter,
                "fluid store {} has no structural support assignment to remove",
                store.value()
            ),
            Self::TargetNotActive { element, lifecycle } => write!(
                formatter,
                "structural element {} is {lifecycle:?} and cannot receive a fluid store",
                element.value()
            ),
            Self::FluidRevisionExhausted => {
                formatter.write_str("fluid state revision space is exhausted")
            }
            Self::Load(error) => write!(formatter, "fluid store support load failed: {error}"),
        }
    }
}

impl Error for FluidSupportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Load(error) => Some(error),
            Self::UnknownStore { store: _store } | Self::NotMounted { store: _store } => None,
            Self::AlreadyMounted {
                store: _store,
                element: _element,
            } => None,
            Self::TargetNotActive {
                element: _element,
                lifecycle: _lifecycle,
            } => None,
            Self::FluidRevisionExhausted => None,
        }
    }
}

/// Failure to commit a revision-bound fluid-store support transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FluidSupportCommitError {
    StaleFluidRevision {
        expected: u64,
        actual: u64,
    },
    UnknownStore {
        store: FluidStoreId,
    },
    SupportChanged {
        store: FluidStoreId,
        expected: Option<StructuralElementId>,
        actual: Option<StructuralElementId>,
    },
    Structure(StructuralCommitError),
}

impl Display for FluidSupportCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleFluidRevision { expected, actual } => write!(
                formatter,
                "validated fluid support change expected fluid revision {expected} but current revision is {actual}"
            ),
            Self::UnknownStore { store } => write!(
                formatter,
                "fluid store {} disappeared before support commit",
                store.value()
            ),
            Self::SupportChanged {
                store,
                expected,
                actual,
            } => write!(
                formatter,
                "fluid store {} support changed from expected {expected:?} to {actual:?} before commit",
                store.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "fluid store support structural commit failed: {error}"
            ),
        }
    }
}

impl Error for FluidSupportCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleFluidRevision {
                expected: _expected,
                actual: _actual,
            } => None,
            Self::UnknownStore { store: _store } => None,
            Self::SupportChanged {
                store: _store,
                expected: _expected,
                actual: _actual,
            } => None,
        }
    }
}

/// Successful fluid-store support change plus any resulting structural damage.
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
    let added = contents_mass_numerator(registries, store, record.contents())
        .map_err(FluidSupportError::Load)?;
    let next = current.checked_add(added).ok_or(FluidSupportError::Load(
        FluidStructuralLoadError::AggregateMassNumeratorOverflow { element },
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
        supported_mass_numerator(registries, state, element, &BTreeMap::new(), Some(store))
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

#[cfg(all(
    test,
    any(not(feature = "test-unit-sharded"), feature = "test-unit-resources")
))]
mod tests {
    use super::*;
    use crate::content::{
        FORM_LOG, MATERIAL_COPPER, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        make_test_registries_with_fluids,
    };
    use crate::core::quantity::{Area, Volume};
    use crate::core::state::{StateValidationError, validate_loaded_state};
    use crate::core::time::WorldSeed;

    #[cfg(feature = "test-soak")]
    use crate::fluid::calculate_fluid_volume_accounting;
    use crate::fluid::{FluidDefinition, FluidDefinitionId, FluidTransferError};
    use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
    use crate::spatial::{VoxelBounds, VoxelCoord};
    use crate::structural::{
        StructuralLoadKind, StructuralMutationError, add_structural_element,
        materialize_structural_element_for_test, validate_activate_structural_element,
        validate_remove_structural_element, validate_set_structural_load,
    };

    use super::super::storage_execution::{
        add_fluid_store, add_fluid_store_with_contents_for_fixture,
        make_test_fluid_transfer_resolution, validate_fluid_transfer,
    };

    const TEST_FLUID: FluidDefinitionId = FluidDefinitionId::new(941_001);
    const TEST_TEMPERATURE: crate::core::quantity::Temperature =
        crate::core::quantity::Temperature::from_millikelvin(293_150);

    fn registries(density_kg_per_m3: u32) -> Registries {
        make_test_registries_with_fluids(vec![FluidDefinition::new(
            TEST_FLUID,
            "structural fluid fixture",
            MATERIAL_COPPER,
            density_kg_per_m3,
        )])
    }

    fn bounds(x: i64) -> VoxelBounds {
        match VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("fluid structural bounds fixture failed: {error}"),
        }
    }

    fn add_active_support(
        registries: &Registries,
        state: &mut AppState,
        x: i64,
    ) -> StructuralElementId {
        let element = match add_structural_element(
            registries,
            state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            crate::structural::make_test_structural_geometry(
                bounds(x),
                crate::core::quantity::Length::from_micrometers(1),
                Area::from_square_millimeters(1_000),
            ),
            true,
        ) {
            Ok(element) => element,
            Err(error) => panic!("fluid structural support fixture failed: {error}"),
        };
        materialize_structural_element_for_test(registries, state, element, FORM_LOG);
        let activation = match validate_activate_structural_element(registries, state, element) {
            Ok(token) => token,
            Err(error) => panic!("fluid structural activation fixture failed: {error}"),
        };
        if let Err(error) = activation.commit(state) {
            panic!("fluid structural activation commit failed: {error}");
        }
        element
    }

    fn add_filled(
        registries: &Registries,
        state: &mut AppState,
        volume_microliters: u64,
    ) -> FluidStoreId {
        match add_fluid_store_with_contents_for_fixture(
            registries,
            state,
            Volume::from_microliters(volume_microliters),
            TEST_FLUID,
            Volume::from_microliters(volume_microliters),
            TEST_TEMPERATURE,
        ) {
            Ok(store) => store,
            Err(error) => panic!("fluid structural filled-store fixture failed: {error}"),
        }
    }

    fn mount(
        registries: &Registries,
        state: &mut AppState,
        store: FluidStoreId,
        support: StructuralElementId,
    ) -> FluidSupportOutcome {
        let token = match validate_mount_fluid_store(registries, state, store, support) {
            Ok(token) => token,
            Err(error) => panic!("fluid support mount validation failed: {error}"),
        };
        match token.commit(state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("fluid support mount commit failed: {error}"),
        }
    }

    #[test]
    fn mounted_fluid_uses_authored_density_for_structural_weight() {
        let registries = registries(1_000);
        let mut state = AppState::new(WorldSeed::new(0x9410_0001));
        let support = add_active_support(&registries, &mut state, 0);
        let store = add_filled(&registries, &mut state, 1_000_000);

        let outcome = mount(&registries, &mut state, store, support);
        let expected = match calculate_aggregate_weight_force_ceiling(
            AggregateMass::from_milligrams(1_000_000),
            registries.core().gravity(),
        ) {
            Some(force) => force,
            None => panic!("fluid structural fixture weight overflowed"),
        };

        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.load(StructuralLoadKind::Fluid)),
            Some(expected)
        );
        assert_eq!(
            state
                .fluid()
                .get_store(store)
                .and_then(|record| record.supported_by()),
            Some(support)
        );
        assert!(outcome.structural_analysis().is_some());
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn fluid_mass_rounding_occurs_after_support_local_aggregation() {
        let registries = registries(1);
        let mut state = AppState::new(WorldSeed::new(0x9410_0002));
        let support = add_active_support(&registries, &mut state, 0);
        for _ in 0..102 {
            let store = add_filled(&registries, &mut state, 1);
            mount(&registries, &mut state, store, support);
        }

        assert_eq!(
            supported_mass_numerator(&registries, &state, support, &BTreeMap::new(), None),
            Ok(102)
        );
        assert_eq!(numerator_to_mass(102).milligrams(), 1);
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.load(StructuralLoadKind::Fluid)),
            Some(Force::from_millinewtons(1))
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn direct_fluid_load_write_and_supported_member_removal_are_blocked() {
        let registries = registries(1_000);
        let mut state = AppState::new(WorldSeed::new(0x9410_0003));
        let support = add_active_support(&registries, &mut state, 0);
        let store = add_filled(&registries, &mut state, 1_000);
        mount(&registries, &mut state, store, support);

        assert_eq!(
            validate_set_structural_load(
                &registries,
                &state,
                support,
                StructuralLoadKind::Fluid,
                Force::ZERO,
            ),
            Err(StructuralMutationError::LoadOwnedBySubsystem {
                kind: StructuralLoadKind::Fluid,
            })
        );
        assert_eq!(
            validate_remove_structural_element(&registries, &state, support),
            Err(StructuralMutationError::ElementSupportsFluidStore {
                element: support,
                store,
            })
        );
    }

    #[test]
    fn failed_support_can_be_drained_but_cannot_receive_new_fluid_weight() {
        let registries = registries(1_000);
        let mut state = AppState::new(WorldSeed::new(0x9410_0004));
        let support = add_active_support(&registries, &mut state, 0);
        let source = add_filled(&registries, &mut state, 5_000_000_000);
        let outcome = mount(&registries, &mut state, source, support);
        assert!(
            outcome
                .structural_analysis()
                .is_some_and(|analysis| !analysis.damage_events().is_empty())
        );
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Failed)
        );
        let destination = match add_fluid_store(&mut state, Volume::from_microliters(5_000_000_000))
        {
            Ok(store) => store,
            Err(error) => panic!("fluid drain destination fixture failed: {error}"),
        };

        let drain = match validate_fluid_transfer(
            &registries,
            &state,
            make_test_fluid_transfer_resolution(
                source,
                destination,
                Volume::from_microliters(1_000_000),
            ),
        ) {
            Ok(token) => token,
            Err(error) => panic!("failed-support drain validation failed: {error}"),
        };
        if let Err(error) = drain.commit(&mut state) {
            panic!("failed-support drain commit failed: {error}");
        }

        assert!(matches!(
            validate_fluid_transfer(
                &registries,
                &state,
                make_test_fluid_transfer_resolution(
                    destination,
                    source,
                    Volume::from_microliters(1),
                ),
            ),
            Err(FluidTransferError::StructuralLoad(
                FluidStructuralLoadError::SupportNotActiveForIncrease {
                    element,
                    lifecycle: StructuralLifecycle::Failed,
                }
            )) if element == support
        ));

        let unmount = match validate_unmount_fluid_store(&registries, &state, source) {
            Ok(token) => token,
            Err(error) => panic!("failed-support unmount validation failed: {error}"),
        };
        if let Err(error) = unmount.commit(&mut state) {
            panic!("failed-support unmount commit failed: {error}");
        }
        assert_eq!(
            state
                .fluid()
                .get_store(source)
                .and_then(|record| record.supported_by()),
            None
        );
    }

    #[test]
    fn fluid_transfer_can_collapse_destination_support_and_reports_damage() {
        let registries = registries(1_000);
        let mut state = AppState::new(WorldSeed::new(0x9410_0010));
        let support = add_active_support(&registries, &mut state, 0);
        let source = add_filled(&registries, &mut state, 5_000_000_000);
        let destination = match add_fluid_store(&mut state, Volume::from_microliters(5_000_000_000))
        {
            Ok(store) => store,
            Err(error) => panic!("fluid collapse destination fixture failed: {error}"),
        };
        mount(&registries, &mut state, destination, support);

        let token = match validate_fluid_transfer(
            &registries,
            &state,
            make_test_fluid_transfer_resolution(
                source,
                destination,
                Volume::from_microliters(5_000_000_000),
            ),
        ) {
            Ok(token) => token,
            Err(error) => panic!("fluid collapse transfer validation failed: {error}"),
        };
        let outcome = match token.commit(&mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("fluid collapse transfer commit failed: {error}"),
        };

        assert!(
            outcome
                .structural_analysis()
                .is_some_and(|analysis| !analysis.damage_events().is_empty())
        );
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Failed)
        );
        assert_eq!(
            state
                .fluid()
                .get_store(destination)
                .map(|record| record.stored_volume()),
            Some(Volume::from_microliters(5_000_000_000))
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn failed_support_allows_same_support_redistribution_without_added_weight() {
        let registries = registries(1_000);
        let mut state = AppState::new(WorldSeed::new(0x9410_0011));
        let support = add_active_support(&registries, &mut state, 0);
        let source = add_filled(&registries, &mut state, 1_000);
        let destination = match add_fluid_store(&mut state, Volume::from_microliters(1_000)) {
            Ok(store) => store,
            Err(error) => panic!("failed redistribution destination fixture failed: {error}"),
        };
        mount(&registries, &mut state, source, support);
        mount(&registries, &mut state, destination, support);
        let overload = match validate_set_structural_load(
            &registries,
            &state,
            support,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(50_000_000),
        ) {
            Ok(token) => token,
            Err(error) => panic!("failed redistribution overload validation failed: {error}"),
        };
        if let Err(error) = overload.commit(&mut state) {
            panic!("failed redistribution overload commit failed: {error}");
        }
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Failed)
        );
        let load_before = state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::Fluid));

        let token = match validate_fluid_transfer(
            &registries,
            &state,
            make_test_fluid_transfer_resolution(source, destination, Volume::from_microliters(100)),
        ) {
            Ok(token) => token,
            Err(error) => panic!("failed same-support redistribution rejected: {error}"),
        };
        let outcome = match token.commit(&mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("failed same-support redistribution commit failed: {error}"),
        };

        assert!(outcome.structural_analysis().is_none());
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.load(StructuralLoadKind::Fluid)),
            load_before
        );
        assert_eq!(
            state
                .fluid()
                .get_store(source)
                .map(|record| record.stored_volume()),
            Some(Volume::from_microliters(900))
        );
        assert_eq!(
            state
                .fluid()
                .get_store(destination)
                .map(|record| record.stored_volume()),
            Some(Volume::from_microliters(100))
        );
    }

    #[test]
    fn transfer_binds_structure_even_when_support_local_weight_is_unchanged() {
        let registries = registries(1_000);
        let mut state = AppState::new(WorldSeed::new(0x9410_0005));
        let support = add_active_support(&registries, &mut state, 0);
        let source = add_filled(&registries, &mut state, 1_000_000);
        let destination = match add_fluid_store(&mut state, Volume::from_microliters(1_000_000)) {
            Ok(store) => store,
            Err(error) => panic!("same-support destination fixture failed: {error}"),
        };
        mount(&registries, &mut state, source, support);
        mount(&registries, &mut state, destination, support);
        let token = match validate_fluid_transfer(
            &registries,
            &state,
            make_test_fluid_transfer_resolution(
                source,
                destination,
                Volume::from_microliters(100_000),
            ),
        ) {
            Ok(token) => token,
            Err(error) => panic!("same-support transfer validation failed: {error}"),
        };
        let fluid_before = state.fluid().clone();
        let snow = match validate_set_structural_load(
            &registries,
            &state,
            support,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(1),
        ) {
            Ok(token) => token,
            Err(error) => panic!("same-support stale structure fixture failed: {error}"),
        };
        if let Err(error) = snow.commit(&mut state) {
            panic!("same-support stale structure commit failed: {error}");
        }

        assert!(matches!(
            token.commit(&mut state),
            Err(super::super::FluidTransferCommitError::Structure(
                StructuralCommitError::StaleRevision {
                    expected: _expected,
                    actual: _actual,
                }
            ))
        ));
        assert_eq!(state.fluid(), &fluid_before);
    }

    #[test]
    fn fluid_support_change_rejects_stale_fluid_owner_before_structural_mutation() {
        let registries = registries(1_000);
        let mut state = AppState::new(WorldSeed::new(0x9410_0006));
        let support = add_active_support(&registries, &mut state, 0);
        let store = add_filled(&registries, &mut state, 1_000_000);
        let token = match validate_mount_fluid_store(&registries, &state, store, support) {
            Ok(token) => token,
            Err(error) => panic!("stale fluid support setup failed: {error}"),
        };
        let structure_before = state.structures().clone();
        if let Err(error) = add_fluid_store(&mut state, Volume::from_microliters(1)) {
            panic!("stale fluid support owner mutation failed: {error}");
        }

        assert!(matches!(
            token.commit(&mut state),
            Err(FluidSupportCommitError::StaleFluidRevision {
                expected: _expected,
                actual: _actual,
            })
        ));
        assert_eq!(state.structures(), &structure_before);
        assert_eq!(
            state
                .fluid()
                .get_store(store)
                .and_then(|record| record.supported_by()),
            None
        );
    }

    #[test]
    fn supported_fluid_round_trip_preserves_support_index_and_derived_load() {
        let registries = registries(1_000);
        let mut state = AppState::new(WorldSeed::new(0x9410_0007));
        let support = add_active_support(&registries, &mut state, 0);
        let store = add_filled(&registries, &mut state, 1_000_000);
        mount(&registries, &mut state, store, support);
        let expected_load = state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::Fluid));

        let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("supported fluid save serialization failed: {error}"),
        };
        let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
            Ok(decoded) => decoded,
            Err(error) => panic!("supported fluid save decode failed: {error}"),
        };
        let loaded = match decoded.into_state(&registries) {
            Ok(loaded) => loaded,
            Err(error) => panic!("supported fluid save validation failed: {error}"),
        };

        assert_eq!(loaded, state);
        assert_eq!(
            loaded
                .fluid()
                .get_store(store)
                .and_then(|record| record.supported_by()),
            Some(support)
        );
        assert_eq!(
            loaded
                .structures()
                .get_element(support)
                .map(|record| record.load(StructuralLoadKind::Fluid)),
            expected_load
        );
    }

    #[test]
    fn tampered_fluid_derived_load_is_rejected_on_load() {
        let registries = registries(1_000);
        let mut state = AppState::new(WorldSeed::new(0x9410_0008));
        let support = add_active_support(&registries, &mut state, 0);
        let store = add_filled(&registries, &mut state, 1_000_000);
        mount(&registries, &mut state, store, support);

        let expected = match state.structures().get_element(support) {
            Some(record) => record.load(StructuralLoadKind::Fluid),
            None => panic!("fluid load tamper support disappeared"),
        };
        let wrong = Force::from_millinewtons(expected.millinewtons() + 1);
        let mut wrong_load = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("fluid load tamper serialization failed: {error}"),
        };
        wrong_load["state"]["systems"]["structures"]["elements"][support.value().to_string()]["loads"]
            ["Fluid"] = serde_json::json!(wrong.millinewtons());
        let wrong_load: LoadedSaveEnvelope = match serde_json::from_value(wrong_load) {
            Ok(decoded) => decoded,
            Err(error) => panic!("fluid load tamper failed decode: {error}"),
        };
        assert_eq!(
            wrong_load.into_state(&registries),
            Err(LoadError::InvalidState(
                StateValidationError::FluidStructuralLoad(
                    FluidStructuralLoadError::ExistingLoadMismatch {
                        element: support,
                        stored: wrong,
                        expected,
                    }
                )
            ))
        );
    }

    #[cfg(feature = "test-soak")]
    #[test]
    #[ignore = "long-horizon soak"]
    fn supported_fluid_transfer_soak_preserves_volume_load_invariants_and_replay() {
        let registries = registries(1_000);
        let mut first = AppState::new(WorldSeed::new(0x9410_0009));
        let left_support = add_active_support(&registries, &mut first, 0);
        let right_support = add_active_support(&registries, &mut first, 2);
        let left = add_filled(&registries, &mut first, 10_000);
        let right = match add_fluid_store(&mut first, Volume::from_microliters(10_000)) {
            Ok(store) => store,
            Err(error) => panic!("supported fluid soak destination failed: {error}"),
        };
        mount(&registries, &mut first, left, left_support);
        mount(&registries, &mut first, right, right_support);
        let initial_volume = match calculate_fluid_volume_accounting(&first) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("supported fluid soak accounting failed: {error}"),
        };
        let mut second = first.clone();

        for step in 0..1_000_u64 {
            let (source, destination) = if step.is_multiple_of(2) {
                (left, right)
            } else {
                (right, left)
            };
            for state in [&mut first, &mut second] {
                let token = match validate_fluid_transfer(
                    &registries,
                    state,
                    make_test_fluid_transfer_resolution(
                        source,
                        destination,
                        Volume::from_microliters(1),
                    ),
                ) {
                    Ok(token) => token,
                    Err(error) => {
                        panic!("supported fluid soak validation failed at {step}: {error}")
                    }
                };
                if let Err(error) = token.commit(state) {
                    panic!("supported fluid soak commit failed at {step}: {error}");
                }
            }
            if step % 97 == 0 {
                if let Err(error) = validate_loaded_state(&registries, &first) {
                    panic!("supported fluid soak audit failed at {step}: {error}");
                }
                assert_eq!(
                    calculate_fluid_volume_accounting(&first).map(|accounting| accounting.total()),
                    Ok(initial_volume)
                );
            }
        }

        assert_eq!(first, second);
        assert_eq!(validate_loaded_state(&registries, &first), Ok(()));
        assert_eq!(
            calculate_fluid_volume_accounting(&first).map(|accounting| accounting.total()),
            Ok(initial_volume)
        );
    }
}
