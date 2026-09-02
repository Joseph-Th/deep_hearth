//! Cheap ore-probe generator contracts kept beside the focused ore executable.

use std::collections::BTreeSet;

use deep_hearth::content::build_registries;

use super::ore_probe::generation::probe_parameters;

#[test]
fn ore_probe_generation_varies_feed_and_operating_state() {
    let registries = build_registries();
    let setups = (1_u64..=8)
        .map(|seed| probe_parameters(&registries, seed))
        .collect::<Vec<_>>();

    let feeds = setups
        .iter()
        .map(|setup| {
            (
                setup.batch_mass.milligrams(),
                setup.copper_ppm,
                setup.clay_share_ppm,
            )
        })
        .collect::<BTreeSet<_>>();
    assert!(
        feeds.len() > 1,
        "ore probe generation collapsed to one physical feed"
    );

    let operating_states = setups
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
        .collect::<BTreeSet<_>>();
    assert!(
        operating_states.len() > 1,
        "ore probe generation collapsed to one operating state"
    );

    assert!(setups.iter().all(|setup| !setup.drive_energy.is_zero()));
}
