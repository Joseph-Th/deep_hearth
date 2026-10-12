"""Compact player-experience evidence for the executable wilderness opening."""

from __future__ import annotations

from .common import compact_fields


def wilderness_summary(lines: list[str]) -> str | None:
    experiences = [
        line for line in lines if line.startswith("WILDERNESS EXPERIENCE ")
    ]
    if not experiences:
        return None
    if len(experiences) != 1:
        raise ValueError(
            f"wilderness report expected one opening experience, found {len(experiences)}"
        )
    experience = experiences[0]
    detail = compact_fields(
        experience,
        (
            "evidence",
            "generation",
            "timing",
            "provisioning",
            "carrying",
            "water",
            "storage-choice",
            "tool-choice",
            "remaining-opportunity",
        ),
    )
    return (
        "ORDINARY SUMMARY probe=wilderness samples=1 "
        "sample-shape=[controlled:1] "
        f"{detail}"
    )
