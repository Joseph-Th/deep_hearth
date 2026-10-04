//! Exploratory woodworking narration kept out of focused test binaries.

use super::*;

fn signed_physical_duration(registries: &Registries, ticks: i128) -> String {
    let magnitude = u64::try_from(ticks.unsigned_abs())
        .unwrap_or_else(|_| panic!("woodworking signed duration exceeds u64"));
    format!(
        "{}{}",
        if ticks < 0 { "-" } else { "+" },
        format_physical_duration(registries, magnitude)
    )
}

fn woodworking_counterfactual_tradeoff(
    registries: &Registries,
    metrics: WoodworkingLifecycleMetrics,
    adze_timber: Mass,
) -> String {
    match (metrics.saw_total_attention, metrics.saw_total_timber) {
        (Some(saw_attention), Some(saw_timber)) => {
            let attention_delta =
                i128::from(saw_attention) - i128::from(metrics.adze_total_attention);
            let timber_delta =
                i128::from(saw_timber.milligrams()) - i128::from(adze_timber.milligrams());
            format!(
                "attention:{attention_delta:+}t/{} timber:{timber_delta:+}mg",
                signed_physical_duration(registries, attention_delta)
            )
        }
        _ => "unavailable:copper".to_owned(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn report_woodworking_result(
    registries: &Registries,
    case: FocusedProbeCase,
    behavior_seed: u64,
    world: &WoodworkingWorld,
    demand: WoodworkingDemandPlan,
    decision: WoodworkingDecisionPlan,
    metrics: WoodworkingLifecycleMetrics,
    selected: &SelectedWoodworkingRoute,
) {
    let bare_time = format_physical_duration(registries, decision.bare_attention);
    let adze_route_time = format_physical_duration(registries, metrics.adze_total_attention);
    let saw_route_attention = metrics.saw_total_attention.unwrap_or(0);
    let saw_route_time = format_physical_duration(registries, saw_route_attention);
    let selected_setup_time = format_physical_duration(registries, selected.setup_ticks);
    let selected_active_time = format_physical_duration(registries, selected.route.active_ticks());
    let selected_total_time = format_physical_duration(registries, selected.attention_ticks);
    let attention_delta_time = signed_physical_duration(registries, selected.attention_delta);
    let saw_counterfactual_tradeoff = woodworking_counterfactual_tradeoff(
        registries,
        metrics,
        selected.adze_route.project_timber,
    );
    let selected_condition = selected
        .route
        .final_condition_ppm
        .map_or_else(|| "no-tool".to_owned(), |ppm| format!("{ppm}ppm"));
    let adze_immediate_total = selected
        .adze_setup
        .checked_add(selected.adze_immediate_ticks)
        .unwrap_or_else(|| panic!("woodworking immediate adze attention overflowed"));
    let adze_immediate_total_time = format_physical_duration(registries, adze_immediate_total);
    let bare_immediate_time = format_physical_duration(registries, selected.bare_immediate_ticks);
    let adze_immediate_time = format_physical_duration(registries, selected.adze_immediate_ticks);
    let saw_route_timber = metrics.saw_total_timber.map_or(0, Mass::milligrams);
    let reinforced_projected = decision
        .reinforced_projected_attention
        .map_or_else(|| "n/a".to_owned(), |ticks| format!("{ticks}t"));
    let reinforced_actual = metrics
        .reinforced_adze_total_attention
        .map_or_else(|| "n/a".to_owned(), |ticks| format!("{ticks}t"));

    reviewln!(
        "WOODWORKING DECISION seed=0x{:016X} basis=pre-action-inventory+authored-routes budget=bare-work-at-least-twice-hand-build bare-work={}t adze-build-budget={}t reinforced-build-budget={} timber=nominal-no-future-service saw-reserve-safe={} reinforced-reserve-safe={} reinforced={} saw={} bare={} future-outcomes=diagnostic-only",
        case.seed(),
        decision.bare_attention,
        decision.adze_budget,
        decision
            .reinforced_adze_budget
            .map_or_else(|| "n/a".to_owned(), |ticks| format!("{ticks}t")),
        decision.reserve_safe_now,
        decision.reinforced_reserve_safe_now,
        decision.invest_in_reinforced_adze,
        decision.invest_in_saw,
        decision.use_bare_hands,
    );
    reviewln!(
        "WOODWORKING BASELINE seed=0x{:016X} bare={}t/{bare_time} adze={}t/{adze_route_time} selected={}t/{selected_total_time} choice={} basis=full-lifecycle-including-tool-construction",
        case.seed(),
        decision.bare_attention,
        metrics.adze_total_attention,
        selected.attention_ticks,
        selected.choice,
    );
    reviewln!(
        "WOODWORKING EXPERIENCE seed=0x{:016X} behavior=0x{behavior_seed:016X} sample={} demand-horizon={} demand=[immediate:{}mg queued:{}mg pipeline:{}mg boards] preference={} policy-basis=pre-action-budget-not-lifecycle-oracle copper-counterfactual=[available:{}mg blade:{}mg reinforcement:{}mg protected-reserve:{}mg reinforced-spend:{}mg saw-spend:{}mg after-saw:{}mg] routes=[adze:{}logs timber:{}mg attention:{}t/{adze_route_time} production:{}t maintenance:{}t/{}services final-condition:{}ppm; reinforced-adze=[projected:{reinforced_projected} actual:{reinforced_actual}]; saw-assisted:min-saw-logs:{} fundable:{} setup-timber:{}mg actual=[saw:{} adze-fallback:{} fallback-copper:{} saw-services:{} adze-services:{}] timber:{}mg attention:{}t/{saw_route_time} attention-payback:{} timber-saving:{} timber-neutral:{} counterfactual-vs-adze=[{saw_counterfactual_tradeoff}]] choice={} reason={} selected=[setup:{}t/{selected_setup_time} active:{}t/{selected_active_time} total:{}t/{selected_total_time} timber:{}mg project-timber:{}mg boards:{}mg surplus:{}mg chips:{}mg condition:{selected_condition}] selected-vs-adze=[attention:{:+}t/{attention_delta_time} timber:{:+}mg] immediate-baseline=[bare:{}t/{bare_immediate_time} adze:{adze_immediate_total}t/{adze_immediate_total_time} adze-work-only:{}t/{adze_immediate_time}] matter=conserved",
        case.seed(),
        case.role().label(),
        demand.horizon,
        demand.immediate_boards.milligrams(),
        demand
            .pipeline_boards
            .checked_sub(demand.immediate_boards)
            .unwrap_or_else(|| unreachable!("pipeline demand includes immediate demand"))
            .milligrams(),
        demand.pipeline_boards.milligrams(),
        decision.preference.label(),
        visible_native_copper(world).milligrams(),
        world.blade_input.milligrams(),
        world.reinforcement_input.milligrams(),
        world.protected_copper_reserve.milligrams(),
        metrics.reinforced_adze_copper_consumed.milligrams(),
        metrics.saw_copper_consumed.milligrams(),
        metrics.copper_after_saw.milligrams(),
        demand.adze_batches,
        selected.adze_route.project_timber.milligrams(),
        metrics.adze_total_attention,
        selected.adze_route.production_ticks,
        selected.adze_route.maintenance_ticks,
        selected.adze_route.maintenance_services,
        selected.adze_route.final_condition_ppm.unwrap_or(0),
        demand.saw_batches,
        decision.saw_budget.is_some(),
        metrics.saw_setup_timber,
        metrics.saw_actual_batches,
        metrics.saw_fallback_adze_batches,
        metrics.saw_fallback_due_to_copper,
        metrics.saw_service_count,
        metrics.saw_fallback_adze_service_count,
        saw_route_timber,
        saw_route_attention,
        metrics.saw_attention_payback,
        metrics.saw_timber_saving,
        metrics.saw_timber_neutral,
        selected.choice,
        decision.reason.label(),
        selected.setup_ticks,
        selected.route.active_ticks(),
        selected.attention_ticks,
        selected.total_timber.milligrams(),
        selected.route.project_timber.milligrams(),
        selected.boards.milligrams(),
        selected.board_surplus.milligrams(),
        selected.chips.milligrams(),
        selected.attention_delta,
        selected.timber_delta,
        selected.bare_immediate_ticks,
        selected.adze_immediate_ticks,
    );
    reviewln!(
        "WOODWORKING FEEDBACK seed=0x{:016X} basis=executed-lifecycle-versus-pre-action-policy-model attention=[setup-budget-met:{} actual-payback:{}] timber=[nominal:{} actual:{}] selected={} choice-revised-after-outcome=false",
        case.seed(),
        decision.setup_attention_budget_met,
        metrics.saw_attention_payback,
        decision.nominal_timber_balance.label(),
        metrics.actual_timber_balance.label(),
        selected.choice,
    );
}
