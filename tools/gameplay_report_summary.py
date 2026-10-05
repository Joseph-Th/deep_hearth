"""Compact factual summaries for gameplay exploration transcripts."""

from __future__ import annotations

import os
import re

from tools.gameplay_summary.common import compact_fields, field
from tools.gameplay_summary.controlled import controlled_gameplay_summary
from tools.gameplay_summary.fieldwork import fieldwork_summary
from tools.gameplay_summary.foundry_bootstrap import foundry_bootstrap_summary
from tools.gameplay_summary.liberation import liberation_summary
from tools.gameplay_summary.loop import player_loop_evidence
from tools.gameplay_summary.power import power_provider_summary
from tools.gameplay_summary.progression import progression_summary
from tools.gameplay_summary.settlement import settlement_summary
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
        settlement_summary(lines),
        foundry_bootstrap_summary(lines),
        survival_summary(lines),
    ):
        if summary is not None:
            summaries.append(summary)
    return summaries


def settlement_specialization_summary(lines: list[str]) -> str | None:
    """Summarize later-workshop capital decisions that execute outside the main settlement probe."""

    experiences = [
        line for line in lines if line.startswith("SETTLEMENT MACHINE EXPERIENCE ")
    ]
    if not experiences:
        return None
    families = sorted(
        value
        for line in experiences
        if (value := field(line, "family")) is not None
    )
    attention_saved = [
        int(match.group(1))
        for line in experiences
        if (match := re.search(r"\battention-saved:(\d+)t", line)) is not None
    ]
    delegated = [
        int(match.group(1))
        for line in experiences
        if (match := re.search(r"\bdelegated:(\d+)t", line)) is not None
    ]
    span = lambda values: f"{min(values)}..{max(values)}t" if values else "n/a"
    short_kept = sum(
        re.search(r"\bshort=\[[^\]]*\bchoice:keep-prior\]", line) is not None
        for line in experiences
    )
    project_upgraded = sum(
        re.search(r"\bproject=\[[^\]]*\bchoice:upgrade\]", line) is not None
        for line in experiences
    )
    return (
        "GAMEPLAY settlement-specialization "
        f"families=[{','.join(families)}] "
        f"short-kept-prior:{short_kept}/{len(experiences)} "
        f"project-upgraded:{project_upgraded}/{len(experiences)} "
        f"attention-saved:{span(attention_saved)} delegated:{span(delegated)}"
    )


_ORDINARY_DIGEST_FIELDS = {
    "primitive-progression": (
        "first-copper",
        "organic-first-copper",
        "feed-reassessment",
        "processing-crossover",
        "disclosed-order-attention",
    ),
    "primitive-liberation": (
        "cleanup-executed",
        "native-copper",
        "kit-acquisition",
        "kit-lifecycle",
        "organic-lifecycle",
        "remaining-frontier",
    ),
    "woodworking": (
        "choice",
        "organic-choice",
        "decision-coverage",
        "attention-payback",
        "timber-saving",
        "saw-feedback",
        "saw-calibration",
    ),
    "fieldwork": (
        "outcomes",
        "organic-outcomes",
        "reserve-knowledge",
        "organic-reserve-knowledge",
        "orders",
        "pacing-physical",
        "heavy-tool-market",
        "initial-shortfall-campaign",
    ),
    "power-provider": (
        "choice",
        "organic-choice",
        "organic-workload",
        "scale",
        "policy-gap",
        "lifecycle-obligations",
        "settlement-choice",
        "organic-settlement-choice",
        "settlement-copper-policy",
        "settlement-lifecycle-obligations",
    ),
    "settlement": (
        "choice",
        "organic-play",
        "prior-wear",
        "baseline-crossover",
        "demand",
        "payoff",
        "followup-payoff",
    ),
    "foundry-bootstrap": (
        "choice",
        "organic-play",
        "inherited-condition",
        "copper",
        "recovery",
        "workshop-reuse",
        "settlement-batch",
    ),
    "survival": (
        "pressure",
        "diet",
        "provisioning",
        "balanced-diet-counterfactual",
        "inherited-preservation",
        "preservation-opportunity",
        "selected-preservation",
        "organic-preservation",
    ),
}

_SCOPED_ORDINARY_DIGEST_FIELDS = {
    "primitive-progression": (
        *_ORDINARY_DIGEST_FIELDS["primitive-progression"],
        "stockpile-alt",
        "executed-manual-fallback",
        "integrated-campaign",
        "reinvestment",
        "next-stage-continuation",
    ),
    "power-provider": (
        *_ORDINARY_DIGEST_FIELDS["power-provider"],
        "market",
        "settlement-market",
    ),
    "settlement": (
        *_ORDINARY_DIGEST_FIELDS["settlement"],
        "attention",
        "mechanization",
    ),
    "foundry-bootstrap": (
        "choice",
        "organic-play",
        "inherited-condition",
        "copper",
        "recovery",
        "investment",
        "workshop-reuse",
        "mold",
        "settlement-batch",
        "rhythm",
        "attention",
    ),
    "survival": (*_ORDINARY_DIGEST_FIELDS["survival"], "work-interlock"),
}

_EXPECTED_ORDINARY_BY_PROBE = {
    "primitive-progression": {"primitive-progression"},
    "primitive-liberation": {"primitive-liberation"},
    "woodworking": {"woodworking"},
    "fieldwork": {"fieldwork"},
    "power-provider": {"power-provider"},
    "settlement": {"settlement"},
    "foundry-bootstrap": {"foundry-bootstrap"},
    "survival-provisioning": {"survival"},
}
_EXPECTED_CONTROLLED_BY_PROBE = {
    "ore-preparation": {"ore"},
    "foundry": {"foundry"},
}
_SPATIAL_PROXY_PROBES = {"primitive-progression", "fieldwork"}


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


_ORGANIC_REPLACES = {
    "primitive-progression": {"first-copper"},
    "primitive-liberation": {"kit-lifecycle"},
    "woodworking": {"choice"},
    "fieldwork": {"outcomes", "reserve-knowledge"},
    "power-provider": {"choice", "settlement-choice"},
    "settlement": {"choice"},
    "foundry-bootstrap": {"choice"},
}


def _sample_count(summary: str, role: str) -> int:
    shape = field(summary, "sample-shape")
    if shape is None:
        return 0
    match = re.search(rf"\b{re.escape(role)}:(\d+)", shape)
    return int(match.group(1)) if match is not None else 0


def _evidence_fields(summary: str, probe: str, fields: tuple[str, ...]) -> tuple[str, ...]:
    """Prefer organic decision evidence without hiding maintained coverage shape."""

    organic = _sample_count(summary, "organic")
    if organic == 0:
        return tuple(
            name
            for name in fields
            if not name.startswith("organic-") and name != "organic-play"
        )
    replaced = _ORGANIC_REPLACES.get(probe, set())
    return tuple(name for name in fields if name not in replaced)


def _digest_summary(summary: str, *, scoped: bool = False) -> str:
    if summary.startswith("ORDINARY SUMMARY "):
        probe = field(summary, "probe")
        if probe is None:
            return summary
        scope = (
            " scope=spatial-proxy"
            if probe in _SPATIAL_PROXY_PROBES
            else ""
        )
        if probe == "fieldwork":
            experience_fields = _evidence_fields(summary, probe, (
                "sample-shape",
                "outcomes",
                "organic-outcomes",
                "orders",
                "reserve-knowledge",
                "organic-reserve-knowledge",
                "knowledge",
                "pacing-physical",
                "reuse-physical",
                "depletion-adaptation",
            ))
            if not scoped:
                experience_fields = tuple(
                    name
                    for name in experience_fields
                    if name not in {"orders", "pacing-physical", "reuse-physical"}
                )
            knowledge = field(summary, "knowledge")
            if knowledge is not None and knowledge.startswith("[frame=[n:0 "):
                experience_fields = tuple(
                    name for name in experience_fields if name != "knowledge"
                )
            if scoped:
                overview = compact_fields(
                    summary,
                    tuple(
                        name
                        for name in experience_fields
                        if name
                        in {
                            "sample-shape",
                            "outcomes",
                            "organic-outcomes",
                            "orders",
                            "reserve-knowledge",
                            "organic-reserve-knowledge",
                        }
                    ),
                ).replace("reserve-knowledge=", "reserve=")
                information = compact_fields(
                    summary,
                    tuple(name for name in experience_fields if name == "knowledge"),
                ).replace("knowledge=", "info=")
                timing = compact_fields(
                    summary,
                    tuple(
                        name
                        for name in experience_fields
                        if name in {"pacing-physical", "reuse-physical"}
                    ),
                )
                adaptation = compact_fields(
                    summary,
                    ("geology", "tools", "depletion-adaptation"),
                )
                market = compact_fields(
                    summary,
                    ("survey-campaign", "heavy-tool-market"),
                )
                recovery = compact_fields(summary, ("shortfall-recovery",))
                sections = [f"GAMEPLAY fieldwork{scope} {overview}".rstrip()]
                for label, detail in (
                    ("fieldwork-info", information),
                    ("fieldwork-timing", timing),
                    ("fieldwork-adaptation", adaptation),
                    ("fieldwork-market", market),
                ):
                    if detail:
                        sections.append(f"GAMEPLAY {label} {detail}")
                if "gain:0/0" not in recovery:
                    sections.append(f"GAMEPLAY fieldwork-recovery {recovery}".rstrip())
                return "\n".join(sections)
            experience = compact_fields(
                summary,
                experience_fields,
            )
            experience = experience.replace("reserve-knowledge=", "reserve=").replace(
                "knowledge=", "info="
            )
            adaptation = compact_fields(
                summary,
                (
                    "geology",
                    "tools",
                    "survey-campaign",
                    "heavy-tool-market",
                ),
            )
            recovery = compact_fields(summary, ("shortfall-recovery",))
            sections = [
                f"GAMEPLAY fieldwork{scope} {experience}".rstrip(),
                f"GAMEPLAY fieldwork-adaptation {adaptation}".rstrip(),
            ]
            if "gain:0/0" not in recovery:
                sections.append(f"GAMEPLAY fieldwork-recovery {recovery}".rstrip())
            return "\n".join(sections)
        fields = (
            _SCOPED_ORDINARY_DIGEST_FIELDS.get(probe)
            if scoped
            else None
        ) or _ORDINARY_DIGEST_FIELDS.get(probe, ("samples",))
        fields = _evidence_fields(summary, probe, fields)
        if "sample-shape" not in fields and field(summary, "sample-shape") is not None:
            fields = ("sample-shape", *fields)
        detail = compact_fields(summary, fields)
        return f"GAMEPLAY {probe}{scope} {detail}".rstrip()

    if summary.startswith("PLAYER LOOP EVIDENCE "):
        evidence_shape = field(summary, "evidence-shape")
        continuity = ""
        marker = "single-state-progression:"
        if evidence_shape is not None and marker in evidence_shape:
            ratio = evidence_shape.split(marker, 1)[1].split(" ", 1)[0].rstrip("]")
            continuity = f"continuity=[primitive:{ratio} later:separate] "
        shape = (
            f"evidence=[single-state-progression:{ratio} later-domain-episodes:separate]"
            if continuity
            else compact_fields(summary, ("evidence-shape",))
        )
        core = compact_fields(
            summary,
            (
                "bootstrap-boundary",
                "observe-infer",
                "prepare-invest",
                "extract",
                "world-feedback",
                "delegate",
                "reassess-reinvest",
            ),
        )
        investment = compact_fields(summary, ("thermal-bootstrap",))
        dynamics = compact_fields(
            summary,
            (
                "survive-adapt",
                "maintain-recover",
                "choice-diversity",
            ),
        )
        return (
            f"GAMEPLAY loop-shape {shape}".rstrip()
            + "\n"
            + f"GAMEPLAY loop {continuity}{core}".rstrip()
            + "\n"
            + f"GAMEPLAY loop-dynamics {dynamics} {investment}".rstrip()
        )

    if summary.startswith("CONTROLLED SUMMARY probe=workshop "):
        return (
            "CAPABILITY probe=workshop "
            + compact_fields(
                summary, ("scenarios", "sample-shape", "orders", "stops", "recovery")
            )
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


def _without_cargo_test_wrapper(stdout: str) -> str:
    """Remove Cargo test-runner boilerplate while retaining gameplay evidence verbatim."""

    lines = stdout.splitlines()
    if not any(line.strip().startswith("test result: ") for line in lines):
        return stdout.rstrip()

    def is_wrapper_line(line: str) -> bool:
        stripped = line.strip()
        if stripped.startswith("test result: "):
            return True
        if stripped.startswith("running "):
            count, separator, noun = stripped.removeprefix("running ").partition(" ")
            if separator and count.isdecimal() and noun in {"test", "tests"}:
                return True
        return bool(stripped) and set(stripped) == {"."}

    return "\n".join(line for line in lines if not is_wrapper_line(line)).strip()


def concise_gameplay_report(stdout: str, environ=None) -> str:
    """Return a decision-oriented digest; verbose retains every gameplay evidence line."""

    environment = os.environ if environ is None else environ
    if environment.get("DEEP_HEARTH_GAMEPLAY_VERBOSE") is not None or environment.get(
        "DEEP_HEARTH_GAMEPLAY_TRACE"
    ) is not None:
        return _without_cargo_test_wrapper(stdout)
    lines = stdout.splitlines()
    selected = [line for line in lines if line.startswith("SIMULATION TIME ")]
    ordinary = ordinary_gameplay_summary(lines)
    controlled = controlled_gameplay_summary(lines)
    _require_summary_coverage(lines, ordinary, controlled)
    scoped_ordinary = len(ordinary) == 1
    if len(ordinary) == len(_ORDINARY_DIGEST_FIELDS):
        selected.extend(line for line in lines if line.startswith("DESIGN TARGET "))
    ordinary_digests = [
        _digest_summary(summary, scoped=scoped_ordinary) for summary in ordinary
    ]
    specialization = settlement_specialization_summary(lines)
    if specialization is not None:
        specialization_detail = specialization.removeprefix(
            "GAMEPLAY settlement-specialization "
        )
        for index, digest in enumerate(ordinary_digests):
            if digest.startswith("GAMEPLAY settlement "):
                ordinary_digests[index] = (
                    f"{digest} specialization=[{specialization_detail}]"
                )
                break
        else:
            ordinary_digests.append(specialization)
    selected.extend(ordinary_digests)
    # The player-loop digest is cross-system evidence. A scoped report intentionally omits
    # unrelated probe families, so synthesizing the loop from partial evidence would fill it with
    # misleading zero/n/a sections. Emit it only when every ordinary probe family is present.
    if len(ordinary) == len(_ORDINARY_DIGEST_FIELDS):
        loop_evidence = player_loop_evidence(lines)
        if loop_evidence is not None:
            selected.append(_digest_summary(loop_evidence))
    selected.extend(_digest_summary(summary) for summary in controlled)
    return "\n".join(selected)
