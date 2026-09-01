//! Cheap ore-probe generator contracts kept beside the focused ore executable.

use std::collections::BTreeSet;

use deep_hearth::content::build_registries;

use super::ore_probe::{energy_fundable_batch_mass, probe_parameters};

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

    let planned = setups
        .iter()
        .map(|setup| {
            energy_fundable_batch_mass(
                &registries,
                setup.batch_mass,
                setup.representable_unit_mg,
                setup.drive_energy,
            )
        })
        .collect::<Vec<_>>();
    assert!(planned.iter().all(|mass| !mass.is_zero()));
    assert!(
        planned
            .iter()
            .zip(&setups)
            .any(|(planned, setup)| *planned < setup.batch_mass),
        "bounded ore generation must include a finite-work order that requires adaptive batching"
    );
    assert!(
        planned
            .iter()
            .zip(&setups)
            .any(|(planned, setup)| *planned == setup.batch_mass),
        "bounded ore generation must include a fully funded order"
    );
}
