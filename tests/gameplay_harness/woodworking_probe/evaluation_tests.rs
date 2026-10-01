//! Replay regression for pre-action woodworking choice versus realized lifecycle cost.

use std::collections::BTreeSet;

use super::*;

#[test]
fn organic_woodworking_generation_varies_demand_and_disclosed_copper_pressure() {
    let registries = deep_hearth::content::build_registries();
    let demands = (1_u64..=64)
        .map(|seed| plan_woodworking_demand(&registries, seed))
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
        .map(|seed| build_woodworking_world(&registries, seed))
        .collect::<Vec<_>>();
    assert!(
        worlds
            .iter()
            .map(|world| world.copper_available.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1,
        "organic woodworking generation collapsed to one copper opportunity"
    );
    assert!(
        worlds.iter().any(|world| world.saw_fundable)
            && worlds.iter().any(|world| !world.saw_fundable),
        "organic woodworking generation must exercise both fundable and blocked saw opportunities"
    );
}

#[test]
fn woodworking_keeps_pre_action_setup_budget_choice_when_realized_saw_is_cheaper() {
    let registries = deep_hearth::content::build_registries();
    // Fixed replay witness: a finite intermediate order with sufficient copper where the
    // conservative actor declines setup even though the completed saw route proves cheaper.
    let (choice, selected_attention, saw_attention) = evaluate_woodworking_probe(
        &registries,
        FocusedProbeCase::new(86, Some(2), FocusedProbeRole::OrganicVariation),
    );
    assert_eq!(choice, "stone-adze");
    assert!(
        saw_attention.is_some_and(|ticks| ticks < selected_attention),
        "replay witness must keep the pre-action adze choice even when the realized saw route is cheaper"
    );
}
