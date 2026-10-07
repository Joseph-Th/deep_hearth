//! Cheap survival world-generation and preservation-route contracts.

use std::collections::BTreeSet;

use deep_hearth::content::{
    FORM_INGOT, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, MATERIAL_CLAY, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD, STORAGE_BULK_TIMBER_PROVISIONS_CRATE,
    STORAGE_CARVED_STONE_PROVISIONS_CROCK, STORAGE_COPPER_BANDED_STONE_PROVISIONS_CROCK,
    STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST, STORAGE_INSULATED_TIMBER_PANTRY,
    STORAGE_ROUGH_TIMBER_FIELD_BOX, STORAGE_TIMBER_PROVISIONS_CHEST, build_registries,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::inventory::StockpileStorageProfile;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::{CommoditySource, CommodityUse};

use super::focused_case::FocusedProbeRole;
use super::focused_seeds::{
    EXPLORATORY_VARIATION_COUNT, FocusedProbeSeedPlan, focused_probe_cases_from,
};
use super::focused_witnesses::{
    SURVIVAL_DECLINE_COVERAGE_SEED, SURVIVAL_STRONG_PRESERVATION_BEHAVIOR_SEED,
    SURVIVAL_STRONG_PRESERVATION_COVERAGE_SEED,
};
use super::preservation_route::preservation_construction_plan;
use super::survival_probe::preservation::preservation_storage_definition_for_policy_with_constraints;
use super::survival_probe::preservation_decision::{
    SignedResourceDelta, evaluate_preservation_decision,
};
use super::survival_probe::preservation_evaluation::{
    project_preservation_candidates_with_raw_opportunity, select_preservation_projection,
    select_preservation_projection_for_attention_value,
};
use super::survival_probe::provisioning_world::minimum_visible_preservation_age_ticks;

fn is_disclosed_preservation_raw_material(commodity: CommodityKey) -> bool {
    commodity == CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
        || commodity == CommodityKey::new(MATERIAL_STONE, FORM_LUMP)
        || commodity == CommodityKey::new(MATERIAL_CLAY, FORM_LUMP)
        || commodity == CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)
}

#[test]
fn full_reserve_worlds_use_varied_proactive_provisioning_windows() {
    let registries = build_registries();
    let ticks_per_day = registries.core().calendar().ticks_per_day();
    let lower = (ticks_per_day / 4).max(1);
    let upper = lower
        .checked_add((ticks_per_day / 4).max(1))
        .unwrap_or_else(|| panic!("survival proactive provisioning window overflowed"));
    let waits = (1_u64..=256)
        .map(|seed| provisioning_world(&registries, seed))
        .filter(|world| world.start_profile == SurvivalStartProfile::FullReserve)
        .map(|world| world.provisioning_wait_ticks)
        .collect::<Vec<_>>();

    assert!(!waits.is_empty());
    assert!(waits.iter().all(|wait| (lower..upper).contains(wait)));
    assert!(
        waits.iter().copied().collect::<BTreeSet<_>>().len() > 8,
        "rested survival worlds collapsed to a scripted provisioning delay"
    );
}

#[test]
fn constrained_food_worlds_execute_real_diet_recovery_instead_of_collapsing_evidence() {
    let registries = build_registries();
    let authored_category_count = registries
        .survival()
        .foods()
        .map(|food| food.category())
        .collect::<BTreeSet<_>>()
        .len();
    let (world, available_category_count) = (1_u64..=256)
        .find_map(|seed| {
            let world =
                super::survival_probe::provisioning_world::provisioning_world(&registries, seed);
            let count = world
                .foods
                .iter()
                .map(|food| food.category())
                .collect::<BTreeSet<_>>()
                .len();
            (count < authored_category_count).then_some((world, count))
        })
        .unwrap_or_else(|| panic!("bounded survival generator produced no constrained-food world"));

    let recovery = super::survival_probe::evaluate_diet_recovery_consequence(&registries, &world);
    assert!(recovery.deprivation_ticks > 0);
    assert!(recovery.provisioning_horizon_ticks > 0);
    assert!(recovery.compact_provisioning_ticks > 0);
    assert!(recovery.balanced_provisioning_ticks > 0);
    assert!(recovery.observation_ticks > 0);
    assert!(recovery.compact_meal_actions > 0);
    assert!(recovery.balanced_meal_actions > 0);
    assert!(recovery.vitality_before_ppm > 0);
    assert!(available_category_count > 0);
}

#[test]
fn four_world_survival_sample_spans_distinct_preservation_capacity_pressure() {
    let registries = build_registries();
    let base = 0xBCE8_0742_3D33_E090_u64;
    let worlds = (0_u64..4)
        .map(|offset| {
            super::survival_probe::provisioning_world::provisioning_world(
                &registries,
                base + offset,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        worlds
            .iter()
            .map(|world| world.preserved_reserve_mass)
            .collect::<BTreeSet<_>>()
            .len(),
        worlds.len(),
        "bounded preservation pressure must retain organic reserve variation"
    );
    let ambient_preservation =
        StockpileStorageProfile::unbounded_solid_only().preservation_multiplier_ppm();
    let feasible_counts = worlds
        .iter()
        .map(|world| {
            registries
                .storage()
                .definitions()
                .filter(|definition| {
                    definition.storage_profile().preservation_multiplier_ppm()
                        > ambient_preservation
                        && definition.maximum_stockpile_capacity() >= world.preserved_reserve_mass
                })
                .count()
        })
        .collect::<Vec<_>>();
    assert!(
        feasible_counts.iter().any(|count| *count > 1),
        "four-world survival exploration must include a real preservation market"
    );
    let distinct_counts = feasible_counts.iter().copied().collect::<BTreeSet<_>>();
    assert!(
        distinct_counts.len() > 1,
        "four-world survival exploration must vary how much of the current preservation market can satisfy the protected reserve"
    );
}

fn storage_capacity(
    registries: &deep_hearth::registry::Registries,
    definition: deep_hearth::inventory::StorageDefinitionId,
) -> Mass {
    registries
        .storage()
        .get(definition)
        .unwrap_or_else(|| panic!("preservation contract storage definition disappeared"))
        .maximum_stockpile_capacity()
}

fn capacity_just_above(
    registries: &deep_hearth::registry::Registries,
    definition: deep_hearth::inventory::StorageDefinitionId,
) -> Mass {
    storage_capacity(registries, definition)
        .checked_add(Mass::from_milligrams(1))
        .unwrap_or_else(|| panic!("preservation contract capacity boundary overflowed"))
}

fn storage_raw_opportunity(
    registries: &deep_hearth::registry::Registries,
    definition: deep_hearth::inventory::StorageDefinitionId,
) -> Vec<(CommodityKey, Mass)> {
    let storage = registries
        .storage()
        .get(definition)
        .unwrap_or_else(|| panic!("preservation contract storage definition disappeared"));
    preservation_construction_plan(registries, storage.assembly_profile())
        .raw_requirements()
        .into_iter()
        .collect()
}

#[test]
fn maintained_survival_coverage_keeps_the_strongest_preservation_endpoint_actionable() {
    let registries = build_registries();
    let world = provisioning_world(&registries, SURVIVAL_STRONG_PRESERVATION_COVERAGE_SEED);
    let decision = evaluate_preservation_decision(
        &registries,
        SURVIVAL_STRONG_PRESERVATION_COVERAGE_SEED,
        SURVIVAL_STRONG_PRESERVATION_BEHAVIOR_SEED,
        world.foods[world.witness_index],
        world.preserved_reserve_mass,
    );
    let strongest_feasible = preservation_storage_definition_for_policy_with_constraints(
        &registries,
        PreservationInvestmentPolicy::MaximumProtection,
        world.preserved_reserve_mass,
        None,
    );
    assert_eq!(
        decision.investment,
        Some(strongest_feasible),
        "maintained survival coverage must keep the currently authored strongest capacity-feasible preservation choice actionable"
    );
}

#[test]
fn preservation_can_decline_a_low_benefit_singleton() {
    use super::survival_probe::preservation_evaluation::{
        PreservationCandidateProjection, select_preservation_investment,
    };
    let candidate = PreservationCandidateProjection {
        definition: STORAGE_TIMBER_PROVISIONS_CHEST,
        production_ticks: 150,
        raw_material_mass_mg: 2_000_000,
        remaining_fresh_ticks: 125,
    };
    assert_eq!(
        select_preservation_investment(1_000_000, 1_000_000, 2_000_000, &[candidate]),
        None
    );
    let worthwhile = PreservationCandidateProjection {
        remaining_fresh_ticks: 600,
        ..candidate
    };
    assert_eq!(
        select_preservation_investment(1_000_000, 1_000_000, 2_000_000, &[worthwhile]),
        Some(worthwhile)
    );
    assert_eq!(
        select_preservation_investment(4_000_000, 4_000_000, 2_000_000, &[worthwhile]),
        None
    );
    assert_eq!(
        select_preservation_investment(10_000_000, 1_000_000, 2_000_000, &[worthwhile]),
        Some(worthwhile),
        "time preference for faster construction must not itself force a worthwhile enclosure to be declined"
    );
}

#[test]
fn preservation_material_budget_makes_intermediate_frontier_actionable() {
    use super::survival_probe::preservation_evaluation::PreservationCandidateProjection;
    let quick = PreservationCandidateProjection {
        definition: STORAGE_ROUGH_TIMBER_FIELD_BOX,
        production_ticks: 100,
        raw_material_mass_mg: 2_000_000,
        remaining_fresh_ticks: 100,
    };
    let balanced = PreservationCandidateProjection {
        definition: STORAGE_TIMBER_PROVISIONS_CHEST,
        production_ticks: 150,
        raw_material_mass_mg: 3_000_000,
        remaining_fresh_ticks: 400,
    };
    let strongest = PreservationCandidateProjection {
        definition: STORAGE_INSULATED_TIMBER_PANTRY,
        production_ticks: 200,
        raw_material_mass_mg: 6_000_000,
        remaining_fresh_ticks: 1_000,
    };
    assert_eq!(
        select_preservation_projection_for_attention_value(
            1_000_000,
            3_000_000,
            &[quick, balanced, strongest],
        ),
        balanced,
        "reserving construction matter must let a middle physical tradeoff win without inventing a content tie-break"
    );
}

#[test]
fn preservation_decline_executes_without_spending_the_raw_opportunity() {
    use super::survival_probe::preservation_decision::evaluate_preservation_decision;
    const DECLINE_POLICY_REPLAY_SEED: u64 = 0x9B76_F388_4EA8_CF64;

    let registries = build_registries();
    // Replayed ordinary world/policy pair where retaining the disclosed raw opportunity outranks
    // every enclosure. The contract is the no-build decision and unspent opportunity, not any
    // particular preservation tuning trajectory.
    let seed = SURVIVAL_DECLINE_COVERAGE_SEED;
    let world = provisioning_world(&registries, seed);
    let decision = evaluate_preservation_decision(
        &registries,
        seed,
        DECLINE_POLICY_REPLAY_SEED,
        world.foods[world.witness_index],
        world.preserved_reserve_mass,
    );
    assert_eq!(decision.investment, None);
    assert!(decision.no_build.elapsed_ticks > 0);
    assert!(decision.no_build.retained_raw_mg > 0);
    assert_eq!(decision.no_build.remaining_fresh_ticks, 0);
}

#[test]
fn preservation_raw_bootstrap_is_explicit_not_inferred_from_missing_producers() {
    assert!(is_disclosed_preservation_raw_material(CommodityKey::new(
        MATERIAL_WOOD,
        FORM_LOG,
    )));
    assert!(is_disclosed_preservation_raw_material(CommodityKey::new(
        MATERIAL_STONE,
        FORM_LUMP,
    )));
    assert!(is_disclosed_preservation_raw_material(CommodityKey::new(
        MATERIAL_COPPER,
        FORM_NATIVE_METAL,
    )));
    assert!(!is_disclosed_preservation_raw_material(CommodityKey::new(
        MATERIAL_COPPER,
        FORM_INGOT,
    )));
}
use super::survival_probe::{
    DietProvisioningPolicy, PreservationInvestmentPolicy, SurvivalStartProfile,
    diet_provisioning_policy_for_behavior_seed, preservation_attention_value_ppm,
    preservation_material_budget_ppm, preservation_minimum_return_ppm,
    prospecting_method_for_work_pressure, provisioning_world,
};

#[test]
fn generated_preservation_witnesses_are_old_enough_to_show_the_authored_rate() {
    let registries = build_registries();
    for seed in 1_u64..=256 {
        let world = provisioning_world(&registries, seed);
        let minimum =
            minimum_visible_preservation_age_ticks(world.inherited_preservation_multiplier_ppm);
        assert!(
            world.age_ticks >= minimum,
            "seed {seed:#x} generated age {}t below visible preservation threshold {minimum}t for {}ppm",
            world.age_ticks,
            world.inherited_preservation_multiplier_ppm,
        );
    }
}

#[test]
fn survival_explanation_marks_singleton_enclosure_without_forcing_investment() {
    use super::survival_explanation::{
        PreservationComparison, preservation_comparison_explanation,
    };
    // This is an explanation-state contract, not a content-market contract. Construct the
    // singleton comparison directly so adding another valid enclosure cannot make formatting tests
    // stale.
    let comparison = PreservationComparison::from_candidates(
        1,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
    );
    assert_eq!(comparison, PreservationComparison::EnclosureSingleton);
    assert_eq!(
        comparison.selection_label("attention-efficient"),
        "enclosure-singleton"
    );
    let _ = preservation_comparison_explanation(comparison, || {
        panic!("a singleton must not format a counterfactual against itself")
    });
}

#[test]
fn survival_explanation_preserves_real_comparisons_and_distinguishes_shared_references() {
    use super::survival_explanation::{
        PreservationComparison, diet_comparison_explanation, preservation_comparison_explanation,
    };
    let comparison = PreservationComparison::from_candidates(
        2,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        STORAGE_TIMBER_PROVISIONS_CHEST,
    );
    assert_eq!(comparison, PreservationComparison::DistinctReferences);
    assert_eq!(
        comparison.selection_label("maximum-protection"),
        "maximum-protection"
    );
    assert_eq!(
        preservation_comparison_explanation(comparison, || "measured tradeoff".into()),
        "measured tradeoff"
    );
    assert_eq!(
        diet_comparison_explanation(true, || "measured diet".into()),
        "measured diet"
    );
    let shared = PreservationComparison::from_candidates(
        2,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        STORAGE_TIMBER_PROVISIONS_CHEST,
    );
    assert_eq!(shared, PreservationComparison::SharedReference);
    let _ = preservation_comparison_explanation(shared, || panic!("same reference"));
}

#[test]
fn survival_explanation_reports_supply_limited_policy_convergence_and_measured_outcome() {
    use super::{
        survival_explanation::diet_comparison_explanation, survival_probe::selected_food_indices,
    };
    let registries = build_registries();
    let world = provisioning_world(&registries, 1);
    let foods = &world.foods[..2];
    let compact = selected_food_indices(foods, DietProvisioningPolicy::CompactCalories)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let balanced = selected_food_indices(foods, DietProvisioningPolicy::BalancedRecovery)
        .into_iter()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        compact, balanced,
        "two-category supply removes the category choice"
    );
    assert_eq!(
        diet_comparison_explanation(false, || "measured constrained outcome".into()),
        "comparison:supply-constrained policy-choice:converged measured constrained outcome"
    );
    assert_ne!(
        DietProvisioningPolicy::CompactCalories,
        DietProvisioningPolicy::BalancedRecovery
    );
}

#[test]
fn preservation_resource_deltas_preserve_comparison_direction() {
    assert_eq!(SignedResourceDelta::between(13, 10).to_string(), "+3");
    assert_eq!(SignedResourceDelta::between(10, 13).to_string(), "-3");
    assert_eq!(SignedResourceDelta::between(10, 10).to_string(), "+0");
}

#[test]
fn preservation_storage_routes_are_authored_recoverable_tradeoffs() {
    let registries = build_registries();
    let ambient = StockpileStorageProfile::unbounded_solid_only().preservation_multiplier_ppm();
    let preservation = registries
        .storage()
        .definitions()
        .filter(|definition| definition.storage_profile().preservation_multiplier_ppm() > ambient)
        .collect::<Vec<_>>();
    assert!(
        !preservation.is_empty(),
        "survival has no authored preservation enclosure"
    );

    for storage in &preservation {
        assert!(!storage.maximum_stockpile_capacity().is_zero());
        assert!(!storage.assembly_profile().input_mass().is_zero());
        let plan = preservation_construction_plan(&registries, storage.assembly_profile());
        assert!(plan.attention_ticks > 0);
        assert!(plan.routes.iter().any(|route| !route.steps.is_empty()));
    }

    let rough = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("rough provisions field box disappeared"));
    let standard = registries
        .storage()
        .get(STORAGE_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("standard provisions chest disappeared"));
    let protected = registries
        .storage()
        .get(STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("double-wall provisions chest disappeared"));
    let bulk = registries
        .storage()
        .get(STORAGE_BULK_TIMBER_PROVISIONS_CRATE)
        .unwrap_or_else(|| panic!("bulk provisions crate disappeared"));
    let pantry = registries
        .storage()
        .get(STORAGE_INSULATED_TIMBER_PANTRY)
        .unwrap_or_else(|| panic!("insulated provisions pantry disappeared"));
    let crock = registries
        .storage()
        .get(STORAGE_CARVED_STONE_PROVISIONS_CROCK)
        .unwrap_or_else(|| panic!("carved stone provisions crock disappeared"));
    let banded_crock = registries
        .storage()
        .get(STORAGE_COPPER_BANDED_STONE_PROVISIONS_CROCK)
        .unwrap_or_else(|| panic!("copper-banded provisions crock disappeared"));
    assert!(rough.maximum_stockpile_capacity() < standard.maximum_stockpile_capacity());
    assert!(
        rough.storage_profile().preservation_multiplier_ppm()
            < standard.storage_profile().preservation_multiplier_ppm()
    );
    assert!(rough.assembly_profile().input_mass() < standard.assembly_profile().input_mass());
    assert!(
        preservation_construction_plan(&registries, rough.assembly_profile()).attention_ticks
            < preservation_construction_plan(&registries, standard.assembly_profile())
                .attention_ticks
    );
    assert_eq!(
        standard.maximum_stockpile_capacity(),
        protected.maximum_stockpile_capacity()
    );
    assert!(
        protected.storage_profile().preservation_multiplier_ppm()
            > standard.storage_profile().preservation_multiplier_ppm()
    );
    assert!(protected.assembly_profile().input_mass() > standard.assembly_profile().input_mass());
    assert!(
        preservation_construction_plan(&registries, protected.assembly_profile()).attention_ticks
            > preservation_construction_plan(&registries, standard.assembly_profile())
                .attention_ticks
    );
    assert!(bulk.maximum_stockpile_capacity() > standard.maximum_stockpile_capacity());
    assert!(
        bulk.storage_profile().preservation_multiplier_ppm()
            < standard.storage_profile().preservation_multiplier_ppm()
    );
    assert!(pantry.maximum_stockpile_capacity() < standard.maximum_stockpile_capacity());
    assert!(
        pantry.storage_profile().preservation_multiplier_ppm()
            > protected.storage_profile().preservation_multiplier_ppm()
    );
    assert!(
        preservation_construction_plan(&registries, pantry.assembly_profile()).attention_ticks
            > preservation_construction_plan(&registries, protected.assembly_profile())
                .attention_ticks
    );
    assert!(crock.maximum_stockpile_capacity() < standard.maximum_stockpile_capacity());
    assert!(
        crock.storage_profile().preservation_multiplier_ppm()
            > standard.storage_profile().preservation_multiplier_ppm()
    );
    assert!(
        crock.storage_profile().preservation_multiplier_ppm()
            < protected.storage_profile().preservation_multiplier_ppm()
    );
    assert!(
        crock
            .assembly_profile()
            .inputs()
            .iter()
            .all(|input| input.commodity().material() == MATERIAL_STONE)
    );
    assert_eq!(
        banded_crock.maximum_stockpile_capacity(),
        crock.maximum_stockpile_capacity(),
        "copper banding must buy preservation rather than hidden bulk capacity"
    );
    assert!(
        banded_crock.storage_profile().preservation_multiplier_ppm()
            > pantry.storage_profile().preservation_multiplier_ppm()
    );
    let banded_plan = preservation_construction_plan(&registries, banded_crock.assembly_profile());
    assert!(
        banded_plan
            .raw_requirements()
            .contains_key(&CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)),
        "copper-banded preservation must expose its finite post-copper material commitment"
    );

    for storage in preservation {
        let mut recoverable_body_count = 0_u32;
        for input in storage.assembly_profile().inputs() {
            let handbook = registries
                .commodity_handbook_entry(input.commodity())
                .unwrap_or_else(|| panic!("preservation body commodity disappeared"));
            assert!(
                handbook
                    .sources()
                    .iter()
                    .any(|source| matches!(source, CommoditySource::ManualCraft { .. })),
                "preservation body {} has no ordinary manual production route",
                input.commodity().value()
            );
            let salvage = handbook.uses().iter().find_map(|usage| {
                let CommodityUse::ManualCraft { process, .. } = *usage else {
                    return None;
                };
                let route = registries
                    .crafting()
                    .get_manual(process)
                    .unwrap_or_else(|| {
                        panic!("commodity handbook exposed an unknown manual process")
                    });
                (route.input_mass() == input.mass()
                    && !route.outputs().is_empty()
                    && route.outputs().iter().all(|output| {
                        output.commodity().material() == input.commodity().material()
                            && output.commodity() != input.commodity()
                    })
                    && route
                        .outputs()
                        .iter()
                        .try_fold(deep_hearth::core::quantity::Mass::ZERO, |total, output| {
                            total.checked_add(output.mass())
                        })
                        == Some(input.mass()))
                .then_some(route)
            });
            if salvage.is_some() {
                recoverable_body_count += 1;
            }
        }
        assert!(
            recoverable_body_count != 0,
            "preservation enclosure {} has no recoverable fabricated body",
            storage.id().value()
        );
    }
}

#[test]
fn survival_generation_covers_authored_options_without_policy_leakage() {
    let registries = build_registries();
    let authored_foods = registries
        .survival()
        .foods()
        .map(|food| food.commodity())
        .collect::<BTreeSet<_>>();
    let authored_categories = registries
        .survival()
        .foods()
        .map(|food| food.category())
        .collect::<BTreeSet<_>>();
    let authored_prospecting = registries
        .labor()
        .prospecting_definitions()
        .map(|definition| definition.id())
        .collect::<BTreeSet<_>>();
    let authored_preservation = registries
        .storage()
        .definitions()
        .map(|definition| {
            (
                definition.id().value(),
                definition.storage_profile().preservation_multiplier_ppm(),
            )
        })
        .collect::<BTreeSet<_>>();

    let sample_count = authored_foods
        .len()
        .max(authored_prospecting.len())
        .max(authored_preservation.len())
        .saturating_mul(64)
        .clamp(256, 1024);
    let worlds = (1_u64
        ..=u64::try_from(sample_count)
            .unwrap_or_else(|_| unreachable!("bounded survival sample count fits u64")))
        .map(|seed| provisioning_world(&registries, seed))
        .collect::<Vec<_>>();

    assert_eq!(
        worlds
            .iter()
            .map(|world| world.start_profile)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            SurvivalStartProfile::FullReserve,
            SurvivalStartProfile::HungerWarningBoundary,
            SurvivalStartProfile::HydrationWarningBoundary,
        ]),
        "bounded survival generation must cover every start-pressure archetype"
    );
    let sampled_foods = worlds
        .iter()
        .flat_map(|world| world.foods.iter().copied())
        .map(|food| food.commodity())
        .collect::<BTreeSet<_>>();
    assert!(sampled_foods.is_subset(&authored_foods));
    if authored_foods.len() > 1 {
        assert!(sampled_foods.len() > 1);
    }
    let sampled_category_counts = worlds
        .iter()
        .map(|world| {
            world
                .foods
                .iter()
                .map(|food| food.category())
                .collect::<BTreeSet<_>>()
                .len()
        })
        .collect::<BTreeSet<_>>();
    assert!(sampled_category_counts.contains(&authored_categories.len()));
    if authored_categories.len() > 1 {
        assert!(
            sampled_category_counts
                .iter()
                .any(|count| *count < authored_categories.len())
        );
    }

    let sampled_preservation = worlds
        .iter()
        .map(|world| {
            (
                world.inherited_preservation_definition.value(),
                world.inherited_preservation_multiplier_ppm,
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        sampled_preservation, authored_preservation,
        "bounded survival generation must exercise every authored preservation enclosure"
    );

    let sampled_prospecting = (1_u64
        ..=u64::try_from(sample_count)
            .unwrap_or_else(|_| unreachable!("bounded prospecting sample count fits u64")))
        .map(|seed| prospecting_method_for_work_pressure(&registries, seed))
        .collect::<BTreeSet<_>>();
    assert!(sampled_prospecting.is_subset(&authored_prospecting));
    if authored_prospecting.len() > 1 {
        assert!(sampled_prospecting.len() > 1);
    }

    let diet_policies = (1_u64..=16)
        .map(diet_provisioning_policy_for_behavior_seed)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        diet_policies,
        BTreeSet::from([
            DietProvisioningPolicy::CompactCalories,
            DietProvisioningPolicy::BalancedRecovery,
        ])
    );
    let preservation_attention_values = (1_u64..=32)
        .map(preservation_attention_value_ppm)
        .collect::<BTreeSet<_>>();
    assert!(preservation_attention_values.len() > 1);
    assert!(
        preservation_attention_values
            .iter()
            .all(|value| (1_000_000..=10_000_000).contains(value))
    );
    let preservation_minimum_returns = (1_u64..=32)
        .map(preservation_minimum_return_ppm)
        .collect::<BTreeSet<_>>();
    assert!(preservation_minimum_returns.len() > 1);
    assert!(
        preservation_minimum_returns
            .iter()
            .all(|value| (1_000_000..=4_000_000).contains(value))
    );
    let preservation_material_budgets = (0_u64..4)
        .map(preservation_material_budget_ppm)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        preservation_material_budgets,
        BTreeSet::from([400_000, 600_000, 850_000, 1_000_000]),
        "four independently stratified behavior cases must cover every preservation material-commitment band"
    );
    let projection_world = provisioning_world(&registries, 0x51A2_0001);
    let projected = project_preservation_candidates_with_raw_opportunity(
        &registries,
        0x51A2_0001,
        projection_world.foods[projection_world.witness_index],
        projection_world.preserved_reserve_mass,
        None,
    );
    let unlimited_material_budget = projected
        .iter()
        .map(|projection| projection.raw_material_mass_mg)
        .max()
        .unwrap_or_else(|| panic!("preservation projection is nonempty"));
    let low_threshold_choice = select_preservation_projection_for_attention_value(
        1_000_000,
        unlimited_material_budget,
        &projected,
    );
    let high_threshold_choice = select_preservation_projection_for_attention_value(
        10_000_000,
        unlimited_material_budget,
        &projected,
    );
    assert!(
        low_threshold_choice.remaining_fresh_ticks >= high_threshold_choice.remaining_fresh_ticks,
        "lower attention valuation must not select less preservation from the same physical frontier"
    );
    assert!(
        low_threshold_choice.production_ticks >= high_threshold_choice.production_ticks,
        "higher attention valuation must not select a slower construction from the same physical frontier"
    );
    let above_field_box = capacity_just_above(&registries, STORAGE_ROUGH_TIMBER_FIELD_BOX);
    assert!(
        above_field_box <= storage_capacity(&registries, STORAGE_TIMBER_PROVISIONS_CHEST),
        "authored standard chest must still provide a capacity step above the field box"
    );
    assert_eq!(
        preservation_storage_definition_for_policy_with_constraints(
            &registries,
            PreservationInvestmentPolicy::AttentionEfficient,
            above_field_box,
            None,
        ),
        STORAGE_TIMBER_PROVISIONS_CHEST,
        "once field-box capacity is insufficient, attention-efficient ranking must choose the standard chest"
    );
    let above_standard_chest = capacity_just_above(&registries, STORAGE_TIMBER_PROVISIONS_CHEST);
    assert!(
        above_standard_chest <= storage_capacity(&registries, STORAGE_BULK_TIMBER_PROVISIONS_CRATE),
        "authored bulk crate must still provide a capacity step above the standard chest"
    );
    assert_eq!(
        preservation_storage_definition_for_policy_with_constraints(
            &registries,
            PreservationInvestmentPolicy::AttentionEfficient,
            above_standard_chest,
            None,
        ),
        STORAGE_BULK_TIMBER_PROVISIONS_CRATE,
        "bulk reserve feasibility must make the bulk crate the ordinary attention-efficient choice"
    );
    let above_smaller_maximum_protection = std::cmp::max(
        capacity_just_above(&registries, STORAGE_INSULATED_TIMBER_PANTRY),
        capacity_just_above(&registries, STORAGE_CARVED_STONE_PROVISIONS_CROCK),
    );
    assert!(
        above_smaller_maximum_protection
            <= storage_capacity(&registries, STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST),
        "double-wall chest must remain a capacity-feasible maximum-protection fallback"
    );
    assert_eq!(
        preservation_storage_definition_for_policy_with_constraints(
            &registries,
            PreservationInvestmentPolicy::MaximumProtection,
            above_smaller_maximum_protection,
            None,
        ),
        STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
        "maximum-protection ranking must reject the pantry and crock when the reserve exceeds their capacity"
    );
    let stone_only = storage_raw_opportunity(&registries, STORAGE_CARVED_STONE_PROVISIONS_CROCK);
    assert!(
        stone_only
            .iter()
            .all(|(commodity, _)| *commodity == CommodityKey::new(MATERIAL_STONE, FORM_LUMP)),
        "carved-crock bootstrap must remain a stone-only disclosed opportunity"
    );
    assert_eq!(
        preservation_storage_definition_for_policy_with_constraints(
            &registries,
            PreservationInvestmentPolicy::MaximumProtection,
            Mass::from_milligrams(1),
            Some(&stone_only),
        ),
        STORAGE_CARVED_STONE_PROVISIONS_CROCK,
        "a stone-only opportunity must not rank timber enclosures the actor cannot construct"
    );
    let scarce_timber =
        storage_raw_opportunity(&registries, STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST);
    assert!(
        scarce_timber
            .iter()
            .all(|(commodity, _)| *commodity == CommodityKey::new(MATERIAL_WOOD, FORM_LOG)),
        "double-wall chest bootstrap must remain a timber-only disclosed opportunity"
    );
    assert_eq!(
        preservation_storage_definition_for_policy_with_constraints(
            &registries,
            PreservationInvestmentPolicy::MaximumProtection,
            Mass::from_milligrams(1),
            Some(&scarce_timber),
        ),
        STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
        "exact double-wall raw stock must not admit a more material-intensive timber route"
    );
    let timber_only = storage_raw_opportunity(&registries, STORAGE_INSULATED_TIMBER_PANTRY);
    assert!(
        timber_only
            .iter()
            .all(|(commodity, _)| *commodity == CommodityKey::new(MATERIAL_WOOD, FORM_LOG)),
        "insulated pantry bootstrap must remain a timber-only disclosed opportunity"
    );
    assert_eq!(
        preservation_storage_definition_for_policy_with_constraints(
            &registries,
            PreservationInvestmentPolicy::MaximumProtection,
            Mass::from_milligrams(1),
            Some(&timber_only),
        ),
        STORAGE_INSULATED_TIMBER_PANTRY,
        "a timber opportunity should retain the authored maximum-protection endpoint"
    );

    let exploratory = focused_probe_cases_from(FocusedProbeSeedPlan {
        variation_count: EXPLORATORY_VARIATION_COUNT,
        scenario_raw: None,
        variation_raw: Some("0x1111"),
        behavior_raw: Some("0x2222"),
        maintained_seed: 0x1234,
        maintained_coverage_seeds: &[],
        probe_salt: 0x5355_5256_5052_4F42,
        default_variation_root: 0,
        default_behavior_root: Some(0),
    })
    .unwrap_or_else(|error| panic!("exploratory survival seed plan failed: {error:?}"));
    assert_eq!(
        exploratory
            .into_iter()
            .filter(|case| case.role() == FocusedProbeRole::OrganicVariation)
            .map(|case| {
                diet_provisioning_policy_for_behavior_seed(
                    case.behavior_seed()
                        .unwrap_or_else(|| panic!("organic behavior seed missing")),
                )
            })
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            DietProvisioningPolicy::CompactCalories,
            DietProvisioningPolicy::BalancedRecovery,
        ])
    );

    let original = provisioning_world(&registries, 0x51A2_0001);
    let projected = project_preservation_candidates_with_raw_opportunity(
        &registries,
        0x51A2_0001,
        original.foods[original.witness_index],
        original.preserved_reserve_mass,
        None,
    );
    for behavior_seed in 1_u64..=4 {
        let _selected =
            select_preservation_projection(behavior_seed, unlimited_material_budget, &projected);
        let replay = provisioning_world(&registries, 0x51A2_0001);
        assert_eq!(
            (
                replay.start_profile,
                replay.inherited_preservation_definition,
                replay.inherited_preservation_multiplier_ppm,
                replay.provisioning_wait_ticks,
                replay.age_ticks,
                replay.witness_index,
                replay.preserved_reserve_mass,
            ),
            (
                original.start_profile,
                original.inherited_preservation_definition,
                original.inherited_preservation_multiplier_ppm,
                original.provisioning_wait_ticks,
                original.age_ticks,
                original.witness_index,
                original.preserved_reserve_mass,
            ),
            "actor preservation policy must not rewrite world-seeded history"
        );
    }
}
