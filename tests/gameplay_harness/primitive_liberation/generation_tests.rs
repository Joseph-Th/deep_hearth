//! Cheap contracts keeping primitive-liberation organic worlds materially varied.

use std::collections::BTreeSet;

use super::*;

#[test]
fn organic_liberation_generation_varies_live_feed_and_campaign() {
    let registries = deep_hearth::content::build_registries();
    let worlds = (1_u64..=64)
        .map(|seed| {
            let case = FocusedProbeCase::new(seed, None, FocusedProbeRole::OrganicVariation);
            primitive_liberation_world_parameters(&registries, case)
        })
        .collect::<Vec<_>>();

    assert!(
        worlds
            .iter()
            .map(|world| world.batch_mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one representable ore batch"
    );
    assert!(
        worlds
            .iter()
            .map(|world| world.copper_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one copper assay"
    );
    assert!(
        worlds
            .iter()
            .map(|world| world.clay_share_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one gangue composition"
    );
    assert!(
        worlds
            .iter()
            .map(|world| world.planned_batches)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one disclosed campaign horizon"
    );
}
