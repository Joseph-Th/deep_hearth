//! Contract tests for physical-world and actor-policy seed separation.

use std::collections::BTreeSet;

use super::scenario::*;
use deep_hearth::content::build_registries;

#[test]
fn behavior_seed_never_changes_physical_scenario_inputs() {
    let registries = build_registries();
    let first = ScenarioVariation::from_seeds(&registries, 0x1234, 0x1111, None);
    let second = ScenarioVariation::from_seeds(&registries, 0x1234, 0x2222, None);

    assert_eq!(first.world_seed, second.world_seed);
    assert_eq!(first.survival, second.survival);
    assert_eq!(first.ore, second.ore);
    assert_eq!(first.crusher, second.crusher);
    assert_eq!(first.structure, second.structure);
    assert_eq!(first.delivery, second.delivery);
}

#[test]
fn world_seed_never_changes_player_policy() {
    let registries = build_registries();
    let first = ScenarioVariation::from_seeds(&registries, 0x1234, 0xCAFE, None);
    let second = ScenarioVariation::from_seeds(&registries, 0x5678, 0xCAFE, None);

    assert_eq!(first.behavior_seed, second.behavior_seed);
    assert_eq!(first.policy, second.policy);
}

#[test]
fn generated_world_seeds_change_real_physical_pressures() {
    let registries = build_registries();
    let variations = (1_u64..=8)
        .map(|seed| ScenarioVariation::from_seeds(&registries, seed, 0xCAFE, None))
        .collect::<Vec<_>>();

    let ore_grades = variations
        .iter()
        .map(|variation| variation.ore.ore_copper_ppm)
        .collect::<BTreeSet<_>>();
    let gangue_clay_shares = variations
        .iter()
        .map(|variation| variation.ore.gangue_clay_share_ppm)
        .collect::<BTreeSet<_>>();
    let batch_masses = variations
        .iter()
        .map(|variation| variation.ore.nominal_batch_mass.milligrams())
        .collect::<BTreeSet<_>>();
    let crusher_conditions = variations
        .iter()
        .map(|variation| {
            variation
                .crusher
                .initial_crusher_condition
                .parts_per_million()
        })
        .collect::<BTreeSet<_>>();
    let support_shapes = variations
        .iter()
        .map(|variation| {
            (
                variation
                    .structure
                    .compact_support_area
                    .square_millimeters(),
                variation
                    .structure
                    .reinforced_support_area
                    .square_millimeters(),
            )
        })
        .collect::<BTreeSet<_>>();
    let resource_pressures = variations
        .iter()
        .map(|variation| {
            (
                variation.crusher.small_drive_batch_budget,
                variation.crusher.small_drive_partial_batch_ppm,
                variation.crusher.large_drive_batch_budget,
                variation.crusher.large_drive_partial_batch_ppm,
                variation.crusher.maintenance_replacement_units,
            )
        })
        .collect::<BTreeSet<_>>();
    let deliveries = variations
        .iter()
        .map(|variation| {
            (
                variation.delivery.mass.milligrams(),
                variation.delivery.destination_is_compact,
            )
        })
        .collect::<BTreeSet<_>>();

    for (label, count) in [
        ("ore grade", ore_grades.len()),
        ("gangue composition", gangue_clay_shares.len()),
        ("nominal batch mass", batch_masses.len()),
        ("crusher condition", crusher_conditions.len()),
        ("support geometry", support_shapes.len()),
        ("finite resources", resource_pressures.len()),
        ("controlled delivery", deliveries.len()),
    ] {
        assert!(
            count > 1,
            "gameplay world variation collapsed: sampled seeds no longer vary {label}"
        );
    }
}
