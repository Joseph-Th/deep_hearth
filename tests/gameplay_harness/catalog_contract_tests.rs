//! Aggregate gameplay-evidence coverage for authored runtime process catalogs.

use std::collections::BTreeSet;

use deep_hearth::content::{
    PROCESS_CAST_PURE_COPPER, PROCESS_CONCENTRATE_COPPER, PROCESS_CRUSH_ORE,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_GRIND_CRUSHED_ORE, PROCESS_MELT_PURE_COPPER,
    PROCESS_SCREEN_CRUSHED_ORE, PROCESS_SEPARATE_NATIVE_COPPER, build_registries,
};

use super::foundry_probe::probe_setup as foundry_probe_setup;
use super::ore_probe::probe_parameters;
use super::progression_probe::varied_four_way_order;
use super::survival_probe::provisioning_world;

#[test]
fn gameplay_machine_process_catalog_has_evidence() {
    let registries = build_registries();
    let manual_processes = registries
        .crafting()
        .definitions()
        .map(|definition| definition.process())
        .collect::<BTreeSet<_>>();
    let actual_machine_processes = registries
        .production()
        .definitions()
        .map(|definition| definition.id())
        .filter(|process| !manual_processes.contains(process))
        .collect::<BTreeSet<_>>();
    let exercised_machine_processes = BTreeSet::from([
        PROCESS_CRUSH_ORE,
        PROCESS_GRIND_CRUSHED_ORE,
        PROCESS_SCREEN_CRUSHED_ORE,
        PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
        PROCESS_CONCENTRATE_COPPER,
        PROCESS_MELT_PURE_COPPER,
        PROCESS_CAST_PURE_COPPER,
        PROCESS_SEPARATE_NATIVE_COPPER,
    ]);

    assert_eq!(
        actual_machine_processes, exercised_machine_processes,
        "gameplay evidence coverage is stale: classify every authored non-manual production process in progression, workshop, ore, or foundry evaluation"
    );

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
                    world.provisioning_wait_ticks,
                    world.age_ticks,
                    world.preservation_multiplier_ppm,
                    world.witness_index,
                    world.target_absorbed_energy,
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
}
