"""Compact factual summaries for gameplay exploration transcripts."""

from __future__ import annotations

import os

from tools.gameplay_summary.controlled import controlled_gameplay_summary
from tools.gameplay_summary.fieldwork import fieldwork_summary
from tools.gameplay_summary.liberation import liberation_summary
from tools.gameplay_summary.loop import player_loop_evidence
from tools.gameplay_summary.power import power_provider_summary
from tools.gameplay_summary.progression import progression_summary
from tools.gameplay_summary.survival import survival_summary
from tools.gameplay_summary.woodworking import woodworking_summary


def ordinary_gameplay_summary(lines: list[str]) -> list[str]:
    """Return compact measured ordinary-play evidence without adding interpretation."""

    summaries: list[str] = []
    for summary in (
        progression_summary(lines),
        liberation_summary(lines),
        woodworking_summary(lines),
        fieldwork_summary(lines),
        power_provider_summary(lines),
        survival_summary(lines),
    ):
        if summary is not None:
            summaries.append(summary)
    return summaries


def concise_gameplay_report(stdout: str, environ=None) -> str:
    """Keep one compact factual summary per probe; verbose retains the full transcript."""

    environment = os.environ if environ is None else environ
    if environment.get("DEEP_HEARTH_GAMEPLAY_VERBOSE") is not None or environment.get(
        "DEEP_HEARTH_GAMEPLAY_TRACE"
    ) is not None:
        return stdout.rstrip()
    lines = stdout.splitlines()
    selected = [
        line
        for line in lines
        if line.startswith("SIMULATION TIME ")
        or line.startswith("PLAYER FANTASY ")
        or line.startswith("EVALUATION SCOPE kind=ordinary-play ")
        or line.startswith("EVALUATION SCOPE kind=controlled-capability ")
    ]
    selected.extend(ordinary_gameplay_summary(lines))
    loop_evidence = player_loop_evidence(lines)
    if loop_evidence is not None:
        selected.append(loop_evidence)
    selected.extend(controlled_gameplay_summary(lines))
    return "\n".join(selected)
