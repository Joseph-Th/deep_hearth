"""Woodworking summary for ordinary gameplay evidence."""

from __future__ import annotations

import re

from .common import field, organic_only, sample_shape


def woodworking_summary(lines: list[str]) -> str | None:
    woodworking = [line for line in lines if line.startswith("WOODWORKING EXPERIENCE ")]
    if not woodworking:
        return None
    count = lambda marker: sum(marker in line for line in woodworking)
    choice_count = lambda sample_lines, choice: sum(
        field(line, "choice") == choice for line in sample_lines
    )
    horizon_choice_count = lambda horizon, choice: sum(
        field(line, "demand-horizon") == horizon
        and field(line, "choice") == choice
        for line in woodworking
    )
    horizon_counts = {
        horizon: sum(field(line, "demand-horizon") == horizon for line in woodworking)
        for horizon in ("immediate-only", "short-queue", "project")
    }
    observed_choices = {
        choice for choice in ("bare-hands", "stone-adze", "frame-saw")
        if choice_count(woodworking, choice) > 0
    }
    organic_woodworking = organic_only(woodworking)
    feedback = [line for line in lines if line.startswith("WOODWORKING FEEDBACK ")]
    setup_budget_met = 0
    realized_payback = 0
    conservative_budget_misses = 0
    optimistic_budget_misses = 0
    timber_model_agrees = 0
    choices_revised = 0
    for line in feedback:
        attention = re.search(
            r"attention=\[setup-budget-met:(true|false) actual-payback:(true|false)\]",
            line,
        )
        timber = re.search(r"timber=\[nominal:([^ ]+) actual:([^\]]+)\]", line)
        if attention is not None:
            budget_met = attention.group(1) == "true"
            payback = attention.group(2) == "true"
            setup_budget_met += budget_met
            realized_payback += payback
            conservative_budget_misses += not budget_met and payback
            optimistic_budget_misses += budget_met and not payback
        if timber is not None and timber.group(1) == timber.group(2):
            timber_model_agrees += 1
        choices_revised += "choice-revised-after-outcome=true" in line
    return (
        "ORDINARY SUMMARY probe=woodworking "
        f"samples={len(woodworking)} sample-shape=[{sample_shape(woodworking)}] "
        f"choice=[saw:{choice_count(woodworking, 'frame-saw')} "
        f"adze:{choice_count(woodworking, 'stone-adze')} "
        f"bare:{choice_count(woodworking, 'bare-hands')}] "
        f"organic-choice=[saw:{choice_count(organic_woodworking, 'frame-saw')} "
        f"adze:{choice_count(organic_woodworking, 'stone-adze')} "
        f"bare:{choice_count(organic_woodworking, 'bare-hands')}] "
        f"decision-coverage=[horizons:{sum(count > 0 for count in horizon_counts.values())}/3 "
        f"choices:{len(observed_choices)}/3] "
        f"demand-horizon=[immediate-only:{horizon_counts['immediate-only']} "
        f"short-queue:{horizon_counts['short-queue']} "
        f"project:{horizon_counts['project']}] "
        f"by-horizon=[immediate:bare{horizon_choice_count('immediate-only', 'bare-hands')}"
        f"/adze{horizon_choice_count('immediate-only', 'stone-adze')}"
        f"/saw{horizon_choice_count('immediate-only', 'frame-saw')} "
        f"short:bare{horizon_choice_count('short-queue', 'bare-hands')}"
        f"/adze{horizon_choice_count('short-queue', 'stone-adze')}"
        f"/saw{horizon_choice_count('short-queue', 'frame-saw')} "
        f"project:bare{horizon_choice_count('project', 'bare-hands')}"
        f"/adze{horizon_choice_count('project', 'stone-adze')}"
        f"/saw{horizon_choice_count('project', 'frame-saw')}] "
        f"blocked-by-copper={count('reason=copper-supply-limited')} "
        f"reserve-protected={count('reason=copper-reserve-protected')} "
        f"fundable={count(' fundable:true ')} "
        f"attention-payback={count('attention-payback:true')} "
        f"timber-saving={count('timber-saving:true')} "
        f"timber-neutral={count('timber-neutral:true')} "
        f"lifecycle-feedback=[samples:{len(feedback)}/{len(woodworking)} "
        f"setup-budget-met:{setup_budget_met}/{len(feedback)} "
        f"realized-payback:{realized_payback}/{len(feedback)} "
        f"budget-vs-payback=[conservative:{conservative_budget_misses} "
        f"optimistic:{optimistic_budget_misses}] "
        f"timber-model-agrees:{timber_model_agrees}/{len(feedback)} "
        f"choice-revised:{choices_revised}/{len(feedback)}]"
    )
