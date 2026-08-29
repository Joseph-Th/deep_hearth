//! Dynamic gameplay-catalog discovery and bounded scenario-generation diversity contracts.

use std::collections::BTreeSet;

use deep_hearth::content::{
    PROCESS_HAND_BREAK_ORE, PROCESS_HAND_SORT_NATIVE_COPPER, PROCESS_SEPARATE_NATIVE_COPPER,
    build_registries,
};
use deep_hearth::inventory::StockpileStorageProfile;

use super::focused_seeds::{
    EXPLORATORY_VARIATION_COUNT, FocusedProbeCase, FocusedProbeRole, FocusedProbeSeedPlan,
    focused_probe_cases_from,
};
use super::foundry_probe::probe_setup as foundry_probe_setup;
use super::ore_probe::probe_parameters;
use super::preservation_route::preservation_construction_plan;
use super::progression_probe::{
    extraction_grade_premium_ppm, manual_processing::manual_processing_setup, varied_four_way_order,
};
use super::report::{ProcessResolverKind, process_catalog_entries};
use super::survival_probe::{
    DietProvisioningPolicy, SurvivalStartProfile, diet_provisioning_policy_for_behavior_seed,
    preservation_storage_definition_for_seed, prospecting_method_for_work_pressure,
    provisioning_world,
};

fn preservation_construction_summary(
    registries: &deep_hearth::registry::Registries,
    definition: deep_hearth::inventory::StorageDefinitionId,
) -> (usize, u64) {
    let definition = registries
        .storage()
        .get(definition)
        .unwrap_or_else(|| panic!("preservation construction summary references unknown storage"));
    let plan = preservation_construction_plan(registries, definition.assembly_profile());
    let stages = plan.routes.iter().map(|route| route.steps.len()).sum();
    (stages, plan.attention_ticks)
}

#[test]
fn gameplay_catalog_is_discovered_from_runtime_owners() {
    let registries = build_registries();
    assert_eq!(
        process_catalog_entries(&registries).len(),
        registries.production().definitions().count(),
        "cold-agent catalog discovery must classify every authored process"
    );
    for entry in process_catalog_entries(&registries) {
        if matches!(
            entry.resolver,
            ProcessResolverKind::ManualCraft
                | ProcessResolverKind::ManualComminution
                | ProcessResolverKind::ManualSeparation
        ) {
            assert_eq!(entry.nominal_provider_count, 0);
            assert_eq!(entry.matching_energy_store_count, 0);
            continue;
        }
        assert!(
            entry.nominal_provider_count > 0,
            "machine process {} ({}) has no authored equipment definition satisfying its nominal capability requirements",
            entry.process.value(),
            entry.name,
        );
        assert!(
            entry.matching_energy_store_count > 0,
            "machine process {} ({}) has no energy store with the required carrier and transfer direction",
            entry.process.value(),
            entry.name,
        );
    }

    let catalog = process_catalog_entries(&registries);
    let manual_entry = catalog
        .iter()
        .find(|entry| entry.process == PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| {
            panic!("manual native-copper sorting is absent from the gameplay catalog")
        });
    assert_eq!(manual_entry.resolver, ProcessResolverKind::ManualSeparation);
    assert_eq!(manual_entry.nominal_provider_count, 0);
    assert_eq!(manual_entry.matching_energy_store_count, 0);

    let hand_break_entry = catalog
        .iter()
        .find(|entry| entry.process == PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("manual ore breaking is absent from the gameplay catalog"));
    assert_eq!(
        hand_break_entry.resolver,
        ProcessResolverKind::ManualComminution
    );
    assert_eq!(hand_break_entry.nominal_provider_count, 0);
    assert_eq!(hand_break_entry.matching_energy_store_count, 0);
    let hand_breaking = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("manual ore-breaking definition disappeared"));
    assert!(!hand_breaking.max_batch_mass().is_zero());
    assert!(!hand_breaking.processing_rate().is_zero());

    let manual = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("manual native-copper sorting definition disappeared"));
    let powered = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("powered native-copper sorting definition disappeared"));
    assert_eq!(manual.input_form(), hand_breaking.output_form());
    assert_eq!(
        manual.input_particle_size_range(),
        hand_breaking.output_particle_size(),
        "manual ore-processing stages must meet through their authored particle envelope"
    );
    assert!(manual.target_recovery_ppm() > 0);
    assert!(manual.target_recovery_ppm() < powered.target_recovery_ppm());
    assert!(!manual.max_batch_mass().is_zero());
    assert!(!manual.processing_rate().is_zero());

    let ambient_preservation =
        StockpileStorageProfile::unbounded_solid_only().preservation_multiplier_ppm();
    let storage_definitions = registries.storage().definitions().collect::<Vec<_>>();
    assert!(
        !storage_definitions.is_empty(),
        "cold-agent catalog lost every authored constructible storage definition"
    );
    for storage in storage_definitions {
        assert!(storage.maximum_stockpile_capacity().milligrams() > 0);
        assert!(storage.assembly_profile().input_mass().milligrams() > 0);
        assert!(
            storage.storage_profile().preservation_multiplier_ppm() > ambient_preservation,
            "constructible preservation storage must materially improve on ambient storage"
        );
        for input in storage.assembly_profile().inputs() {
            let producers = registries
                .crafting()
                .definitions()
                .filter(|definition| {
                    definition
                        .outputs()
                        .iter()
                        .any(|output| output.commodity() == input.commodity())
                })
                .collect::<Vec<_>>();
            assert!(
                !producers.is_empty(),
                "storage body commodity {} must expose at least one ordinary manual-production route to the gameplay actor",
                input.commodity().value()
            );
            for producer in producers {
                let process = catalog
                    .iter()
                    .find(|entry| entry.process == producer.process())
                    .unwrap_or_else(|| {
                        panic!("storage body producer is absent from the gameplay catalog")
                    });
                assert_eq!(process.resolver, ProcessResolverKind::ManualCraft);
            }
        }
        let (stages, attention_ticks) =
            preservation_construction_summary(&registries, storage.id());
        assert!(
            stages > 0 && attention_ticks > 0,
            "every authored preservation enclosure must expose a nontrivial ordinary manual-production route"
        );
    }
}

#[test]
fn gameplay_generators_retain_meaningful_physical_variation() {
    let registries = build_registries();
    let ore_setups = (1_u64..=8)
        .map(|seed| probe_parameters(&registries, seed))
        .collect::<Vec<_>>();
    assert!(
        ore_setups
            .iter()
            .map(|setup| {
                (
                    setup.batch_mass.milligrams(),
                    setup.copper_ppm,
                    setup.clay_share_ppm,
                )
            })
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "gameplay ore probe variation collapsed to one physical feed"
    );
    assert!(
        ore_setups
            .iter()
            .map(|setup| {
                (
                    setup.crusher_condition.parts_per_million(),
                    setup.grinder_condition.parts_per_million(),
                    setup.screen_condition.parts_per_million(),
                    setup.separator_condition.parts_per_million(),
                    setup.drive_energy.nanojoules(),
                )
            })
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "gameplay ore probe variation collapsed to one operating state"
    );

    let clue_orders = (1_u64..=12)
        .map(varied_four_way_order)
        .collect::<BTreeSet<_>>();
    assert!(
        clue_orders.len() > 1,
        "gameplay progression variation collapsed to one clue order"
    );
    assert!(clue_orders.iter().all(|order| {
        let mut sorted = *order;
        sorted.sort_unstable();
        sorted == [0, 1, 2, 3]
    }));

    let progression_actor_policies = (1_u64..=16)
        .map(|behavior_seed| {
            extraction_grade_premium_ppm(FocusedProbeCase::new(
                0xCAFE,
                Some(behavior_seed),
                FocusedProbeRole::OrganicVariation,
            ))
        })
        .collect::<BTreeSet<_>>();
    assert!(
        progression_actor_policies.len() > 1,
        "organic progression actor variation collapsed to one extraction-versus-mechanization policy"
    );

    let survival_actor_policies = (1_u64..=16)
        .map(diet_provisioning_policy_for_behavior_seed)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        survival_actor_policies,
        BTreeSet::from([
            DietProvisioningPolicy::CompactCalories,
            DietProvisioningPolicy::BalancedRecovery,
        ]),
        "organic survival actor variation must exercise both maintained provisioning preferences"
    );
    let exploratory_survival_cases = focused_probe_cases_from(FocusedProbeSeedPlan {
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
    .unwrap_or_else(|error| panic!("exploratory survival actor plan failed: {error:?}"));
    assert_eq!(
        exploratory_survival_cases
            .into_iter()
            .filter(|case| case.role() == FocusedProbeRole::OrganicVariation)
            .map(|case| {
                diet_provisioning_policy_for_behavior_seed(
                    case.behavior_seed()
                        .unwrap_or_else(|| panic!("organic survival actor seed missing")),
                )
            })
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            DietProvisioningPolicy::CompactCalories,
            DietProvisioningPolicy::BalancedRecovery,
        ]),
        "two-case exploratory survival sampling must show both actor preferences"
    );

    let manual_processing_setups = (1_u64..=16)
        .map(|seed| manual_processing_setup(&registries, seed))
        .collect::<Vec<_>>();
    assert!(
        manual_processing_setups
            .iter()
            .map(|setup| setup.ore_mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "manual copper fallback variation collapsed to one ore batch"
    );
    assert!(
        manual_processing_setups
            .iter()
            .map(|setup| setup.copper_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "manual copper fallback variation collapsed to one ore grade"
    );
    assert!(
        manual_processing_setups
            .iter()
            .map(|setup| setup.clay_share_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "manual copper fallback variation collapsed to one gangue composition"
    );

    let foundry_setups = (1_u64..=8)
        .map(|seed| foundry_probe_setup(&registries, seed))
        .collect::<Vec<_>>();
    assert!(
        foundry_setups
            .iter()
            .map(|setup| (
                setup.mass.milligrams(),
                setup.input_temperature.millikelvin()
            ))
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "gameplay foundry probe variation collapsed to one thermal batch"
    );
    assert!(
        foundry_setups
            .iter()
            .map(|setup| {
                (
                    setup.furnace_condition.parts_per_million(),
                    setup.mold_condition.parts_per_million(),
                    setup.electrical_energy.nanojoules(),
                    setup.thermal_sink_energy.nanojoules(),
                )
            })
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "gameplay foundry probe variation collapsed to one operating state"
    );

    let survival_worlds = (1_u64..=8)
        .map(|seed| provisioning_world(&registries, seed))
        .collect::<Vec<_>>();
    assert!(
        survival_worlds
            .iter()
            .map(|world| {
                (
                    world.start_profile,
                    world.provisioning_wait_ticks,
                    world.age_ticks,
                    world.preservation_multiplier_ppm,
                    world.witness_index,
                    world
                        .foods
                        .iter()
                        .map(|food| food.commodity().value())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "gameplay survival probe variation collapsed to one provisioning world"
    );
    assert!(
        survival_worlds
            .iter()
            .map(|world| world.start_profile)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "gameplay survival variation must span distinct starting reserve pressures"
    );
    assert!(
        survival_worlds
            .iter()
            .map(|world| {
                world
                    .foods
                    .iter()
                    .map(|food| food.category())
                    .collect::<BTreeSet<_>>()
                    .len()
            })
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "gameplay survival variation must span distinct food-category availability"
    );
    assert!(
        survival_worlds
            .iter()
            .map(|world| {
                world
                    .offered_masses
                    .iter()
                    .map(|mass| mass.milligrams())
                    .collect::<Vec<_>>()
            })
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "gameplay survival probe variation collapsed to one offered meal"
    );

    let authored_storage_count = registries.storage().definitions().count();
    let authored_food_count = registries.survival().foods().count();
    let authored_prospecting_count = registries.labor().prospecting_definitions().count();
    let broader_survival_sample_count = authored_storage_count
        .max(authored_food_count)
        .max(authored_prospecting_count)
        .saturating_mul(8)
        .clamp(32, 256);
    let broader_survival_sample = (1_u64
        ..=u64::try_from(broader_survival_sample_count)
            .unwrap_or_else(|_| unreachable!("bounded survival generator sample fits u64")))
        .map(|seed| provisioning_world(&registries, seed))
        .collect::<Vec<_>>();
    assert_eq!(
        broader_survival_sample
            .iter()
            .map(|world| world.start_profile)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            SurvivalStartProfile::FullReserve,
            SurvivalStartProfile::HungerWarningBoundary,
            SurvivalStartProfile::HydrationWarningBoundary,
        ]),
        "cheap survival generation must cover every authored starting reserve-pressure archetype without dedicated full-simulation coverage seeds"
    );
    let authored_category_count = registries
        .survival()
        .foods()
        .map(|food| food.category())
        .collect::<BTreeSet<_>>()
        .len();
    let sampled_category_counts = broader_survival_sample
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
    assert!(sampled_category_counts.contains(&authored_category_count));
    assert!(
        sampled_category_counts
            .iter()
            .any(|count| *count < authored_category_count),
        "cheap survival generation must retain at least one supply-collapsed dietary world"
    );
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
    let sampled_preservation = (1_u64
        ..=u64::try_from(broader_survival_sample_count)
            .unwrap_or_else(|_| unreachable!("bounded preservation selector sample fits u64")))
        .map(|seed| preservation_storage_definition_for_seed(&registries, seed))
        .map(|definition| {
            let record = registries
                .storage()
                .get(definition)
                .unwrap_or_else(|| unreachable!("sampled preservation definition is authored"));
            (
                definition.value(),
                record.storage_profile().preservation_multiplier_ppm(),
            )
        })
        .collect::<BTreeSet<_>>();
    assert!(
        sampled_preservation.is_subset(&authored_preservation),
        "survival worlds must use authored storage definitions rather than a harness-only preservation profile"
    );
    if authored_preservation.len() > 1 {
        assert!(
            sampled_preservation.len() > 1,
            "bounded organic survival generation collapsed multiple authored preservation options to one repeated choice"
        );
    }

    let authored_food_options = registries
        .survival()
        .foods()
        .map(|food| food.commodity())
        .collect::<BTreeSet<_>>();
    let sampled_food_options = broader_survival_sample
        .iter()
        .flat_map(|world| world.foods.iter().copied())
        .map(|food| food.commodity())
        .collect::<BTreeSet<_>>();
    assert!(
        sampled_food_options.is_subset(&authored_food_options),
        "bounded organic survival worlds must select only authored food options"
    );
    if authored_food_options.len() > 1 {
        assert!(
            sampled_food_options.len() > 1,
            "bounded organic survival generation collapsed multiple authored foods to one repeated option; the full authored set remains visible in the registry-derived content catalog"
        );
    }

    let authored_prospecting_methods = registries
        .labor()
        .prospecting_definitions()
        .map(|definition| definition.id())
        .collect::<BTreeSet<_>>();
    let sampled_prospecting_methods = (1_u64
        ..=u64::try_from(broader_survival_sample_count)
            .unwrap_or_else(|_| unreachable!("bounded prospecting sample fits u64")))
        .map(|seed| prospecting_method_for_work_pressure(&registries, seed))
        .collect::<BTreeSet<_>>();
    assert!(
        sampled_prospecting_methods.is_subset(&authored_prospecting_methods),
        "bounded organic survival work must select only authored prospecting methods"
    );
    if authored_prospecting_methods.len() > 1 {
        assert!(
            sampled_prospecting_methods.len() > 1,
            "bounded organic survival work collapsed multiple authored prospecting methods to one repeated action; the full authored set remains visible in the registry-derived content catalog"
        );
    }
}
