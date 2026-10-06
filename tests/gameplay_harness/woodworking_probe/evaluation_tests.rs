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
            > 1,
        "organic woodworking generation collapsed to one copper opportunity"
    );
    assert!(
        worlds
            .iter()
            .any(|world| visible_native_copper(world) >= world.blade_input)
            && worlds
                .iter()
                .any(|world| visible_native_copper(world) < world.blade_input),
        "organic woodworking generation must exercise both fundable and blocked saw opportunities"
    );
}

#[test]
fn four_world_woodworking_sample_spans_workload_and_copper_pressure() {
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

    let worlds = (0_u64..4)
        .map(|offset| build_woodworking_world(&registries, base + offset, true))
        .collect::<Vec<_>>();
    let copper_pressure = worlds
        .iter()
        .map(|world| {
            let available = visible_native_copper(world);
            let reserve_safe = world
                .blade_input
                .checked_add(world.protected_copper_reserve)
                .unwrap_or_else(|| panic!("bounded woodworking reserve threshold overflowed"));
            if available < world.blade_input {
                "blocked"
            } else if available < reserve_safe {
                "fundable-reserve-at-risk"
            } else {
                "reserve-safe"
            }
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        copper_pressure,
        BTreeSet::from(["blocked", "fundable-reserve-at-risk", "reserve-safe"]),
        "four-world woodworking exploration must span the live copper decision pressures without pinning them to seed positions"
    );
}
