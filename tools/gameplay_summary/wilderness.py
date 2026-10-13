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


def wilderness_summary(lines: list[str]) -> str | None:
    experiences = [
        line for line in lines if line.startswith("WILDERNESS EXPERIENCE ")
    ]
    if not experiences:
        return None
    maintained = [line for line in experiences if " sample=maintained " in line]
    coverage = [line for line in experiences if " sample=coverage " in line]
    organic = [line for line in experiences if " sample=organic " in line]
    pressure_coverage = [
        line
        for line in coverage
        if " start=hydration-pressure " in line and " intent=stay-mobile " in line
    ]
    if len(maintained) != 1 or len(pressure_coverage) != 1:
        raise ValueError(
            "wilderness report requires one maintained mobile baseline and one mobile hydration-pressure coverage experience"
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
    pressure_steps = re.search(r"\bmovement=\[.*?\bsteps:(\d+)", pressure_coverage[0])
    pressure_drink = re.search(r"\bwater=\[.*?\bconsumed:(\d+)uL", pressure_coverage[0])
    if pressure_steps is None or pressure_drink is None:
        raise ValueError("wilderness pressure experience lost route or drink telemetry")
    forage_opportunities: list[int] = []
    forage_secured: list[int] = []
    forage_runway_tenths: list[int] = []
    water_runway_tenths: list[int] = []
    camp_work_ticks: list[int] = []
    camp_cached_mass: list[int] = []
    camp_staged_timber: list[int] = []
    camp_metabolic_budget: list[int] = []
    camp_hydration_budget: list[int] = []
    project_meals = 0
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
            r"\bcamp-project=\[.*?\bwork:(\d+)t budget:(\d+)nJ/(\d+)uL "
            r"project-meal:(\d+)mg cached-total:(\d+)mg staged-timber:(\d+)mg\]",
            experience,
        )
        if project is None:
            raise ValueError("wilderness experience lost camp-project telemetry")
        work = int(project.group(1))
        meal = int(project.group(4))
        if meal > 0:
            project_meals += 1
        if work > 0:
            camp_work_ticks.append(work)
            camp_metabolic_budget.append(int(project.group(2)))
            camp_hydration_budget.append(int(project.group(3)))
            camp_cached_mass.append(int(project.group(5)))
            camp_staged_timber.append(int(project.group(6)))
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
    camp_strategy = (
        "camp-strategy=["
        f"mobile:{sum(' intent=stay-mobile ' in line for line in experiences)} "
        f"bulk-cache:{sum(' intent=bulk-cache ' in line for line in experiences)} "
        f"preserve-reserve:{sum(' intent=preserve-reserve ' in line for line in experiences)} "
        "built=["
        f"field-box:{sum(' storage:field-box ' in line for line in experiences)} "
        f"stone-crock:{sum(' storage:stone-crock ' in line for line in experiences)}] "
        "materials=["
        f"defer:{sum(' material:none ' in line for line in experiences)} "
        f"timber:{sum(' material:timber ' in line for line in experiences)} "
        f"stone:{sum(' material:stone ' in line for line in experiences)}] "
        f"project-meals:{project_meals}]"
    )
    camp_projects = "camp-projects=[none]"
    if camp_work_ticks:
        camp_projects = (
            "camp-projects=["
            f"work:{min(camp_work_ticks)}..{max(camp_work_ticks)}t "
            f"cached:{scaled_span(camp_cached_mass, 1_000_000, 'kg')} "
            f"staged-timber:{scaled_span(camp_staged_timber, 1_000_000, 'kg')} "
            f"body-budget:{scaled_span(camp_metabolic_budget, 1_000_000_000_000, 'kJ')}/"
            f"{scaled_span(camp_hydration_budget, 1_000, 'mL')}]"
        )
    bulk_projects = [line for line in experiences if " intent=bulk-cache " in line]
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
        f"bulk-projects:{len(bulk_projects)} "
        f"first-box-adze-deferred:{sum(' reason:first-box-below-adze-crossover]' in line for line in bulk_projects)}/{len(bulk_projects)} "
        f"crossover:{adze_crossover}batches]"
    )
    organic_detail = ""
    if organic:
        organic_detail = (
            " organic-adaptation=["
            f"rested:{sum(' start=rested ' in line for line in organic)} "
            f"hunger:{sum(' start=hunger-pressure ' in line for line in organic)} "
            f"hydration:{sum(' start=hydration-pressure ' in line for line in organic)} "
            "intent=["
            f"mobile:{sum(' intent=stay-mobile ' in line for line in organic)} "
            f"bulk:{sum(' intent=bulk-cache ' in line for line in organic)} "
            f"preserve:{sum(' intent=preserve-reserve ' in line for line in organic)}] "
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
        f"pressure-route=[steps:{pressure_steps.group(1)} water:visited "
        f"consumed:{pressure_drink.group(1)}uL] "
        f"{resource_runway} "
        f"{camp_strategy} "
        f"{camp_projects} "
        f"{tool_policy} "
        "adaptation=[rested:skip-water hydration-pressure:visit-and-drink]"
        f"{organic_detail}"
    )
