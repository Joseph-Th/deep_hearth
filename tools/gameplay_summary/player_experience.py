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
    """Show one maintained actor's first choices, costs and unresolved need.

    This is a single generated-world continuation, not an aggregate or a substitute
    for the separately reported need-triggered fire response.
    """

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
    return (
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
        parts.append(
            "alternative=["
            "matched-executed-hand-assay "
            f"extra:{_required(assay, 'extra-attention')} "
            f"reserve:{_required(assay, 'visible-before')}->{_required(assay, 'visible-after')} "
            f"selected:{_required(assay, 'selected-next-action')} "
            f"followup:{_required(assay, 'followup')}]"
        )
    return " ".join(parts)
