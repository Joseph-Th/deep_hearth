//! Actor-visible preservation comparison over one finite disclosed raw-material opportunity.

use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};

use super::preservation::{
    PreservationRawOpportunity, preservation_raw_opportunity,
    preservation_storage_definition_for_policy_with_constraints,
};
use super::preservation_evaluation::{
    PreservationCandidateProjection, PreservationInfrastructureReview,
    evaluate_preservation_infrastructure_definition_with_raw_opportunity,
    preservation_physical_frontier, project_preservation_candidates_with_raw_opportunity,
    select_preservation_investment, select_preservation_projection,
};
use super::{
    FoodDefinition, Mass, PreservationInvestmentPolicy, Registries, StorageDefinitionId,
    preservation_attention_value_ppm, preservation_material_budget_ppm,
    preservation_minimum_return_ppm,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct SignedResourceDelta {
    magnitude: u128,
    negative: bool,
}

impl SignedResourceDelta {
    pub(in super::super) const fn between(after: u128, before: u128) -> Self {
        if after >= before {
            Self {
                magnitude: after - before,
                negative: false,
            }
        } else {
            Self {
                magnitude: before - after,
                negative: true,
            }
        }
    }
}

impl Display for SignedResourceDelta {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}{}",
            if self.negative { '-' } else { '+' },
            self.magnitude
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(in super::super) struct PreservationNoBuildReview {
    pub(in super::super) elapsed_ticks: u64,
    pub(in super::super) retained_raw_mg: u64,
    pub(in super::super) remaining_fresh_ticks: u64,
}

pub(in super::super) struct PreservationDecisionReview {
    pub(super) opportunity: PreservationRawOpportunity,
    pub(super) attention: PreservationInfrastructureReview,
    pub(super) protection: PreservationInfrastructureReview,
    pub(super) best_enclosure: PreservationInfrastructureReview,
    pub(in super::super) investment: Option<StorageDefinitionId>,
    pub(in super::super) no_build: PreservationNoBuildReview,
    pub(super) projections: Vec<PreservationCandidateProjection>,
    pub(super) physical_frontier: BTreeSet<StorageDefinitionId>,
    pub(super) material_budget_ppm: u32,
    pub(super) material_budget_mg: u64,
    pub(super) material_budget_eligible_count: usize,
    pub(super) selected_on_physical_frontier: bool,
    pub(super) selected_within_material_budget: bool,
    pub(super) protection_attention_delta_ticks: u64,
    pub(super) protection_raw_delta_mg: SignedResourceDelta,
    pub(super) protection_metabolic_delta_nj: SignedResourceDelta,
    pub(super) protection_hydration_delta_ul: SignedResourceDelta,
    pub(super) protection_freshness_delta_ticks: i128,
    pub(super) protection_remaining_fresh_delta_ticks: i128,
    pub(super) preservation_return_ppm: u32,
    pub(super) preservation_attention_value_ppm: u32,
    pub(super) preservation_minimum_return_ppm: u32,
    pub(super) capacity_utilization_ppm: u32,
}

impl PreservationDecisionReview {
    pub(super) fn comparison(&self) -> super::explanation::PreservationComparison {
        super::explanation::PreservationComparison::from_candidates(
            self.projections.len(),
            self.attention.storage_definition,
            self.protection.storage_definition,
        )
    }
}

fn evaluate_no_build(
    registries: &Registries,
    seed: u64,
    food: FoodDefinition,
    food_mass: Mass,
    available: &[(super::CommodityKey, Mass)],
    reference: &PreservationInfrastructureReview,
) -> PreservationNoBuildReview {
    use super::*;
    let mut state = AppState::new(WorldSeed::new(seed ^ 0x5052_4553_4552_5643));
    let stockpile = seed_stockpile(
        &mut state,
        food_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let lot = seed_lot(
        registries,
        &mut state,
        stockpile,
        food.commodity(),
        food_mass,
        ROOM_TEMPERATURE,
    );
    seed_preexisting_world_age(
        &mut state,
        SimulationTick::new(reference.bootstrap_age_ticks),
    );
    let mut retained_raw_mg = 0_u64;
    for (commodity, mass) in available {
        let raw = seed_stockpile(
            &mut state,
            *mass,
            StockpileStorageProfile::unbounded_solid_only(),
        );
        seed_lot(
            registries,
            &mut state,
            raw,
            *commodity,
            *mass,
            ROOM_TEMPERATURE,
        );
        retained_raw_mg = retained_raw_mg
            .checked_add(mass.milligrams())
            .unwrap_or_else(|| panic!("bounded raw mass"));
    }
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("no-build player admission: {error:?}"));
    let before = state.clone();
    let elapsed_ticks = reference
        .production_ticks
        .checked_add(reference.observation_ticks)
        .unwrap_or_else(|| panic!("bounded preservation horizon"));
    assert!(
        matches!(assess_food_freshness(registries, &state, lot),
        Ok(FoodFreshness::Fresh { remaining, .. }) if remaining.value() == elapsed_ticks),
        "the declared comparison horizon must be the observable ambient edible lifetime"
    );
    advance_idle_ticks(
        registries,
        &mut state,
        elapsed_ticks,
        "preservation no-build",
    );
    assert_eq!(
        state.inventory(),
        before.inventory(),
        "no-build must retain food and raw materials without construction"
    );
    assert_eq!(state.equipment(), before.equipment());
    assert_eq!(state.production(), before.production());
    assert_eq!(state.player_work(), before.player_work());
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("no-build matter: {error:?}"))
            .total(),
        calculate_matter_accounting(&before)
            .unwrap_or_else(|error| panic!("initial matter: {error:?}"))
            .total()
    );
    assert!(
        matches!(
            assess_food_freshness(registries, &state, lot),
            Ok(super::FoodFreshness::Spoiled { .. })
        ),
        "no-build must reach the same ambient spoilage endpoint, not receive free preservation"
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("no-build trusted load: {error:?}"));
    PreservationNoBuildReview {
        elapsed_ticks,
        retained_raw_mg,
        remaining_fresh_ticks: 0,
    }
}

pub(in super::super) fn evaluate_preservation_decision(
    registries: &Registries,
    seed: u64,
    behavior_seed: u64,
    protected_food: FoodDefinition,
    protected_reserve_mass: Mass,
) -> PreservationDecisionReview {
    let opportunity = preservation_raw_opportunity(registries, seed, protected_reserve_mass);
    let available = opportunity.available();
    let attention_definition = preservation_storage_definition_for_policy_with_constraints(
        registries,
        PreservationInvestmentPolicy::AttentionEfficient,
        protected_reserve_mass,
        Some(available),
    );
    let protection_definition = preservation_storage_definition_for_policy_with_constraints(
        registries,
        PreservationInvestmentPolicy::MaximumProtection,
        protected_reserve_mass,
        Some(available),
    );
    // Freeze intent from observable projections before executing any comparison branch.
    // The common scenario endpoint is ambient spoilage, so the no-build value is zero.
    let projections = project_preservation_candidates_with_raw_opportunity(
        registries,
        seed,
        protected_food,
        protected_reserve_mass,
        Some(available),
    );
    let preservation_attention_value_ppm = preservation_attention_value_ppm(behavior_seed);
    let preservation_minimum_return_ppm = preservation_minimum_return_ppm(behavior_seed);
    let material_budget_ppm = preservation_material_budget_ppm(behavior_seed);
    let available_raw_mg = available
        .iter()
        .try_fold(0_u64, |total, (_, mass)| {
            total.checked_add(mass.milligrams())
        })
        .unwrap_or_else(|| panic!("preservation disclosed raw opportunity overflowed"));
    let minimum_candidate_raw_mg = projections
        .iter()
        .map(|projection| projection.raw_material_mass_mg)
        .min()
        .unwrap_or_else(|| panic!("preservation projection has no authored candidates"));
    let proportional_budget_mg =
        u64::try_from(u128::from(available_raw_mg) * u128::from(material_budget_ppm) / 1_000_000)
            .unwrap_or_else(|_| unreachable!("bounded preservation material budget fits u64"));
    let material_budget_mg = proportional_budget_mg.max(minimum_candidate_raw_mg);
    let material_budget_eligible_count = projections
        .iter()
        .filter(|projection| projection.raw_material_mass_mg <= material_budget_mg)
        .count();
    let investment = select_preservation_investment(
        preservation_attention_value_ppm,
        preservation_minimum_return_ppm,
        material_budget_mg,
        &projections,
    )
    .map(|projection| projection.definition);
    let attention = evaluate_preservation_infrastructure_definition_with_raw_opportunity(
        registries,
        seed,
        protected_food,
        protected_reserve_mass,
        attention_definition,
        Some(available),
    );
    let protection = evaluate_preservation_infrastructure_definition_with_raw_opportunity(
        registries,
        seed,
        protected_food,
        protected_reserve_mass,
        protection_definition,
        Some(available),
    );
    assert_eq!(
        attention.food_commodity, protection.food_commodity,
        "matched preservation choices must protect the same food"
    );
    assert_eq!(
        attention.bootstrap_age_ticks, protection.bootstrap_age_ticks,
        "matched preservation choices must begin from the same food age"
    );
    assert_eq!(
        attention.ambient_age_after_ticks, protection.ambient_age_after_ticks,
        "matched preservation choices must be judged at the same wall-clock endpoint"
    );

    let protection_attention_delta_ticks = protection
        .production_ticks
        .checked_sub(attention.production_ticks)
        .unwrap_or_else(|| unreachable!("maximum protection is not cheaper to construct"));
    let protection_raw_delta_mg = SignedResourceDelta::between(
        u128::from(protection.raw_material_mass_mg),
        u128::from(attention.raw_material_mass_mg),
    );
    let protection_metabolic_delta_nj =
        SignedResourceDelta::between(protection.metabolic_cost_nj, attention.metabolic_cost_nj);
    let protection_hydration_delta_ul = SignedResourceDelta::between(
        u128::from(protection.hydration_cost_ul),
        u128::from(attention.hydration_cost_ul),
    );
    let protection_freshness_delta_ticks = i128::from(attention.enclosed_age_after_ticks)
        - i128::from(protection.enclosed_age_after_ticks);
    let protection_remaining_fresh_delta_ticks =
        i128::from(protection.enclosed_remaining_fresh_ticks)
            - i128::from(attention.enclosed_remaining_fresh_ticks);
    let protection_remaining_gain_ticks =
        u64::try_from(protection_remaining_fresh_delta_ticks.max(0))
            .unwrap_or_else(|_| panic!("bounded preservation benefit exceeds u64"));
    let preservation_return_ppm = if protection_attention_delta_ticks == 0 {
        0
    } else {
        u32::try_from(
            u128::from(protection_remaining_gain_ticks) * 1_000_000
                / u128::from(protection_attention_delta_ticks),
        )
        .unwrap_or(u32::MAX)
    };
    let selected_projection =
        select_preservation_projection(behavior_seed, material_budget_mg, &projections);
    let physical_frontier = preservation_physical_frontier(&projections);
    let selected_on_physical_frontier = physical_frontier.contains(&selected_projection.definition);
    let selected_within_material_budget =
        selected_projection.raw_material_mass_mg <= material_budget_mg;
    let selected = if selected_projection.definition == attention.storage_definition {
        attention
    } else if selected_projection.definition == protection.storage_definition {
        protection
    } else {
        evaluate_preservation_infrastructure_definition_with_raw_opportunity(
            registries,
            seed,
            protected_food,
            protected_reserve_mass,
            selected_projection.definition,
            Some(available),
        )
    };
    assert_eq!(
        selected.enclosed_remaining_fresh_ticks, selected_projection.remaining_fresh_ticks,
        "executed preservation choice must match the canonical projected frontier consequence"
    );
    let capacity_utilization_ppm = u32::try_from(
        u128::from(protected_reserve_mass.milligrams()) * 1_000_000
            / u128::from(selected.capacity_mass_mg),
    )
    .unwrap_or_else(|_| panic!("preservation capacity utilization exceeded normalized range"));

    let no_build_review = evaluate_no_build(
        registries,
        seed,
        protected_food,
        protected_reserve_mass,
        available,
        &selected,
    );
    PreservationDecisionReview {
        opportunity,
        attention,
        protection,
        best_enclosure: selected,
        investment,
        no_build: no_build_review,
        projections,
        physical_frontier,
        material_budget_ppm,
        material_budget_mg,
        material_budget_eligible_count,
        selected_on_physical_frontier,
        selected_within_material_budget,
        protection_attention_delta_ticks,
        protection_raw_delta_mg,
        protection_metabolic_delta_nj,
        protection_hydration_delta_ul,
        protection_freshness_delta_ticks,
        protection_remaining_fresh_delta_ticks,
        preservation_return_ppm,
        preservation_attention_value_ppm,
        preservation_minimum_return_ppm,
        capacity_utilization_ppm,
    }
}
