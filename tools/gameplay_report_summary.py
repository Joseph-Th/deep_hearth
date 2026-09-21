"""Compact factual summaries for gameplay exploration transcripts."""

from __future__ import annotations

import os
import re


REPORT_PREFIXES = (
    "CONTENT registry_schema=",
    "CONTENT ACQUISITION EDGES ",
)


def _field(line: str, name: str) -> str | None:
    """Extract one top-level whitespace-delimited key=value field."""

    match = re.search(rf"(?:^|\s){re.escape(name)}=(\[[^]]*\]|\S+)", line)
    return match.group(1) if match is not None else None


def _compact_fields(line: str, names: tuple[str, ...]) -> str:
    return " ".join(
        f"{name}={value}"
        for name in names
        if (value := _field(line, name)) is not None
    )


def controlled_gameplay_summary(lines: list[str]) -> list[str]:
    """Keep high-value capability evidence visible without dumping capability transcripts."""

    summaries: list[str] = []
    workshop = next(
        (line for line in lines if line.startswith("WORKSHOP CAPABILITY ")), None
    )
    if workshop is not None:
        detail = _compact_fields(workshop, ("scenarios", "orders", "adaptive", "stops"))
        experience = next(
            (line for line in lines if line.startswith("WORKSHOP EXPERIENCE REVIEW ")),
            None,
        )
        experience_detail = ""
        if experience is not None:
            dynamic = re.search(r"\bdynamic-scenarios:(\d+/\d+)", experience)
            interlocks = re.search(
                r"\binterlocks=\[stored-work\+throughput:(\d+) body\+power:(\d+) "
                r"wear\+maintenance:(\d+) structure\+production:(\d+)\]",
                experience,
            )
            recovery = re.search(
                r"\brecovery=\[suspensions:(\d+) resumed:(\d+) stranded:(\d+)\]",
                experience,
            )
            parts: list[str] = []
            if dynamic is not None:
                parts.append(f"dynamic={dynamic.group(1)}")
            if interlocks is not None:
                parts.append(
                    "interlocks=[stored-work:"
                    f"{interlocks.group(1)} body-power:{interlocks.group(2)} "
                    f"wear-maintenance:{interlocks.group(3)} "
                    f"structure-production:{interlocks.group(4)}]"
                )
            if recovery is not None:
                parts.append(
                    "recovery=[suspended:"
                    f"{recovery.group(1)} resumed:{recovery.group(2)} "
                    f"stranded:{recovery.group(3)}]"
                )
            if parts:
                experience_detail = " " + " ".join(parts)
        summaries.append(
            f"CONTROLLED SUMMARY probe=workshop {detail}{experience_detail}".rstrip()
        )

    agency = next((line for line in lines if line.startswith("AGENCY SUMMARY ")), None)
    if agency is not None:
        detail = _compact_fields(
            agency,
            ("worlds", "worlds-with-multiple-signatures", "observed-counterfactual-effects"),
        )
        summaries.append(f"CONTROLLED SUMMARY probe=agency {detail}".rstrip())

    ore_lines = [
        line
        for line in lines
        if line.startswith("CAPABILITY ORE_PREP ") or line.startswith("ORE REVIEW ")
    ]
    ore_completed = [line for line in ore_lines if " outcome=completed " in line]
    ore_stopped = [line for line in ore_lines if " outcome=stopped " in line]
    if ore_completed or ore_stopped:
        feed_signatures = {
            line.split(" feed=[", 1)[1].split("]", 1)[0]
            for line in ore_completed
            if " feed=[" in line
        }
        summaries.append(
            "ORE CAPABILITY SUMMARY "
            f"samples={len(ore_completed) + len(ore_stopped)} "
            f"completed={len(ore_completed)} stopped={len(ore_stopped)} "
            f"finite-energy-stops={sum('blocker=finite-energy' in line for line in ore_stopped)} "
            f"variable-feed={len(feed_signatures)}"
        )

    foundry = [
        line
        for line in lines
        if line.startswith("CAPABILITY FOUNDRY ") or line.startswith("FOUNDRY REVIEW ")
    ]
    if foundry:
        recovery_casts = sum(
            (match := re.search(r"\brecovery-cast=(\d+)mg", line)) is not None
            and int(match.group(1)) > 0
            for line in foundry
        )
        summaries.append(
            "FOUNDRY CAPABILITY SUMMARY "
            f"samples={len(foundry)} "
            f"full={sum(' outcome=full-order-' in line for line in foundry)} "
            f"partial={sum(' outcome=partial-order-' in line for line in foundry)} "
            f"melt-limited={sum('melt-limit=finite-energy' in line for line in foundry)} "
            f"cast-capacity-limited={sum('cast-limit=thermal-sink-capacity' in line for line in foundry)} "
            f"cooldown-recovery={recovery_casts} "
            f"full-after-cooldown={sum('outcome=full-order-recovered-after-cooldown' in line for line in foundry)}"
        )
    return summaries


def _sample_shape(sample_lines: list[str]) -> str:
    return (
        f"anchor:{sum(' sample=anchor ' in line for line in sample_lines)} "
        f"coverage:{sum(' sample=coverage ' in line for line in sample_lines)} "
        f"organic:{sum(' sample=organic ' in line for line in sample_lines)} "
        f"replay:{sum(' sample=replay ' in line for line in sample_lines)}"
    )


def _organic_only(sample_lines: list[str]) -> list[str]:
    return [line for line in sample_lines if " sample=organic " in line]


def _woodworking_summary(lines: list[str]) -> str | None:
    woodworking = [line for line in lines if line.startswith("WOODWORKING EXPERIENCE ")]
    if not woodworking:
        return None
    count = lambda marker: sum(marker in line for line in woodworking)
    choice_count = lambda sample_lines, choice: sum(
        _field(line, "choice") == choice for line in sample_lines
    )
    horizon_choice_count = lambda horizon, choice: sum(
        _field(line, "demand-horizon") == horizon
        and _field(line, "choice") == choice
        for line in woodworking
    )
    organic_woodworking = _organic_only(woodworking)
    return (
        "ORDINARY SUMMARY probe=woodworking "
        f"samples={len(woodworking)} sample-shape=[{_sample_shape(woodworking)}] "
        f"choice=[saw:{choice_count(woodworking, 'frame-saw')} "
        f"adze:{choice_count(woodworking, 'stone-adze')} "
        f"bare:{choice_count(woodworking, 'bare-hands')}] "
        f"organic-choice=[saw:{choice_count(organic_woodworking, 'frame-saw')} "
        f"adze:{choice_count(organic_woodworking, 'stone-adze')} "
        f"bare:{choice_count(organic_woodworking, 'bare-hands')}] "
        f"demand-horizon=[immediate-only:{sum(_field(line, 'demand-horizon') == 'immediate-only' for line in woodworking)} "
        f"short-queue:{sum(_field(line, 'demand-horizon') == 'short-queue' for line in woodworking)} "
        f"project:{sum(_field(line, 'demand-horizon') == 'project' for line in woodworking)}] "
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
        f"net-timber-payback={count('net-timber-payback:true')}"
    )


def _fieldwork_summary(lines: list[str]) -> str | None:
    fieldwork = [line for line in lines if line.startswith("FIELDWORK EXPERIENCE ")]
    if not fieldwork:
        return None
    count = lambda marker: sum(marker in line for line in fieldwork)
    count_field = lambda name, value: sum(
        _field(line, name) == value for line in fieldwork
    )
    geology_tool_count = lambda geology, tool: sum(
        _field(line, "geology") == geology and _field(line, "tool") == tool
        for line in fieldwork
    )
    organic_fieldwork = _organic_only(fieldwork)
    inspections = [
        int(match.group(1))
        for line in fieldwork
        if (match := re.search(r"\bfield-inspections=(\d+)", line)) is not None
    ]
    inspection_span = f"{min(inspections)}..{max(inspections)}" if inspections else "n/a"
    fulfillment_ppm = []
    for line in fieldwork:
        requested = re.search(r"\brequested=(\d+)mg", line)
        mined = re.search(r"\bmining=(\d+)mg", line)
        if requested is not None and mined is not None and int(requested.group(1)) > 0:
            fulfillment_ppm.append(
                int(mined.group(1)) * 1_000_000 // int(requested.group(1))
            )
    fulfillment_span = (
        f"{min(fulfillment_ppm)}..{max(fulfillment_ppm)}ppm"
        if fulfillment_ppm
        else "n/a"
    )
    resource_capped = 0
    organic_resource_capped = 0
    for line in fieldwork:
        requested = re.search(r"\brequested=(\d+)mg", line)
        planned = re.search(r"\bplanned-local-work=(\d+)mg", line)
        if (
            requested is not None
            and planned is not None
            and int(planned.group(1)) < int(requested.group(1))
        ):
            resource_capped += 1
            if " sample=organic " in line:
                organic_resource_capped += 1
    return (
        "ORDINARY SUMMARY probe=fieldwork "
        f"samples={len(fieldwork)} sample-shape=[{_sample_shape(fieldwork)}] inspections={inspection_span} "
        f"outcomes=[completed:{count('outcome=completed')} "
        f"local-supply-ended:{count('outcome=known-target-supply')} "
        f"fulfillment:{fulfillment_span}] "
        f"organic-outcomes=[completed:{sum('outcome=completed' in line for line in organic_fieldwork)} "
        f"local-supply-ended:{sum('outcome=known-target-supply' in line for line in organic_fieldwork)}] "
        f"reserve-knowledge=[workload-capped:{resource_capped} "
        f"tool-changed:{count('resource-knowledge-effect=changed-tool')}] "
        f"organic-reserve-knowledge=[workload-capped:{organic_resource_capped} "
        f"tool-changed:{sum('resource-knowledge-effect=changed-tool' in line for line in organic_fieldwork)}] "
        f"orders=[short:{count('order-horizon=short')} long:{count('order-horizon=long')}] "
        f"geology=[soft:{count('geology=quarry-soft')} "
        f"reinforcement:{count('geology=quarry-reinforcement')} "
        f"hard-specialist:{count('geology=hard-pick-specialist')}] "
        f"copper=[available:{count('copper-opportunity=available')} "
        f"absent:{count('copper-opportunity=absent')}] "
        f"tools=[stone-pick:{count_field('tool', 'stone-pick')} "
        f"soft-quarry:{count_field('tool', 'stone-quarry')} "
        f"reinforced-quarry:{count_field('tool', 'copper-reinforced-quarry')} "
        f"hard-pick:{count_field('tool', 'copper-reinforced-hard-pick')}] "
        f"geology-tool=[soft:pick{geology_tool_count('quarry-soft', 'stone-pick')}"
        f"/quarry{geology_tool_count('quarry-soft', 'stone-quarry')}"
        f"/reinforced{geology_tool_count('quarry-soft', 'copper-reinforced-quarry')}"
        f"/hard{geology_tool_count('quarry-soft', 'copper-reinforced-hard-pick')} "
        f"reinforcement:pick{geology_tool_count('quarry-reinforcement', 'stone-pick')}"
        f"/quarry{geology_tool_count('quarry-reinforcement', 'stone-quarry')}"
        f"/reinforced{geology_tool_count('quarry-reinforcement', 'copper-reinforced-quarry')}"
        f"/hard{geology_tool_count('quarry-reinforcement', 'copper-reinforced-hard-pick')} "
        f"hard-specialist:pick{geology_tool_count('hard-pick-specialist', 'stone-pick')}"
        f"/quarry{geology_tool_count('hard-pick-specialist', 'stone-quarry')}"
        f"/reinforced{geology_tool_count('hard-pick-specialist', 'copper-reinforced-quarry')}"
        f"/hard{geology_tool_count('hard-pick-specialist', 'copper-reinforced-hard-pick')}]"
    )


def _power_provider_summary(lines: list[str]) -> str | None:
    power = [line for line in lines if line.startswith("POWER PROVIDER EXPERIENCE ")]
    if not power:
        return None
    organic_power = _organic_only(power)
    settlement = [line for line in lines if line.startswith("POWER SETTLEMENT ")]
    organic_settlement = _organic_only(settlement)
    break_evens = [
        int(match.group(1))
        for line in power
        if (match := re.search(r"break-even-charges:(\d+)", line)) is not None
    ]
    planned_charges = [
        int(match.group(1))
        for line in power
        if (match := re.search(r"planned-charges:(\d+)", line)) is not None
    ]
    metabolic_wins = 0
    for line in power:
        crank_cost = re.search(r"metabolic-crank:(\d+)nJ", line)
        treadle_cost = re.search(r"metabolic-treadle:(\d+)nJ", line)
        if (
            crank_cost is not None
            and treadle_cost is not None
            and int(treadle_cost.group(1)) < int(crank_cost.group(1))
        ):
            metabolic_wins += 1
    break_even_span = (
        f"{min(break_evens)}..{max(break_evens)}" if break_evens else "n/a"
    )
    workload_span = (
        f"{min(planned_charges)}..{max(planned_charges)}"
        if planned_charges
        else "n/a"
    )
    settlement_workloads = [
        int(match.group(1))
        for line in settlement
        if (match := re.search(r"planned-charges=(\d+)", line)) is not None
    ]
    settlement_break_evens = [
        int(match.group(1))
        for line in settlement
        if (match := re.search(r"break-even:(\d+)charges", line)) is not None
    ]
    settlement_workload_span = (
        f"{min(settlement_workloads)}..{max(settlement_workloads)}"
        if settlement_workloads
        else "n/a"
    )
    settlement_break_even_span = (
        f"{min(settlement_break_evens)}..{max(settlement_break_evens)}"
        if settlement_break_evens
        else "n/a"
    )

    def selected_workload_span(
        candidate_lines: list[str], selected: str, pattern: str
    ) -> str:
        values = []
        for line in candidate_lines:
            decision = re.search(r"\bdecision=\[selected:([^\s\]]+)", line)
            workload = re.search(pattern, line)
            if (
                decision is not None
                and workload is not None
                and decision.group(1) == selected
            ):
                values.append(int(workload.group(1)))
        return f"{min(values)}..{max(values)}" if values else "n/a"

    return (
        "ORDINARY SUMMARY probe=power-provider "
        f"samples={len(power)} sample-shape=[{_sample_shape(power)}] "
        f"choice=[crank:{sum('selected:crank' in line for line in power)} "
        f"treadle:{sum('selected:treadle' in line for line in power)}] "
        f"organic-choice=[crank:{sum('selected:crank' in line for line in organic_power)} "
        f"treadle:{sum('selected:treadle' in line for line in organic_power)}] "
        f"planned-charges={workload_span} break-even-charges={break_even_span} "
        f"choice-load=[crank:{selected_workload_span(power, 'crank', r'planned-charges:(\d+)')} "
        f"treadle:{selected_workload_span(power, 'treadle', r'planned-charges:(\d+)')}] "
        f"metabolic-lower-treadle={metabolic_wins} "
        f"settlement-choice=[treadle:{sum('selected:treadle' in line for line in settlement)} "
        f"walking:{sum('selected:walking-wheel' in line for line in settlement)}] "
        f"organic-settlement-choice=[treadle:{sum('selected:treadle' in line for line in organic_settlement)} "
        f"walking:{sum('selected:walking-wheel' in line for line in organic_settlement)}] "
        f"settlement-planned-charges={settlement_workload_span} "
        f"settlement-break-even-charges={settlement_break_even_span} "
        f"settlement-load=[treadle:{selected_workload_span(settlement, 'treadle', r'planned-charges=(\d+)')} "
        f"walking:{selected_workload_span(settlement, 'walking-wheel', r'planned-charges=(\d+)')}]"
    )


def _survival_summary(lines: list[str]) -> str | None:
    survival = [line for line in lines if line.startswith("SURVIVAL EXPERIENCE ")]
    if not survival:
        return None
    count = lambda marker: sum(marker in line for line in survival)
    organic_survival = _organic_only(survival)
    candidate_counts = [
        int(match.group(1))
        for line in survival
        if (match := re.search(r"\bcandidates:(\d+)", line)) is not None
    ]
    initial_work_drinks = []
    follow_up_work_drinks = []
    for line in survival:
        integrated = re.search(
            r"integrated=\[drink:(\d+)uL/\d+t .*?reprovision:(?:true|false):(\d+)uL/\d+t",
            line,
        )
        if integrated is not None:
            initial_work_drinks.append(int(integrated.group(1)))
            follow_up_work_drinks.append(int(integrated.group(2)))
    volume_span = (
        lambda values: f"{min(values)}..{max(values)}uL" if values else "n/a"
    )
    return (
        "ORDINARY SUMMARY probe=survival "
        f"samples={len(survival)} sample-shape=[{_sample_shape(survival)}] "
        f"pressure=[hydration:{count('pressure=hydration')} energy:{count('pressure=energy')}] "
        f"diet=[balanced:{count('diet:balanced-recovery')} compact:{count('diet:compact-calories')}] "
        f"preservation-opportunity=[scarce:{count('mode:scarce-timber')} "
        f"choice-rich:{count('mode:choice-rich-timber')} "
        f"alternate:{count('mode:alternate-material')} finite:{count('mode:finite-material')} "
        f"singleton:{sum(value == 1 for value in candidate_counts)} "
        f"multi:{sum(value > 1 for value in candidate_counts)}] "
        f"preservation=[declined:{count('storage-policy:decline')} "
        f"efficient:{count('storage-policy:attention-efficient')} "
        f"singleton:{count('storage-policy:enclosure-singleton')} "
        f"frontier:{count('storage-policy:balanced-frontier')} "
        f"maximum:{count('storage-policy:maximum-protection')}] "
        f"commitment=[cleared:{count('commitment-reason:return-clears-threshold')} "
        f"declined-return:{count('commitment-reason:return-does-not-clear-threshold')}] "
        f"work-interlock=[opportunity-power:{count('opportunity-power:true')} "
        f"reprovisioned:{count('reprovision:true:')} "
        f"initial-drink:{volume_span(initial_work_drinks)} "
        f"follow-up-drink:{volume_span(follow_up_work_drinks)} "
        f"warning-safe:{count('warning-safe:true')}/{len(survival)}] "
        f"organic-preservation=[declined:{sum('storage-policy:decline' in line for line in organic_survival)} "
        f"efficient:{sum('storage-policy:attention-efficient' in line for line in organic_survival)} "
        f"singleton:{sum('storage-policy:enclosure-singleton' in line for line in organic_survival)} "
        f"frontier:{sum('storage-policy:balanced-frontier' in line for line in organic_survival)} "
        f"maximum:{sum('storage-policy:maximum-protection' in line for line in organic_survival)}]"
    )


def _progression_summary(lines: list[str]) -> str | None:
    progression = [line for line in lines if line.startswith("PROGRESSION EXPERIENCE ")]
    if not progression:
        return None
    organic_progression = _organic_only(progression)
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
    delay_avoided_span = (
        f"{min(stockpiling_delay_avoided)}..{max(stockpiling_delay_avoided)}t"
        if stockpiling_delay_avoided
        else "not-comparable"
    )
    hard_leads = [
        int(match.group(1))
        for line in progression
        if (match := re.search(r"hard-access-lead:(\d+)t", line)) is not None
    ]
    maintenance_overlap = [
        int(match.group(1))
        for line in progression
        if (match := re.search(r"\bmaintenance-prep-overlap:(\d+)t", line)) is not None
    ]
    hard_span = f"{min(hard_leads)}..{max(hard_leads)}t" if hard_leads else "n/a"
    returned_attention = [
        int(match.group(1))
        for line in progression
        if (match := re.search(r"returned-attention:(\d+)t", line)) is not None
    ]
    returned_span = (
        f"{min(returned_attention)}..{max(returned_attention)}t"
        if returned_attention
        else "n/a"
    )
    returned_share = [
        int(match.group(1))
        for line in progression
        if (match := re.search(r"\breturned:(\d+)ppm", line)) is not None
    ]
    overlap_vs_setup = [
        int(match.group(1))
        for line in progression
        if (match := re.search(r"\boverlap/setup:(\d+)ppm", line)) is not None
    ]
    post_overlap_equivalent_cycles = [
        int(match.group(1))
        for line in progression
        if (match := re.search(r"\bpost-equivalent:(\d+)cycles", line)) is not None
    ]
    returned_share_span = (
        f"{min(returned_share)}..{max(returned_share)}ppm" if returned_share else "n/a"
    )
    maintenance_overlap_span = (
        f"{min(maintenance_overlap)}..{max(maintenance_overlap)}t"
        if maintenance_overlap
        else "n/a"
    )
    overlap_vs_setup_span = (
        f"{min(overlap_vs_setup)}..{max(overlap_vs_setup)}ppm"
        if overlap_vs_setup
        else "n/a"
    )
    post_overlap_span = (
        f"{min(post_overlap_equivalent_cycles)}..{max(post_overlap_equivalent_cycles)}"
        if post_overlap_equivalent_cycles
        else "n/a"
    )
    manual_recovery = []
    powered_recovery = []
    manual_bridge_attention = []
    powered_line_attention = []
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
    manual_recovery_span = (
        f"{min(manual_recovery)}..{max(manual_recovery)}ppm"
        if manual_recovery
        else "n/a"
    )
    powered_recovery_span = (
        f"{min(powered_recovery)}..{max(powered_recovery)}ppm"
        if powered_recovery
        else "n/a"
    )
    manual_order_attention = []
    mechanized_order_attention = []
    order_attention_saved = []
    preaction_manual_attention = []
    preaction_machine_upper = []
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
    span = lambda values: f"{min(values)}..{max(values)}t" if values else "n/a"
    return (
        "ORDINARY SUMMARY probe=primitive-progression "
        f"samples={len(progression)} sample-shape=[{_sample_shape(progression)}] "
        f"first-copper=[pick:{sum('local-copper-sequence=pick-first' in line for line in progression)} "
        f"crank:{sum('local-copper-sequence=crank-first' in line for line in progression)}] "
        f"organic-first-copper=[pick:{sum('local-copper-sequence=pick-first' in line for line in organic_progression)} "
        f"crank:{sum('local-copper-sequence=crank-first' in line for line in organic_progression)}] "
        f"scarcity-bridge=[direct-second-blocked:{sum('direct-second-upgrade-blocked:true' in line for line in progression)} "
        f"processed-output-playable:{sum('processed-output-playable:true' in line for line in progression)} "
        f"converged:{sum('converged-both-upgrades:true' in line for line in progression)}] "
        f"hard-access-lead={hard_span} "
        f"stockpiling-counterfactual=[returned-attention:{returned_span} returned-share:{returned_share_span} "
        f"maintenance-prep-overlap:{maintenance_overlap_span} "
        f"useful-overlap/setup:{overlap_vs_setup_span} "
        f"post-overlap-equivalent-cycles:{post_overlap_span}] "
        f"stockpile=[complete:{sum('economics:finite-stockpile-order-complete' in line for line in progression)} "
        f"supply-ended:{sum('economics:supply-ended' in line for line in progression)}] "
        f"processing-recovery=[manual:{manual_recovery_span} powered:{powered_recovery_span}] "
        f"processing-crossover=[one-bridge-manual:{span(manual_bridge_attention)} "
        f"line-setup:{span(powered_line_attention)} "
        f"long-order-mechanized:{frozen_processing_choices}/{len(progression)}] "
        f"preaction-processing-investment=[selected:mechanized manual:{span(preaction_manual_attention)} "
        f"conservative-machine-upper:{span(preaction_machine_upper)} "
        f"frozen:{frozen_processing_choices}/{len(progression)}] "
        f"disclosed-order-attention=[manual:{span(manual_order_attention)} "
        f"mechanized:{span(mechanized_order_attention)} saved:{span(order_attention_saved)}] "
        f"reinvestment=[completed:{sum('selected-reinvestment=[completed' in line for line in progression)} "
        f"blocked:{sum('selected-reinvestment=[blocked:' in line for line in progression)}] "
        f"continuation=[immediate:{immediate_completed}/{immediate_blocked} "
        f"stockpile-first:{delayed_completed}/{delayed_blocked} "
        f"delay-avoided:{delay_avoided_span}]"
    )


def _liberation_summary(lines: list[str]) -> str | None:
    liberation = [
        line for line in lines if line.startswith("LIBERATION FRONTIER CAPABILITY ")
    ]
    if not liberation:
        return None
    final_grades = [
        int(match.group(1))
        for line in liberation
        if (match := re.search(r"\bfinal:\d+mg/(\d+)ppm", line)) is not None
    ]
    scavenged_copper = [
        int(match.group(1))
        for line in liberation
        if (match := re.search(r"\bscavenger-recovered:(\d+)mg", line)) is not None
    ]
    frontiers = [
        match.group(1)
        for line in lines
        if line.startswith("LIBERATION FRONTIER ")
        and (match := re.search(r"\bsmelting-frontier=(\S+)", line)) is not None
    ]
    grade_span = (
        f"{min(final_grades)}..{max(final_grades)}ppm" if final_grades else "n/a"
    )
    scavenged_span = (
        f"{min(scavenged_copper)}..{max(scavenged_copper)}mg"
        if scavenged_copper
        else "n/a"
    )
    frontier = frontiers[0] if frontiers and len(set(frontiers)) == 1 else "mixed-or-unknown"
    return (
        "FRONTIER SUMMARY probe=primitive-liberation current-player-selected=false "
        f"samples={len(liberation)} final-concentrate-grade={grade_span} "
        f"scavenger-copper={scavenged_span} "
        f"conserved={sum('matter=conserved' in line for line in liberation)} "
        f"smelting-frontier={frontier}"
    )


def ordinary_gameplay_summary(lines: list[str]) -> list[str]:
    """Return compact measured ordinary-play evidence without adding interpretation."""

    summaries: list[str] = []
    for summary in (
        _progression_summary(lines),
        _liberation_summary(lines),
        _woodworking_summary(lines),
        _fieldwork_summary(lines),
        _power_provider_summary(lines),
        _survival_summary(lines),
    ):
        if summary is not None:
            summaries.append(summary)
    return summaries


def concise_gameplay_report(stdout: str, environ=None) -> str:
    """Keep one compact factual summary per probe; verbose retains the full transcript."""

    environment = os.environ if environ is None else environ
    if environment.get("DEEP_HEARTH_GAMEPLAY_VERBOSE") is not None or environment.get(
        "DEEP_HEARTH_GAMEPLAY_TRACE"
    ) is not None:
        return stdout.rstrip()
    lines = stdout.splitlines()
    selected = [
        line
        for line in lines
        if line.startswith(REPORT_PREFIXES)
        or line.startswith("SIMULATION TIME ")
        or line.startswith("PLAYER FANTASY ")
        or line.startswith("EVALUATION SCOPE kind=ordinary-play ")
        or line.startswith("EVALUATION SCOPE kind=controlled-capability ")
    ]
    selected.extend(ordinary_gameplay_summary(lines))
    selected.extend(controlled_gameplay_summary(lines))
    return "\n".join(selected)
