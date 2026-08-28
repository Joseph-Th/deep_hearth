//! Deterministic structural load propagation and damage-cascade analysis over sibling persistent records.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::arithmetic::{
    scale_u128_fraction_ceil, scale_u128_fraction_floor, scaled_ratio_floor_saturating,
};
use crate::core::quantity::{Area, Force};
use crate::material::{MaterialDefinition, MaterialId, MaterialRegistry};

use super::definitions::{
    STRUCTURAL_PARTS_PER_MILLION, StructuralLoadMode, StructuralProfileDefinition,
    StructuralProfileId, StructuralRegistry,
};
use super::state::{
    StructuralElementId, StructuralElementRecord, StructuralLifecycle, StructuralLoadKind,
    StructureState,
};

mod cascade;

/// Player-readable structural state derived from load, material capacity, and persistent damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StructuralStage {
    Stable,
    Strained,
    Cracking,
    Failed,
}

/// Projects structural utilization using the same normalized ratio as authoritative analysis.
#[must_use]
pub fn calculate_structural_utilization_ppm(load: Force, capacity: Force) -> u128 {
    if capacity.is_zero() {
        return if load.is_zero() { 0 } else { u128::MAX };
    }
    scaled_ratio_floor_saturating(
        load.millinewtons(),
        capacity.millinewtons(),
        STRUCTURAL_PARTS_PER_MILLION,
    )
}

/// Projects the pristine axial capacity of one material/profile cross-section.
///
/// This is the same capacity calculation used by authoritative structural analysis. Planning,
/// presentation, and gameplay evaluation should use this projection instead of reproducing
/// strength-axis selection or unit conversion outside the structural owner.
#[must_use]
pub fn calculate_pristine_member_capacity(
    profile: &StructuralProfileDefinition,
    material: &MaterialDefinition,
    cross_section: Area,
) -> Force {
    let mechanical = material.properties().mechanical();
    let strength_kpa = match profile.load_mode() {
        StructuralLoadMode::Compression => mechanical.compressive_strength_kpa(),
        StructuralLoadMode::Tension => mechanical.tensile_strength_kpa(),
    };

    // 1 kPa * 1 mm^2 = 1 mN, so authored strength and cross-section multiply exactly.
    Force::from_millinewtons(
        u128::from(strength_kpa) * u128::from(cross_section.square_millimeters()),
    )
}

/// Why a structural member crossed into irreversible failure during one analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralFailureCause {
    Unsupported,
    Overloaded {
        carried_load: Force,
        effective_capacity: Force,
    },
}

/// Irreversible structural damage discovered by deterministic analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralDamageEvent {
    Cracked {
        element: StructuralElementId,
        carried_load: Force,
        pristine_capacity: Force,
    },
    Failed {
        element: StructuralElementId,
        cause: StructuralFailureCause,
    },
}

impl StructuralDamageEvent {
    #[must_use]
    pub const fn element(self) -> StructuralElementId {
        match self {
            Self::Cracked {
                element,
                carried_load: _carried_load,
                pristine_capacity: _pristine_capacity,
            } => element,
            Self::Failed {
                element,
                cause: _cause,
            } => element,
        }
    }
}

/// Read-only load and capacity projection for one active or failed structural member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralAssessment {
    element: StructuralElementId,
    carried_load: Force,
    pristine_capacity: Force,
    effective_capacity: Force,
    utilization_ppm: u128,
    stage: StructuralStage,
}

impl StructuralAssessment {
    #[must_use]
    pub const fn element(self) -> StructuralElementId {
        self.element
    }

    #[must_use]
    pub const fn carried_load(self) -> Force {
        self.carried_load
    }

    #[must_use]
    pub const fn pristine_capacity(self) -> Force {
        self.pristine_capacity
    }

    #[must_use]
    pub const fn effective_capacity(self) -> Force {
        self.effective_capacity
    }

    #[must_use]
    pub const fn utilization_ppm(self) -> u128 {
        self.utilization_ppm
    }

    #[must_use]
    pub const fn stage(self) -> StructuralStage {
        self.stage
    }
}

/// Complete deterministic structural projection plus irreversible damage that must be committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralAnalysis {
    assessments: Vec<StructuralAssessment>,
    damage_events: Vec<StructuralDamageEvent>,
}

impl StructuralAnalysis {
    #[must_use]
    pub fn assessments(&self) -> &[StructuralAssessment] {
        &self.assessments
    }

    #[must_use]
    pub fn damage_events(&self) -> &[StructuralDamageEvent] {
        &self.damage_events
    }
}

/// Structural analysis cannot be completed because authoritative references or arithmetic are invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralAnalysisError {
    UnknownProfile {
        element: StructuralElementId,
        profile: StructuralProfileId,
    },
    UnknownMaterial {
        element: StructuralElementId,
        material: MaterialId,
    },
    LoadOverflow {
        support: StructuralElementId,
    },
    AppliedLoadOverflow {
        element: StructuralElementId,
    },
    UnsupportedActiveElement {
        element: StructuralElementId,
    },
    ActiveGraphCycle,
}

impl Display for StructuralAnalysisError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProfile { element, profile } => write!(
                formatter,
                "structural element {} references unknown profile {} during analysis",
                element.value(),
                profile.value()
            ),
            Self::UnknownMaterial { element, material } => write!(
                formatter,
                "structural element {} references unknown material {} during analysis",
                element.value(),
                material.value()
            ),
            Self::LoadOverflow { support } => write!(
                formatter,
                "structural load accumulation overflowed support {}",
                support.value()
            ),
            Self::AppliedLoadOverflow { element } => write!(
                formatter,
                "structural load contributions overflowed element {}",
                element.value()
            ),
            Self::UnsupportedActiveElement { element } => write!(
                formatter,
                "structural analysis reached unsupported active element {} after collapse closure",
                element.value()
            ),
            Self::ActiveGraphCycle => {
                formatter.write_str("active structural support graph contains a cycle")
            }
        }
    }
}

impl Error for StructuralAnalysisError {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LoadProjection {
    carried: BTreeMap<StructuralElementId, Force>,
}

#[cfg(any(test, feature = "test-gameplay"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StructuralSupportOverlay {
    Link {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    Remove {
        element: StructuralElementId,
        support: StructuralElementId,
    },
}

/// One-operation read overlay used to analyze a proposed mutation without cloning structural state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StructuralAnalysisOverlay {
    #[cfg(any(test, feature = "test-gameplay"))]
    support: Option<StructuralSupportOverlay>,
    lifecycle: Option<(StructuralElementId, StructuralLifecycle)>,
    loads: BTreeMap<(StructuralElementId, StructuralLoadKind), Force>,
    removed: Option<StructuralElementId>,
}

impl StructuralAnalysisOverlay {
    #[must_use]
    #[cfg(any(test, feature = "test-gameplay"))]
    pub(crate) fn link_support(element: StructuralElementId, support: StructuralElementId) -> Self {
        Self {
            support: Some(StructuralSupportOverlay::Link { element, support }),
            lifecycle: None,
            loads: BTreeMap::new(),
            removed: None,
        }
    }

    #[must_use]
    #[cfg(any(test, feature = "test-gameplay"))]
    pub(crate) fn remove_support(
        element: StructuralElementId,
        support: StructuralElementId,
    ) -> Self {
        Self {
            support: Some(StructuralSupportOverlay::Remove { element, support }),
            lifecycle: None,
            loads: BTreeMap::new(),
            removed: None,
        }
    }

    #[must_use]
    #[cfg(any(test, feature = "test-gameplay"))]
    pub(crate) fn activate(element: StructuralElementId) -> Self {
        Self {
            support: None,
            lifecycle: Some((element, StructuralLifecycle::Active)),
            loads: BTreeMap::new(),
            removed: None,
        }
    }

    #[must_use]
    pub(crate) fn set_load(
        element: StructuralElementId,
        kind: StructuralLoadKind,
        load: Force,
    ) -> Self {
        Self {
            #[cfg(any(test, feature = "test-gameplay"))]
            support: None,
            lifecycle: None,
            loads: BTreeMap::from([((element, kind), load)]),
            removed: None,
        }
    }

    #[must_use]
    pub(crate) fn set_loads(
        loads: BTreeMap<(StructuralElementId, StructuralLoadKind), Force>,
    ) -> Self {
        Self {
            #[cfg(any(test, feature = "test-gameplay"))]
            support: None,
            lifecycle: None,
            loads,
            removed: None,
        }
    }

    #[must_use]
    #[cfg(any(test, feature = "test-gameplay"))]
    pub(crate) fn remove_element(element: StructuralElementId) -> Self {
        Self {
            support: None,
            lifecycle: None,
            loads: BTreeMap::new(),
            removed: Some(element),
        }
    }

    fn is_removed(&self, element: StructuralElementId) -> bool {
        self.removed == Some(element)
    }

    fn lifecycle(&self, record: &StructuralElementRecord) -> StructuralLifecycle {
        match self.lifecycle {
            Some((element, lifecycle)) if element == record.id => lifecycle,
            Some(_) | None => record.lifecycle,
        }
    }

    fn supports<'state>(
        &self,
        state: &'state StructureState,
        element: StructuralElementId,
    ) -> Cow<'state, BTreeSet<StructuralElementId>> {
        let Some(base) = state.support_set(element) else {
            return Cow::Owned(BTreeSet::new());
        };
        #[cfg(any(test, feature = "test-gameplay"))]
        let support_change_applies = matches!(
            self.support,
            Some(StructuralSupportOverlay::Link { element: changed, .. })
                | Some(StructuralSupportOverlay::Remove { element: changed, .. })
                if changed == element
        );
        #[cfg(not(any(test, feature = "test-gameplay")))]
        let support_change_applies = false;
        let removal_applies = self
            .removed
            .is_some_and(|removed| removed == element || base.contains(&removed));
        if !support_change_applies && !removal_applies {
            return Cow::Borrowed(base);
        }

        let mut owned = base.clone();
        #[cfg(any(test, feature = "test-gameplay"))]
        match self.support {
            Some(StructuralSupportOverlay::Link {
                element: changed,
                support,
            }) if changed == element => {
                owned.insert(support);
            }
            Some(StructuralSupportOverlay::Remove {
                element: changed,
                support,
            }) if changed == element => {
                owned.remove(&support);
            }
            Some(_) | None => {}
        }
        if let Some(removed) = self.removed {
            if removed == element {
                owned.clear();
            } else {
                owned.remove(&removed);
            }
        }
        Cow::Owned(owned)
    }

    fn dependents<'state>(
        &self,
        state: &'state StructureState,
        support: StructuralElementId,
    ) -> Cow<'state, BTreeSet<StructuralElementId>> {
        let Some(base) = state.dependent_set(support) else {
            return Cow::Owned(BTreeSet::new());
        };
        #[cfg(any(test, feature = "test-gameplay"))]
        let support_change_applies = matches!(
            self.support,
            Some(StructuralSupportOverlay::Link { support: changed, .. })
                | Some(StructuralSupportOverlay::Remove { support: changed, .. })
                if changed == support
        );
        #[cfg(not(any(test, feature = "test-gameplay")))]
        let support_change_applies = false;
        let removal_applies = self
            .removed
            .is_some_and(|removed| removed == support || base.contains(&removed));
        if !support_change_applies && !removal_applies {
            return Cow::Borrowed(base);
        }

        let mut owned = base.clone();
        #[cfg(any(test, feature = "test-gameplay"))]
        match self.support {
            Some(StructuralSupportOverlay::Link {
                element,
                support: changed,
            }) if changed == support => {
                owned.insert(element);
            }
            Some(StructuralSupportOverlay::Remove {
                element,
                support: changed,
            }) if changed == support => {
                owned.remove(&element);
            }
            Some(_) | None => {}
        }
        if let Some(removed) = self.removed {
            if removed == support {
                owned.clear();
            } else {
                owned.remove(&removed);
            }
        }
        Cow::Owned(owned)
    }

    fn sum_applied_load(&self, record: &StructuralElementRecord) -> Option<Force> {
        let mut total = Force::ZERO;
        for (kind, stored) in &record.loads {
            let load = self
                .loads
                .get(&(record.id, *kind))
                .copied()
                .unwrap_or(*stored);
            total = total.checked_add(load)?;
        }
        for ((element, kind), load) in &self.loads {
            if *element == record.id && !record.loads.contains_key(kind) {
                total = total.checked_add(*load)?;
            }
        }
        Some(total)
    }
}

fn collect_connected_scope(
    state: &StructureState,
    overlay: &StructuralAnalysisOverlay,
    seeds: &BTreeSet<StructuralElementId>,
) -> BTreeSet<StructuralElementId> {
    let mut pending = seeds.clone();
    let mut scope = BTreeSet::new();
    while let Some(element) = pending.pop_first() {
        if overlay.is_removed(element)
            || !state.element_map().contains_key(&element)
            || !scope.insert(element)
        {
            continue;
        }
        pending.extend(overlay.supports(state, element).iter().copied());
        pending.extend(overlay.dependents(state, element).iter().copied());
    }
    scope
}

fn active_ids(
    state: &StructureState,
    failed: &BTreeSet<StructuralElementId>,
    overlay: &StructuralAnalysisOverlay,
    scope: &BTreeSet<StructuralElementId>,
) -> BTreeSet<StructuralElementId> {
    scope
        .iter()
        .copied()
        .filter(|element| {
            state.element_map().get(element).is_some_and(|record| {
                !overlay.is_removed(record.id)
                    && overlay.lifecycle(record) == StructuralLifecycle::Active
                    && !failed.contains(&record.id)
            })
        })
        .collect()
}

fn project_loads(
    state: &StructureState,
    failed: &BTreeSet<StructuralElementId>,
    overlay: &StructuralAnalysisOverlay,
    scope: &BTreeSet<StructuralElementId>,
) -> Result<LoadProjection, StructuralAnalysisError> {
    let active = active_ids(state, failed, overlay, scope);
    let mut carried = BTreeMap::new();
    let mut remaining_dependents = BTreeMap::new();
    let mut ready = BTreeSet::new();

    for element in &active {
        let record = &state.element_map()[element];
        let load = overlay
            .sum_applied_load(record)
            .ok_or(StructuralAnalysisError::AppliedLoadOverflow { element: *element })?;
        carried.insert(*element, load);
        let dependent_count = overlay
            .dependents(state, *element)
            .iter()
            .filter(|dependent| active.contains(dependent))
            .count();
        remaining_dependents.insert(*element, dependent_count);
        if dependent_count == 0 {
            ready.insert(*element);
        }
    }

    let mut processed = 0_usize;
    while let Some(element) = ready.pop_first() {
        processed += 1;
        let record = &state.element_map()[&element];
        let load = carried[&element];
        let active_supports: Vec<_> = overlay
            .supports(state, element)
            .iter()
            .copied()
            .filter(|support| active.contains(support))
            .collect();

        if !record.is_grounded() && active_supports.is_empty() {
            return Err(StructuralAnalysisError::UnsupportedActiveElement { element });
        }
        if record.is_grounded() {
            continue;
        }

        let support_count = active_supports.len() as u128;
        let base = load.millinewtons() / support_count;
        let remainder = load.millinewtons() % support_count;
        for (index, support) in active_supports.into_iter().enumerate() {
            let extra = u128::from((index as u128) < remainder);
            let share = Force::from_millinewtons(base + extra);
            let current = carried[&support];
            let next = current
                .checked_add(share)
                .ok_or(StructuralAnalysisError::LoadOverflow { support })?;
            carried.insert(support, next);

            let Some(remaining) = remaining_dependents.get_mut(&support) else {
                return Err(StructuralAnalysisError::ActiveGraphCycle);
            };
            *remaining -= 1;
            if *remaining == 0 {
                ready.insert(support);
            }
        }
    }

    if processed != active.len() {
        return Err(StructuralAnalysisError::ActiveGraphCycle);
    }

    Ok(LoadProjection { carried })
}

fn expand_unsupported_failures(
    state: &StructureState,
    failed: &BTreeSet<StructuralElementId>,
    overlay: &StructuralAnalysisOverlay,
    scope: &BTreeSet<StructuralElementId>,
) -> BTreeSet<StructuralElementId> {
    let active = active_ids(state, failed, overlay, scope);
    let mut remaining_supports = BTreeMap::new();
    let mut ready = BTreeSet::new();

    for element in &active {
        let record = &state.element_map()[element];
        if record.is_grounded() {
            continue;
        }
        let support_count = overlay
            .supports(state, *element)
            .iter()
            .filter(|support| active.contains(support))
            .count();
        remaining_supports.insert(*element, support_count);
        if support_count == 0 {
            ready.insert(*element);
        }
    }

    let mut unsupported = BTreeSet::new();
    while let Some(element) = ready.pop_first() {
        if !unsupported.insert(element) {
            continue;
        }
        for dependent in overlay.dependents(state, element).iter() {
            if !active.contains(dependent) || unsupported.contains(dependent) {
                continue;
            }
            let Some(record) = state.element_map().get(dependent) else {
                continue;
            };
            if record.is_grounded() {
                continue;
            }
            let Some(remaining) = remaining_supports.get_mut(dependent) else {
                continue;
            };
            if *remaining > 0 {
                *remaining -= 1;
            }
            if *remaining == 0 {
                ready.insert(*dependent);
            }
        }
    }
    unsupported
}

fn pristine_capacity(
    profiles: &StructuralRegistry,
    materials: &MaterialRegistry,
    state: &StructureState,
    element: StructuralElementId,
) -> Result<Force, StructuralAnalysisError> {
    let record = &state.element_map()[&element];
    let Some(profile) = profiles.get_profile(record.profile()) else {
        return Err(StructuralAnalysisError::UnknownProfile {
            element,
            profile: record.profile(),
        });
    };
    let Some(material) = materials.get_material(record.material()) else {
        return Err(StructuralAnalysisError::UnknownMaterial {
            element,
            material: record.material(),
        });
    };
    Ok(calculate_pristine_member_capacity(
        profile,
        material,
        record.cross_section(),
    ))
}

fn scale_capacity(capacity: Force, ppm: u32) -> Force {
    Force::from_millinewtons(scale_u128_fraction_floor(
        capacity.millinewtons(),
        ppm,
        STRUCTURAL_PARTS_PER_MILLION,
    ))
}

fn is_at_or_above_fraction(load: Force, capacity: Force, threshold_ppm: u32) -> bool {
    if capacity.is_zero() {
        return !load.is_zero();
    }
    let threshold_load = scale_u128_fraction_ceil(
        capacity.millinewtons(),
        threshold_ppm,
        STRUCTURAL_PARTS_PER_MILLION,
    );
    load.millinewtons() >= threshold_load
}

/// Calculates load distribution, warning stages, cracks, and cascading failures without mutation.
pub fn analyze_structure(
    profiles: &StructuralRegistry,
    materials: &MaterialRegistry,
    state: &StructureState,
) -> Result<StructuralAnalysis, StructuralAnalysisError> {
    let scope: BTreeSet<_> = state.element_ids().collect();
    let overlay = StructuralAnalysisOverlay::default();
    cascade::analyze_structure_scoped(profiles, materials, state, &overlay, &scope)
}

pub(crate) fn analyze_structure_components_with_overlay(
    profiles: &StructuralRegistry,
    materials: &MaterialRegistry,
    state: &StructureState,
    overlay: StructuralAnalysisOverlay,
    seeds: &BTreeSet<StructuralElementId>,
) -> Result<StructuralAnalysis, StructuralAnalysisError> {
    let scope = collect_connected_scope(state, &overlay, seeds);
    cascade::analyze_structure_scoped(profiles, materials, state, &overlay, &scope)
}

#[cfg(test)]
#[path = "analysis_tests.rs"]
mod tests;
