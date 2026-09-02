//! Read-only mutation overlays for structural what-if analysis without cloning authoritative state.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use crate::core::quantity::Force;

use crate::structural::state::{
    StructuralElementId, StructuralElementRecord, StructuralLifecycle, StructuralLoadKind,
    StructureState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RelationChange {
    #[cfg(test)]
    Insert(StructuralElementId),
    #[cfg(test)]
    Remove(StructuralElementId),
}

#[cfg(test)]
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
    #[cfg(test)]
    support: Option<StructuralSupportOverlay>,
    lifecycle: Option<(StructuralElementId, StructuralLifecycle)>,
    loads: BTreeMap<(StructuralElementId, StructuralLoadKind), Force>,
    removed: Option<StructuralElementId>,
}

impl StructuralAnalysisOverlay {
    #[must_use]
    #[cfg(test)]
    pub(crate) fn link_support(element: StructuralElementId, support: StructuralElementId) -> Self {
        Self {
            support: Some(StructuralSupportOverlay::Link { element, support }),
            lifecycle: None,
            loads: BTreeMap::new(),
            removed: None,
        }
    }

    #[must_use]
    #[cfg(test)]
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
            #[cfg(test)]
            support: None,
            lifecycle: Some((element, StructuralLifecycle::Active)),
            loads: BTreeMap::new(),
            removed: None,
        }
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn set_load(
        element: StructuralElementId,
        kind: StructuralLoadKind,
        load: Force,
    ) -> Self {
        Self {
            #[cfg(test)]
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
            #[cfg(test)]
            support: None,
            lifecycle: None,
            loads,
            removed: None,
        }
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn remove_element(element: StructuralElementId) -> Self {
        Self {
            support: None,
            lifecycle: None,
            loads: BTreeMap::new(),
            removed: Some(element),
        }
    }

    pub(super) fn is_removed(&self, element: StructuralElementId) -> bool {
        self.removed == Some(element)
    }

    pub(super) fn lifecycle(&self, record: &StructuralElementRecord) -> StructuralLifecycle {
        match self.lifecycle {
            Some((element, lifecycle)) if element == record.id => lifecycle,
            Some(_) | None => record.lifecycle,
        }
    }

    fn overlay_relation<'state>(
        &self,
        base: Option<&'state BTreeSet<StructuralElementId>>,
        owner: StructuralElementId,
        changed_neighbor: Option<RelationChange>,
    ) -> Cow<'state, BTreeSet<StructuralElementId>> {
        let Some(base) = base else {
            return Cow::Owned(BTreeSet::new());
        };
        let removal_applies = self
            .removed
            .is_some_and(|removed| removed == owner || base.contains(&removed));
        if changed_neighbor.is_none() && !removal_applies {
            return Cow::Borrowed(base);
        }

        let mut owned = base.clone();
        match changed_neighbor {
            #[cfg(test)]
            Some(RelationChange::Insert(neighbor)) => {
                owned.insert(neighbor);
            }
            #[cfg(test)]
            Some(RelationChange::Remove(neighbor)) => {
                owned.remove(&neighbor);
            }
            None => {}
        }
        if let Some(removed) = self.removed {
            if removed == owner {
                owned.clear();
            } else {
                owned.remove(&removed);
            }
        }
        Cow::Owned(owned)
    }

    pub(super) fn supports<'state>(
        &self,
        state: &'state StructureState,
        element: StructuralElementId,
    ) -> Cow<'state, BTreeSet<StructuralElementId>> {
        #[cfg(test)]
        let changed_neighbor = match self.support {
            Some(StructuralSupportOverlay::Link {
                element: changed,
                support,
            }) if changed == element => Some(RelationChange::Insert(support)),
            Some(StructuralSupportOverlay::Remove {
                element: changed,
                support,
            }) if changed == element => Some(RelationChange::Remove(support)),
            Some(_) | None => None,
        };
        #[cfg(not(test))]
        let changed_neighbor = None;
        self.overlay_relation(state.support_set(element), element, changed_neighbor)
    }

    pub(super) fn dependents<'state>(
        &self,
        state: &'state StructureState,
        support: StructuralElementId,
    ) -> Cow<'state, BTreeSet<StructuralElementId>> {
        #[cfg(test)]
        let changed_neighbor = match self.support {
            Some(StructuralSupportOverlay::Link {
                element,
                support: changed,
            }) if changed == support => Some(RelationChange::Insert(element)),
            Some(StructuralSupportOverlay::Remove {
                element,
                support: changed,
            }) if changed == support => Some(RelationChange::Remove(element)),
            Some(_) | None => None,
        };
        #[cfg(not(test))]
        let changed_neighbor = None;
        self.overlay_relation(state.dependent_set(support), support, changed_neighbor)
    }

    pub(super) fn sum_applied_load(&self, record: &StructuralElementRecord) -> Option<Force> {
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
