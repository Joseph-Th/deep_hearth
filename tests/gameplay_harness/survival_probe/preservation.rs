//! Actor policy for choosing ordinary preservation infrastructure.

use super::super::preservation_route::{
    PreservationConstructionPlan, preservation_construction_plan,
};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in super::super) enum PreservationInvestmentPolicy {
    AttentionEfficient,
    MaximumProtection,
}

impl PreservationInvestmentPolicy {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::AttentionEfficient => "attention-efficient",
            Self::MaximumProtection => "maximum-protection",
        }
    }
}

pub(in super::super) fn preservation_investment_policy_for_behavior_seed(
    behavior_seed: u64,
) -> PreservationInvestmentPolicy {
    if mix64(behavior_seed ^ 0x5052_4553_504F_4C59) & 1 == 0 {
        PreservationInvestmentPolicy::AttentionEfficient
    } else {
        PreservationInvestmentPolicy::MaximumProtection
    }
}

#[derive(Clone, Debug)]
pub(super) struct PreservationCandidate {
    pub(super) definition: StorageDefinitionId,
    pub(super) construction_plan: PreservationConstructionPlan,
    pub(super) preservation_multiplier_ppm: u32,
    pub(super) capacity: Mass,
}

pub(super) fn preservation_candidates(registries: &Registries) -> Vec<PreservationCandidate> {
    let ambient_preservation =
        StockpileStorageProfile::unbounded_solid_only().preservation_multiplier_ppm();
    let candidates = registries
        .storage()
        .definitions()
        .filter(|definition| {
            definition.storage_profile().preservation_multiplier_ppm() > ambient_preservation
        })
        .map(|definition| PreservationCandidate {
            definition: definition.id(),
            construction_plan: preservation_construction_plan(
                registries,
                definition.assembly_profile(),
            ),
            preservation_multiplier_ppm: definition.storage_profile().preservation_multiplier_ppm(),
            capacity: definition.maximum_stockpile_capacity(),
        })
        .collect::<Vec<_>>();
    assert!(
        !candidates.is_empty(),
        "survival gameplay has no authored preservation enclosure"
    );
    candidates
}

pub(super) fn preservation_candidate_for_policy(
    candidates: &[PreservationCandidate],
    policy: PreservationInvestmentPolicy,
) -> usize {
    let key = |candidate: &PreservationCandidate| match policy {
        PreservationInvestmentPolicy::AttentionEfficient => (
            candidate.construction_plan.attention_ticks,
            candidate.construction_plan.raw_mass.milligrams(),
            u64::from(u32::MAX - candidate.preservation_multiplier_ppm),
        ),
        PreservationInvestmentPolicy::MaximumProtection => (
            u64::from(u32::MAX - candidate.preservation_multiplier_ppm),
            candidate.construction_plan.attention_ticks,
            candidate.construction_plan.raw_mass.milligrams(),
        ),
    };
    let best_key = candidates
        .iter()
        .map(&key)
        .min()
        .unwrap_or_else(|| unreachable!("preservation candidates are nonempty"));
    let finalists = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| key(candidate) == best_key)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(
        finalists.len(),
        1,
        "preservation investment policy has physically equivalent best candidates; author another observable tradeoff instead of breaking ties by definition identity"
    );
    finalists[0]
}

pub(in super::super) fn preservation_storage_definition_for_policy(
    registries: &Registries,
    policy: PreservationInvestmentPolicy,
) -> StorageDefinitionId {
    let candidates = preservation_candidates(registries);
    candidates[preservation_candidate_for_policy(&candidates, policy)].definition
}
