//! Replay regression for pre-action woodworking choice versus realized lifecycle cost.

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
        demands[3].saw_batches > demands[2].saw_batches,
        "the high-project stratum must remain materially larger than the lower project stratum"
    );

    let worlds = (0_u64..4)
        .map(|offset| build_woodworking_world(&registries, base + offset, true))
        .collect::<Vec<_>>();
    let copper = worlds
        .iter()
        .map(visible_native_copper)
        .collect::<Vec<_>>();
    assert!(copper[0] < worlds[0].blade_input);
    assert_eq!(copper[1], worlds[1].blade_input);
    assert_eq!(
        copper[2],
        worlds[2]
            .blade_input
            .checked_add(worlds[2].protected_copper_reserve)
            .unwrap_or_else(|| panic!("bounded woodworking reserve opportunity overflowed"))
    );
    assert!(copper[3] > copper[2]);
}

#[test]
fn woodworking_keeps_pre_action_non_saw_choice_when_realized_saw_is_cheaper() {
    let registries = deep_hearth::content::build_registries();
    // Fixed replay witness: a finite intermediate order with sufficient copper where the
    // conservative actor declines setup even though the completed saw route proves cheaper.
    let (choice, selected_attention, saw_attention) = evaluate_woodworking_probe(
        &registries,
        FocusedProbeCase::new(86, Some(2), FocusedProbeRole::OrganicVariation),
    );
    assert_ne!(choice, "frame-saw");
    assert!(
        saw_attention.is_some_and(|ticks| ticks < selected_attention),
        "replay witness must keep its pre-action non-saw choice even when the realized saw route is cheaper"
    );
}
