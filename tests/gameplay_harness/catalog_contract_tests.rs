//! Dynamic gameplay-catalog discovery and bounded scenario-generation diversity contracts.

use std::collections::BTreeSet;

use deep_hearth::content::build_registries;
use deep_hearth::production::validate_start_process;

use super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::foundry_probe::{
    CastBatchLimit, MeltBatchLimit, probe_setup as foundry_probe_setup,
    resolve_largest_feasible_cast, resolve_largest_feasible_melt,
};
use super::foundry_setup::setup_foundry_probe;
use super::ore_probe::{
    OreProbeOutcome, OreStopReason, evaluate_ore_preparation_capability_probe, probe_parameters,
};
use super::production_support::finish_uninterrupted_production_job;
use super::progression_probe::varied_four_way_order;
use super::report::{ProcessResolverKind, process_catalog_entries};
use super::survival_probe::provisioning_world;

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
                    world.foods.len(),
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
            .map(|world| world.foods.len())
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

    let mut ore_completed = false;
    let mut ore_energy_limited = false;
    for seed in 1_u64..=16 {
        match evaluate_ore_preparation_capability_probe(
            &registries,
            FocusedProbeCase::new(seed, FocusedProbeRole::OrganicVariation),
        ) {
            OreProbeOutcome::Completed => ore_completed = true,
            OreProbeOutcome::Stopped {
                stage: _,
                reason: OreStopReason::FiniteEnergy,
            } => ore_energy_limited = true,
            OreProbeOutcome::Stopped { .. } => {}
        }
        if ore_completed && ore_energy_limited {
            break;
        }
    }
    assert!(
        ore_completed && ore_energy_limited,
        "gameplay ore variation must span both completed and finite-energy-limited runtime paths"
    );

    let mut foundry_full_offer = false;
    let mut foundry_constrained_offer = false;
    for seed in 1_u64..=24 {
        let setup = foundry_probe_setup(&registries, seed);
        let offered = setup.mass;
        let (state, ids) = setup_foundry_probe(&registries, seed, setup);
        let (_, _, limit) = resolve_largest_feasible_melt(&registries, &state, ids, offered)
            .unwrap_or_else(|| panic!("foundry variation seed {seed} admitted no melt batch"));
        match limit {
            MeltBatchLimit::OfferedBatch => foundry_full_offer = true,
            MeltBatchLimit::EquipmentCapacity
            | MeltBatchLimit::FiniteEnergy
            | MeltBatchLimit::ConditionLifetime => foundry_constrained_offer = true,
        }
        if foundry_full_offer && foundry_constrained_offer {
            break;
        }
    }
    assert!(
        foundry_full_offer && foundry_constrained_offer,
        "gameplay foundry variation must span both full-offer and constrained runtime decisions"
    );

    let mut foundry_full_cast = false;
    let mut foundry_thermal_limited_cast = false;
    for seed in 1_u64..=24 {
        let setup = foundry_probe_setup(&registries, seed);
        let offered = setup.mass;
        let (mut state, ids) = setup_foundry_probe(&registries, seed, setup);
        let Some((melt, melted_mass, _)) =
            resolve_largest_feasible_melt(&registries, &state, ids, offered)
        else {
            continue;
        };
        let melt_duration = melt.process_resolution().duration();
        let melt_job = validate_start_process(
            &registries,
            &state,
            melt.process_resolution(),
            ids.pure_copper_source,
            ids.molten_vessel,
        )
        .unwrap_or_else(|error| {
            panic!("foundry variation seed {seed} melt admission failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("foundry variation seed {seed} melt commit failed: {error}")
        });
        finish_uninterrupted_production_job(
            &registries,
            &mut state,
            melt_job,
            melt_duration,
            "foundry variation melt",
        );
        let Some((_, _, cast_limit)) =
            resolve_largest_feasible_cast(&registries, &state, ids, melted_mass)
        else {
            continue;
        };
        match cast_limit {
            CastBatchLimit::OfferedBatch => foundry_full_cast = true,
            CastBatchLimit::ThermalSinkCapacity => foundry_thermal_limited_cast = true,
            CastBatchLimit::EquipmentCapacity | CastBatchLimit::ConditionLifetime => {}
        }
        if foundry_full_cast && foundry_thermal_limited_cast {
            break;
        }
    }
    assert!(
        foundry_full_cast && foundry_thermal_limited_cast,
        "gameplay foundry variation must span both full casting and thermal-sink-limited casting"
    );
}
