//! Explanatory comparison boundaries; raw branch evidence remains separate from interpretation.

use deep_hearth::inventory::StorageDefinitionId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum PreservationComparison {
    ForcedSingleton,
    SharedReference,
    DistinctReferences,
}

impl PreservationComparison {
    pub(in super::super) fn selection_label(self, selected_policy: &'static str) -> &'static str {
        if self == Self::ForcedSingleton {
            "physically-forced"
        } else {
            selected_policy
        }
    }

    pub(in super::super) fn from_candidates(
        candidate_count: usize,
        fastest: StorageDefinitionId,
        strongest: StorageDefinitionId,
    ) -> Self {
        assert!(
            candidate_count > 0,
            "preservation comparison needs a feasible candidate"
        );
        if candidate_count == 1 {
            assert_eq!(
                fastest, strongest,
                "singleton references must identify the same enclosure"
            );
            Self::ForcedSingleton
        } else if fastest == strongest {
            Self::SharedReference
        } else {
            Self::DistinctReferences
        }
    }
}

pub(in super::super) fn preservation_comparison_explanation(
    comparison: PreservationComparison,
    tradeoff: impl FnOnce() -> String,
) -> String {
    match comparison {
        PreservationComparison::ForcedSingleton =>
            "choice:physically-forced reason:capacity-or-raw-material-singleton comparison:not-applicable".to_string(),
        PreservationComparison::SharedReference =>
            "choice:shared-reference reason:fastest-and-strongest-are-the-same-enclosure comparison:not-applicable".to_string(),
        PreservationComparison::DistinctReferences => tradeoff(),
    }
}

pub(in super::super) fn diet_comparison_explanation(
    policy_sensitive: bool,
    tradeoff: impl FnOnce() -> String,
) -> String {
    if policy_sensitive {
        tradeoff()
    } else {
        "comparison:supply-collapsed reason:available-categories-below-authored-diet-set recovery-comparison:not-applicable".to_string()
    }
}
