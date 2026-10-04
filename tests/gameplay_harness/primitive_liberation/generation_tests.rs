//! Cheap contracts keeping primitive-liberation organic worlds materially varied.

use std::collections::BTreeSet;

use super::*;

#[test]
fn organic_liberation_generation_varies_live_feed_and_campaign() {
    let registries = deep_hearth::content::build_registries();
    let worlds = (1_u64..=64)
        .map(|seed| {
            let case = FocusedProbeCase::new(seed, None, FocusedProbeRole::OrganicVariation);
            (
                case,
                primitive_liberation_world_parameters(&registries, case),
            )
        })
        .collect::<Vec<_>>();

    assert!(
        worlds.iter().all(|(case, world)| {
            !world.batch_mass.is_zero()
                && world.batch_mass <= primitive_liberation_batch_ceiling(&registries, *case)
        }),
        "generated liberation batches must remain inside the current complete processing route at the inherited live condition"
    );

    assert!(
        worlds
            .iter()
            .map(|(_, world)| world.batch_mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one representable ore batch"
    );
    assert!(
        worlds
            .iter()
            .map(|(_, world)| world.copper_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one copper assay"
    );
    assert!(
        worlds
            .iter()
            .map(|(_, world)| world.clay_share_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one gangue composition"
    );
    assert!(
        worlds
            .iter()
            .map(|(_, world)| world.planned_batches)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic liberation generation collapsed to one disclosed campaign horizon"
    );
}
