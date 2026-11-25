//! Actor policy for choosing ordinary preservation infrastructure.

use std::collections::BTreeMap;

use deep_hearth::content::MATERIAL_WOOD;

use super::super::preservation_route::{
    PreservationConstructionPlan, preservation_construction_plan,
};
use super::super::survival_preservation_catalog::preservation_storage_candidates;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in super::super) enum PreservationInvestmentPolicy {
    AttentionEfficient,
    MaximumProtection,
}

/// Fixed comparison criterion: one additional fresh-food tick must repay more
/// than one construction-attention tick. This is an experiment rule, not a
/// prediction of a player's preferences.
pub(in super::super) const PRESERVATION_RETURN_PPM: u32 = 1_000_000;

#[derive(Clone, Debug)]
pub(super) struct PreservationCandidate {
    pub(super) definition: StorageDefinitionId,
    pub(super) construction_plan: PreservationConstructionPlan,
    pub(super) preservation_multiplier_ppm: u32,
    pub(super) capacity: Mass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreservationRawOpportunityKind {
    ChoiceRichTimber,
    ScarceTimber,
    AlternateMaterial,
}

impl PreservationRawOpportunityKind {
    #[cfg(not(test))]
    const fn label(self) -> &'static str {
        match self {
            Self::ChoiceRichTimber => "choice-rich-timber",
            Self::ScarceTimber => "scarce-timber",
            Self::AlternateMaterial => "alternate-material",
        }
    }
}

pub(in super::super) struct PreservationRawOpportunity {
    #[cfg_attr(test, allow(dead_code, reason = "report-only opportunity provenance"))]
    origin: StorageDefinitionId,
    available: Vec<(CommodityKey, Mass)>,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "report-only observed market classification")
    )]
    kind: PreservationRawOpportunityKind,
}

impl PreservationRawOpportunity {
    #[cfg(not(test))]
    pub(super) const fn origin(&self) -> StorageDefinitionId {
        self.origin
    }

    pub(in super::super) fn available(&self) -> &[(CommodityKey, Mass)] {
        &self.available
    }

    #[cfg(not(test))]
    pub(super) const fn mode_label(&self) -> &'static str {
        self.kind.label()
    }

    fn from_available(
        registries: &Registries,
        minimum_capacity: Mass,
        origin: StorageDefinitionId,
        available: Vec<(CommodityKey, Mass)>,
    ) -> Self {
        let buildable =
            preservation_candidates_for_opportunity(registries, minimum_capacity, Some(&available));
        assert!(
            !buildable.is_empty(),
            "preservation raw opportunity is not buildable"
        );
        assert!(
            buildable
                .iter()
                .any(|candidate| candidate.definition == origin),
            "preservation opportunity origin must remain buildable from its disclosed raw material"
        );
        let alternate_material = available
            .iter()
            .any(|(commodity, _)| commodity.material() != MATERIAL_WOOD);
        let kind = if alternate_material {
            PreservationRawOpportunityKind::AlternateMaterial
        } else if buildable.len() > 1 {
            PreservationRawOpportunityKind::ChoiceRichTimber
        } else {
            PreservationRawOpportunityKind::ScarceTimber
        };
        Self {
            origin,
            available,
            kind,
        }
    }
}

pub(super) fn preservation_candidates(registries: &Registries) -> Vec<PreservationCandidate> {
    preservation_storage_candidates(registries)
        .into_iter()
        .map(|candidate| PreservationCandidate {
            definition: candidate.definition,
            construction_plan: preservation_construction_plan(
                registries,
                registries
                    .storage()
                    .get(candidate.definition)
                    .unwrap_or_else(|| unreachable!("cataloged preservation storage disappeared"))
                    .assembly_profile(),
            ),
            preservation_multiplier_ppm: candidate.preservation_multiplier_ppm,
            capacity: candidate.capacity,
        })
        .collect()
}

pub(super) fn preservation_raw_opportunity(
    registries: &Registries,
    seed: u64,
    protected_reserve_mass: Mass,
) -> PreservationRawOpportunity {
    let feasible = preservation_candidates(registries)
        .into_iter()
        .filter(|candidate| candidate.capacity >= protected_reserve_mass)
        .collect::<Vec<_>>();
    assert!(
        !feasible.is_empty(),
        "survival preservation world has no enclosure large enough for its protected reserve"
    );
    let opportunities = feasible
        .iter()
        .map(|candidate| {
            let requirements = candidate
                .construction_plan
                .raw_requirements()
                .into_iter()
                .collect::<Vec<_>>();
            assert!(!requirements.is_empty());
            let raw_mass = requirements
                .iter()
                .try_fold(Mass::ZERO, |total, (_, mass)| total.checked_add(*mass))
                .unwrap_or_else(|| panic!("survival preservation opportunity mass overflowed"));
            let timber_only = requirements
                .iter()
                .all(|(commodity, _)| commodity.material() == MATERIAL_WOOD);
            (candidate.definition, requirements, raw_mass, timber_only)
        })
        .collect::<Vec<_>>();
    let alternate_material = opportunities
        .iter()
        .filter(|(_, _, _, timber_only)| !*timber_only)
        .collect::<Vec<_>>();
    if !alternate_material.is_empty() && mix64(seed ^ 0x5052_4553_414C_544D).is_multiple_of(8) {
        let index = usize::try_from(
            mix64(seed ^ 0x5052_4553_414C_5449)
                % u64::try_from(alternate_material.len()).unwrap_or(u64::MAX),
        )
        .unwrap_or_else(|_| unreachable!("bounded alternate preservation opportunity fits usize"));
        let (origin, available, _, _) = alternate_material[index];
        return PreservationRawOpportunity::from_available(
            registries,
            protected_reserve_mass,
            *origin,
            available.clone(),
        );
    }

    let timber = opportunities
        .iter()
        .filter(|(_, _, _, timber_only)| *timber_only)
        .collect::<Vec<_>>();
    if timber.is_empty() {
        assert!(
            !alternate_material.is_empty(),
            "preservation opportunity must expose timber or alternate raw material"
        );
        let index = usize::try_from(
            mix64(seed ^ 0x5052_4553_5241_574F)
                % u64::try_from(alternate_material.len()).unwrap_or(u64::MAX),
        )
        .unwrap_or_else(|_| unreachable!("bounded alternate preservation opportunity fits usize"));
        let (origin, available, _, _) = alternate_material[index];
        return PreservationRawOpportunity::from_available(
            registries,
            protected_reserve_mass,
            *origin,
            available.clone(),
        );
    }

    let choice_rich = timber.len() > 1 && !mix64(seed ^ 0x5052_4553_4348_4F49).is_multiple_of(4);
    let target_raw_mass_mg = if choice_rich {
        timber
            .iter()
            .map(|(_, _, raw_mass, _)| raw_mass.milligrams())
            .max()
    } else {
        timber
            .iter()
            .map(|(_, _, raw_mass, _)| raw_mass.milligrams())
            .min()
    }
    .unwrap_or_else(|| unreachable!("timber opportunities are nonempty"));
    let tied = timber
        .into_iter()
        .filter(|(_, _, raw_mass, _)| raw_mass.milligrams() == target_raw_mass_mg)
        .collect::<Vec<_>>();
    let tie_index = usize::try_from(
        mix64(seed ^ 0x5052_4553_5449_4542) % u64::try_from(tied.len()).unwrap_or(u64::MAX),
    )
    .unwrap_or_else(|_| unreachable!("bounded timber opportunity tie fits usize"));
    let selected = tied[tie_index];
    let (origin, available, _, _) = selected;
    PreservationRawOpportunity::from_available(
        registries,
        protected_reserve_mass,
        *origin,
        available.clone(),
    )
}

pub(super) fn preservation_raw_material_totals(
    available_raw_materials: &[(CommodityKey, Mass)],
) -> BTreeMap<CommodityKey, Mass> {
    let mut totals = BTreeMap::new();
    for (commodity, mass) in available_raw_materials {
        let total = totals
            .get(commodity)
            .copied()
            .unwrap_or(Mass::ZERO)
            .checked_add(*mass)
            .unwrap_or_else(|| panic!("preservation raw opportunity overflowed"));
        totals.insert(*commodity, total);
    }
    totals
}

pub(super) fn preservation_candidates_for_opportunity(
    registries: &Registries,
    minimum_capacity: Mass,
    available_raw_materials: Option<&[(CommodityKey, Mass)]>,
) -> Vec<PreservationCandidate> {
    let available = available_raw_materials.map(preservation_raw_material_totals);
    preservation_candidates(registries)
        .into_iter()
        .filter(|candidate| candidate.capacity >= minimum_capacity)
        .filter(|candidate| {
            available.as_ref().is_none_or(|available| {
                candidate
                    .construction_plan
                    .raw_requirements()
                    .into_iter()
                    .all(|(commodity, required)| {
                        available.get(&commodity).copied().unwrap_or(Mass::ZERO) >= required
                    })
            })
        })
        .collect()
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
    let available_raw_materials = available_raw_materials.map(preservation_raw_material_totals);
    let eligible = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.capacity >= minimum_capacity)
        .filter(|(_, candidate)| {
            available_raw_materials.as_ref().is_none_or(|available| {
                candidate
                    .construction_plan
                    .raw_requirements()
                    .into_iter()
                    .all(|(commodity, required_mass)| {
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
