//! Replay regression for pre-action woodworking choice versus realized lifecycle cost.

use super::*;

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
