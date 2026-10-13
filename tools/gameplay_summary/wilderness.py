"""Compact player-experience evidence for the executable wilderness opening."""

from __future__ import annotations

import re

from .common import compact_fields


def wilderness_summary(lines: list[str]) -> str | None:
    experiences = [
        line for line in lines if line.startswith("WILDERNESS EXPERIENCE ")
    ]
    if not experiences:
        return None
    maintained = [line for line in experiences if " sample=maintained " in line]
    coverage = [line for line in experiences if " sample=coverage " in line]
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
    return (
        "ORDINARY SUMMARY probe=wilderness samples=2 "
        "sample-shape=[maintained:1 coverage:1] "
        f"{baseline} "
        f"pressure-route=[steps:{pressure_steps.group(1)} water:visited "
        f"consumed:{pressure_drink.group(1)}uL] "
        "adaptation=[rested:skip-water hydration-pressure:visit-and-drink]"
    )
