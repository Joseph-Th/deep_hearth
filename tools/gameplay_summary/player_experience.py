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
    forecast = _required(adaptation, "forecast")
    open_runway, covered_runway = _capture(
        forecast,
        r"unheated-open:(\d+)t roof-if-built:(\d+)t no-local-heat",
        "conditional shelter forecast",
    )
    provision = re.search(r"\bself-chosen-provisions:(\d+)t", adaptation)
    passive_exposure = _required(adaptation, "passive-exposure")
    after_ticks, after_minutes = _capture(
        adaptation, r"\bafter:(\d+)t/([0-9.]+m)", "cold onset"
    )
    if (
        int(covered_runway) <= int(open_runway)
        or int(after_ticks) == 0
        or (provision is not None and int(after_ticks) < int(provision.group(1)))
        or not re.fullmatch(r"[0-9.]+m", passive_exposure)
    ):
        raise ValueError("player adaptation lost its actual provisioning, exposure or shelter tradeoff")
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
        + " continuation=[chilly-after:" + after_minutes
        + (" provisioning:" + provision.group(1) + "t" if provision is not None else "")
        + " passive:" + passive_exposure
        + " roof-forecast:" + covered_runway + "t-vs-open:" + open_runway + "t"
        + " reacted:built-fire-at-" + fueled + "t"
        + " comfort:" + comfort + "ppm vs-no-fire:" + no_fire + "ppm]"
        + " alternative=[matched-early-fire-at:" + earlier + "t"
        + " comfort:" + planned_comfort + "ppm"
        + " fuel-runway:" + planned_runway + "t-vs-late:" + reactive_runway + "t]"
    )


def wilderness_worksite_player_experience(lines: list[str]) -> str | None:
    """One specified construction goal with an actual self-directed site-preparation decision.

    Do not promote a ready workstation into productive output or an assigned
    project into a spontaneously discovered player goal.
    """

    record = _one_record(lines, "PLAYER PROJECT TRADEOFF ")
    if record is None:
        return None
    witness = _one_seeded(lines, "WILDERNESS CASTING BED ", _required(record, "seed"))
    if witness is None:
        raise ValueError("worksite project tradeoff lacks its finished physical workstation")
    provision = _one_seeded(lines, "PLAYER PROJECT PROVISION ", _required(record, "seed"))
    if provision is None:
        raise ValueError("worksite project lost its direct-control provisions")
    payoff = _one_seeded(lines, "PLAYER WORKSITE PAYOFF ", _required(record, "seed"))
    if payoff is None:
        raise ValueError("worksite project lost the same-actor productive food-cache continuation")
    priority = _one_seeded(lines, "PLAYER PRIORITY CHOICE ", _required(record, "seed"))
    if priority is None:
        raise ValueError("worksite project lost its actually executed food-first alternative")
    if (
        _required(provision, "evidence") != "generated-direct-control"
        or _required(provision, "automatic-other-source") != "false"
        or _required(provision, "matter") != "conserved"
    ):
        raise ValueError("worksite provisions do not establish an exact controlled player choice")
    picked, initial = _capture(
        _required(provision, "choice"),
        r"protein-wild-nuts:(\d+)mg from:(\d+)mg",
        "chosen wild protein portion",
    )
    source, queue, forage = _capture(
        _required(provision, "action"),
        r"explicit-source:(\d+) buffered:(\d+)ct slow-work:(\d+)t",
        "executed explicit gathering action",
    )
    carried, left = _capture(
        _required(provision, "consequence"),
        r"carried:(\d+)mg left-at-source:(\d+)mg",
        "finite source and carried custody",
    )
    baseline_nuts, fire_nuts, exposed_nuts = _capture(
        _required(record, "ecology"),
        r"after-ration:(\d+)mg fire:(\d+)mg exposed:(\d+)mg",
        "actual natural-food exposure during construction",
    )
    if (
        not 0 < int(picked) == int(carried) < int(initial)
        or int(initial) - int(picked) != int(left)
        or int(source) == 0
        or int(queue) == 0
        or int(forage) == 0
        or int(baseline_nuts) != int(left)
        or int(fire_nuts) > int(baseline_nuts)
        or int(exposed_nuts) > int(baseline_nuts)
    ):
        raise ValueError("worksite project lost real finite-source selection or local ecology")
    if (
        _required(record, "goal") != "specified-not-discovered"
        or _required(witness, "goal") != "specified-workstation"
        or _required(witness, "evidence") != "generated-exact-local"
        or _required(witness, "payoff") != "deferred-until-molten-metal"
        or _required(witness, "matter") != "conserved"
        or _required(record, "matter") != "conserved"
        or _required(record, "replay") != "exact"
        or _required(record, "defer") != "[unneeded-pristine-service:true]"
    ):
        raise ValueError("worksite narrative mislabels project ownership or physical evidence")
    remaining, estimate = _capture(
        _required(record, "observation"),
        r"remaining-warmth:(\d+)t predicted-project-work-at-least:(\d+)t",
        "actor-observed project exposure",
    )
    choice, = _capture(
        _required(record, "policy"), r"choose-prework-fire:(true|false)", "worksite decision"
    )
    work, = _capture(
        _required(record, "action"),
        r"same-acquisition-and-crafting:(\d+)t both-branches",
        "identical counterfactual work",
    )
    preparation, stone, timber, runway = _capture(
        _required(record, "investment"),
        r"extra-fire-preparation:(\d+)t stone:(\d+)mg timber-fuel:(\d+)mg remaining-runway:(\d+)t",
        "physical fire investment",
    )
    protected, exposed = _capture(
        _required(record, "consequence"),
        r"prepared-comfort:(\d+)ppm exposed-comfort:(\d+)ppm",
        "matched actual worker comfort",
    )
    delta, = _capture(
        _required(record, "benefit"), r"comfort-delta:\+(\d+)ppm", "physical warmth benefit"
    )
    if (
        (choice == "true") != (int(estimate) >= int(remaining))
        or any(int(value) <= 0 for value in (remaining, estimate, work, preparation, stone, timber, runway))
        or int(work) < int(estimate)
        or int(protected) <= int(exposed)
        or int(protected) - int(exposed) != int(delta)
        or _required(record, "productive-output") != "[stationary-casting-bed:9.6kg metal-cast:0]"
        or "output-produced:false" not in _required(witness, "worksite")
        or f"pre-work-fire:{choice}" not in _required(witness, "choice")
    ):
        raise ValueError("worksite project failed to establish an earned preparation tradeoff")
    if (
        _required(payoff, "continuity") != "one-actor-after-casting-bed"
        or _required(payoff, "action")
        != "[gather-timber>shape-boards>join-box>same-berry-expedition>store-real-provisions>recover-food]"
        or _required(payoff, "counterfactual")
        != "equal-clock-matched-berry-trip-but-no-box"
        or _required(payoff, "metal-produced") != "0"
        or _required(payoff, "custody") != "conserved"
        or _required(payoff, "replay") != "exact"
        or _required(payoff, "after-observation") != "120t"
    ):
        raise ValueError("worksite cache evidence must describe actual matched player continuation")
    box_ticks, box_wood = _capture(
        _required(payoff, "investment"),
        r"extra-attention:(\d+)t timber:(\d+)mg",
        "box project resource and attention expense",
    )
    stored_nuts, stored_berries, recovered_berries, preserve = _capture(
        _required(payoff, "result"),
        r"stored-nuts:(\d+)mg stored-berries:(\d+)mg recovered-berries:(\d+)mg container:rough-timber-field-box preservation:(\d+)ppm",
        "embodied worksite provisions",
    )
    nut_box, nut_carried, berry_box, berry_carried = _capture(
        _required(payoff, "age"),
        r"nuts-boxed:(\d+)parts nuts-carried:(\d+)parts berries-boxed:(\d+)parts berries-carried:(\d+)parts",
        "observed physical age of matched real food",
    )
    nut_saved = int(nut_carried) - int(nut_box)
    berry_saved = int(berry_carried) - int(berry_box)
    if (
        not 0 < int(box_ticks)
        or not 0 < int(box_wood)
        or int(stored_nuts) != int(picked)
        or int(stored_berries) <= int(stored_nuts)
        or int(recovered_berries) != int(stored_berries)
        or int(preserve) <= 1_000_000
        or not 0 < nut_saved <= 120_000_000
        or not 0 < berry_saved <= 120_000_000
        or nut_saved != berry_saved
    ):
        raise ValueError("worksite cache produced no physically earned food-preservation benefit")
    if (
        _required(priority, "observation")
        != "[local-molten-metal:none local-melting-provider:none]"
        or _required(priority, "priority") != "protect-actual-food-before-inert-foundry"
        or _required(priority, "action")
        != "[finite-fire>timber-box>same-berry-expedition>recover-real-food]"
        or _required(priority, "unresolved") != "metal-production"
        or _required(priority, "custody") != "conserved"
        or _required(priority, "replay") != "exact"
    ):
        raise ValueError("worksite priority claim lost actor-observed constraints or real action")
    found, food_first, metal_first = _capture(
        _required(priority, "payoff"),
        r"recovered-berries:(\d+)mg first-security:(\d+)t bed-first-security:(\d+)t",
        "executed food-first and metal-first chronology",
    )
    delay, = _capture(
        priority, r"\bextra-workshop-delay=(\d+)t", "time cost of premature metal work"
    )
    if (
        int(found) != int(recovered_berries)
        or not 0 < int(food_first) < int(metal_first)
        or int(metal_first) - int(food_first) != int(delay)
    ):
        raise ValueError("food-first alternative did not earn its earlier useful outcome")
    first_metal = _one_seeded(lines, "PLAYER FIRST METAL ", _required(record, "seed"))
    if first_metal is None:
        raise ValueError("worksite project lost its generated native-copper cold-working payoff")
    if (
        _required(first_metal, "origin") != "finite-native-copper-float"
        or _required(first_metal, "action")
        != "[visit-clay-spur>collect-copper>cold-work>carve-stone>band-crock>forage>recover-food]"
        or _required(first_metal, "smelted-metal") != "0"
        or _required(first_metal, "matter") != "conserved"
        or _required(first_metal, "replay") != "exact"
    ):
        raise ValueError("first-metal branch must execute physical cold-working, not counterfeit smelting")
    metal_mass, stone_mass, metal_ticks = _capture(
        _required(first_metal, "cost"),
        r"metal:(\d+)mg stone:(\d+)mg security-elapsed:(\d+)t",
        "first-metal project material and time",
    )
    source_before, source_after = _capture(
        _required(first_metal, "discovery"),
        r"float-before:(\d+)mg float-after:(\d+)mg",
        "depleting discovered copper float",
    )
    recovered, metal_preserve, metal_saved = _capture(
        _required(first_metal, "payoff"),
        r"recovered-berries:(\d+)mg preservation:(\d+)ppm saved-age:(\d+)t-per-120t",
        "physically recovered copper preservation result",
    )
    basic_preserve, basic_ticks, premium_delay = _capture(
        _required(first_metal, "comparison"),
        r"timber-box:(\d+)ppm security-elapsed:(\d+)t extra-copper-route:(\d+)t",
        "realized early preservation alternative",
    )
    if (
        int(metal_mass) != 40_000
        or int(stone_mass) != 3_000_000
        or int(source_before) - int(source_after) != int(metal_mass)
        or not 0 < int(source_after)
        or not 0 < int(metal_ticks)
        or not 0 < int(basic_ticks) < int(metal_ticks)
        or int(metal_ticks) - int(basic_ticks) != int(premium_delay)
        or int(recovered) != int(stored_berries)
        or not 0 < int(basic_preserve) == int(preserve) < int(metal_preserve)
        or not berry_saved // 1_000_000 < int(metal_saved) <= 120
    ):
        raise ValueError("first-metal branch did not earn distinct finite metal and food preservation")
    metal_tool = _one_seeded(lines, "PLAYER COPPER TOOL ", _required(record, "seed"))
    if metal_tool is None:
        raise ValueError("worksite project lost its executed alternative first-metal tool investment")
    if (
        _required(metal_tool, "alternative") != "same-opening-before-copper"
        or _required(metal_tool, "goal") != "prepare-roof-panel-stock"
        or _required(metal_tool, "allocation") != "metal-tool-not-crock"
        or _required(metal_tool, "source") != "[native-copper:20000mg]"
        or _required(metal_tool, "matter") != "conserved"
        or _required(metal_tool, "replay") != "exact"
    ):
        raise ValueError("first-metal tool evidence lost its actor or conservation contract")
    tool_prep, = _capture(
        _required(metal_tool, "investment"),
        r"extra-acquisition:(\d+)t",
        "finite-copper tool acquisition",
    )
    lumber_mass, roof_mass, lumber_batches, stone_work, copper_work = _capture(
        _required(metal_tool, "matched"),
        r"boards:(\d+)mg roof-demand:(\d+)mg batches:(\d+) stone-work:(\d+)t copper-work:(\d+)t",
        "identical, fully executed woodworking orders",
    )
    stone_total, copper_total = _capture(
        _required(metal_tool, "realized"),
        r"stone-complete:(\d+)t copper-complete:(\d+)t",
        "full attention cost from the same upgrade decision",
    )
    stone_condition, copper_condition = _capture(
        _required(metal_tool, "wear"),
        r"stone:(\d+)ppm copper:(\d+)ppm",
        "remaining real tool condition",
    )
    tool_savings = int(stone_total) - int(copper_total)
    if (
        not 0 < int(tool_prep) < int(stone_work)
        or not 0 < int(lumber_batches)
        or int(lumber_mass) != 800_000 * int(lumber_batches)
        or not 0 < int(roof_mass) <= int(lumber_mass)
        or not 0 < int(copper_work) < int(stone_work)
        or int(stone_total) != int(stone_work)
        or int(copper_total) != int(tool_prep) + int(copper_work)
        or tool_savings <= 0
        or not 0 < int(stone_condition) < int(copper_condition) <= 1_000_000
    ):
        raise ValueError("first-metal tool did not repay its actual copper investment on matched boards")
    return (
        "PLAYER EXPERIENCE worksite-project goal=specified-not-discovered "
        f"provision=[selected-protein:{_kg(picked)} left-in-world:{_kg(left)} "
        f"buffer:{queue}ct gather:{forage}t] "
        f"forecast=[warmth:{remaining}t necessary-work:{estimate}t] "
        f"choice=[prepare-fire:{choice} extra:{preparation}t "
        f"stone:{_kg(stone)} fuel:{_kg(timber)}] "
        f"matched=[same-work:{work}t comfort:{protected}ppm-vs:{exposed}ppm "
        f"fire-left:{runway}t] "
        f"local-ecology=[food-fire:{_kg(fire_nuts)} exposed:{_kg(exposed_nuts)} "
        f"scout-difference:{int(fire_nuts) - int(exposed_nuts)}mg] "
        "defer=[no-service-on-pristine-machine] "
        f"usable-payoff=[box-time:{box_ticks}t timber:{_kg(box_wood)} "
        f"stored:{_kg(stored_nuts)}-nuts+{_kg(stored_berries)}-berries "
        f"recovered:{_kg(recovered_berries)}-berries preservation:{preserve}ppm "
        f"saved-age:{berry_saved / 1_000_000:.1f}t/120t] "
        f"priority=[food-first:{food_first}t bed-first:{metal_first}t "
        f"saved:{delay}t before-metal-feed] "
        f"first-metal=[native-float:{_kg(source_before)} took:{_kg(metal_mass)} "
        f"crock-security:{metal_ticks}t vs-wood:{basic_ticks}t cost-extra:{premium_delay}t "
        f"saved-age:{metal_saved}t/120t vs-wood:{berry_saved / 1_000_000:.1f}t/120t smelted:0] "
        f"first-metal-tool=[copper:0.02kg cover-stock:{_kg(lumber_mass)}/{_kg(roof_mass)} "
        f"setup:{tool_prep}t shape:{stone_work}t-vs:{copper_work}t "
        f"net-saved:{tool_savings}t condition:{stone_condition}-vs:{copper_condition}ppm] "
        "outcome=[station:9.6kg metal-produced:0 next:molten-metal]"
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
    part, machine_attention, machine_overlap, built, successor = _capture(
        _required(record, "next-machine"),
        r"component:([\w-]+) attention:(\d+)t overlap:(\d+)t built:(true|false) used-for:([\w-]+)",
        "delegated-time preparation for the next machine",
    )
    if (part != "drawplate" or successor != "wire-drawbench"
            or built != "true" or int(machine_attention) <= 0
            or int(machine_overlap) != int(machine_attention)):
        raise ValueError("settlement portfolio failed to build its next machine from real parallel preparation")
    if int(overlap) + int(machine_overlap) > int(delegated.removesuffix("t")):
        raise ValueError("settlement portfolio counted more overlap than available delegated time")
    used, before, after, service_attention, followup_batches = _capture(
        _required(record, "service-option"),
        r"choice:defer-spare matched-bit-used:(true|false) spindle-condition:(\d+)->(\d+)ppm matched-service:(\d+)t pending-batches:(\d+)",
        "usable parallel-prepared replacement",
    )
    if used != "true" or int(after) <= int(before) or int(service_attention) <= 0:
        raise ValueError("settlement portfolio did not validate the matched spare service option")
    if int(followup_batches) != 0:
        raise ValueError("settlement portfolio deferred service with active follow-up demand")
    unobserved = int(delegated.removesuffix("t")) - int(overlap) - int(machine_overlap)
    return (
        "PLAYER EXPERIENCE settlement-workshop continuity=single-executed-worksite"
        + " investment=[shared-bank families:" + families + "]"
        + " operation=[batches:" + batches + " charge-cycles:" + charges
        + " charge-labor:" + charge_attention + " delegated:" + delegated
        + " elapsed:" + elapsed + "]"
        + " proved-parallel-work=[" + component + ":" + overlap
        + "t drawplate:" + machine_overlap + "t next-machine:" + successor + "]"
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
