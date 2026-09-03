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

pub(in super::super) fn preservation_freshness_return_threshold_ppm(behavior_seed: u64) -> u32 {
    1_000_000 + (mix64(behavior_seed ^ 0x5052_4553_5641_4C55) % 3_000_001) as u32
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
    let mut candidates = registries
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
    candidates.sort_by_key(|candidate| candidate.definition);
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
    preservation_candidate_for_policy_with_constraints(candidates, policy, Mass::ZERO, None)
}

fn preservation_candidate_for_policy_with_constraints(
    candidates: &[PreservationCandidate],
    policy: PreservationInvestmentPolicy,
    minimum_capacity: Mass,
    available_raw_materials: Option<&[(CommodityKey, Mass)]>,
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
    let available_raw_materials = available_raw_materials.map(|available| {
        let mut totals = BTreeMap::<CommodityKey, Mass>::new();
        for (commodity, mass) in available {
            let total = totals
                .get(commodity)
                .copied()
                .unwrap_or(Mass::ZERO)
                .checked_add(*mass)
                .unwrap_or_else(|| panic!("preservation raw opportunity overflowed"));
            totals.insert(*commodity, total);
        }
        totals
    });
    let eligible = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.capacity >= minimum_capacity)
        .filter(|(_, candidate)| {
            available_raw_materials.as_ref().is_none_or(|available| {
                let mut required = BTreeMap::<CommodityKey, Mass>::new();
                for route in &candidate.construction_plan.routes {
                    let total = required
                        .get(&route.raw_commodity)
                        .copied()
                        .unwrap_or(Mass::ZERO)
                        .checked_add(route.raw_mass)
                        .unwrap_or_else(|| panic!("preservation raw requirement overflowed"));
                    required.insert(route.raw_commodity, total);
                }
                required.into_iter().all(|(commodity, required_mass)| {
                    available.get(&commodity).copied().unwrap_or(Mass::ZERO) >= required_mass
                })
            })
        })
        .collect::<Vec<_>>();
    assert!(
        !eligible.is_empty(),
        "no authored preservation enclosure satisfies the reserve-capacity and raw-material opportunity"
    );
    let best_key = eligible
        .iter()
        .map(|(_, candidate)| key(candidate))
        .min()
        .unwrap_or_else(|| unreachable!("eligible preservation candidates are nonempty"));
    let finalists = eligible
        .into_iter()
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

pub(in super::super) fn preservation_storage_definition_for_policy_and_capacity(
    registries: &Registries,
    policy: PreservationInvestmentPolicy,
    minimum_capacity: Mass,
) -> StorageDefinitionId {
    preservation_storage_definition_for_policy_with_constraints(
        registries,
        policy,
        minimum_capacity,
        None,
    )
}

pub(in super::super) fn preservation_storage_definition_for_policy_with_constraints(
    registries: &Registries,
    policy: PreservationInvestmentPolicy,
    minimum_capacity: Mass,
    available_raw_materials: Option<&[(CommodityKey, Mass)]>,
) -> StorageDefinitionId {
    let candidates = preservation_candidates(registries);
    candidates[preservation_candidate_for_policy_with_constraints(
        &candidates,
        policy,
        minimum_capacity,
        available_raw_materials,
    )]
    .definition
}
