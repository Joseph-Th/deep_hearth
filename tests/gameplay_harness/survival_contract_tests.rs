//! Cheap survival world-generation and preservation-route contracts.

use std::collections::BTreeSet;

use deep_hearth::content::{
    FORM_LOG, FORM_LUMP, MATERIAL_STONE, MATERIAL_WOOD, STORAGE_BULK_TIMBER_PROVISIONS_CRATE,
    STORAGE_CARVED_STONE_PROVISIONS_CROCK, STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
    STORAGE_INSULATED_TIMBER_PANTRY, STORAGE_ROUGH_TIMBER_FIELD_BOX,
    STORAGE_TIMBER_PROVISIONS_CHEST, build_registries,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::inventory::StockpileStorageProfile;
use deep_hearth::material::CommodityKey;

use super::focused_seeds::{
    EXPLORATORY_VARIATION_COUNT, FocusedProbeRole, FocusedProbeSeedPlan, focused_probe_cases_from,
};
use super::preservation_route::preservation_construction_plan;
use super::survival_probe::{
    DietProvisioningPolicy, PreservationInvestmentPolicy, SurvivalStartProfile,
    diet_provisioning_policy_for_behavior_seed, preservation_freshness_return_threshold_ppm,
    preservation_policy_for_projected_return, preservation_storage_definition_for_policy,
    preservation_storage_definition_for_policy_and_capacity,
    preservation_storage_definition_for_policy_and_opportunity,
    prospecting_method_for_work_pressure, provisioning_world,
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

    for storage in preservation {
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
            let salvage =
                registries
                    .crafting()
                    .manual_consumers(input.commodity())
                    .find(|route| {
                        route.input_mass() == input.mass()
                            && !route.outputs().is_empty()
                            && route.outputs().iter().all(|output| {
                                output.commodity().material() == input.commodity().material()
                                    && output.commodity() != input.commodity()
                            })
                            && route.outputs().iter().try_fold(
                                deep_hearth::core::quantity::Mass::ZERO,
                                |total, output| total.checked_add(output.mass()),
                            ) == Some(input.mass())
                    });
            assert!(
                salvage.is_some(),
                "preservation body {} has no exact same-material salvage route",
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
        DietProvisioningPolicy::ALL.into_iter().collect()
    );
    let preservation_thresholds = (1_u64..=32)
        .map(preservation_freshness_return_threshold_ppm)
        .collect::<BTreeSet<_>>();
    assert!(preservation_thresholds.len() > 1);
    assert!(
        preservation_thresholds
            .iter()
            .all(|threshold| (1_000_000..=4_000_000).contains(threshold))
    );
    assert_eq!(
        preservation_policy_for_projected_return(1, 0, 140),
        PreservationInvestmentPolicy::AttentionEfficient
    );
    let low_threshold_seed = (1_u64..=1_024)
        .min_by_key(|seed| preservation_freshness_return_threshold_ppm(*seed))
        .unwrap_or_else(|| unreachable!("nonempty bounded behavior seed search"));
    let high_threshold_seed = (1_u64..=1_024)
        .max_by_key(|seed| preservation_freshness_return_threshold_ppm(*seed))
        .unwrap_or_else(|| unreachable!("nonempty bounded behavior seed search"));
    assert_eq!(
        preservation_policy_for_projected_return(low_threshold_seed, 560, 140),
        PreservationInvestmentPolicy::MaximumProtection
    );
    assert_eq!(
        preservation_policy_for_projected_return(high_threshold_seed, 140, 140),
        PreservationInvestmentPolicy::AttentionEfficient
    );
    assert_eq!(
        preservation_storage_definition_for_policy_and_capacity(
            &registries,
            PreservationInvestmentPolicy::AttentionEfficient,
            Mass::from_milligrams(15_000_000),
        ),
        STORAGE_TIMBER_PROVISIONS_CHEST,
        "a medium reserve should reject the cheap field box before ranking construction attention"
    );
    assert_eq!(
        preservation_storage_definition_for_policy_and_capacity(
            &registries,
            PreservationInvestmentPolicy::AttentionEfficient,
            Mass::from_milligrams(30_000_000),
        ),
        STORAGE_BULK_TIMBER_PROVISIONS_CRATE,
        "bulk reserve feasibility must make the bulk crate the ordinary attention-efficient choice"
    );
    assert_eq!(
        preservation_storage_definition_for_policy_and_capacity(
            &registries,
            PreservationInvestmentPolicy::MaximumProtection,
            Mass::from_milligrams(9_000_000),
        ),
        STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
        "maximum-protection ranking must reject the pantry and crock when the reserve exceeds their capacity"
    );
    let stone_only = [(
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(3_000_000),
    )];
    assert_eq!(
        preservation_storage_definition_for_policy_and_opportunity(
            &registries,
            PreservationInvestmentPolicy::MaximumProtection,
            Mass::from_milligrams(5_000_000),
            &stone_only,
        ),
        STORAGE_CARVED_STONE_PROVISIONS_CROCK,
        "a stone-only opportunity must not rank timber enclosures the actor cannot construct"
    );
    let scarce_timber = [(
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(5_000_000),
    )];
    assert_eq!(
        preservation_storage_definition_for_policy_and_opportunity(
            &registries,
            PreservationInvestmentPolicy::MaximumProtection,
            Mass::from_milligrams(5_000_000),
            &scarce_timber,
        ),
        STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
        "five kilograms of timber must not admit the six-kilogram pantry route"
    );
    let timber_only = [(
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(6_000_000),
    )];
    assert_eq!(
        preservation_storage_definition_for_policy_and_opportunity(
            &registries,
            PreservationInvestmentPolicy::MaximumProtection,
            Mass::from_milligrams(5_000_000),
            &timber_only,
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
        DietProvisioningPolicy::ALL.into_iter().collect()
    );

    let original = provisioning_world(&registries, 0x51A2_0001);
    for behavior_seed in 1_u64..=4 {
        let policy = preservation_policy_for_projected_return(behavior_seed, 100, 140);
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
