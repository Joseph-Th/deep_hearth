//! Dynamic gameplay-catalog discovery and bounded scenario-generation diversity contracts.

use std::collections::BTreeSet;

use deep_hearth::content::build_registries;

use super::foundry_probe::probe_setup as foundry_probe_setup;
use super::ore_probe::probe_parameters;
use super::progression_probe::varied_four_way_order;
use super::report::{ProcessResolverKind, process_catalog_entries};
use super::survival_probe::{prospecting_method_for_work_pressure, provisioning_world};

#[test]
fn gameplay_catalog_is_discovered_from_runtime_owners() {
    let registries = build_registries();
    assert_eq!(
        process_catalog_entries(&registries).len(),
        registries.production().definitions().count(),
        "cold-agent catalog discovery must classify every authored process"
    );
    for entry in process_catalog_entries(&registries) {
        if entry.resolver == ProcessResolverKind::ManualCraft {
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

    let authored_food_options = registries
        .survival()
        .foods()
        .map(|food| food.commodity())
        .collect::<BTreeSet<_>>();
    let sampled_food_options = (1_u64..=32)
        .flat_map(|seed| provisioning_world(&registries, seed).foods)
        .map(|food| food.commodity())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        sampled_food_options, authored_food_options,
        "bounded organic survival worlds must expose every authored food option to the cold-agent harness"
    );

    let authored_prospecting_methods = registries
        .labor()
        .prospecting_definitions()
        .map(|definition| definition.id())
        .collect::<BTreeSet<_>>();
    let sampled_prospecting_methods = (1_u64..=32)
        .map(|seed| prospecting_method_for_work_pressure(&registries, seed))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        sampled_prospecting_methods, authored_prospecting_methods,
        "bounded organic survival work must exercise every authored prospecting method"
    );
}
