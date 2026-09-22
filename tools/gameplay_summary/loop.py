"""Cross-domain evidence for the ordinary player-control loop."""

from __future__ import annotations

import re

from .common import field


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
    survival: list[str],
    survey_campaigns: list[str],
    bulk_crossovers: list[str],
) -> str:
    woodworking_choices = {
        choice
        for line in woodworking
        if (choice := field(line, "choice")) is not None
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
        f"fieldwork-selected:{len(fieldwork_tools)} "
        f"fieldwork-catalog:4 bulk-crossover-tools:{len(bulk_tools)}/2 "
        f"power-market:{len(power_choices)}/2 "
        f"survey-strategy:{len(survey_strategies)}/2 "
        f"preservation:{len(preservation_policies)}/5]"
    )


def _prepare_invest_evidence(
    woodworking: list[str],
    power: list[str],
    survey_campaigns: list[str],
) -> str:
    invested_woodworking = sum(
        field(line, "choice") not in (None, "bare-hands") for line in woodworking
    )
    indexed_investments = sum(
        " selected=indexed-channel " in line for line in survey_campaigns
    )
    return (
        "prepare-invest=["
        f"woodworking-tool:{invested_woodworking}/{len(woodworking)} "
        f"power-market:{len(power)}/{len(power)} "
        f"knowledge-tech:{indexed_investments}/{len(survey_campaigns)}]"
    )


def _delegate_reinvest_evidence(progression: list[str]) -> tuple[str, str]:
    mechanized = sum(
        "processing-investment=[selected:mechanized" in line for line in progression
    )
    reinvested = sum(
        re.search(r"\bselected-reinvestment=\[completed(?:\s|\])", line) is not None
        for line in progression
    )
    delegate = (
        "delegate=["
        f"mechanized-processing:{mechanized}/{len(progression)} "
        f"attention-saved:{_attention_saved_span(progression)}]"
    )
    reinvest = f"reassess-reinvest=[completed:{reinvested}/{len(progression)}]"
    return delegate, reinvest


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
    return (
        "world-feedback=["
        f"initial-supply-ended:{initial_supply_ended}/{len(fieldwork)} "
        f"initial-shortfall-reroute-proved:{initial_reroute_proved}/{initial_supply_ended} "
        f"known-site-depletion:{depleted}/{len(eligible)} "
        f"depletion-reroute-proved:{reroute_proved}/{depleted} "
        f"horizon-live:{sum(' terminal=horizon-live-target ' in line for line in eligible)}/{len(eligible)}]"
    )


def player_loop_evidence(lines: list[str]) -> str | None:
    """Summarize whether ordinary probes actually exercise the stated player-control loop."""

    progression = [line for line in lines if line.startswith("PROGRESSION EXPERIENCE ")]
    liberation = [
        line for line in lines if line.startswith("LIBERATION FRONTIER CAPABILITY ")
    ]
    woodworking = [line for line in lines if line.startswith("WOODWORKING EXPERIENCE ")]
    fieldwork = [line for line in lines if line.startswith("FIELDWORK EXPERIENCE ")]
    power = [line for line in lines if line.startswith("POWER PROVIDER EXPERIENCE ")]
    survival = [line for line in lines if line.startswith("SURVIVAL EXPERIENCE ")]
    survey_campaigns = [
        line for line in lines if line.startswith("FIELDWORK SURVEY CAMPAIGN ")
    ]
    bulk_crossovers = [
        line for line in lines if line.startswith("FIELDWORK BULK CROSSOVER ")
    ]
    if not any((progression, liberation, woodworking, fieldwork, power, survival)):
        return None

    extracted = _fieldwork_extracted(fieldwork)
    delegate, reinvest = _delegate_reinvest_evidence(progression)
    return (
        "PLAYER LOOP EVIDENCE "
        f"observe-infer=[evidence-gated-extraction:{extracted}/{len(fieldwork)} "
        f"reserve-knowledge-changed-plan:{_reserve_knowledge_changed_plan(fieldwork)}/{len(fieldwork)} "
        f"avoided-tool-overinvestment:{sum('resource-knowledge-effect=changed-tool' in line for line in fieldwork)}/{len(fieldwork)}] "
        f"{_prepare_invest_evidence(woodworking, power, survey_campaigns)} "
        f"extract=[fieldwork:{extracted}/{len(fieldwork)} "
        f"liberation:{sum('selected-by-current-player=true' in line for line in liberation)}/{len(liberation)}] "
        f"{_world_feedback_evidence(lines, fieldwork)} "
        f"{delegate} "
        f"{reinvest} "
        f"{_choice_diversity(woodworking, fieldwork, power, survival, survey_campaigns, bulk_crossovers)}"
    )
