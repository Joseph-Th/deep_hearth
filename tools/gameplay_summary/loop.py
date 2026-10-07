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
    preservation_paths: list[str]
    survey_campaigns: list[str]
    shortfall_recoveries: list[str]
    bulk_crossovers: list[str]


def _lines_with_prefix(lines: list[str], prefix: str) -> list[str]:
    return [line for line in lines if line.startswith(prefix)]


def _collect_loop_evidence(lines: list[str]) -> _LoopEvidenceLines:
    return _LoopEvidenceLines(
        progression=_lines_with_prefix(lines, "PROGRESSION EXPERIENCE "),
        progression_reviews=_lines_with_prefix(lines, "PROGRESSION REVIEW "),
        liberation=_lines_with_prefix(lines, "LIBERATION EXPERIENCE "),
        liberation_kit=_lines_with_prefix(lines, "LIBERATION KIT ACQUISITION "),
        first_foundry=_lines_with_prefix(lines, "FIRST FOUNDRY EXPERIENCE "),
        woodworking=_lines_with_prefix(lines, "WOODWORKING EXPERIENCE "),
        fieldwork=_lines_with_prefix(lines, "FIELDWORK EXPERIENCE "),
        power=_lines_with_prefix(lines, "POWER PROVIDER EXPERIENCE "),
        settlement=_lines_with_prefix(lines, "SETTLEMENT EXPERIENCE "),
        power_projects=_lines_with_prefix(lines, "POWER PROJECT EXPERIENCE "),
        survival=_lines_with_prefix(lines, "SURVIVAL EXPERIENCE "),
        preservation_paths=_lines_with_prefix(lines, "SURVIVAL PRESERVATION PATH "),
        survey_campaigns=_lines_with_prefix(lines, "FIELDWORK SURVEY CAMPAIGN "),
        shortfall_recoveries=_lines_with_prefix(
            lines, "FIELDWORK INITIAL SHORTFALL RECOVERY "
        ),
        bulk_crossovers=_lines_with_prefix(lines, "FIELDWORK BULK CROSSOVER "),
    )


def _bootstrap_boundary_evidence(liberation_kit: list[str]) -> str:
    selected = [line for line in liberation_kit if " branch=selected " in line]
    surface_source = sum(
        " raw-origin=controlled-finite-surface " in line for line in selected
    )
    runtime_gathering = sum(
        " runtime-surface-gathering-proved=true " in line for line in selected
    )
    ordinary_generation = sum(
        " ordinary-world-source-generation-proved=true " in line
        for line in selected
    )
    return (
        "bootstrap-boundary=["
        f"selected-builds:{len(selected)}/{len(liberation_kit)} "
        f"fixture-surface:{surface_source}/{len(selected)} "
        f"gather:{runtime_gathering}/{len(selected)} "
        f"world-gen:{ordinary_generation}/{len(selected)}]"
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


def _continuity_evidence(evidence: _LoopEvidenceLines) -> str:
    progression_single_state = sum(
        " continuity=single-state " in line for line in evidence.progression_reviews
    )
    settlement_same_state_followup = sum(" followup=[" in line for line in evidence.settlement)
    selected_liberation = [
        line for line in evidence.liberation_kit if " branch=selected " in line
    ]
    liberation_carryover = sum(
        " continuity=separate-episode-inherited-progression-line " in line
        for line in selected_liberation
    )
    foundry_carryover = sum(
        " continuity=separate-episode " in line and " inherited-workshop=[" in line
        for line in evidence.first_foundry
    )
    return (
        "continuity=["
        f"progression:{progression_single_state}/{len(evidence.progression_reviews)} "
        f"settlement-repeat:{settlement_same_state_followup}/{len(evidence.settlement)} "
        f"liberation-carry:{liberation_carryover}/{len(selected_liberation)} "
        f"foundry-carry:{foundry_carryover}/{len(evidence.first_foundry)} "
        "cross-era:modeled-handoffs]"
    )


def _observe_infer_evidence(fieldwork: list[str], extracted: int) -> str:
    return (
        "observe-infer=["
        f"evidence-gated-extraction:{extracted}/{len(fieldwork)} "
        f"reserve-knowledge-changed-plan:{_reserve_knowledge_changed_plan(fieldwork)}/{len(fieldwork)} "
        f"avoided-tool-overinvestment:{sum('resource-knowledge-effect=changed-tool' in line for line in fieldwork)}/{len(fieldwork)}]"
    )


def _extract_evidence(fieldwork: list[str], liberation: list[str], extracted: int) -> str:
    completed_liberation = sum(
        (match := re.search(r"\bnative-copper:(\d+)mg", line)) is not None
        and int(match.group(1)) > 0
        for line in liberation
    )
    return (
        f"extract=[fieldwork:{extracted}/{len(fieldwork)} "
        f"liberation-native-copper:{completed_liberation}/{len(liberation)}]"
    )


def _thermal_bootstrap_evidence(first_foundry: list[str]) -> str:
    foundry_builds = sum(" foundry-build=true " in line for line in first_foundry)
    batch_foundry = sum(
        " continuation=settlement-batch-proven" in line for line in first_foundry
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
    power_projects: list[str],
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
    primitive_power_choices = {
        selected
        for line in power_projects
        if " era=primitive " in line
        and (selected := field(line, "selected")) is not None
    }
    settlement_power_choices = {
        selected
        for line in power_projects
        if " era=settlement " in line
        and (selected := field(line, "selected")) is not None
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
        f"woodworking:{len(woodworking_choices)} "
        f"fieldwork-selected:{len(fieldwork_tools)} "
        f"bulk-crossover-tools:{len(bulk_tools)} "
        f"power-market=[primitive:{len(primitive_power_choices)} settlement:{len(settlement_power_choices)}] "
        f"settlement-investment:{len(settlement_choices)} "
        f"survey-strategy:{len(survey_strategies)} "
        f"preservation:{len(preservation_policies)}]"
    )


def _prepare_invest_evidence(
    woodworking: list[str],
    power: list[str],
    settlement: list[str],
    liberation_kit: list[str],
    survey_campaigns: list[str],
    shortfall_recoveries: list[str],
    preservation_paths: list[str],
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
    selected_liberation_builds = [
        line for line in liberation_kit if " branch=selected " in line
    ]
    progression_assets_reused = sum(
        " continuity=separate-episode-inherited-progression-line " in line
        for line in selected_liberation_builds
    )
    preservation_investment = ""
    if preservation_paths:
        preservation_builds = sum(" action=build " in line for line in preservation_paths)
        preservation_investment = (
            f"preservation-build:{preservation_builds}/{len(preservation_paths)} "
        )
    return (
        "prepare-invest=["
        f"liberation-ext:{len(selected_liberation_builds)}/{len(liberation_kit)} "
        f"progression-reuse:{progression_assets_reused}/{len(selected_liberation_builds)} "
        f"woodworking-tool:{invested_woodworking}/{len(woodworking)} "
        f"power-market:{len(power)}/{len(power)} "
        f"settlement-machine:{settlement_builds}/{len(settlement)} "
        f"{preservation_investment}"
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
    settlement_reinvested = sum(" reinvested:true " in line for line in settlement)
    settlement_reused = sum(
        " machine-owned-before:true reinvested:false " in line for line in settlement
    )
    settlement_stayed_direct = sum(
        " machine-owned-before:false reinvested:false " in line for line in settlement
    )
    delegate = (
        "delegate=["
        f"mechanized-processing:{mechanized}/{len(progression)} "
        f"settlement-orders:{settlement_delegated}/{len(settlement)} "
        f"order-attention-saved:{_attention_saved_span(progression)} "
        "stockpile-alt=[selected:false "
        f"overlap:{overlap_span} open:{room_span}]]"
    )
    reinvest = (
        "reassess-reinvest=["
        f"progression:{reinvested}/{len(progression)} "
        f"settlement=[reinvest:{settlement_reinvested} "
        f"reuse:{settlement_reused} direct:{settlement_stayed_direct}]]"
    )
    return delegate, reinvest


def _survival_adaptation_evidence(
    survival: list[str], power_projects: list[str], preservation_paths: list[str]
) -> str:
    followup_surveys = sum(" followup-survey:true:" in line for line in survival)
    continuations = sum(" continuation:true " in line for line in survival)
    followup_drinks = sum(" followup-drink:true:" in line for line in survival)
    power_drinks = sum(" power-drink:true:" in line for line in survival)
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
    preservation = ""
    if preservation_paths:
        preservation = (
            " preservation=["
            f"build:{sum(' action=build ' in line for line in preservation_paths)} "
            f"decline:{sum(' action=decline ' in line for line in preservation_paths)} "
            f"fresh:{sum(' outcome=fresh ' in line for line in preservation_paths)} "
            f"spoiled:{sum(' outcome=spoiled ' in line for line in preservation_paths)}]"
        )
    return (
        "survive-adapt=["
        "short-expedition=["
        f"serving-floor:{serving_floor} "
        f"followup-surveys:{followup_surveys}/{len(survival)} "
        f"continuations:{continuations}/{followup_surveys} "
        f"followup-drinks:{followup_drinks}/{followup_surveys} "
        f"stored-work:{executed_power}/{opportunity_power} "
        f"power-drinks:{power_drinks}/{opportunity_power} "
        f"policy:task-floor{task_floor}/working-reserve{working_reserve}] "
        f"sustained-project-provisioning=[breaks:{sum(value > 0 for value in project_breaks)}/{len(project_breaks)} "
        f"events:{sum(project_breaks)} drinks:{sum(project_drinks)} meals:{sum(project_meals)}] "
        f"warning-safe:{sum(' warning-safe:true' in line for line in survival)}/{len(survival)}"
        f"{preservation}]"
    )


def _selected_woodworking_services(line: str) -> int:
    choice = field(line, "choice")
    if choice in ("stone-adze", "reinforced-adze"):
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
    primitive_active_share = []
    settlement_active_share = []
    primitive_elapsed_share = []
    settlement_elapsed_share = []
    for line in power_projects:
        match = re.search(r"maintenance=\[services:(\d+)", line)
        if match is not None:
            power_service_counts.append(int(match.group(1)))
        attention = re.search(
            r"\bactive-attention:(\d+)t .*?maintenance=\[services:\d+ "
            r"preparation:(\d+)t service:(\d+)t",
            line,
        )
        era = re.search(r"\bera=([^\s]+)", line)
        if attention is None or era is None:
            continue
        active, preparation, service = map(int, attention.groups())
        maintenance = preparation + service
        if active == 0:
            continue
        active_share = (maintenance * 100 + active // 2) // active
        elapsed = re.search(r"\belapsed:(\d+)t", line)
        elapsed_share = None
        if elapsed is not None and int(elapsed.group(1)) > 0:
            elapsed_ticks = int(elapsed.group(1))
            elapsed_share = (maintenance * 100 + elapsed_ticks // 2) // elapsed_ticks
        if era.group(1) == "primitive":
            primitive_active_share.append(active_share)
            if elapsed_share is not None:
                primitive_elapsed_share.append(elapsed_share)
        elif era.group(1) == "settlement":
            settlement_active_share.append(active_share)
            if elapsed_share is not None:
                settlement_elapsed_share.append(elapsed_share)

    def percent_span(values: list[int]) -> str:
        return f"{min(values)}..{max(values)}%" if values else "n/a"

    return (
        "maintain-recover=["
        f"woodworking-service-worlds:{sum(count > 0 for count in service_counts)}/{len(woodworking)} "
        f"woodworking-service-events:{sum(service_counts)} "
        f"power-market-projects-with-service:{sum(count > 0 for count in power_service_counts)}/{len(power_service_counts)} "
        f"power-market-service-events:{sum(power_service_counts)} "
        f"power-market-maintenance-share=[active=[primitive:{percent_span(primitive_active_share)} "
        f"settlement:{percent_span(settlement_active_share)}] "
        f"elapsed=[primitive:{percent_span(primitive_elapsed_share)} "
        f"settlement:{percent_span(settlement_elapsed_share)}]]]"
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
        (
            (build := re.search(r"\btool-builds:(\d+)", line)) is not None
            and int(build.group(1)) > 0
        )
        or (
            (upgrade := re.search(r"\btool-upgrades:(\d+)", line)) is not None
            and int(upgrade.group(1)) > 0
        )
        for line in shortfall_recoveries
    )
    barren_sites = sum(
        int(match.group(1))
        for line in shortfall_recoveries
        if (match := re.search(r"\bbarren-sites:(\d+)", line)) is not None
    )
    shortfall_horizon_exhausted = sum(
        " terminal=planned-search-horizon-exhausted" in line
        for line in shortfall_recoveries
    )
    shortfall_local_exhausted = sum(
        " terminal=local-search-area-exhausted" in line for line in shortfall_recoveries
    )
    depletion_retooled = sum(
        line.startswith("FIELDWORK DEPLETION RECOVERY ")
        and (
            " mining-tool-reused=false " in line
            or " mining-tool-upgraded=true " in line
        )
        for line in lines
    )
    search_ended = shortfall_horizon_exhausted + shortfall_local_exhausted
    return (
        "world-feedback=["
        f"shortfall:{initial_supply_ended}/{len(fieldwork)} "
        f"rerouted:{initial_reroute_proved}/{initial_supply_ended} "
        f"indexed:{indexed_shortfall}/{len(shortfall_recoveries)} "
        f"geology-shift:{geology_changed}/{len(shortfall_recoveries)} "
        f"retooled:{retooled}/{len(shortfall_recoveries)} "
        f"barren:{barren_sites} "
        f"search-ended:{search_ended}/{len(shortfall_recoveries)} "
        f"depleted:{depleted}/{len(eligible)} "
        f"depletion-rerouted:{reroute_proved}/{depleted} "
        f"depletion-retooled:{depletion_retooled}/{depleted}]"
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
        f"{_continuity_evidence(evidence)} "
        f"{_bootstrap_boundary_evidence(evidence.liberation_kit)} "
        f"{_observe_infer_evidence(evidence.fieldwork, extracted)} "
        f"{_prepare_invest_evidence(evidence.woodworking, evidence.power, evidence.settlement, evidence.liberation_kit, evidence.survey_campaigns, evidence.shortfall_recoveries, evidence.preservation_paths)} "
        f"{_extract_evidence(evidence.fieldwork, evidence.liberation, extracted)} "
        f"{_thermal_bootstrap_evidence(evidence.first_foundry)} "
        f"{_world_feedback_evidence(lines, evidence.fieldwork)} "
        f"{_survival_adaptation_evidence(evidence.survival, evidence.power_projects, evidence.preservation_paths)} "
        f"{_maintenance_evidence(evidence.woodworking, evidence.power_projects)} "
        f"{delegate} "
        f"{reinvest} "
        f"{_choice_diversity(evidence.woodworking, evidence.fieldwork, evidence.power_projects, evidence.settlement, evidence.survival, evidence.survey_campaigns, evidence.bulk_crossovers)}"
    )
