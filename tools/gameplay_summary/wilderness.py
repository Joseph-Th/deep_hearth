"""Compact player-experience evidence for the executable wilderness opening."""

from __future__ import annotations

import re

from .common import compact_fields, scaled_span


def _tenths_span(values: list[int], unit: str) -> str:
    if not values:
        return "n/a"

    def render(value: int) -> str:
        return f"{value // 10}.{value % 10}"

    return f"{render(min(values))}..{render(max(values))}{unit}"


def _mass_kg(milligrams: int) -> str:
    whole, remainder = divmod(milligrams, 1_000_000)
    if remainder == 0:
        return str(whole)
    tenths = (remainder * 10 + 500_000) // 1_000_000
    return str(whole + 1) if tenths == 10 else f"{whole}.{tenths}"


def wilderness_summary(lines: list[str]) -> str | None:
    experiences = [
        line for line in lines if line.startswith("WILDERNESS EXPERIENCE ")
    ]
    if not experiences:
        return None
    direct_use_lines = [
        line for line in lines if line.startswith("WILDERNESS DIRECT USE ")
    ]
    if len(direct_use_lines) > 1:
        raise ValueError("wilderness report has multiple direct-use witnesses")
    direct_use = ""
    if direct_use_lines:
        movement = re.search(
            r"\bmovement=continuous-body route=\[voxels:(\d+) unique-edges:(\d+)/(\d+) control-ticks:(\d+)\]",
            direct_use_lines[0],
        )
        known_path = re.search(
            r"\bknown-path-projection=\[edges:(\d+) control-ticks:(\d+) matched:true\]",
            direct_use_lines[0],
        )
        material_choice = re.search(
            r"\bmaterial-choice=\[stone\+timber:(\d+) visited:(\d+)\]",
            direct_use_lines[0],
        )
        haul = re.search(
            r"\bhaul=\[timber:(\d+)mg carried:(\d+)mg/(\d+)mg return:(\d+)ct\]",
            direct_use_lines[0],
        )
        queue = re.search(
            r"\bqueue=\[forage:(\d+)ct drink:(\d+)ct timber:(\d+)ct\]",
            direct_use_lines[0],
        )
        body_effort = re.search(
            r"\bbody-effort=\[jump-load:(\d+)mg vertical:(\d+)nm/s energy:(\d+)nJ>(\d+)nJ-idle "
            r"hydration:(\d+)uL>(\d+)uL-idle settle:(\d+)ct\]",
            direct_use_lines[0],
        )
        if (
            movement is None
            or known_path is None
            or material_choice is None
            or haul is None
            or queue is None
            or body_effort is None
        ):
            raise ValueError(
                "wilderness direct-use witness lost route/projection/material/haul/body-effort/queue telemetry"
            )
        if int(body_effort.group(3)) <= int(body_effort.group(4)) or int(
            body_effort.group(5)
        ) <= int(body_effort.group(6)):
            raise ValueError("wilderness jump no longer carries a survival cost above idle")
        # The one-shot action is acknowledged on its input control tick, but the work
        # cannot begin until a slow boundary. Surface the lived delay in seconds: raw
        # control-tick counts conceal a multi-second interaction response.
        queued_seconds = "/".join(
            f"{int(queue.group(index)) / 20:.2f}"
            for index in (1, 2, 3)
        )
        direct_use = (
            f"direct-use=[route={movement.group(2)}/{movement.group(3)}e/{movement.group(4)}ct "
            f"plan={known_path.group(1)}e/{known_path.group(2)}ct "
            f"mats={material_choice.group(2)}/{material_choice.group(1)} "
            f"haul={_mass_kg(int(haul.group(1)))}/{_mass_kg(int(haul.group(2)))}/{_mass_kg(int(haul.group(3)))}kg@{haul.group(4)}ct "
            f"lag={queued_seconds}s use=one-shot "
            + compact_fields(
                direct_use_lines[0],
                ("gather", "water", "hydration", "timed"),
            )
            + "]"
        )
    shelter_lines = [
        line for line in lines if line.startswith("WILDERNESS SHELTER EXPERIENCE ")
    ]
    if len(shelter_lines) != 1:
        raise ValueError("wilderness report requires exactly one lived long-stay shelter witness")
    shelter_match = re.search(
        r'\bsource=fresh-world goal=long-stay selection=player-catalog tool="([^"]+)" '
        r'travel=continuous/(\d+)legs materials=\[stone:(\d+)mg timber:(\d+)mg\] '
        r'shelter=\[supports:(\d+)x(\d+)mg panel:(\d+)mg board-work:(\d+)b\] '
        r'attention=\[setup:(\d+)t bare:(\d+)t assisted:(\d+)t with-setup:(\d+)t saved:(\d+)t\] '
        r'exposure=\[open:(\d+)ppm/t covered:(\d+)ppm/t observed:(\d+)ppm/t\] '
        r'output=(\d+)mg wear=(true|false) matter-conserved=(true|false) '
        r'continuity=same-opening-state staged-timber-used:(\d+)mg fresh-timber:(\d+)mg',
        shelter_lines[0],
    )
    if shelter_match is None:
        raise ValueError("wilderness shelter witness lost workload/support/exposure telemetry")
    setup = int(shelter_match.group(9))
    bare = int(shelter_match.group(10))
    assisted = int(shelter_match.group(11))
    with_setup = int(shelter_match.group(12))
    saved = int(shelter_match.group(13))
    open_loss = int(shelter_match.group(14))
    covered_loss = int(shelter_match.group(15))
    observed_loss = int(shelter_match.group(16))
    if (
        int(shelter_match.group(5)) <= 0
        or int(shelter_match.group(6)) <= 0
        or int(shelter_match.group(7)) <= 0
        or int(shelter_match.group(8)) <= 0
        or with_setup != setup + assisted
        or saved != bare - with_setup
        or saved <= 0
        or covered_loss >= open_loss
        or observed_loss != covered_loss
        or shelter_match.group(18) != "true"
        or shelter_match.group(19) != "true"
        or int(shelter_match.group(20)) <= 0
        or int(shelter_match.group(21)) <= 0
    ):
        raise ValueError("wilderness shelter no longer proves earned structural survival leverage")
    postfire = re.search(
        r"\bpostfire=\[deferred:(true|false) covered:(\d+)ppm unbuilt:(\d+)ppm(?: fuel-only:(\d+)ppm)?\]",
        shelter_lines[0],
    )
    fuel_free = ""
    if postfire is not None:
        protected = int(postfire.group(2))
        exposed = int(postfire.group(3))
        if postfire.group(1) != "true" or protected <= exposed:
            raise ValueError("wilderness roof did not protect an actual post-fire fuel deferral")
        fuel_free = f" fuel-free-hour:{protected // 1_000}k>{exposed // 1_000}kppm"
        if postfire.group(4) is not None:
            fuel_only = int(postfire.group(4))
            if fuel_only < protected:
                raise ValueError("wilderness fuel-only alternative unexpectedly lost short-horizon warmth")
            fuel_free += f" alternative-fire:{fuel_only // 1_000}kppm/1kg"
    shelter = (
        "shelter=["
        f"long adze:+{saved}t "
        f"roof:{_mass_kg(int(shelter_match.group(7)))}kg/{shelter_match.group(8)}b "
        f"posts:{shelter_match.group(5)} "
        f"exposure:{open_loss}->{covered_loss}ppm/t{fuel_free}]"
    )
    maintained = [line for line in experiences if " sample=maintained " in line]
    coverage = [line for line in experiences if " sample=coverage " in line]
    organic = [line for line in experiences if " sample=organic " in line]
    long_stay = [line for line in experiences if " goal=long-stay-shelter " in line]
    pressure_coverage = [
        line
        for line in coverage
        if " start=hydration-pressure " in line and " goal=stay-mobile " in line
    ]
    if len(maintained) != 1 or len(pressure_coverage) != 1 or len(long_stay) != 1:
        raise ValueError(
            "wilderness report requires one maintained mobile baseline, one mobile hydration-pressure coverage experience, and one integrated long-stay shelter experience"
        )
    baseline = compact_fields(
        maintained[0],
        (
            "evidence",
            "generation",
            "movement",
            "timing",
            "forage",
            "provisioning",
            "carrying",
            "water",
            "storage-choice",
            "tool-choice",
            "material-choice",
        ),
    )
    pressure_legs = re.search(
        r"\bmovement=\[.*?\bbody:continuous legs:(\d+)", pressure_coverage[0]
    )
    pressure_drink = re.search(r"\bwater=\[.*?\bconsumed:(\d+)uL", pressure_coverage[0])
    if pressure_legs is None or pressure_drink is None:
        raise ValueError("wilderness pressure experience lost route or drink telemetry")
    forage_opportunities: list[int] = []
    forage_secured: list[int] = []
    forage_runway_tenths: list[int] = []
    water_runway_tenths: list[int] = []
    camp_work_ticks: list[int] = []
    camp_cached_mass: list[int] = []
    camp_staged_timber: list[int] = []
    worksite_residue: list[int] = []
    worksite_support_mass: list[int] = []
    worksite_support_ticks: list[int] = []
    worksite_support_built = 0
    campfire_fuel: list[int] = []
    campfire_recovery_ticks: list[int] = []
    campfire_runway_minutes_tenths: list[int] = []
    campfire_before_comfort: list[int] = []
    campfire_after_comfort: list[int] = []
    campfire_heat_remaining: list[int] = []
    campfire_light_levels: list[int] = []
    campfires_built = 0
    direct_control_fires = 0
    camp_logistics_legs: list[int] = []
    selected_source_opportunity: list[int] = []
    selected_source_remaining: list[int] = []
    camp_metabolic_budget: list[int] = []
    camp_hydration_budget: list[int] = []
    project_meals = 0
    storage_catalog_projects = 0
    for experience in experiences:
        forage = re.search(
            r"\bforage=\[opportunity:(\d+)mg secured:(\d+)mg .*?rest-runway:(\d+)\.(\d)h\]",
            experience,
        )
        if forage is None:
            raise ValueError("wilderness experience lost forage opportunity/runway telemetry")
        forage_opportunities.append(int(forage.group(1)))
        forage_secured.append(int(forage.group(2)))
        forage_runway_tenths.append(int(forage.group(3)) * 10 + int(forage.group(4)))
        water = re.search(r"\bwater=\[.*?\brest-runway:(\d+)\.(\d)h", experience)
        if water is not None:
            water_runway_tenths.append(int(water.group(1)) * 10 + int(water.group(2)))
        project = re.search(
            r"\bcamp-project=\[.*?\bwork:(\d+)t logistics-legs:(\d+) "
            r"source:(\d+)mg->(\d+)mg project-upper-budget:(\d+)nJ/(\d+)uL "
            r"project-meal:(\d+)mg cached-total:(\d+)mg staged-timber:(\d+)mg "
            r"retained-residue:(\d+)mg grounded-support:(true|false):(\d+)mg/(\d+)t\]",
            experience,
        )
        if project is None:
            raise ValueError("wilderness experience lost camp-project telemetry")
        warmth = re.search(
            r"\bwarmth=\[fire:true(?: direct-control:(true))? fuel:(\d+)mg logistics-legs:(\d+) recovery:(\d+)t "
            r"burn-runway:(\d+)t/(\d+)\.(\d)m comfort:(\d+)->(\d+)ppm heat-remaining:(\d+)nJ(?: light:(\d+))?\]",
            experience,
        )
        if " goal=worksite-cache " in experience or " goal=long-stay-shelter " in experience:
            if warmth is None:
                raise ValueError("wilderness worksite experience lost campfire warmth telemetry")
            campfires_built += 1
            direct_control_fires += warmth.group(1) == "true"
            campfire_fuel.append(int(warmth.group(2)))
            campfire_recovery_ticks.append(int(warmth.group(4)))
            campfire_runway_minutes_tenths.append(
                int(warmth.group(6)) * 10 + int(warmth.group(7))
            )
            campfire_before_comfort.append(int(warmth.group(8)))
            campfire_after_comfort.append(int(warmth.group(9)))
            campfire_heat_remaining.append(int(warmth.group(10)))
            if warmth.group(11) is not None:
                campfire_light_levels.append(int(warmth.group(11)))
        work = int(project.group(1))
        meal = int(project.group(7))
        if meal > 0:
            project_meals += 1
        if " crafting=[storage-via-player-catalog:true] " in experience:
            storage_catalog_projects += 1
        if work > 0:
            camp_work_ticks.append(work)
            camp_logistics_legs.append(int(project.group(2)))
            selected_source_opportunity.append(int(project.group(3)))
            selected_source_remaining.append(int(project.group(4)))
            camp_metabolic_budget.append(int(project.group(5)))
            camp_hydration_budget.append(int(project.group(6)))
            camp_cached_mass.append(int(project.group(8)))
            camp_staged_timber.append(int(project.group(9)))
            if project.group(11) == "true":
                worksite_support_built += 1
                worksite_residue.append(int(project.group(10)))
                worksite_support_mass.append(int(project.group(12)))
                worksite_support_ticks.append(int(project.group(13)))
    if not water_runway_tenths:
        raise ValueError("wilderness report has no exact-local visited water runway witness")
    resource_runway = (
        "resource-runway=["
        f"forage:{_tenths_span(forage_runway_tenths, 'h')} "
        f"opportunity:{scaled_span(forage_opportunities, 1_000_000, 'kg')} "
        f"secured:{scaled_span(forage_secured, 1_000, 'g')} "
        f"visited-water:{_tenths_span(water_runway_tenths, 'h')} "
        "unvisited-water:hidden]"
    )
    camp_goals = (
        "camp-goals=["
        f"mobile:{sum(' goal=stay-mobile ' in line for line in experiences)} "
        f"worksite-cache:{sum(' goal=worksite-cache ' in line for line in experiences)} "
        f"long-stay-shelter:{sum(' goal=long-stay-shelter ' in line for line in experiences)} "
        f"preserve-provisions:{sum(' goal=preserve-provisions ' in line for line in experiences)} "
        "built=["
        f"field-box:{sum(' storage:field-box ' in line for line in experiences)} "
        f"stone-crock:{sum(' storage:stone-crock ' in line for line in experiences)}] "
        "materials=["
        f"defer:{sum(' material:none ' in line for line in experiences)} "
        f"timber:{sum(' material:timber ' in line for line in experiences)} "
        f"stone:{sum(' material:stone ' in line for line in experiences)}] "
        f"project-meals:{project_meals}]"
    )
    expected_storage_projects = sum(" goal=stay-mobile " not in line for line in experiences)
    has_crafting_telemetry = any(" crafting=[" in line for line in experiences)
    if has_crafting_telemetry and storage_catalog_projects != expected_storage_projects:
        raise ValueError(
            "wilderness storage projects must execute through the player recipe catalog"
        )
    crafting = (
        "crafting=[storage-via-player-catalog:"
        f"{storage_catalog_projects}/{expected_storage_projects}]"
        if has_crafting_telemetry
        else ""
    )
    cache_projects = [line for line in experiences if " goal=worksite-cache " in line]
    long_stay_projects = [line for line in experiences if " goal=long-stay-shelter " in line]
    worksite_projects = cache_projects + long_stay_projects
    camp_projects = "projects=[none]"
    if camp_work_ticks:
        camp_projects = (
            "projects=["
            f"work:{min(camp_work_ticks)}..{max(camp_work_ticks)}t "
            f"cached:{scaled_span(camp_cached_mass, 1_000_000, 'kg')} "
            f"staged-timber:{scaled_span(camp_staged_timber, 1_000_000, 'kg')} "
            f"worksite-support=[built:{worksite_support_built}/{len(worksite_projects)} "
            f"mass:{scaled_span(worksite_support_mass, 1_000_000, 'kg')} "
            f"work:{scaled_span(worksite_support_ticks, 1, 't')} "
            f"retained-residue:{scaled_span(worksite_residue, 1_000, 'g')}] "
            f"legs:{min(camp_logistics_legs)}..{max(camp_logistics_legs)} "
            f"source:{scaled_span(selected_source_opportunity, 1_000_000, 'kg')}->"
            f"{scaled_span(selected_source_remaining, 1_000_000, 'kg')} "
            f"upper-budget:{scaled_span(camp_metabolic_budget, 1_000_000_000_000, 'kJ')}/"
            f"{scaled_span(camp_hydration_budget, 1_000, 'mL')}]"
        )
    camp_warmth = ""
    if worksite_projects:
        has_direct_fire_telemetry = any(" direct-control:" in line for line in worksite_projects)
        if has_direct_fire_telemetry and direct_control_fires != len(worksite_projects):
            raise ValueError(
                "wilderness worksite fires must assemble and fuel through direct control"
            )
        direct_fire = (
            f"direct:{direct_control_fires}/{len(worksite_projects)} "
            if has_direct_fire_telemetry
            else ""
        )
        light = (
            f"light:{min(campfire_light_levels)}..{max(campfire_light_levels)} "
            if campfire_light_levels
            else ""
        )
        camp_warmth = (
            "camp-warmth=["
            f"fires:{campfires_built}/{len(worksite_projects)} "
            f"{direct_fire}"
            f"fuel:{scaled_span(campfire_fuel, 1_000_000, 'kg')} "
            f"runway:{_tenths_span(campfire_runway_minutes_tenths, 'm')} "
            f"recovery:{scaled_span(campfire_recovery_ticks, 1, 't')} "
            f"comfort:{scaled_span(campfire_before_comfort, 1_000, 'kppm')}->"
            f"{scaled_span(campfire_after_comfort, 1_000, 'kppm')} "
            f"{light}"
            f"heat-left:{scaled_span(campfire_heat_remaining, 1_000_000_000_000_000, 'MJ')}]"
        )
    adze_crossover = next(
        (
            int(match.group(1))
            for line in experiences
            if (match := re.search(r"\bcrossover:(\d+)batches", line)) is not None
        ),
        None,
    )
    if adze_crossover is None:
        raise ValueError("wilderness experience lost early adze crossover telemetry")
    tool_policy = (
        "tool-policy=["
        f"short-cache-projects:{len(cache_projects)} "
        f"first-box-adze-deferred:{sum(' reason:first-box-below-adze-crossover]' in line for line in cache_projects)}/{len(cache_projects)} "
        f"long-stay-adze:{sum(' reason:roof-workload-repays-tool]' in line for line in long_stay_projects)}/{len(long_stay_projects)} "
        f"crossover:{adze_crossover}batches]"
    )
    organic_detail = ""
    if organic:
        organic_detail = (
            " organic-adaptation=["
            f"rested:{sum(' start=rested ' in line for line in organic)} "
            f"hunger:{sum(' start=hunger-pressure ' in line for line in organic)} "
            f"hydration:{sum(' start=hydration-pressure ' in line for line in organic)} "
            "goals=["
            f"mobile:{sum(' goal=stay-mobile ' in line for line in organic)} "
            f"cache:{sum(' goal=worksite-cache ' in line for line in organic)} "
            f"preserve:{sum(' goal=preserve-provisions ' in line for line in organic)}] "
            f"meals:{sum(' meal:eat-needed ' in line for line in organic)} "
            f"water-visits:{sum(' visited:true ' in line for line in organic)}]"
        )
    sample_shape = f"maintained:1 coverage:{len(coverage)}"
    if organic:
        sample_shape += f" organic:{len(organic)}"
    return (
        f"ORDINARY SUMMARY probe=wilderness samples={len(experiences)} "
        f"sample-shape=[{sample_shape}] "
        f"{baseline} "
        f"{direct_use + ' ' if direct_use else ''}"
        f"pressure=[legs:{pressure_legs.group(1)} water:visited "
        f"consumed:{pressure_drink.group(1)}uL] "
        f"{resource_runway} "
        f"{camp_goals} "
        f"{crafting + ' ' if crafting else ''}"
        f"{camp_projects} "
        f"{camp_warmth + ' ' if camp_warmth else ''}"
        f"{shelter} "
        f"{tool_policy} "
        "adaptation=[rested:skip-water hydration-pressure:visit-and-drink]"
        f"{organic_detail}"
    )
