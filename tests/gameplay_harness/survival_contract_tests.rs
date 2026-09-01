//! Cheap survival world-generation and preservation-route contracts.

use std::collections::BTreeSet;

use deep_hearth::content::{
    FORM_BOARD, FORM_CHIP, MATERIAL_WOOD, STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
    STORAGE_TIMBER_PROVISIONS_CHEST, build_registries,
};
use deep_hearth::inventory::StockpileStorageProfile;
use deep_hearth::material::CommodityKey;

use super::focused_seeds::{
    EXPLORATORY_VARIATION_COUNT, FocusedProbeRole, FocusedProbeSeedPlan, focused_probe_cases_from,
};
use super::preservation_route::preservation_construction_plan;
use super::survival_probe::{
    DietProvisioningPolicy, PreservationInvestmentPolicy, SurvivalStartProfile,
    diet_provisioning_policy_for_behavior_seed, preservation_investment_policy_for_behavior_seed,
    preservation_storage_definition_for_policy, prospecting_method_for_work_pressure,
    provisioning_world,
};

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

    for storage in preservation {
        assert!(!storage.maximum_stockpile_capacity().is_zero());
        assert!(!storage.assembly_profile().input_mass().is_zero());
        let plan = preservation_construction_plan(&registries, storage.assembly_profile());
        assert!(plan.attention_ticks > 0);
        assert!(plan.routes.iter().any(|route| !route.steps.is_empty()));
    }

    let standard = registries
        .storage()
        .get(STORAGE_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("standard provisions chest disappeared"));
    let protected = registries
        .storage()
        .get(STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("double-wall provisions chest disappeared"));
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

    for storage in [standard, protected] {
        for input in storage.assembly_profile().inputs() {
            assert!(
                registries
                    .crafting()
                    .manual_producers(input.commodity())
                    .next()
                    .is_some(),
                "preservation body {} has no ordinary manual production route",
                input.commodity().value()
            );
            let salvage = registries
                .crafting()
                .manual_consumers(input.commodity())
                .find(|route| {
                    if route.input_mass() != input.mass() {
                        return false;
                    }
                    let board = route
                        .outputs()
                        .iter()
                        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
                        .map(|output| output.mass());
                    let chips = route
                        .outputs()
                        .iter()
                        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_CHIP))
                        .map(|output| output.mass());
                    matches!((board, chips), (Some(board), Some(chips)) if !board.is_zero() && !chips.is_zero() && board.checked_add(chips) == Some(input.mass()))
                });
            assert!(
                salvage.is_some(),
                "preservation body {} has no exact board-plus-chip salvage route",
                input.commodity().value()
            );
        }
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
        .saturating_mul(8)
        .clamp(32, 256);
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
        SurvivalStartProfile::ALL.into_iter().collect(),
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
    assert!(sampled_preservation.is_subset(&authored_preservation));
    if authored_preservation.len() > 1 {
        assert!(sampled_preservation.len() > 1);
    }

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
        DietProvisioningPolicy::ALL.into_iter().collect()
    );
    let preservation_policies = (1_u64..=32)
        .map(preservation_investment_policy_for_behavior_seed)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        preservation_policies,
        PreservationInvestmentPolicy::ALL.into_iter().collect()
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
        DietProvisioningPolicy::ALL.into_iter().collect()
    );

    let original = provisioning_world(&registries, 0x51A2_0001);
    for behavior_seed in 1_u64..=4 {
        let policy = preservation_investment_policy_for_behavior_seed(behavior_seed);
        let _selected = preservation_storage_definition_for_policy(&registries, policy);
        let replay = provisioning_world(&registries, 0x51A2_0001);
        assert_eq!(
            (
                replay.start_profile,
                replay.inherited_preservation_definition,
                replay.inherited_preservation_multiplier_ppm,
                replay.provisioning_wait_ticks,
                replay.age_ticks,
                replay.witness_index,
            ),
            (
                original.start_profile,
                original.inherited_preservation_definition,
                original.inherited_preservation_multiplier_ppm,
                original.provisioning_wait_ticks,
                original.age_ticks,
                original.witness_index,
            ),
            "actor preservation policy must not rewrite world-seeded history"
        );
    }
}
