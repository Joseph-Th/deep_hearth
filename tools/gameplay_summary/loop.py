"""Cross-domain evidence for the ordinary player-control loop."""

from __future__ import annotations

import re
from dataclasses import dataclass

from .common import field


@dataclass(frozen=True)
class _LoopEvidenceLines:
    wilderness: list[str]
    progression: list[str]
    progression_reviews: list[str]
    progression_local_campaigns: list[str]
    liberation: list[str]
    liberation_kit: list[str]
    first_foundry: list[str]
    woodworking: list[str]
    fieldwork: list[str]
    fieldwork_expeditions: list[str]
    fieldwork_local_orders: list[str]
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
        wilderness=_lines_with_prefix(lines, "WILDERNESS EXPERIENCE "),
        progression=_lines_with_prefix(lines, "PROGRESSION EXPERIENCE "),
        progression_reviews=_lines_with_prefix(lines, "PROGRESSION REVIEW "),
        progression_local_campaigns=_lines_with_prefix(lines, "PROGRESSION LOCAL CAMPAIGN "),
        liberation=_lines_with_prefix(lines, "LIBERATION EXPERIENCE "),
        liberation_kit=_lines_with_prefix(lines, "LIBERATION KIT ACQUISITION "),
        first_foundry=_lines_with_prefix(lines, "FIRST FOUNDRY EXPERIENCE "),
        woodworking=_lines_with_prefix(lines, "WOODWORKING EXPERIENCE "),
        fieldwork=_lines_with_prefix(lines, "FIELDWORK EXPERIENCE "),
        fieldwork_expeditions=_lines_with_prefix(lines, "FIELDWORK EXPEDITION "),
        fieldwork_local_orders=_lines_with_prefix(lines, "FIELDWORK LOCAL ORDER "),
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


def _bootstrap_boundary_evidence(
    wilderness: list[str], liberation_kit: list[str]
) -> str:
    opening_surface_generation = sum(
        " generation=[surface:ordinary " in line for line in wilderness
    )
    opening_water_generation = sum(
        " water:ordinary]" in line for line in wilderness
    )
    selected = [line for line in liberation_kit if " branch=selected " in line]
    runtime_gathering = sum(
        " runtime-surface-gathering-proved=true " in line for line in selected
    )
    ordinary_generation = sum(
        " ordinary-world-source-generation-proved=true " in line
        for line in selected
    )
    return (
        "bootstrap-boundary=["
        f"opening=[surface:{opening_surface_generation}/{len(wilderness)} "
        f"water:{opening_water_generation}/{len(wilderness)}] "
        f"liberation=[selected-builds:{len(selected)}/{len(liberation_kit)} "
        f"gather:{runtime_gathering}/{len(selected)} "
        f"world-gen:{ordinary_generation}/{len(selected)}]]"
    )


def _evidence_shape(evidence: _LoopEvidenceLines) -> str:
    single_state = sum(
        " continuity=single-state " in line for line in evidence.progression_reviews
    )
    exact_local = (
        f" exact-local-fieldwork:{len(evidence.fieldwork_expeditions)}"
        if evidence.fieldwork_expeditions
        else ""
    )
    return (
        "evidence-shape=["
        f"wilderness:{len(evidence.wilderness)} "
        f"single-state-progression:{single_state}/{len(evidence.progression_reviews)} "
        f"domain-episodes:survival{len(evidence.survival)}/"
        f"woodworking{len(evidence.woodworking)}/fieldwork{len(evidence.fieldwork)}/"
        f"power{len(evidence.power)}/settlement{len(evidence.settlement)}/"
        f"liberation{len(evidence.liberation)}/"
        f"first-foundry{len(evidence.first_foundry)}{exact_local}]"
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


def _extract_evidence(
    progression_local_campaigns: list[str],
    fieldwork: list[str],
    liberation: list[str],
    extracted: int,
    fieldwork_expeditions: list[str],
    fieldwork_local_orders: list[str],
) -> str:
    completed_liberation = sum(
        (match := re.search(r"\bnative-copper:(\d+)mg", line)) is not None
        and int(match.group(1)) > 0
        for line in liberation
    )
    exact_local = ""
    if fieldwork_expeditions:
        hauled = sum(
            " cache=[returned:" in line and " matter=conserved" in line
            for line in fieldwork_expeditions
        )
        exact_local = f" exact-local-haul:{hauled}/{len(fieldwork_expeditions)}"
    generated_local = ""
    if fieldwork_local_orders:
        hauled = sum(
            " cache=[returned:" in line
            and " tools-dropped:true] matter=conserved" in line
            for line in fieldwork_local_orders
        )
        generated_local = (
            f" generated-local-haul:{hauled}/{len(fieldwork_local_orders)}"
        )
    progression_local = ""
    if progression_local_campaigns:
        hauled = sum(
            " evidence=ordinary-exact-local " in line
            and " convergence=[both-upgrades:true " in line
            and " matter=conserved" in line
            for line in progression_local_campaigns
        )
        progression_local = (
            f" progression-local-campaign:{hauled}/{len(progression_local_campaigns)}"
        )
    return (
        f"extract=[fieldwork:{extracted}/{len(fieldwork)}{progression_local}"
        f"{exact_local}{generated_local} liberation-native-copper:{completed_liberation}/{len(liberation)}]"
    )


def _lived_shared_power_evidence(
    lived: list[str], treadle_entry: list[int]
) -> str | None:
    if not lived or not treadle_entry:
        return None
    assert len(lived) == 1
    portfolio = lived[0]
    families = re.search(r"\bfamilies=(\d+)", portfolio)
    productive_batches = re.search(r"\bproductive-batches=(\d+)", portfolio)
    charge_events = re.search(r"\bcharge-events=(\d+)", portfolio)
    active = re.search(r"\bcharge-attention=(\d+)t", portfolio)
    delegated = re.search(r"\bdelegated=(\d+)t", portfolio)
    concurrency = re.search(r"\bshared-bank-concurrency=([^ ]+)", portfolio)
    if None in (families, productive_batches, charge_events, active, delegated):
        raise AssertionError("settlement lived portfolio lost shared-power telemetry")
    charges = int(charge_events.group(1))
    threshold = min(treadle_entry)
    return (
        "shared-power=["
        f"lived:1 families:{families.group(1)} productive-batches:{productive_batches.group(1)} "
        f"charges:{charges} provider:treadle provider-basis:matched-market-frontier "
        f"treadle-entry:{threshold} crosses:{str(charges >= threshold).lower()} "
        f"active:{active.group(1)}t delegated:{delegated.group(1)}t "
        f"concurrency:{concurrency.group(1) if concurrency is not None else 'unspecified'}]"
    )


def _shared_power_portfolio_evidence(lines: list[str]) -> str:
    """Expose when shared workshop demand crosses a provider-investment threshold.

    A single later-settlement lumber order can rationally stay on a crank behind the large timber
    bank, while the same bank serves enough total workshop work to justify a treadle. Keep that
    portfolio interaction explicit so the report does not make provider economics look
    contradictory across otherwise valid episodes.
    """

    lived = [line for line in lines if line.startswith("SETTLEMENT PORTFOLIO EXPERIENCE ")]
    bank_charge_events: list[int] = []
    for line in lines:
        if not line.startswith("SETTLEMENT STORAGE EXPERIENCE "):
            continue
        match = re.search(
            r"\bbank:\[[^\]]*portfolio-charges:(\d+)",
            line,
        )
        if match is not None:
            bank_charge_events.append(int(match.group(1)))

    treadle_entry: list[int] = []
    for line in lines:
        if not line.startswith("POWER SETTLEMENT "):
            continue
        if " copper-policy:preserve-for-other-uses " not in line:
            continue
        frontier = re.search(r"\bmarket-frontier:([^\s\]]+)", line)
        if frontier is None:
            continue
        for entry in frontier.group(1).split(","):
            charge, separator, provider = entry.partition(":")
            if separator and provider == "treadle" and charge.isdecimal():
                treadle_entry.append(int(charge))
                break

    if lived_evidence := _lived_shared_power_evidence(lived, treadle_entry):
        return lived_evidence

    if not bank_charge_events or not treadle_entry:
        return ""
    crosses = max(bank_charge_events) >= min(treadle_entry)
    return (
        "shared-power-projection=["
        f"bank-portfolio-charges:{min(bank_charge_events)}..{max(bank_charge_events)} "
        f"treadle-entry:{min(treadle_entry)}..{max(treadle_entry)} "
        f"crosses:{str(crosses).lower()}]"
    )


def _thermal_bootstrap_evidence(first_foundry: list[str]) -> str:
    foundry_builds = sum(" foundry-build=true " in line for line in first_foundry)
    batch_foundry = sum(
        " continuation=settlement-batch-proven" in line for line in first_foundry
    )
    return f"thermal-bootstrap={batch_foundry}/{foundry_builds}"


def _agency_growth_evidence(evidence: _LoopEvidenceLines) -> str:
    self_sufficient_camps = sum(
        " warning-safe:true]" in line
        and " water=[observed:" in line
        and " provisioning=[" in line
        for line in evidence.wilderness
    )
    rational_mobility = sum(
        " storage-choice=[selected:defer " in line
        and " tool-choice=[selected:defer " in line
        and " goal=stay-mobile " in line
        for line in evidence.wilderness
    )
    camp_leverage = 0
    for line in evidence.wilderness:
        project = re.search(
            r"\bcamp-project=\[.*?\blogistics-steps:(\d+) "
            r"source:(\d+)mg->(\d+)mg",
            line,
        )
        if project is None:
            continue
        logistics_steps = int(project.group(1))
        source_before = int(project.group(2))
        source_after = int(project.group(3))
        if logistics_steps > 0 and source_before > source_after:
            camp_leverage += 1
    worksite_projects = [
        line for line in evidence.wilderness if " goal=worksite-cache " in line
    ]
    worksite_construction = sum(
        re.search(
            r"\bcamp-project=\[.*?\bgrounded-support:true:(\d+)mg/(\d+)t\]",
            line,
        )
        is not None
        for line in worksite_projects
    )
    worksite_construction_evidence = (
        f"worksite-construction:{worksite_construction}/{len(worksite_projects)} "
        if worksite_projects
        else ""
    )
    worksite_warmth = 0
    for line in worksite_projects:
        warmth = re.search(
            r"\bwarmth=\[fire:true .*?comfort:(\d+)->(\d+)ppm heat-remaining:(\d+)nJ\]",
            line,
        )
        if (
            warmth is not None
            and int(warmth.group(2)) > int(warmth.group(1))
            and int(warmth.group(3)) > 0
        ):
            worksite_warmth += 1
    worksite_warmth_evidence = (
        f"camp-warmth:{worksite_warmth}/{len(worksite_projects)} "
        if worksite_projects
        else ""
    )
    workload_sensitive_processing = sum(
        " processing-market=[one-cycle:manual " in line
        and " disclosed:mechanized]" in line
        for line in evidence.progression
    )
    exact_local_progression = ""
    if evidence.progression_local_campaigns:
        completed = sum(
            " evidence=ordinary-exact-local " in line
            and " mechanization=[line-local:true " in line
            and " convergence=[both-upgrades:true " in line
            and " matter=conserved" in line
            for line in evidence.progression_local_campaigns
        )
        exact_local_progression = (
            f" generated-exact-local-progression-campaigns:{completed}/{len(evidence.progression_local_campaigns)}"
        )
    delegated_orders = sum(
        (match := re.search(r"\bdelegated:(\d+)t", line)) is not None
        and int(match.group(1)) > 0
        for line in evidence.settlement
    )
    electrical_foundries = sum(
        " foundry-build=true " in line
        and "electrical-prime-mover=[base:treadle-built conversion:treadle-dynamo]" in line
        for line in evidence.first_foundry
    )
    foundry_builds = sum(" foundry-build=true " in line for line in evidence.first_foundry)
    exact_local = ""
    if evidence.fieldwork_expeditions:
        completed = sum(
            " cache=[returned:" in line
            and " tool-dropped:true] matter=conserved" in line
            for line in evidence.fieldwork_expeditions
        )
        exact_local = (
            f" exact-local-field-expedition:{completed}/{len(evidence.fieldwork_expeditions)}"
        )
    generated_exact_local = ""
    if evidence.fieldwork_local_orders:
        conserved = sum(
            " evidence=ordinary-exact-local " in line
            and " tools-dropped:true] matter=conserved" in line
            for line in evidence.fieldwork_local_orders
        )
        generated_exact_local = (
            f" generated-exact-local-field-orders:{conserved}/{len(evidence.fieldwork_local_orders)}"
        )
    return (
        "agency-growth=["
        f"opening-stabilized:{self_sufficient_camps}/{len(evidence.wilderness)} "
        f"rational-mobility:{rational_mobility}/{len(evidence.wilderness)} "
        f"camp-leverage:{camp_leverage}/{len(evidence.wilderness)} "
        f"{worksite_construction_evidence}"
        f"{worksite_warmth_evidence}"
        f"workload-sensitive-processing:{workload_sensitive_processing}/{len(evidence.progression)} "
        f"delegated-settlement-orders:{delegated_orders}/{len(evidence.settlement)} "
        f"electrical-foundry:{electrical_foundries}/{foundry_builds}"
        f"{exact_local_progression}{exact_local}{generated_exact_local}]"
    )


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
    wilderness = [line for line in lines if line.startswith("WILDERNESS EXPERIENCE ")]
    opening_deferred = sum(" material:none " in line for line in wilderness)
    opening_timber = sum(" material:timber " in line for line in wilderness)
    opening_stone = sum(" material:stone " in line for line in wilderness)
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
        "opening-material-response=["
        f"defer:{opening_deferred} timber:{opening_timber} stone:{opening_stone}] "
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
            evidence.wilderness,
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
    shared_power = _shared_power_portfolio_evidence(lines)
    return (
        "PLAYER LOOP EVIDENCE "
        "role=self-reliant-field-engineer "
        f"{_evidence_shape(evidence)} "
        f"{_agency_growth_evidence(evidence)} "
        f"{_continuity_evidence(evidence)} "
        f"{_bootstrap_boundary_evidence(evidence.wilderness, evidence.liberation_kit)} "
        f"{_observe_infer_evidence(evidence.fieldwork, extracted)} "
        f"{_prepare_invest_evidence(evidence.woodworking, evidence.power, evidence.settlement, evidence.liberation_kit, evidence.survey_campaigns, evidence.shortfall_recoveries, evidence.preservation_paths)} "
        f"{shared_power + ' ' if shared_power else ''}"
        f"{_extract_evidence(evidence.progression_local_campaigns, evidence.fieldwork, evidence.liberation, extracted, evidence.fieldwork_expeditions, evidence.fieldwork_local_orders)} "
        f"{_thermal_bootstrap_evidence(evidence.first_foundry)} "
        f"{_world_feedback_evidence(lines, evidence.fieldwork)} "
        f"{_survival_adaptation_evidence(evidence.survival, evidence.power_projects, evidence.preservation_paths)} "
        f"{_maintenance_evidence(evidence.woodworking, evidence.power_projects)} "
        f"{delegate} "
        f"{reinvest} "
        f"{_choice_diversity(evidence.woodworking, evidence.fieldwork, evidence.power_projects, evidence.settlement, evidence.survival, evidence.survey_campaigns, evidence.bulk_crossovers)}"
    )
