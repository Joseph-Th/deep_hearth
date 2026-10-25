"""Readable consequences of maintained gameplay decisions, not success counters.

Only use actor observations and committed outcomes. A maintained case is a specific
playthrough, not a sample median or a claim about unvisited/generated locations.
"""

from __future__ import annotations

import re

from .common import field


def _one_seeded(
    lines: list[str], prefix: str, seed: str
) -> str | None:
    matching = [
        line
        for line in lines
        if line.startswith(prefix) and field(line, "seed") == seed
    ]
    if len(matching) > 1:
        raise ValueError(f"duplicate {prefix.strip()} evidence for {seed}")
    return matching[0] if matching else None


def _one_record(lines: list[str], prefix: str) -> str | None:
    matching = [line for line in lines if line.startswith(prefix)]
    if len(matching) > 1:
        raise ValueError(f"duplicate {prefix.strip()} player experience evidence")
    return matching[0] if matching else None


def _capture(value: str, pattern: str, description: str) -> tuple[str, ...]:
    match = re.search(pattern, value)
    if match is None:
        raise ValueError(f"player experience lost {description}: {value[:120]}")
    return match.groups()


def _required(line: str, key: str) -> str:
    value = field(line, key)
    if value is None:
        raise ValueError(f"player experience evidence lost {key} in {line[:100]}")
    return value


def _kg(milligrams: str) -> str:
    if not milligrams.isdecimal():
        raise ValueError(f"unexpected player-experience mass: {milligrams}")
    centi_kilograms = (int(milligrams) + 5_000) // 10_000
    return f"{centi_kilograms // 100}.{centi_kilograms % 100:02d}kg"


def wilderness_player_experience(lines: list[str]) -> str | None:
    """Show the actor's rational first restraint and subsequent physical adaptation."""

    anchors = [
        line for line in lines
        if line.startswith("WILDERNESS EXPERIENCE ")
        and field(line, "sample") == "maintained"
    ]
    if not anchors:
        return None
    if len(anchors) != 1:
        raise ValueError("wilderness player experience requires one maintained actor")
    anchor = anchors[0]
    if (
        field(anchor, "start") != "rested"
        or field(anchor, "goal") != "stay-mobile"
        or field(anchor, "evidence") != "generated+exact-local"
        or "body:continuous" not in _required(anchor, "movement")
        or field(anchor, "episode") not in (None, "maintained/rested/stay-mobile")
    ):
        raise ValueError("maintained wilderness actor lost its ordinary continuity")
    timing = _required(anchor, "timing")
    forage = _required(anchor, "forage")
    water = _required(anchor, "water")
    carrying = _required(anchor, "carrying")
    storage = _required(anchor, "storage-choice")
    tool = _required(anchor, "tool-choice")
    material = _required(anchor, "material-choice")
    warmth = _required(anchor, "warmth")
    food_time = re.search(r"food-secured:\d+t/([0-9.]+[smh])", timing)
    gathered = re.search(r"secured:(\d+)mg left:(\d+)mg rest-runway:([0-9.]+[smh])", forage)
    load = re.search(r"peak:(\d+)mg capacity:(\d+)mg", carrying)
    water_choice = re.search(r"\bdecision:([a-z-]+)", water)
    storage_choice = re.search(r"\bselected:([a-z-]+)", storage)
    tool_choice = re.search(r"\bselected:([a-z-]+)", tool)
    material_choice = re.search(r"\bselected:([a-z+-]+)", material)
    fire = re.search(r"\bfire:(true|false)", warmth)
    if any(
        item is None
        for item in (
            food_time, gathered, load, water_choice, storage_choice,
            tool_choice, material_choice, fire,
        )
    ):
        raise ValueError("wilderness player experience lost observable choice/result evidence")
    assert food_time is not None and gathered is not None and load is not None
    assert water_choice is not None and storage_choice is not None
    assert tool_choice is not None and material_choice is not None and fire is not None
    opening = (
        "PLAYER EXPERIENCE wilderness witness=maintained/rested/stay-mobile "
        "mode=one-generated-world "
        "choice=[forage:" + _kg(gathered.group(1))
        + " water:" + water_choice.group(1)
        + " storage:" + storage_choice.group(1)
        + " tool:" + tool_choice.group(1)
        + " material:" + material_choice.group(1) + "] "
        "result=[first-food:" + food_time.group(1)
        + " unharvested-forage:" + _kg(gathered.group(2))
        + " carried:" + _kg(load.group(1)) + "/" + _kg(load.group(2))
        + " local-food-runway:" + gathered.group(3)
        + " fire:" + ("built" if fire.group(1) == "true" else "not-yet-built") + "]"
    )
    adaptation = _one_record(lines, "PLAYER ADAPTATION opening=stay-mobile ")
    if adaptation is None:
        return opening
    after = _capture(adaptation, r"\bafter:(\d+)t/([0-9.]+m)", "cold onset")
    reactive = _required(adaptation, "reactive")
    planned = _required(adaptation, "planned")
    if "stone-ring:built" not in reactive:
        raise ValueError("player adaptation lost the physically built campfire")
    fueled, comfort, no_fire = _capture(
        reactive,
        r"fueled:(\d+)t comfort:(\d+)ppm no-fire:(\d+)ppm",
        "actual warmth intervention",
    )
    earlier, planned_comfort, planned_runway, reactive_runway = _capture(
        planned,
        r"fueled:(\d+)t comfort:(\d+)ppm fire-runway:(\d+)t vs-reactive:(\d+)t",
        "matched earlier preparation",
    )
    if not (
        int(earlier) < int(fueled)
        and int(planned_comfort) > int(comfort) > int(no_fire)
        and int(planned_runway) < int(reactive_runway)
    ):
        raise ValueError("player adaptation lost its observed warmth and finite-fuel tradeoff")
    return (
        opening
        + " continuation=[chilly-after:" + after[1]
        + " reacted:built-fire-at-" + fueled + "t"
        + " comfort:" + comfort + "ppm vs-no-fire:" + no_fire + "ppm]"
        + " alternative=[matched-early-fire-at:" + earlier + "t"
        + " comfort:" + planned_comfort + "ppm"
        + " fuel-runway:" + planned_runway + "t-vs-late:" + reactive_runway + "t]"
    )


def fieldwork_exact_local_player_experience(lines: list[str]) -> str | None:
    """A separate, actually walked field expedition, not the proxy campaign's body."""

    record = _one_record(lines, "FIELDWORK EXPEDITION ")
    if record is None:
        return None
    if (
        _required(record, "evidence") != "ordinary-exact-local"
        or "body:continuous-generated-terrain" not in _required(record, "route")
        or _required(record, "matter") != "conserved"
    ):
        raise ValueError("fieldwork expedition lost executed body or matter evidence")
    edges, barren, trips = _capture(
        _required(record, "route"),
        r"edges:(\d+) barren-sites:(\d+) target-distance:\d+ haul-trips:(\d+)",
        "physical expedition path",
    )
    inspections, detailed, lower, upper = _capture(
        _required(record, "survey"),
        r"inspections:(\d+) detailed:(\d+).*observed-mass:(\d+)\.\.(\d+)mg",
        "surveyed uncertainty",
    )
    order, batches, ticks, complete = _capture(
        _required(record, "extraction"),
        r"order:(\d+)mg batches:(\d+) ticks:(\d+) complete:(true|false)",
        "executed mining",
    )
    capacity, travel_ticks = _capture(
        _required(record, "haul"),
        r"capacity:(\d+)mg.*moving-control:(\d+)ct",
        "physical carrying and executed movement time",
    )
    returned, = _capture(
        _required(record, "cache"), r"returned:(\d+)mg", "returned custody"
    )
    if complete != "true" or returned != order:
        raise ValueError("fieldwork expedition does not evidence complete recovered delivery")
    return (
        "PLAYER EXPERIENCE fieldwork-expedition continuity=separate-generated-world"
        + " observe=[barren-sites:" + barren
        + " inspections:" + inspections + " detailed-surveys:" + detailed
        + " reserve:" + _kg(lower) + ".." + _kg(upper) + "]"
        + " act=[walked-edges:" + edges
        + " mined-batches:" + batches + " work:" + ticks + "t"
        + " haul-trips:" + trips + " capacity:" + _kg(capacity)
        + " moving-control:" + travel_ticks + "ct]"
        + " result=[recovered:" + _kg(returned) + " conserved:true]"
    )


def settlement_portfolio_player_experience(lines: list[str]) -> str | None:
    """Count useful *executed* parallel actions, not just machine-running time."""

    record = _one_record(lines, "SETTLEMENT PORTFOLIO EXPERIENCE ")
    if record is None:
        return None
    if (
        _required(record, "continuity") != "single-state"
        or _required(record, "matter") != "conserved"
    ):
        raise ValueError("settlement portfolio lost its single-state outcome")
    families = _required(record, "families")
    batches = _required(record, "productive-batches")
    charges = _required(record, "charge-events")
    charge_attention = _required(record, "charge-attention")
    delegated = _required(record, "delegated")
    elapsed = _required(record, "elapsed")
    component, maintenance, overlap, completed = _capture(
        _required(record, "parallel-maintenance"),
        r"component:([\w-]+) attention:(\d+)t overlap:(\d+)t completed:(true|false)",
        "actual parallel work",
    )
    if (
        completed != "true"
        or int(overlap) > int(maintenance)
        or int(overlap) > int(delegated.removesuffix("t"))
    ):
        raise ValueError("settlement portfolio claims unexecuted parallel player work")
    used, before, after, service_attention, followup_batches = _capture(
        _required(record, "service-option"),
        r"choice:defer-spare matched-bit-used:(true|false) spindle-condition:(\d+)->(\d+)ppm matched-service:(\d+)t pending-batches:(\d+)",
        "usable parallel-prepared replacement",
    )
    if used != "true" or int(after) <= int(before) or int(service_attention) <= 0:
        raise ValueError("settlement portfolio did not validate the matched spare service option")
    if int(followup_batches) != 0:
        raise ValueError("settlement portfolio deferred service with active follow-up demand")
    unobserved = int(delegated.removesuffix("t")) - int(overlap)
    return (
        "PLAYER EXPERIENCE settlement-workshop continuity=single-executed-worksite"
        + " investment=[shared-bank families:" + families + "]"
        + " operation=[batches:" + batches + " charge-cycles:" + charges
        + " charge-labor:" + charge_attention + " delegated:" + delegated
        + " elapsed:" + elapsed + "]"
        + " proved-parallel-work=[" + component + ":" + overlap + "t]"
        + " rational-defer=[spare-kept:true pending-batches:" + followup_batches
        + " matched-service:" + before + "->" + after
        + "ppm cost-not-spent:" + service_attention + "t]"
        + " unobserved-freed-time=" + str(unobserved) + "t"
    )


def scavenger_den_player_experience(lines: list[str]) -> str | None:
    """One actor deliberately leaves a finite food patch vulnerable, then raids the den."""

    records = [
        line for line in lines
        if line.startswith("PLAYER JOURNEY ")
        and field(line, "phase") == "raid-full-den"
    ]
    if not records:
        return None
    if len(records) != 1:
        raise ValueError("duplicate scavenger-den player experience evidence")
    record = records[0]
    if (
        _required(record, "world") != "ordinary-generated-opening"
        or _required(record, "matter-conserved") != "true"
        or "recover-cache:ordinary-pickup" not in _required(record, "choice")
    ):
        raise ValueError("scavenger-den experience lost physical recovery authority")
    route, offgraph, separation, unattended = _capture(
        _required(record, "observation-camp"),
        r"recorded-route:(\d+)legs off-graph:(\d+)legs separation:(\d+)voxels passive:(\d+)t",
        "actual retreat route",
    )
    source_before, source_after, hoard = _capture(
        _required(record, "observed"),
        r"source:(\d+)mg reduced-to:(\d+)mg cache:(\d+)mg",
        "locally observed natural depletion",
    )
    carried, reopened = _capture(
        _required(record, "consequence"),
        r"carried:(\d+)mg source-further-depleted:(\d+)mg",
        "ordinary recovery and resumed foraging",
    )
    if (
        int(route) < 2
        or int(offgraph) > int(route) - 2
        or int(separation) < 3
        or not (int(source_before) > int(source_after) > 0)
        or hoard != carried
        or int(reopened) <= 0
    ):
        raise ValueError("scavenger-den experience lost meaningful world feedback")
    return (
        "PLAYER EXPERIENCE scavenger-den continuity=one-generated-world"
        + " choice=[retreat:" + route + "legs off-graph:" + offgraph
        + "legs separation:" + separation + "voxels]"
        + " risk=[unattended:" + unattended + "t"
        + " source:" + _kg(source_before) + "->" + _kg(source_after) + "]"
        + " recovery=[den:" + _kg(carried)
        + " renewed-foraging:" + _kg(reopened) + " conserved:true]"
    )


def fieldwork_player_experience(lines: list[str]) -> str | None:
    """Reconstruct the maintained actor's risk, effort, result, and response.

    Do not read FIELDWORK FIXTURE DIAGNOSTIC: exact hidden reserves are not available
    when the actor chooses a tool. Report learned resource bounds as uncertainty,
    even when the evaluator knows how much ore the world contains.
    """

    anchors = [
        line
        for line in lines
        if line.startswith("FIELDWORK EXPERIENCE ") and field(line, "sample") == "anchor"
    ]
    if not anchors:
        return None
    if len(anchors) != 1:
        raise ValueError("fieldwork player experience requires one maintained anchor")
    anchor = anchors[0]
    seed = _required(anchor, "seed")
    decision = _one_seeded(lines, "FIELDWORK DECISION ", seed)
    if decision is None:
        # Short contract fixtures can exercise the ordinary summary without a complete
        # chronological playthrough. Never manufacture a journey from partial evidence.
        return None
    pacing = _one_seeded(lines, "FIELDWORK PACING ", seed)
    supply = _one_seeded(lines, "FIELDWORK SUPPLY ", seed)
    if pacing is None or supply is None:
        return None

    parts = [
        "PLAYER EXPERIENCE fieldwork witness=maintained-anchor mode=spatial-proxy",
        "choice=["
        f"observed-reserve:{_required(decision, 'observed-resource-mass')} "
        f"order:{_kg(_required(decision, 'requested').removesuffix('mg'))} "
        f"tool:{_required(decision, 'selected')}]",
        "first-ore=["
        f"after:{_required(pacing, 'first-ore')} "
        f"search:{_required(pacing, 'search')} "
        f"sampling-kit:{_required(pacing, 'sampling-tool')} "
        f"mining-kit:{_required(pacing, 'extraction-tool')} "
        f"working:{_required(pacing, 'extraction')} "
        f"recovered:{_kg(_required(pacing, 'output').removesuffix('mg'))}]",
        "local-result=["
        f"shortfall:{_kg(_required(supply, 'shortfall').removesuffix('mg'))} "
        f"stop:{_required(supply, 'stop')}]",
    ]
    recovery = _one_seeded(lines, "FIELDWORK INITIAL SHORTFALL RECOVERY ", seed)
    if recovery is not None:
        parts.append(
            "response=["
            f"additional-sites:{_required(recovery, 'sites-visited')} "
            f"search:{_required(recovery, 'search')} "
            f"fulfilled:{_kg(_required(recovery, 'fulfilled').removesuffix('mg'))} "
            f"remaining:{_kg(_required(recovery, 'remaining').removesuffix('mg'))} "
            f"outcome:{_required(recovery, 'terminal')}]"
        )
    assay = _one_seeded(lines, "FIELDWORK OPTIONAL ASSAY ", seed)
    if assay is not None:
        followup = _required(assay, "followup")
        alternative = (
            "alternative=[matched-executed-hand-assay "
            f"extra:{_required(assay, 'extra-attention')} "
            f"reserve:{_required(assay, 'visible-before')}->{_required(assay, 'visible-after')} "
            f"selected:{_required(assay, 'selected-next-action')} "
        )
        mined = re.fullmatch(
            r"searched:(\d+)t:located:(\d+)\.\.(\d+)mg:prepared:(\d+)t:"
            r"([^:]+):first-ore-after-assay-decision:(\d+)t:recovered:(\d+)mg:"
            r"extraction:(\d+)t:stop:([a-z-]+)",
            followup,
        )
        if mined is not None:
            direct_first_ore = (
                int(_required(pacing, "first-ore").split("t", 1)[0])
                - int(_required(pacing, "search").split("t", 1)[0])
                - int(_required(pacing, "sampling-tool").split("t", 1)[0])
            )
            alternative_first_ore = int(mined.group(6))
            alternative += (
                f"followup=[search:{mined.group(1)}t "
                f"reserve:{_kg(mined.group(2))}..{_kg(mined.group(3))} "
                f"tool:{mined.group(5)} preparation:{mined.group(4)}t "
                f"extracted:{_kg(mined.group(7))} stop:{mined.group(9)}] "
                f"first-ore-same-survey=[direct:{direct_first_ore}t "
                f"survey-other-seam:{alternative_first_ore}t "
                f"delta:{alternative_first_ore - direct_first_ore:+}t]"
            )
        else:
            alternative += f"followup:{followup}"
        parts.append(alternative + "]")
    return " ".join(parts)
