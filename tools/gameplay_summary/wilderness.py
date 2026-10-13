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
    if len(maintained) != 1 or len(coverage) != 1:
        raise ValueError(
            "wilderness report requires one maintained rested experience and one pressure coverage experience"
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
    pressure_steps = re.search(r"\bmovement=\[.*?\bsteps:(\d+)", coverage[0])
    pressure_drink = re.search(r"\bwater=\[.*?\bconsumed:(\d+)uL", coverage[0])
    if pressure_steps is None or pressure_drink is None:
        raise ValueError("wilderness pressure experience lost route or drink telemetry")
    forage_opportunities: list[int] = []
    forage_secured: list[int] = []
    forage_runway_tenths: list[int] = []
    water_runway_tenths: list[int] = []
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
    organic_detail = ""
    if organic:
        organic_detail = (
            " organic-adaptation=["
            f"rested:{sum(' start=rested ' in line for line in organic)} "
            f"hunger:{sum(' start=hunger-pressure ' in line for line in organic)} "
            f"hydration:{sum(' start=hydration-pressure ' in line for line in organic)} "
            f"meals:{sum(' meal:eat-needed ' in line for line in organic)} "
            f"water-visits:{sum(' visited:true ' in line for line in organic)}]"
        )
    sample_shape = "maintained:1 coverage:1"
    if organic:
        sample_shape += f" organic:{len(organic)}"
    return (
        f"ORDINARY SUMMARY probe=wilderness samples={len(experiences)} "
        f"sample-shape=[{sample_shape}] "
        f"{baseline} "
        f"pressure-route=[steps:{pressure_steps.group(1)} water:visited "
        f"consumed:{pressure_drink.group(1)}uL] "
        f"{resource_runway} "
        "adaptation=[rested:skip-water hydration-pressure:visit-and-drink]"
        f"{organic_detail}"
    )
