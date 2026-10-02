//! Cheap contracts keeping primitive-liberation organic worlds materially varied.

use std::collections::BTreeSet;

use super::*;

#[test]
fn organic_liberation_generation_varies_live_feed_campaign_and_material_opportunity() {
    let registries = deep_hearth::content::build_registries();
    let worlds = (1_u64..=64)
        .map(|seed| {
            let case = FocusedProbeCase::new(seed, None, FocusedProbeRole::OrganicVariation);
            (
                primitive_liberation_world_parameters(&registries, case),
                acquisition::staging_material_available(seed),
            )
        })
        .collect::<Vec<_>>();

    assert!(
        worlds
            .iter()
            .map(|(world, _)| world.batch_mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one representable ore batch"
    );
    assert!(
        worlds
            .iter()
            .map(|(world, _)| world.copper_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one copper assay"
    );
    assert!(
        worlds
            .iter()
            .map(|(world, _)| world.clay_share_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one gangue composition"
    );
    assert!(
        worlds
            .iter()
            .map(|(world, _)| world.planned_batches)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one disclosed campaign horizon"
    );
    assert!(
        worlds.iter().any(|(_, staging)| *staging) && worlds.iter().any(|(_, staging)| !*staging),
        "organic liberation generation must vary whether local raw material can fund optional fabrication staging"
    );
}
