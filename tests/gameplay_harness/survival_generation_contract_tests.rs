//! Cheap contracts for survival world generation and provisioning-policy variation.

use std::collections::BTreeSet;

use deep_hearth::content::build_registries;

use super::survival_preservation_catalog::preservation_storage_candidates;
use super::survival_provisioning_policy::{
    DietProvisioningPolicy, diet_provisioning_policy_for_behavior_seed,
};
use super::survival_world_generation::{
    SurvivalStartProfile, minimum_visible_preservation_age_ticks, provisioning_world,
};

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
        .collect::<BTreeSet<_>>();

    assert!(
        waits.len() > 1,
        "rested survival provisioning became scripted"
    );
    assert!(waits.iter().all(|wait| (lower..upper).contains(wait)));
}

#[test]
fn four_world_survival_sample_spans_distinct_preservation_capacity_pressure() {
    let registries = build_registries();
    let candidates = preservation_storage_candidates(&registries);
    let base = 0xBCE8_0742_3D33_E090_u64;
    let worlds = (0_u64..4)
        .map(|offset| provisioning_world(&registries, base + offset))
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
    let feasible_counts = worlds
        .iter()
        .map(|world| {
            candidates
                .iter()
                .filter(|candidate| candidate.capacity >= world.preserved_reserve_mass)
                .count()
        })
        .collect::<BTreeSet<_>>();
    assert!(
        feasible_counts.iter().any(|count| *count > 1),
        "four-world survival exploration must include a real preservation market"
    );
    assert!(
        feasible_counts.len() > 1,
        "four-world survival exploration must vary preservation capacity pressure"
    );
}

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
fn survival_world_generation_stays_within_authored_options_and_varies_visible_pressure() {
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
    let authored_drinks = registries
        .survival()
        .drinks()
        .map(|drink| drink.fluid())
        .collect::<BTreeSet<_>>();
    let authored_preservation = preservation_storage_candidates(&registries)
        .into_iter()
        .map(|candidate| (candidate.definition, candidate.preservation_multiplier_ppm))
        .collect::<BTreeSet<_>>();
    let sample_count = authored_foods
        .len()
        .max(authored_preservation.len())
        .saturating_mul(64)
        .clamp(256, 1024);
    let worlds = (1_u64
        ..=u64::try_from(sample_count)
            .unwrap_or_else(|_| unreachable!("bounded survival sample count fits u64")))
        .map(|seed| provisioning_world(&registries, seed))
        .collect::<Vec<_>>();

    for world in &worlds {
        assert!(!world.foods.is_empty());
        assert_eq!(world.foods.len(), world.offered_masses.len());
        assert!(world.offered_masses.iter().all(|mass| !mass.is_zero()));
        assert!(world.witness_index < world.foods.len());
        assert!(authored_drinks.contains(&world.drink.fluid()));
    }

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
        assert!(
            sampled_foods.len() > 1,
            "food generation collapsed to one option"
        );
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
                .any(|count| *count < authored_categories.len()),
            "survival generation lost constrained-food worlds"
        );
    }

    let sampled_preservation = worlds
        .iter()
        .map(|world| {
            (
                world.inherited_preservation_definition,
                world.inherited_preservation_multiplier_ppm,
            )
        })
        .collect::<BTreeSet<_>>();
    assert!(sampled_preservation.is_subset(&authored_preservation));
    if authored_preservation.len() > 1 {
        assert!(
            sampled_preservation.len() > 1,
            "inherited preservation generation collapsed to one authored option"
        );
    }
}

#[test]
fn survival_behavior_seed_spans_both_diet_policies() {
    assert_eq!(
        (0_u64..4)
            .map(diet_provisioning_policy_for_behavior_seed)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            DietProvisioningPolicy::CompactCalories,
            DietProvisioningPolicy::BalancedRecovery,
        ])
    );
}
