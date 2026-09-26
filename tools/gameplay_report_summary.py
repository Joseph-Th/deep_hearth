"""Compact factual summaries for gameplay exploration transcripts."""

from __future__ import annotations

import os

from tools.gameplay_summary.common import compact_fields, field
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


_ORDINARY_DIGEST_FIELDS = {
    "primitive-progression": (
        "samples",
        "first-copper",
        "processing-crossover",
        "disclosed-order-attention",
        "parallel-work",
        "reinvestment",
        "next-stage-continuation",
    ),
    "primitive-liberation": (
        "samples",
        "cleanup-executed",
        "native-copper",
        "kit-acquisition",
        "kit-decision",
        "first-foundry",
        "remaining-frontier",
        "industrial-foundry-frontier",
    ),
    "woodworking": (
        "samples",
        "choice",
        "decision-coverage",
        "attention-payback",
        "timber-saving",
        "lifecycle-feedback",
    ),
    "fieldwork": (
        "samples",
        "outcomes",
        "reserve-knowledge",
        "orders",
        "heavy-tool-market",
        "initial-shortfall-campaign",
    ),
    "power-provider": (
        "samples",
        "choice",
        "interaction-density",
        "decision-crossover-charges",
        "evidence-scope",
        "settlement-choice",
        "settlement-interaction-density",
        "settlement-decision-crossover-charges",
        "settlement-evidence-scope",
    ),
    "survival": (
        "samples",
        "pressure",
        "preservation",
        "commitment",
        "work-interlock",
    ),
}

_EXPECTED_ORDINARY_BY_PROBE = {
    "primitive-progression": {"primitive-progression", "primitive-liberation"},
    "woodworking": {"woodworking"},
    "fieldwork": {"fieldwork"},
    "power-provider": {"power-provider"},
    "survival-provisioning": {"survival"},
}
_EXPECTED_CONTROLLED_BY_PROBE = {
    "ore-preparation": {"ore"},
    "foundry": {"foundry"},
}


def _controlled_summary_probe(summary: str) -> str | None:
    if summary.startswith("CONTROLLED SUMMARY "):
        return field(summary, "probe")
    if summary.startswith("ORE CAPABILITY SUMMARY "):
        return "ore"
    if summary.startswith("FOUNDRY CAPABILITY SUMMARY "):
        return "foundry"
    return None


def _require_summary_coverage(
    lines: list[str], ordinary: list[str], controlled: list[str]
) -> None:
    """Fail if an executed probe disappeared from the concise parser contract."""

    expected_ordinary: set[str] = set()
    expected_controlled: set[str] = set()
    for line in lines:
        if line.startswith("PROBE INPUT "):
            name = field(line, "name")
            if name is not None:
                expected_ordinary.update(_EXPECTED_ORDINARY_BY_PROBE.get(name, ()))
                expected_controlled.update(_EXPECTED_CONTROLLED_BY_PROBE.get(name, ()))
        elif line.startswith("HARNESS INPUT "):
            expected_controlled.add("workshop")
        elif line.startswith("AGENCY INPUT "):
            expected_controlled.add("agency")

    actual_ordinary = {
        probe
        for summary in ordinary
        if (probe := field(summary, "probe")) is not None
    }
    actual_controlled = {
        probe
        for summary in controlled
        if (probe := _controlled_summary_probe(summary)) is not None
    }
    missing_ordinary = sorted(expected_ordinary - actual_ordinary)
    missing_controlled = sorted(expected_controlled - actual_controlled)
    if missing_ordinary or missing_controlled:
        missing = [
            *(f"ordinary:{probe}" for probe in missing_ordinary),
            *(f"controlled:{probe}" for probe in missing_controlled),
        ]
        raise ValueError(
            "concise gameplay summary lost executed probe evidence: " + ", ".join(missing)
        )


def _digest_summary(summary: str) -> str:
    if summary.startswith("ORDINARY SUMMARY "):
        probe = field(summary, "probe")
        if probe is None:
            return summary
        detail = compact_fields(summary, _ORDINARY_DIGEST_FIELDS.get(probe, ("samples",)))
        return f"GAMEPLAY probe={probe} {detail}".rstrip()

    if summary.startswith("PLAYER LOOP EVIDENCE "):
        detail = compact_fields(
            summary,
            (
                "observe-infer",
                "thermal-bootstrap",
                "world-feedback",
                "delegate",
                "reassess-reinvest",
                "choice-diversity",
            ),
        )
        return f"GAMEPLAY loop {detail}".rstrip()

    if summary.startswith("CONTROLLED SUMMARY probe=workshop "):
        return (
            "CAPABILITY probe=workshop "
            + compact_fields(summary, ("scenarios", "orders", "stops", "recovery"))
        ).rstrip()
    if summary.startswith("CONTROLLED SUMMARY probe=agency "):
        return (
            "CAPABILITY probe=agency "
            + compact_fields(
                summary,
                (
                    "worlds",
                    "worlds-with-multiple-signatures",
                    "observed-counterfactual-effects",
                ),
            )
        ).rstrip()
    if summary.startswith("ORE CAPABILITY SUMMARY "):
        return "CAPABILITY probe=ore " + summary.removeprefix("ORE CAPABILITY SUMMARY ")
    if summary.startswith("FOUNDRY CAPABILITY SUMMARY "):
        return "CAPABILITY probe=foundry " + summary.removeprefix(
            "FOUNDRY CAPABILITY SUMMARY "
        )
    return summary


def concise_gameplay_report(stdout: str, environ=None) -> str:
    """Return a decision-oriented digest; verbose retains every detailed evidence line."""

    environment = os.environ if environ is None else environ
    if environment.get("DEEP_HEARTH_GAMEPLAY_VERBOSE") is not None or environment.get(
        "DEEP_HEARTH_GAMEPLAY_TRACE"
    ) is not None:
        return stdout.rstrip()
    lines = stdout.splitlines()
    selected = [line for line in lines if line.startswith("SIMULATION TIME ")]
    ordinary = ordinary_gameplay_summary(lines)
    controlled = controlled_gameplay_summary(lines)
    _require_summary_coverage(lines, ordinary, controlled)
    selected.extend(_digest_summary(summary) for summary in ordinary)
    # The player-loop digest is cross-system evidence. A scoped report intentionally omits
    # unrelated probe families, so synthesizing the loop from partial evidence would fill it with
    # misleading zero/n/a sections. Emit it only when every ordinary probe family is present.
    if len(ordinary) == len(_ORDINARY_DIGEST_FIELDS):
        loop_evidence = player_loop_evidence(lines)
        if loop_evidence is not None:
            selected.append(_digest_summary(loop_evidence))
    selected.extend(_digest_summary(summary) for summary in controlled)
    return "\n".join(selected)
