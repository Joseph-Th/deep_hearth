//! Scheduled construction activation and interruption settlement.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;

use super::super::{
    StructuralElementId, StructuralMutationError, StructureState, ValidatedStructuralMutation,
    validate_activate_structural_element_in,
};

#[must_use]
pub(crate) struct StructuralConstructionTickPlan {
    element: StructuralElementId,
    // A due work item can lose its support before activation. Retain its
    // completion as an observable interruption without a structural write.
    activation: Option<ValidatedStructuralMutation>,
}

impl StructuralConstructionTickPlan {
    /// Exact post-activation state, including damage to existing supports.
    #[must_use]
    pub(crate) fn project_structures(&self, structures: &StructureState) -> Option<StructureState> {
        self.activation
            .as_ref()
            .map(|activation| activation.project(structures))
    }

    #[must_use]
    pub(crate) const fn mutates_structures(&self) -> bool {
        self.activation.is_some()
    }
}

pub(crate) fn decide_structural_construction_tick(
    registries: &Registries,
    state: &AppState,
    projected_structures: &StructureState,
    next_tick: SimulationTick,
) -> Option<StructuralConstructionTickPlan> {
    let work = state
        .player_work()
        .structural_construction_due_at(next_tick)?;
    let activation = match validate_activate_structural_element_in(
        registries,
        projected_structures,
        work.element(),
    ) {
        Ok(activation) => Some(activation),
        // A retained support may physically fail after construction admission. The already-
        // embodied planned member stays conserved and recoverable, while this completion becomes
        // an interrupted attempt rather than activating unsupported structure.
        Err(StructuralMutationError::ActivationUnsupported { element })
            if element == work.element() =>
        {
            None
        }
        Err(error) => panic!("admitted structural construction became invalid: {error}"),
    };
    Some(StructuralConstructionTickPlan {
        element: work.element(),
        activation,
    })
}

pub(crate) fn apply_structural_construction_tick(
    state: &mut AppState,
    plan: Option<StructuralConstructionTickPlan>,
) -> (Option<StructuralElementId>, Option<StructuralElementId>) {
    let Some(plan) = plan else {
        return (None, None);
    };
    let Some(activation) = plan.activation else {
        return (None, Some(plan.element));
    };
    let _ = activation
        .commit(state)
        .unwrap_or_else(|error| panic!("prechecked structural activation failed: {error}"));
    (Some(plan.element), None)
}
