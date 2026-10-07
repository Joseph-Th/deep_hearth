//! Content-relative variation contracts for ordinary woodworking evaluation.

use std::collections::BTreeSet;

use super::*;

#[test]
fn organic_woodworking_generation_varies_demand_and_disclosed_copper_pressure() {
    let registries = deep_hearth::content::build_registries();
    let demands = (1_u64..=64)
        .map(|seed| plan_woodworking_demand(&registries, seed, true))
        .collect::<Vec<_>>();
    assert!(
        demands
            .iter()
            .map(|demand| demand.horizon)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic woodworking generation collapsed to one planning horizon"
    );
    assert!(
        demands
            .iter()
            .map(|demand| (demand.immediate_scale, demand.saw_batches))
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic woodworking generation collapsed to one disclosed workload"
    );

    let worlds = (1_u64..=32)
        .map(|seed| build_woodworking_world(&registries, seed, true))
        .collect::<Vec<_>>();
    assert!(
        worlds
            .iter()
            .map(|world| visible_native_copper(world).milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 16,
        "organic woodworking generation collapsed to a narrow inherited-copper opportunity set"
    );
}

#[test]
fn four_world_woodworking_sample_spans_workload_without_encoding_copper_decisions() {
    let registries = deep_hearth::content::build_registries();
    let base = 0x51A2_7F00_u64;
    let demands = (0_u64..4)
        .map(|offset| plan_woodworking_demand(&registries, base + offset, true))
        .collect::<Vec<_>>();
    assert_eq!(
        demands
            .iter()
            .map(|demand| demand.horizon)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["immediate-only", "project", "short-queue"]),
        "four-world woodworking exploration must span the current planning horizons"
    );
    assert!(
        demands
            .iter()
            .filter(|demand| demand.horizon == "project")
            .map(|demand| demand.saw_batches)
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "bounded woodworking exploration must retain materially different project workloads"
    );

    let copper = (0_u64..4)
        .map(|offset| organic_woodworking_native_copper(&registries, base + offset).milligrams())
        .collect::<BTreeSet<_>>();
    assert!(
        copper.len() > 1,
        "bounded woodworking exploration must retain physical copper variation without pinning route thresholds"
    );
}

#[test]
fn organic_woodworking_copper_varies_within_each_demand_stratum() {
    let registries = deep_hearth::content::build_registries();
    for stratum in 0_u64..4 {
        let values = (0_u64..16)
            .map(|entropy| {
                let seed = stratum | (entropy << 2);
                organic_woodworking_native_copper(&registries, seed).milligrams()
            })
            .collect::<BTreeSet<_>>();
        assert!(
            values.len() > 8,
            "woodworking copper opportunity became coupled to demand stratum {stratum}"
        );
    }
}
