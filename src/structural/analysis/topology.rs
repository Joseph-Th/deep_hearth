//! Connected-scope discovery, deterministic load propagation, and unsupported-support closure.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::quantity::Force;
use crate::structural::state::{StructuralElementId, StructuralLifecycle, StructureState};

use super::{StructuralAnalysisError, StructuralAnalysisOverlay};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LoadProjection {
    pub(super) carried: BTreeMap<StructuralElementId, Force>,
}

struct LoadPropagation {
    carried: BTreeMap<StructuralElementId, Force>,
    remaining_dependents: BTreeMap<StructuralElementId, usize>,
    ready: BTreeSet<StructuralElementId>,
    processed: usize,
}

impl LoadPropagation {
    fn new(
        state: &StructureState,
        overlay: &StructuralAnalysisOverlay,
        active: &BTreeSet<StructuralElementId>,
    ) -> Result<Self, StructuralAnalysisError> {
        let mut carried = BTreeMap::new();
        let mut remaining_dependents = BTreeMap::new();
        let mut ready = BTreeSet::new();
        for element in active {
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
        Ok(Self {
            carried,
            remaining_dependents,
            ready,
            processed: 0,
        })
    }

    fn next_ready(&mut self) -> Option<StructuralElementId> {
        let element = self.ready.pop_first()?;
        self.processed += 1;
        Some(element)
    }

    fn add_support_load(
        &mut self,
        support: StructuralElementId,
        share: Force,
    ) -> Result<(), StructuralAnalysisError> {
        let current = self.carried[&support];
        let next = current
            .checked_add(share)
            .ok_or(StructuralAnalysisError::LoadOverflow { support })?;
        self.carried.insert(support, next);

        let remaining = self
            .remaining_dependents
            .get_mut(&support)
            .ok_or(StructuralAnalysisError::ActiveGraphCycle)?;
        *remaining = remaining
            .checked_sub(1)
            .ok_or(StructuralAnalysisError::ActiveGraphCycle)?;
        if *remaining == 0 {
            self.ready.insert(support);
        }
        Ok(())
    }

    fn finish(self, active_count: usize) -> Result<LoadProjection, StructuralAnalysisError> {
        if self.processed != active_count {
            return Err(StructuralAnalysisError::ActiveGraphCycle);
        }
        Ok(LoadProjection {
            carried: self.carried,
        })
    }
}

pub(super) fn collect_connected_scope(
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

pub(super) fn project_loads(
    state: &StructureState,
    failed: &BTreeSet<StructuralElementId>,
    overlay: &StructuralAnalysisOverlay,
    scope: &BTreeSet<StructuralElementId>,
) -> Result<LoadProjection, StructuralAnalysisError> {
    let active = active_ids(state, failed, overlay, scope);
    let mut propagation = LoadPropagation::new(state, overlay, &active)?;
    while let Some(element) = propagation.next_ready() {
        let record = &state.element_map()[&element];
        let load = propagation.carried[&element];
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
            propagation.add_support_load(support, share)?;
        }
    }
    propagation.finish(active.len())
}

fn initialize_support_failure_frontier(
    state: &StructureState,
    overlay: &StructuralAnalysisOverlay,
    active: &BTreeSet<StructuralElementId>,
) -> (
    BTreeMap<StructuralElementId, usize>,
    BTreeSet<StructuralElementId>,
) {
    let mut remaining_supports = BTreeMap::new();
    let mut ready = BTreeSet::new();
    for element in active {
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
    (remaining_supports, ready)
}

fn remove_failed_support(
    remaining_supports: &mut BTreeMap<StructuralElementId, usize>,
    unsupported: &BTreeSet<StructuralElementId>,
    dependent: StructuralElementId,
) -> bool {
    if unsupported.contains(&dependent) {
        return false;
    }
    let Some(remaining) = remaining_supports.get_mut(&dependent) else {
        return false;
    };
    if *remaining > 0 {
        *remaining -= 1;
    }
    *remaining == 0
}

pub(super) fn expand_unsupported_failures(
    state: &StructureState,
    failed: &BTreeSet<StructuralElementId>,
    overlay: &StructuralAnalysisOverlay,
    scope: &BTreeSet<StructuralElementId>,
) -> BTreeSet<StructuralElementId> {
    let active = active_ids(state, failed, overlay, scope);
    let (mut remaining_supports, mut ready) =
        initialize_support_failure_frontier(state, overlay, &active);

    let mut unsupported = BTreeSet::new();
    while let Some(element) = ready.pop_first() {
        if !unsupported.insert(element) {
            continue;
        }
        for dependent in overlay.dependents(state, element).iter().copied() {
            if remove_failed_support(&mut remaining_supports, &unsupported, dependent) {
                ready.insert(dependent);
            }
        }
    }
    unsupported
}
