"""Primitive-progression summary for ordinary gameplay evidence."""

from __future__ import annotations

import re

from .common import organic_only, physical_duration_span, sample_shape, scaled_span


def _span(values: list[int], unit: str = "t", fallback: str = "n/a") -> str:
    return f"{min(values)}..{max(values)}{unit}" if values else fallback


def _continuation_summary(lines: list[str]) -> str:
    progression_goals = [line for line in lines if line.startswith("PROGRESSION GOAL ")]
    immediate_completed = 0
    immediate_blocked = 0
    delayed_completed = 0
    delayed_blocked = 0
    stockpiling_delay_avoided: list[int] = []
    for line in progression_goals:
        immediate = re.search(r"\bimmediate=(\d+)t", line)
        delayed = re.search(r"\bdelayed=(\d+)t", line)
        if immediate is not None:
            immediate_completed += 1
        elif " immediate=blocked:" in line:
            immediate_blocked += 1
        if delayed is not None:
            delayed_completed += 1
        elif " delayed=blocked:" in line:
            delayed_blocked += 1
        if immediate is not None and delayed is not None:
            stockpiling_delay_avoided.append(
                int(delayed.group(1)) - int(immediate.group(1))
            )
    return (
        "reinvestment-timing=["
        f"selected-immediate:{immediate_completed}/{immediate_blocked} "
        f"stockpile-first-counterfactual:{delayed_completed}/{delayed_blocked} "
        f"delay-avoided:{_span(stockpiling_delay_avoided, fallback='not-comparable')}] "
        "reinvestment-timing-physical=["
        f"delay-avoided:{physical_duration_span(lines, stockpiling_delay_avoided, 'not-comparable')}]"
    )


def _stockpiling_evidence(progression: list[str]) -> tuple[str, str]:
    def values(pattern: str) -> list[int]:
        return [
            int(match.group(1))
            for line in progression
            if (match := re.search(pattern, line)) is not None
        ]

    hard_span = _span(values(r"hard-access-lead:(\d+)t"))
    stockpiling = (
        "stockpiling-counterfactual=["
        f"returned-attention:{_span(values(r'returned-attention:(\d+)t'))} "
        f"returned-share:{_span(values(r'\breturned:(\d+)ppm'), unit='ppm')} "
        f"maintenance-prep-overlap:{_span(values(r'\bmaintenance-prep-overlap:(\d+)t'))} "
        f"useful-overlap/setup:{_span(values(r'\boverlap/setup:(\d+)ppm'), unit='ppm')}]"
    )
    return hard_span, stockpiling


def _parallel_work_evidence(progression: list[str]) -> str:
    def values(pattern: str) -> list[int]:
        return [
            int(match.group(1))
            for line in progression
            if (match := re.search(pattern, line)) is not None
        ]

    return (
        "parallel-work=["
        f"feed-replenishment:{_span(values(r'\bfeed-attention:(\d+)t'))} "
        f"maintenance-prep:{_span(values(r'\bmaintenance-prep-overlap:(\d+)t'))} "
        f"useful-overlap:{_span(values(r'\bproductive-attention:(\d+)t'))} "
        f"remaining-autonomous:{_span(values(r'\breturned-attention:(\d+)t'))}]"
    )


def _investment_evidence(
    lines: list[str],
    progression: list[str],
) -> tuple[int, str, str]:
    manual_order_attention: list[int] = []
    mechanized_order_attention: list[int] = []
    order_attention_saved: list[int] = []
    preaction_manual_attention: list[int] = []
    preaction_machine_upper: list[int] = []
    frozen_processing_choices = 0
    for line in progression:
        economics = re.search(
            r"disclosed-order-economics=\[cycles:(\d+) manual-player-attention:(\d+)t mechanized-player-attention:(\d+)t saved:(\d+)t\]",
            line,
        )
        if economics is not None:
            manual_order_attention.append(int(economics.group(2)))
            mechanized_order_attention.append(int(economics.group(3)))
            order_attention_saved.append(int(economics.group(4)))
        investment = re.search(
            r"processing-investment=\[selected:mechanized preaction-manual:(\d+)t "
            r"conservative-machine-upper:(\d+)t .*?choice-frozen-before-action:true\]",
            line,
        )
        if investment is not None:
            preaction_manual_attention.append(int(investment.group(1)))
            preaction_machine_upper.append(int(investment.group(2)))
            frozen_processing_choices += 1
    return (
        frozen_processing_choices,
        (
            "preaction-processing-investment=["
            f"selected:mechanized manual:{_span(preaction_manual_attention)} "
            f"conservative-machine-upper:{_span(preaction_machine_upper)} "
            f"frozen:{frozen_processing_choices}/{len(progression)}]"
        ),
        (
            "disclosed-order-attention=["
            f"manual:{_span(manual_order_attention)} "
            f"mechanized:{_span(mechanized_order_attention)} "
            f"saved:{_span(order_attention_saved)}] "
            "disclosed-order-physical=["
            f"manual:{physical_duration_span(lines, manual_order_attention)} "
            f"mechanized:{physical_duration_span(lines, mechanized_order_attention)} "
            f"saved:{physical_duration_span(lines, order_attention_saved)}]"
        ),
    )


def _bridge_tradeoff_evidence(
    lines: list[str],
    progression: list[str],
    frozen_processing_choices: int,
) -> tuple[str, str, str]:
    manual_recovery: list[int] = []
    powered_recovery: list[int] = []
    manual_bridge_attention: list[int] = []
    powered_line_attention: list[int] = []
    manual_bridge_energy: list[int] = []
    manual_bridge_hydration: list[int] = []
    powered_line_energy: list[int] = []
    powered_line_hydration: list[int] = []
    for line in progression:
        recovery = re.search(
            r"bridge-tradeoff=\[manual-second:.*?recovery:(\d+)ppm.*?powered-line:.*?recovery:(\d+)ppm",
            line,
        )
        if recovery is not None:
            manual_recovery.append(int(recovery.group(1)))
            powered_recovery.append(int(recovery.group(2)))
        bridge = re.search(
            r"bridge-tradeoff=\[manual-second:(\d+)t\b.*?powered-line:(\d+)t\b",
            line,
        )
        if bridge is not None:
            manual_bridge_attention.append(int(bridge.group(1)))
            powered_line_attention.append(int(bridge.group(2)))
        body = re.search(
            r"bridge-tradeoff=\[manual-second:.*?body:(\d+)nJ/(\d+)uL;"
            r" powered-line:.*?body:(\d+)nJ/(\d+)uL\]",
            line,
        )
        if body is not None:
            manual_bridge_energy.append(int(body.group(1)))
            manual_bridge_hydration.append(int(body.group(2)))
            powered_line_energy.append(int(body.group(3)))
            powered_line_hydration.append(int(body.group(4)))
    return (
        (
            "processing-recovery=["
            f"manual:{_span(manual_recovery, unit='ppm')} "
            f"powered:{_span(powered_recovery, unit='ppm')}]"
        ),
        (
            "processing-crossover=["
            f"one-bridge-manual:{_span(manual_bridge_attention)} "
            f"line-setup:{_span(powered_line_attention)} "
            f"long-order-mechanized:{frozen_processing_choices}/{len(progression)}] "
            "processing-crossover-physical=["
            f"manual:{physical_duration_span(lines, manual_bridge_attention)} "
            f"line-setup:{physical_duration_span(lines, powered_line_attention)}]"
        ),
        (
            "bridge-body=["
            f"manual-energy:{scaled_span(manual_bridge_energy, 1_000_000_000_000, 'kJ')} "
            f"manual-hydration:{scaled_span(manual_bridge_hydration, 1_000, 'mL')} "
            f"line-energy:{scaled_span(powered_line_energy, 1_000_000_000_000, 'kJ')} "
            f"line-hydration:{scaled_span(powered_line_hydration, 1_000, 'mL')}]"
        ),
    )


def _executed_power_loop(lines: list[str], progression: list[str]) -> str:
    reviews = [line for line in lines if line.startswith("PROGRESSION REVIEW ")]
    passive_loss_nj: list[int] = []
    reserve_recharge_ticks: list[int] = []
    repeat_cycles: list[int] = []
    repeat_horizons: list[int] = []
    consumer_backed = 0
    for line in reviews:
        stored_work = re.search(
            r"stored-work=\[passive-loss:(\d+)nJ reserve-recharge:(\d+)t\]",
            line,
        )
        repeat = re.search(
            r"repeat-horizon:(\d+)/(\d+)cycles stop:([^\]\s]+)",
            line,
        )
        if stored_work is None or repeat is None:
            continue
        consumer_backed += 1
        passive_loss_nj.append(int(stored_work.group(1)))
        reserve_recharge_ticks.append(int(stored_work.group(2)))
        repeat_cycles.append(int(repeat.group(1)))
        repeat_horizons.append(int(repeat.group(2)))
    return (
        "executed-power-loop=["
        f"consumer-backed:{consumer_backed}/{len(progression)} "
        f"repeat-cycles:{_span(repeat_cycles, unit='')} "
        f"horizon:{_span(repeat_horizons, unit='')} "
        f"passive-loss:{scaled_span(passive_loss_nj, 1_000_000_000, 'J')} "
        f"reserve-recharge:{_span(reserve_recharge_ticks)}]"
    )


def _executed_manual_fallback(lines: list[str], progression: list[str]) -> str:
    fallbacks = [line for line in lines if line.startswith("PROGRESSION FALLBACK ")]
    attention_ticks: list[int] = []
    metabolic_nj: list[int] = []
    hydration_ul: list[int] = []
    manual_recovery: list[int] = []
    powered_recovery: list[int] = []
    for line in fallbacks:
        attention = re.search(r"attention=\[[^]]* total:(\d+)t\]", line)
        survival = re.search(r"survival-cost=\[(\d+)nJ (\d+)uL\]", line)
        recovery = re.search(r"recovery=\[manual:(\d+)ppm powered:(\d+)ppm\]", line)
        if attention is not None:
            attention_ticks.append(int(attention.group(1)))
        if survival is not None:
            metabolic_nj.append(int(survival.group(1)))
            hydration_ul.append(int(survival.group(2)))
        if recovery is not None:
            manual_recovery.append(int(recovery.group(1)))
            powered_recovery.append(int(recovery.group(2)))
    return (
        "executed-manual-fallback=["
        f"routes:{len(fallbacks)}/{len(progression)} "
        f"attention:{_span(attention_ticks)} "
        f"physical:{physical_duration_span(lines, attention_ticks)} "
        f"body:{scaled_span(metabolic_nj, 1_000_000_000_000, 'kJ')}/"
        f"{scaled_span(hydration_ul, 1_000, 'mL')} "
        f"recovery:{_span(manual_recovery, unit='ppm')}vs"
        f"{_span(powered_recovery, unit='ppm')}]"
    )


def _next_stage_continuation(lines: list[str], progression: list[str]) -> str:
    ticks = [
        int(match.group(1))
        for line in lines
        if line.startswith("PROGRESSION REVIEW ")
        and (
            match := re.search(r"sizing-plate-continuation:(\d+)t", line)
        )
        is not None
    ]
    return (
        "next-stage-continuation=["
        f"sizing-plate:{len(ticks)}/{len(progression)} "
        f"attention:{_span(ticks)} "
        f"physical:{physical_duration_span(lines, ticks)}]"
    )


def _integration_evidence(lines: list[str], progression: list[str]) -> str:
    reviews = [line for line in lines if line.startswith("PROGRESSION REVIEW ")]
    single_state = sum(" continuity=single-state " in line for line in reviews)
    fantasy_captured = sum(
        " captured:true " in line or line.endswith(" captured:true") for line in reviews
    )
    return (
        "integrated-campaign=["
        f"single-state:{single_state}/{len(progression)} "
        f"fantasy-captured:{fantasy_captured}/{len(progression)}]"
    )


def progression_summary(lines: list[str]) -> str | None:
    progression = [line for line in lines if line.startswith("PROGRESSION EXPERIENCE ")]
    if not progression:
        return None
    organic_progression = organic_only(progression)
    hard_span, stockpiling = _stockpiling_evidence(progression)
    frozen_choices, preaction, disclosed_order = _investment_evidence(lines, progression)
    processing_recovery, processing_crossover, bridge_body = _bridge_tradeoff_evidence(
        lines,
        progression,
        frozen_choices,
    )
    return (
        "ORDINARY SUMMARY probe=primitive-progression "
        f"samples={len(progression)} sample-shape=[{sample_shape(progression)}] "
        f"first-copper=[pick:{sum('local-copper-sequence=pick-first' in line for line in progression)} "
        f"crank:{sum('local-copper-sequence=crank-first' in line for line in progression)}] "
        f"organic-first-copper=[pick:{sum('local-copper-sequence=pick-first' in line for line in organic_progression)} "
        f"crank:{sum('local-copper-sequence=crank-first' in line for line in organic_progression)}] "
        f"scarcity-bridge=[direct-second-blocked:{sum('direct-second-upgrade-blocked:true' in line for line in progression)} "
        f"processed-output-playable:{sum('processed-output-playable:true' in line for line in progression)} "
        f"converged:{sum('converged-both-upgrades:true' in line for line in progression)}] "
        f"hard-access-lead={hard_span} "
        f"{stockpiling} "
        f"{_parallel_work_evidence(progression)} "
        f"stockpiling-counterfactual-outcome=[complete:{sum('economics:finite-stockpile-order-complete' in line for line in progression)} "
        f"supply-ended:{sum('economics:supply-ended' in line for line in progression)}] "
        f"{processing_recovery} "
        f"{processing_crossover} "
        f"{bridge_body} "
        f"{preaction} "
        f"{disclosed_order} "
        f"{_executed_manual_fallback(lines, progression)} "
        f"{_executed_power_loop(lines, progression)} "
        f"{_integration_evidence(lines, progression)} "
        f"reinvestment=[completed:{sum('selected-reinvestment=[completed' in line for line in progression)} "
        f"target-supply:{sum('selected-reinvestment=[blocked:known-target-supply' in line for line in progression)} "
        f"storage-capacity:{sum('selected-reinvestment=[blocked:crushed-storage' in line for line in progression)}] "
        f"{_next_stage_continuation(lines, progression)} "
        f"{_continuation_summary(lines)}"
    )
