//! Actor-visible preservation comparison over one finite disclosed raw-material opportunity.

use std::collections::BTreeSet;

use super::preservation::{
    PreservationRawOpportunity, preservation_raw_opportunity,
    preservation_storage_definition_for_policy_with_constraints,
};
use super::preservation_evaluation::{
    PreservationCandidateProjection, PreservationInfrastructureReview,
    evaluate_preservation_infrastructure_definition_with_raw_opportunity,
    preservation_physical_frontier, preservation_policy_reachable_definitions,
    project_preservation_candidates_with_raw_opportunity, select_preservation_projection,
};
use super::{
    FoodDefinition, Mass, PreservationInvestmentPolicy, Registries, StorageDefinitionId,
    preservation_freshness_return_threshold_ppm,
};

pub(super) struct PreservationDecisionReview {
    pub(super) opportunity: PreservationRawOpportunity,
    pub(super) attention: PreservationInfrastructureReview,
    pub(super) protection: PreservationInfrastructureReview,
    pub(super) selected: PreservationInfrastructureReview,
    pub(super) projections: Vec<PreservationCandidateProjection>,
    pub(super) physical_frontier: BTreeSet<StorageDefinitionId>,
    pub(super) policy_reachable: BTreeSet<StorageDefinitionId>,
    pub(super) selected_on_physical_frontier: bool,
    pub(super) selected_policy_reachable: bool,
    pub(super) protection_attention_delta_ticks: u64,
    pub(super) protection_raw_delta_mg: u64,
    pub(super) protection_metabolic_delta_nj: u128,
    pub(super) protection_hydration_delta_ul: u64,
    pub(super) protection_freshness_delta_ticks: i128,
    pub(super) protection_remaining_fresh_delta_ticks: i128,
    pub(super) preservation_return_ppm: u32,
    pub(super) preservation_return_threshold_ppm: u32,
    pub(super) capacity_utilization_ppm: u32,
}

pub(super) fn evaluate_preservation_decision(
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
    let protection_raw_delta_mg = protection
        .raw_material_mass_mg
        .checked_sub(attention.raw_material_mass_mg)
        .unwrap_or_else(|| unreachable!("maximum protection does not use less raw matter"));
    let protection_metabolic_delta_nj = protection
        .metabolic_cost_nj
        .checked_sub(attention.metabolic_cost_nj)
        .unwrap_or_else(|| unreachable!("maximum protection does not cost less manual exertion"));
    let protection_hydration_delta_ul = protection
        .hydration_cost_ul
        .checked_sub(attention.hydration_cost_ul)
        .unwrap_or_else(|| unreachable!("maximum protection does not cost less hydration"));
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
    let preservation_return_threshold_ppm =
        preservation_freshness_return_threshold_ppm(behavior_seed);

    let projections = project_preservation_candidates_with_raw_opportunity(
        registries,
        seed,
        protected_food,
        protected_reserve_mass,
        Some(available),
    );
    let selected_projection = select_preservation_projection(behavior_seed, &projections);
    let physical_frontier = preservation_physical_frontier(&projections);
    let policy_reachable = preservation_policy_reachable_definitions(&projections);
    let selected_on_physical_frontier = physical_frontier.contains(&selected_projection.definition);
    let selected_policy_reachable = policy_reachable.contains(&selected_projection.definition);
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

    PreservationDecisionReview {
        opportunity,
        attention,
        protection,
        selected,
        projections,
        physical_frontier,
        policy_reachable,
        selected_on_physical_frontier,
        selected_policy_reachable,
        protection_attention_delta_ticks,
        protection_raw_delta_mg,
        protection_metabolic_delta_nj,
        protection_hydration_delta_ul,
        protection_freshness_delta_ticks,
        protection_remaining_fresh_delta_ticks,
        preservation_return_ppm,
        preservation_return_threshold_ppm,
        capacity_utilization_ppm,
    }
}
