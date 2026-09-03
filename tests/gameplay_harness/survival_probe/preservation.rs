//! Actor policy for choosing ordinary preservation infrastructure.

use deep_hearth::content::MATERIAL_WOOD;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreservationRawOpportunityKind {
    ChoiceRichTimber,
    ScarceTimber,
    AlternateMaterial,
    FiniteMaterial,
}

impl PreservationRawOpportunityKind {
    const fn label(self) -> &'static str {
        match self {
            Self::ChoiceRichTimber => "choice-rich-timber",
            Self::ScarceTimber => "scarce-timber",
            Self::AlternateMaterial => "alternate-material",
            Self::FiniteMaterial => "finite-material",
        }
    }
}

pub(super) struct PreservationRawOpportunity {
    origin: StorageDefinitionId,
    available: Vec<(CommodityKey, Mass)>,
    kind: PreservationRawOpportunityKind,
}

impl PreservationRawOpportunity {
    pub(super) const fn origin(&self) -> StorageDefinitionId {
        self.origin
    }

    pub(super) fn available(&self) -> &[(CommodityKey, Mass)] {
        &self.available
    }

    pub(super) const fn mode_label(&self) -> &'static str {
        self.kind.label()
    }

    fn assert_mode_contract(&self, registries: &Registries, minimum_capacity: Mass) {
        let buildable =
            preservation_buildable_definitions(registries, minimum_capacity, self.available());
        match self.kind {
            PreservationRawOpportunityKind::ChoiceRichTimber => assert!(
                buildable.len() > 1,
                "choice-rich preservation opportunity must expose multiple buildable enclosures"
            ),
            PreservationRawOpportunityKind::ScarceTimber => assert_eq!(
                buildable.len(),
                1,
                "scarce preservation opportunity must expose exactly one buildable enclosure"
            ),
            PreservationRawOpportunityKind::AlternateMaterial
            | PreservationRawOpportunityKind::FiniteMaterial => {}
        }
        assert!(
            buildable.contains(&self.origin),
            "preservation opportunity origin must remain buildable from its disclosed raw material"
        );
    }
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

pub(in super::super) fn preservation_raw_requirements_for_definition(
    registries: &Registries,
    definition: StorageDefinitionId,
) -> Vec<(CommodityKey, Mass)> {
    let candidate = preservation_candidates(registries)
        .into_iter()
        .find(|candidate| candidate.definition == definition)
        .unwrap_or_else(|| panic!("unknown preservation definition {}", definition.value()));
    let mut requirements = BTreeMap::<CommodityKey, Mass>::new();
    for route in candidate.construction_plan.routes {
        let total = requirements
            .get(&route.raw_commodity)
            .copied()
            .unwrap_or(Mass::ZERO)
            .checked_add(route.raw_mass)
            .unwrap_or_else(|| panic!("preservation raw requirement overflowed"));
        requirements.insert(route.raw_commodity, total);
    }
    requirements.into_iter().collect()
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
            let requirements =
                preservation_raw_requirements_for_definition(registries, candidate.definition);
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
        let opportunity = PreservationRawOpportunity {
            origin: *origin,
            available: available.clone(),
            kind: PreservationRawOpportunityKind::AlternateMaterial,
        };
        opportunity.assert_mode_contract(registries, protected_reserve_mass);
        return opportunity;
    }

    let timber = opportunities
        .iter()
        .filter(|(_, _, _, timber_only)| *timber_only)
        .collect::<Vec<_>>();
    if timber.is_empty() {
        let index = usize::try_from(
            mix64(seed ^ 0x5052_4553_5241_574F)
                % u64::try_from(opportunities.len()).unwrap_or(u64::MAX),
        )
        .unwrap_or_else(|_| unreachable!("bounded preservation opportunity index fits usize"));
        let (origin, available, _, _) = &opportunities[index];
        let opportunity = PreservationRawOpportunity {
            origin: *origin,
            available: available.clone(),
            kind: PreservationRawOpportunityKind::FiniteMaterial,
        };
        opportunity.assert_mode_contract(registries, protected_reserve_mass);
        return opportunity;
    }

    let choice_rich = timber.len() > 1 && !mix64(seed ^ 0x5052_4553_4348_4F49).is_multiple_of(4);
    let selected = if choice_rich {
        timber
            .into_iter()
            .max_by_key(|(definition, _, raw_mass, _)| (raw_mass.milligrams(), *definition))
            .unwrap_or_else(|| unreachable!("timber opportunities are nonempty"))
    } else {
        timber
            .into_iter()
            .min_by_key(|(definition, _, raw_mass, _)| (raw_mass.milligrams(), *definition))
            .unwrap_or_else(|| unreachable!("timber opportunities are nonempty"))
    };
    let (origin, available, _, _) = selected;
    let opportunity = PreservationRawOpportunity {
        origin: *origin,
        available: available.clone(),
        kind: if choice_rich {
            PreservationRawOpportunityKind::ChoiceRichTimber
        } else {
            PreservationRawOpportunityKind::ScarceTimber
        },
    };
    opportunity.assert_mode_contract(registries, protected_reserve_mass);
    opportunity
}

pub(in super::super) fn preservation_buildable_definitions(
    registries: &Registries,
    minimum_capacity: Mass,
    available_raw_materials: &[(CommodityKey, Mass)],
) -> BTreeSet<StorageDefinitionId> {
    let mut available = BTreeMap::<CommodityKey, Mass>::new();
    for (commodity, mass) in available_raw_materials {
        let total = available
            .get(commodity)
            .copied()
            .unwrap_or(Mass::ZERO)
            .checked_add(*mass)
            .unwrap_or_else(|| panic!("preservation raw opportunity overflowed"));
        available.insert(*commodity, total);
    }
    preservation_candidates(registries)
        .into_iter()
        .filter(|candidate| candidate.capacity >= minimum_capacity)
        .filter(|candidate| {
            preservation_raw_requirements_for_definition(registries, candidate.definition)
                .into_iter()
                .all(|(commodity, required)| {
                    available.get(&commodity).copied().unwrap_or(Mass::ZERO) >= required
                })
        })
        .map(|candidate| candidate.definition)
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
