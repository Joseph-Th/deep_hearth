//! Industrial-workshop actor-agency search and seed-channel contracts.

use super::*;

#[test]
fn gameplay_agency_bounded_search_preserves_unfiltered_replay() {
    let registries = build_registries();
    let root = 0x16F6_C93F_A53A_1C98;
    let worlds = exploratory_agency_worlds(root);
    let maintained_count = maintained_agency_worlds().len();
    let unfiltered = organic_agency_worlds(root, ORGANIC_UNFILTERED_COUNT);
    for (actual, original) in worlds
        .iter()
        .skip(maintained_count)
        .take(ORGANIC_UNFILTERED_COUNT)
        .zip(&unfiltered)
    {
        assert_eq!(actual.world_seed, original.world_seed);
        assert_eq!(actual.focus, AgencyFocus::OrganicVariation);
        assert_eq!(actual.anchor, None);
    }
    assert_eq!(
        worlds.len(),
        maintained_count + ORGANIC_UNFILTERED_COUNT + ORGANIC_SEARCH_LIMIT
    );
    let selected = run_agency_probe(&registries, &worlds);
    assert_eq!(selected.len(), ORGANIC_QUALIFIED_TARGET);
    assert!(selected.iter().all(|seed| {
        worlds
            .iter()
            .skip(maintained_count + ORGANIC_UNFILTERED_COUNT)
            .any(|world| world.world_seed == *seed && world.anchor.is_none())
    }));
    assert_eq!(selected, run_agency_probe(&registries, &worlds));
    let first_qualified_index = worlds
        .iter()
        .position(|world| world.world_seed == selected[0])
        .unwrap_or_else(|| {
            unreachable!("qualified agency seed must come from the searched worlds")
        });
    let exhausted = run_agency_probe(&registries, &worlds[..first_qualified_index + 1]);
    assert_eq!(exhausted, selected[..1]);
    assert!(exhausted.len() < ORGANIC_QUALIFIED_TARGET);
}

#[test]
fn gameplay_agency_gate_is_maintained_only_unless_variation_is_requested() {
    let maintained = gate_agency_worlds(None);
    let first = gate_agency_worlds(Some(0x1111));
    let second = gate_agency_worlds(Some(0x2222));
    let maintained_count = maintained_agency_worlds().len();

    assert_eq!(maintained.len(), maintained_count);
    assert!(maintained.iter().all(|world| world.anchor.is_some()));
    assert_eq!(first.len(), maintained_count + 1);
    for variation in [&first, &second] {
        assert!(
            variation[..maintained_count]
                .iter()
                .zip(&maintained)
                .all(
                    |(actual, expected)| actual.world_seed == expected.world_seed
                        && actual.focus == expected.focus
                        && actual.anchor == expected.anchor
                )
        );
    }
    assert_eq!(first[maintained_count].focus, AgencyFocus::OrganicVariation);
    assert_eq!(first[maintained_count].anchor, None);
    assert_ne!(
        first[maintained_count].world_seed,
        second[maintained_count].world_seed
    );
}

#[test]
fn gameplay_agency_counterfactual_behavior_channel_is_world_independent() {
    let registries = build_registries();
    let behavior_seed = counterfactual_behavior_seed();
    let first = ScenarioVariation::from_seeds(&registries, 0x1111, behavior_seed, None);
    let second = ScenarioVariation::from_seeds(&registries, 0x2222, behavior_seed, None);

    assert_ne!(first.world_seed, second.world_seed);
    assert_eq!(first.behavior_seed, second.behavior_seed);
    assert_eq!(first.policy, second.policy);
}
