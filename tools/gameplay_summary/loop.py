"""Cross-domain evidence for the ordinary player-control loop."""

from __future__ import annotations

import re
from dataclasses import dataclass

from .common import field


@dataclass(frozen=True)
class _LoopEvidenceLines:
    progression: list[str]
    progression_reviews: list[str]
    liberation: list[str]
    liberation_kit: list[str]
    first_foundry: list[str]
    woodworking: list[str]
    fieldwork: list[str]
    power: list[str]
    settlement: list[str]
    power_projects: list[str]
    survival: list[str]
    survey_campaigns: list[str]
    shortfall_recoveries: list[str]
    bulk_crossovers: list[str]


def _lines_with_prefix(lines: list[str], prefix: str) -> list[str]:
    return [line for line in lines if line.startswith(prefix)]


def _collect_loop_evidence(lines: list[str]) -> _LoopEvidenceLines:
    return _LoopEvidenceLines(
        progression=_lines_with_prefix(lines, "PROGRESSION EXPERIENCE "),
        progression_reviews=_lines_with_prefix(lines, "PROGRESSION REVIEW "),
        liberation=_lines_with_prefix(lines, "LIBERATION FRONTIER CAPABILITY "),
        liberation_kit=_lines_with_prefix(lines, "LIBERATION KIT ACQUISITION "),
        first_foundry=_lines_with_prefix(lines, "FIRST FOUNDRY EXPERIENCE "),
        woodworking=_lines_with_prefix(lines, "WOODWORKING EXPERIENCE "),
        fieldwork=_lines_with_prefix(lines, "FIELDWORK EXPERIENCE "),
        power=_lines_with_prefix(lines, "POWER PROVIDER EXPERIENCE "),
        settlement=_lines_with_prefix(lines, "SETTLEMENT EXPERIENCE "),
        power_projects=_lines_with_prefix(lines, "POWER PROJECT EXPERIENCE "),
        survival=_lines_with_prefix(lines, "SURVIVAL EXPERIENCE "),
        survey_campaigns=_lines_with_prefix(lines, "FIELDWORK SURVEY CAMPAIGN "),
        shortfall_recoveries=_lines_with_prefix(
            lines, "FIELDWORK INITIAL SHORTFALL RECOVERY "
        ),
        bulk_crossovers=_lines_with_prefix(lines, "FIELDWORK BULK CROSSOVER "),
    )


def _evidence_shape(evidence: _LoopEvidenceLines) -> str:
    single_state = sum(
        " continuity=single-state " in line for line in evidence.progression_reviews
    )
    return (
        "evidence-shape=["
        f"single-state-progression:{single_state}/{len(evidence.progression_reviews)} "
        f"domain-episodes:survival{len(evidence.survival)}/"
        f"woodworking{len(evidence.woodworking)}/fieldwork{len(evidence.fieldwork)}/"
        f"power{len(evidence.power)}/settlement{len(evidence.settlement)}/"
        f"liberation{len(evidence.liberation)}/"
        f"first-foundry{len(evidence.first_foundry)}]"
    )


def _observe_infer_evidence(fieldwork: list[str], extracted: int) -> str:
    return (
        "observe-infer=["
        f"evidence-gated-extraction:{extracted}/{len(fieldwork)} "
        f"reserve-knowledge-changed-plan:{_reserve_knowledge_changed_plan(fieldwork)}/{len(fieldwork)} "
        f"avoided-tool-overinvestment:{sum('resource-knowledge-effect=changed-tool' in line for line in fieldwork)}/{len(fieldwork)}]"
    )


def _extract_evidence(fieldwork: list[str], liberation: list[str], extracted: int) -> str:
    completed_liberation = sum("cleanup-executed=true" in line for line in liberation)
    return (
        f"extract=[fieldwork:{extracted}/{len(fieldwork)} "
        f"liberation-native-copper:{completed_liberation}/{len(liberation)}]"
    )


def _thermal_bootstrap_evidence(first_foundry: list[str]) -> str:
    foundry_builds = sum(" foundry-build=true " in line for line in first_foundry)
    batch_foundry = sum(
        " continuation=settlement-batch-foundry" in line for line in first_foundry
    )
    return f"thermal-bootstrap={batch_foundry}/{foundry_builds}"


def _reserve_knowledge_changed_plan(fieldwork: list[str]) -> int:
    changed = 0
    for line in fieldwork:
        requested = re.search(r"\brequested=(\d+)mg", line)
        planned = re.search(r"\bplanned-local-work=(\d+)mg", line)
        if "resource-knowledge-effect=changed-tool" in line or (
            requested is not None
            and planned is not None
            and int(planned.group(1)) < int(requested.group(1))
        ):
            changed += 1
    return changed


def _attention_saved_span(progression: list[str]) -> str:
    values = [
        int(match.group(1))
        for line in progression
        if (match := re.search(r"\bsaved:(\d+)t", line)) is not None
    ]
    return f"{min(values)}..{max(values)}t" if values else "n/a"


def _fieldwork_extracted(fieldwork: list[str]) -> int:
    return sum(
        (match := re.search(r"\bmining=(\d+)mg", line)) is not None
        and int(match.group(1)) > 0
        for line in fieldwork
    )


def _choice_diversity(
    woodworking: list[str],
    fieldwork: list[str],
    power: list[str],
    settlement: list[str],
    survival: list[str],
    survey_campaigns: list[str],
    bulk_crossovers: list[str],
) -> str:
    woodworking_choices = {
        choice
        for line in woodworking
        if (choice := field(line, "choice")) is not None
    }
    settlement_choices = {
        match.group(1)
        for line in settlement
        if (match := re.search(r"\bchoice:([^\s\]]+)", line)) is not None
    }
    fieldwork_tools = {
        tool for line in fieldwork if (tool := field(line, "tool")) is not None
    }
    power_choices = {
        match.group(1)
        for line in power
        if (match := re.search(r"\bdecision=\[selected:([^\s\]]+)", line))
        is not None
    }
    preservation_policies = {
        match.group(1)
        for line in survival
        if (match := re.search(r"\bstorage-policy:([^\s\]]+)", line)) is not None
    }
    survey_strategies = {
        strategy
        for line in survey_campaigns
        if (strategy := field(line, "selected")) is not None
    }
    bulk_tools = {
        match.group(1)
        for line in bulk_crossovers
        if " available=true " in line
        and (match := re.search(r"\btool=([^\s]+)", line)) is not None
    }
    return (
        "choice-diversity=["
        f"woodworking:{len(woodworking_choices)}/3 "
        f"fieldwork-selected:{len(fieldwork_tools)}/4 "
        f"bulk-crossover-tools:{len(bulk_tools)}/2 "
        f"power-market:{len(power_choices)}/2 "
        f"settlement-investment:{len(settlement_choices)}/2 "
        f"survey-strategy:{len(survey_strategies)}/2 "
        f"preservation:{len(preservation_policies)}/5]"
    )


def _prepare_invest_evidence(
    woodworking: list[str],
    power: list[str],
    settlement: list[str],
    liberation_kit: list[str],
    survey_campaigns: list[str],
    shortfall_recoveries: list[str],
) -> str:
    invested_woodworking = sum(
        field(line, "choice") not in (None, "bare-hands") for line in woodworking
    )
    indexed_campaigns = sum(
        " selected=indexed-channel " in line for line in survey_campaigns
    )
    indexed_shortfalls = sum(
        " strategy=indexed-channel " in line for line in shortfall_recoveries
    )
    settlement_builds = sum(" upgraded:true " in line for line in settlement)
    return (
        "prepare-invest=["
        f"primitive-kit:{len(liberation_kit)} "
        f"woodworking-tool:{invested_woodworking}/{len(woodworking)} "
        f"power-market:{len(power)}/{len(power)} "
        f"settlement-machine:{settlement_builds}/{len(settlement)} "
        f"knowledge-tech=[campaign:{indexed_campaigns}/{len(survey_campaigns)} "
        f"lived-shortfall:{indexed_shortfalls}/{len(shortfall_recoveries)}]]"
    )


def _delegate_reinvest_evidence(
    progression: list[str], settlement: list[str]
) -> tuple[str, str]:
    mechanized = sum(
        "processing-investment=[selected:mechanized" in line for line in progression
    )
    reinvested = sum(
        re.search(r"\bselected-reinvestment=\[completed(?:\s|\])", line) is not None
        for line in progression
    )
    productive_overlap = [
        int(match.group(1))
        for line in progression
        if (match := re.search(r"\bproductive-attention:(\d+)t", line)) is not None
    ]
    autonomous_room = [
        int(match.group(1))
        for line in progression
        if (match := re.search(r"\breturned-attention:(\d+)t", line)) is not None
    ]
    overlap_span = (
        f"{min(productive_overlap)}..{max(productive_overlap)}t"
        if productive_overlap
        else "n/a"
    )
    room_span = (
        f"{min(autonomous_room)}..{max(autonomous_room)}t"
        if autonomous_room
        else "n/a"
    )
    settlement_delegated = sum(
        (match := re.search(r"\bdelegated:(\d+)t", line)) is not None
        and int(match.group(1)) > 0
        for line in settlement
    )
    delegate = (
        "delegate=["
        f"mechanized-processing:{mechanized}/{len(progression)} "
        f"settlement-orders:{settlement_delegated}/{len(settlement)} "
        f"attention-saved:{_attention_saved_span(progression)} "
        f"productive-overlap:{overlap_span} autonomous-room:{room_span}]"
    )
    reinvest = f"reassess-reinvest=[completed:{reinvested}/{len(progression)}]"
    return delegate, reinvest


def _survival_adaptation_evidence(
    survival: list[str], power_projects: list[str]
) -> str:
    follow_up = sum(" reprovision:true:" in line for line in survival)
    task_floor = sum("hydration-policy:task-floor " in line for line in survival)
    working_reserve = sum("hydration-policy:working-reserve " in line for line in survival)
    opportunity_power = sum(" opportunity-power:true " in line for line in survival)
    executed_power = 0
    for line in survival:
        match = re.search(r"\bpower:(\d+)t stored:(\d+)nJ", line)
        if match is not None and int(match.group(1)) > 0 and int(match.group(2)) > 0:
            executed_power += 1
    project_breaks = []
    project_drinks = []
    project_meals = []
    for line in power_projects:
        match = re.search(r"provisioning=\[stops:(\d+)", line)
        if match is not None:
            project_breaks.append(int(match.group(1)))
        drinks = re.search(r"provisioning=\[.*?drinks:(\d+)", line)
        if drinks is not None:
            project_drinks.append(int(drinks.group(1)))
        meals = re.search(r"provisioning=\[.*?meals:(\d+)", line)
        if meals is not None:
            project_meals.append(int(meals.group(1)))
    serving_floor_ul = [
        int(match.group(1))
        for line in survival
        if (
            match := re.search(r"\bshort-loop-serving-floor:(\d+)uL", line)
        )
        is not None
    ]
    serving_floor = (
        f"{min(serving_floor_ul) / 1_000:g}..{max(serving_floor_ul) / 1_000:g}mL"
        if serving_floor_ul
        else "n/a"
    )
    return (
        "survive-adapt=["
        f"short-loop-serving-floor:{serving_floor} "
        f"short-loop-reprovision:{follow_up}/{len(survival)} "
        f"hydration-policy:task-floor{task_floor}/working-reserve{working_reserve} "
        f"opportunistic-power:{executed_power}/{opportunity_power} "
        f"sustained-project-provisioning=[breaks:{sum(value > 0 for value in project_breaks)}/{len(project_breaks)} "
        f"events:{sum(project_breaks)} drinks:{sum(project_drinks)} meals:{sum(project_meals)}] "
        f"warning-safe:{sum(' warning-safe:true' in line for line in survival)}/{len(survival)}]"
    )


def _selected_woodworking_services(line: str) -> int:
    choice = field(line, "choice")
    if choice == "stone-adze":
        match = re.search(r"routes=\[adze:.*?maintenance:\d+t/(\d+)services", line)
        return int(match.group(1)) if match is not None else 0
    if choice == "frame-saw":
        match = re.search(r"actual=\[.*?saw-services:(\d+) adze-services:(\d+)\]", line)
        return int(match.group(1)) + int(match.group(2)) if match is not None else 0
    return 0


def _maintenance_evidence(
    woodworking: list[str], power_projects: list[str]
) -> str:
    service_counts = [_selected_woodworking_services(line) for line in woodworking]
    power_service_counts = []
    for line in power_projects:
        match = re.search(r"maintenance=\[services:(\d+)", line)
        if match is not None:
            power_service_counts.append(int(match.group(1)))
    return (
        "maintain-recover=["
        f"woodworking-service-worlds:{sum(count > 0 for count in service_counts)}/{len(woodworking)} "
        f"woodworking-service-events:{sum(service_counts)} "
        f"mechanized-projects-with-service:{sum(count > 0 for count in power_service_counts)}/{len(power_service_counts)} "
        f"mechanized-service-events:{sum(power_service_counts)}]"
    )


def _world_feedback_evidence(lines: list[str], fieldwork: list[str]) -> str:
    depletion = [
        line
        for line in lines
        if line.startswith("FIELDWORK DEPLETION ")
        and not line.startswith("FIELDWORK DEPLETION RECOVERY ")
    ]
    eligible = [line for line in depletion if " eligible=true " in line]
    depleted = sum(" supply-ended=true " in line for line in eligible)
    initial_supply_ended = sum("outcome=known-target-supply" in line for line in fieldwork)
    reroute_proved = sum(
        line.startswith("FIELDWORK DEPLETION RECOVERY ")
        and " reroute-proved=true " in line
        for line in lines
    )
    initial_reroute_proved = sum(
        line.startswith("FIELDWORK INITIAL SHORTFALL RECOVERY ")
        and " reroute-proved=true " in line
        for line in lines
    )
    shortfall_recoveries = [
        line
        for line in lines
        if line.startswith("FIELDWORK INITIAL SHORTFALL RECOVERY ")
    ]
    indexed_shortfall = sum(
        " strategy=indexed-channel " in line for line in shortfall_recoveries
    )
    geology_changed = sum(
        (match := re.search(r"\bhardness-tier-changes:(\d+)", line)) is not None
        and int(match.group(1)) > 0
        for line in shortfall_recoveries
    )
    retooled = sum(
        (match := re.search(r"\btool-builds:(\d+)", line)) is not None
        and int(match.group(1)) > 0
        for line in shortfall_recoveries
    )
    shortfall_salvaged = sum(
        (match := re.search(r"\bsalvage-retools:(\d+)", line)) is not None
        and int(match.group(1)) > 0
        for line in shortfall_recoveries
    )
    blocked_sites = sum(
        int(match.group(1))
        for line in shortfall_recoveries
        if (match := re.search(r"\bblocked-sites:(\d+)", line)) is not None
    )
    shortfall_ore_funded = sum(
        (match := re.search(r"\bore-recovery-events:(\d+)", line)) is not None
        and int(match.group(1)) > 0
        for line in shortfall_recoveries
    )
    shortfall_ore_payback = sum(
        int(match.group(1))
        for line in shortfall_recoveries
        if (match := re.search(r"\bore-recovery-payback:(\d+)", line)) is not None
    )
    shortfall_ore_required = sum(
        int(match.group(1))
        for line in shortfall_recoveries
        if (match := re.search(r"\bore-recovery-required-access:(\d+)", line)) is not None
    )
    depletion_retooled = sum(
        line.startswith("FIELDWORK DEPLETION RECOVERY ")
        and " mining-tool-reused=false " in line
        for line in lines
    )
    depletion_salvaged = sum(
        line.startswith("FIELDWORK DEPLETION RECOVERY ") and " salvage=true " in line
        for line in lines
    )
    depletion_ore_funded = sum(
        line.startswith("FIELDWORK DEPLETION RECOVERY ")
        and (match := re.search(r"\bore-recovery=\[reason:[^\s]+ ticks:(\d+)", line))
        is not None
        and int(match.group(1)) > 0
        for line in lines
    )
    depletion_ore_payback = sum(
        line.startswith("FIELDWORK DEPLETION RECOVERY ")
        and " ore-recovery=[reason:payback " in line
        for line in lines
    )
    depletion_ore_required = sum(
        line.startswith("FIELDWORK DEPLETION RECOVERY ")
        and " ore-recovery=[reason:required-access " in line
        for line in lines
    )
    return (
        "world-feedback=["
        f"initial-short:{initial_supply_ended}/{len(fieldwork)} "
        f"shortfall-rerouted:{initial_reroute_proved}/{initial_supply_ended} "
        f"knowledge-upgraded:{indexed_shortfall}/{len(shortfall_recoveries)} "
        f"geology-changed:{geology_changed}/{len(shortfall_recoveries)} "
        f"retooled:{retooled}/{len(shortfall_recoveries)} "
        f"salvaged:{shortfall_salvaged}/{len(shortfall_recoveries)} "
        f"ore-funded:{shortfall_ore_funded}/{len(shortfall_recoveries)}"
        f"(payback:{shortfall_ore_payback}/access:{shortfall_ore_required}) "
        f"blocked-sites:{blocked_sites} "
        f"depleted:{depleted}/{len(eligible)} "
        f"dep-rerouted:{reroute_proved}/{depleted} "
        f"dep-retooled:{depletion_retooled}/{depleted} "
        f"dep-salvaged:{depletion_salvaged}/{depleted} "
        f"dep-ore-funded:{depletion_ore_funded}/{depleted}"
        f"(payback:{depletion_ore_payback}/access:{depletion_ore_required}) "
        f"horizon-live:{sum(' terminal=horizon-live-target ' in line for line in eligible)}/{len(eligible)}]"
    )


def player_loop_evidence(lines: list[str]) -> str | None:
    """Summarize whether ordinary probes actually exercise the stated player-control loop."""
    evidence = _collect_loop_evidence(lines)
    if not any(
        (
            evidence.progression,
            evidence.liberation,
            evidence.first_foundry,
            evidence.woodworking,
            evidence.fieldwork,
            evidence.power,
            evidence.settlement,
            evidence.survival,
        )
    ):
        return None

    extracted = _fieldwork_extracted(evidence.fieldwork)
    delegate, reinvest = _delegate_reinvest_evidence(
        evidence.progression, evidence.settlement
    )
    return (
        "PLAYER LOOP EVIDENCE "
        f"{_evidence_shape(evidence)} "
        f"{_observe_infer_evidence(evidence.fieldwork, extracted)} "
        f"{_prepare_invest_evidence(evidence.woodworking, evidence.power, evidence.settlement, evidence.liberation_kit, evidence.survey_campaigns, evidence.shortfall_recoveries)} "
        f"{_extract_evidence(evidence.fieldwork, evidence.liberation, extracted)} "
        f"{_thermal_bootstrap_evidence(evidence.first_foundry)} "
        f"{_world_feedback_evidence(lines, evidence.fieldwork)} "
        f"{_survival_adaptation_evidence(evidence.survival, evidence.power_projects)} "
        f"{_maintenance_evidence(evidence.woodworking, evidence.power_projects)} "
        f"{delegate} "
        f"{reinvest} "
        f"{_choice_diversity(evidence.woodworking, evidence.fieldwork, evidence.power, evidence.settlement, evidence.survival, evidence.survey_campaigns, evidence.bulk_crossovers)}"
    )
