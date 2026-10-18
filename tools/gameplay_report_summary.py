"""Compact factual summaries for gameplay exploration transcripts."""

from __future__ import annotations

import os
import re

from tools import gameplay_targets
from tools.gameplay_summary.common import compact_fields, field
from tools.gameplay_summary.controlled import controlled_gameplay_summary
from tools.gameplay_summary.fieldwork import fieldwork_summary
from tools.gameplay_summary.foundry_bootstrap import foundry_bootstrap_summary
from tools.gameplay_summary.liberation import liberation_summary
from tools.gameplay_summary.loop import player_loop_evidence
from tools.gameplay_summary.player_experience import fieldwork_player_experience
from tools.gameplay_summary.power import power_provider_summary
from tools.gameplay_summary.progression import progression_summary
from tools.gameplay_summary.settlement import settlement_summary
from tools.gameplay_summary.survival import survival_summary
from tools.gameplay_summary.wilderness import wilderness_summary
from tools.gameplay_summary.woodworking import woodworking_summary


def ordinary_gameplay_summary(lines: list[str]) -> list[str]:
    """Return compact measured ordinary-play evidence without adding interpretation."""

    summaries: list[str] = []
    for summary in (
        wilderness_summary(lines),
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


def settlement_specialization_summary(
    lines: list[str], *, include_cadence: bool = False
) -> str | None:
    """Summarize later-workshop capital decisions that execute outside the main settlement probe."""

    manual_experiences = [
        line
        for line in lines
        if line.startswith("SETTLEMENT MANUAL MACHINE EXPERIENCE ")
    ]
    experiences = [
        line for line in lines if line.startswith("SETTLEMENT MACHINE EXPERIENCE ")
    ]
    if not experiences and not manual_experiences:
        return None
    manual_families = sorted(
        value
        for line in manual_experiences
        if (value := field(line, "family")) is not None
    )
    manual_crossovers = [
        int(match.group(1))
        for line in manual_experiences
        if (match := re.search(r"\bcrossover:(\d+)batches", line)) is not None
    ]
    manual_attention_saved = [
        int(match.group(1))
        for line in manual_experiences
        if (match := re.search(r"\battention-saved:(\d+)t", line)) is not None
    ]
    manual_short_kept = sum(
        re.search(r"\bshort=\[.*?\bchoice:keep-prior\]", line) is not None
        for line in manual_experiences
    )
    manual_project_built = sum(
        re.search(r"\bproject=\[.*?\bchoice:build\]", line) is not None
        for line in manual_experiences
    )
    families = sorted(
        value
        for line in experiences
        if (value := field(line, "family")) is not None
    )
    crossovers = [
        int(match.group(1))
        for line in experiences
        if (match := re.search(r"\bcrossover:(\d+)batches", line)) is not None
    ]
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
    charge_events = [
        int(match.group(1))
        for line in experiences
        if (match := re.search(r"\bpower-cycle=\[charges:(\d+)", line)) is not None
    ]
    packed_batches = [
        int(match.group(1))
        for line in experiences
        if (match := re.search(r"\bmax-batches-per-charge:(\d+)", line)) is not None
    ]
    span = lambda values: f"{min(values)}..{max(values)}t" if values else "n/a"
    batch_span = (
        lambda values: f"{min(values)}..{max(values)}batches" if values else "n/a"
    )
    short_kept = sum(
        re.search(r"\bshort=\[.*?\bchoice:keep-prior\]", line) is not None
        for line in experiences
    )
    project_upgraded = sum(
        re.search(r"\bproject=\[.*?\bchoice:upgrade\]", line) is not None
        for line in experiences
    )
    cadence = ""
    if include_cadence and charge_events and packed_batches:
        cadence = (
            " power-cycle=["
            f"charges:{min(charge_events)}..{max(charge_events)} "
            f"max-batches-per-charge:{min(packed_batches)}..{max(packed_batches)}]"
        )
    manual = ""
    if manual_experiences:
        manual = (
            " manual=["
            f"families:{','.join(manual_families)} "
            f"crossover:{batch_span(manual_crossovers)} "
            f"short-kept:{manual_short_kept}/{len(manual_experiences)} "
            f"project-built:{manual_project_built}/{len(manual_experiences)} "
            f"attention-saved:{span(manual_attention_saved)} delegation:none]"
        )
    return (
        "GAMEPLAY settlement-specialization scope=separate-executed-projects "
        f"families=[{','.join(families)}] "
        f"crossover:{batch_span(crossovers)} "
        f"short-kept-prior:{short_kept}/{len(experiences)} "
        f"project-upgraded:{project_upgraded}/{len(experiences)} "
        f"attention-saved:{span(attention_saved)} delegated:{span(delegated)}"
        f"{cadence}{manual}"
    )


_ORDINARY_DIGEST_FIELDS = {
    "wilderness": (
        "movement",
        "direct-use",
        "pressure-route",
        "camp-projects",
        "camp-warmth",
        "shelter",
    ),
    "primitive-progression": (
        "generated-exact-local-campaign",
        "continuous-body",
        "first-copper",
        "organic-first-copper",
        "sequencing-physical",
        "processing-market",
        "disclosed-order-attention",
    ),
    "primitive-liberation": (
        "selected-route",
        "organic-route",
        "attention",
        "native",
        "kit-lifecycle",
        "organic-lifecycle",
        "remaining-frontier",
    ),
    "woodworking": (
        "choice",
        "organic-choice",
        "attention-payback",
        "timber-saving",
        "saw-feedback",
    ),
    "fieldwork": (
        "exact-local-expedition",
        "generated-exact-local",
        "exact-local-depletion-reroute",
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
        "organic-survival",
        "lifecycle-obligations",
        "settlement-choice",
        "organic-settlement-choice",
        "organic-settlement-survival",
        "settlement-copper-policy",
        "settlement-frontier",
    ),
    "settlement": (
        "discovery",
        "organic-play",
        "baseline-crossover",
        "payoff",
        "followup-basis",
        "followup-payoff",
        "storage-scale",
        "shared-portfolio",
    ),
    "foundry-bootstrap": (
        "choice",
        "organic-play",
        "copper",
        "recovery",
        "electrical-transition",
        "workshop-reuse",
        "settlement-batch",
    ),
    "survival": (
        "pressure",
        "diet",
        "provisioning",
        "balanced-diet-counterfactual",
        "preservation-continuity",
        "selected-preservation",
        "organic-preservation",
    ),
}

_SCOPED_ORDINARY_DIGEST_FIELDS = {
    "wilderness": (
        "evidence",
        "generation",
        *_ORDINARY_DIGEST_FIELDS["wilderness"],
        "resource-runway",
        "camp-goals",
        "organic-adaptation",
        "crafting",
        "tool-policy",
        "water",
        "adaptation",
        "timing",
        "forage",
        "provisioning",
        "carrying",
        "storage-choice",
        "tool-choice",
        "material-choice",
    ),
    "primitive-progression": (
        *_ORDINARY_DIGEST_FIELDS["primitive-progression"],
        "sequencing-consequence",
        "processing-market-detail",
        "processing-setup-tradeoff",
        "stockpile-alt",
        "executed-manual-fallback",
        "integrated-campaign",
        "reinvestment",
        "next-stage-continuation",
    ),
    "power-provider": (
        *_ORDINARY_DIGEST_FIELDS["power-provider"],
        "market",
        "settlement-candidates",
        "settlement-off-frontier",
    ),
    "settlement": (
        *_ORDINARY_DIGEST_FIELDS["settlement"],
        "attention",
        "mechanization",
        "power-cycle",
    ),
    "foundry-bootstrap": (
        "choice",
        "organic-play",
        "inherited-condition",
        "copper",
        "recovery",
        "electrical-transition",
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
    "wilderness": {"wilderness"},
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
        evidence_scope = gameplay_targets.GAMEPLAY_REPORT_SCOPE_BY_PROBE.get(probe)
        if evidence_scope is None:
            return summary
        evidence_mode = gameplay_targets.gameplay_evidence_mode(evidence_scope)
        # Exact-local is the ordinary-report default. Only annotate the exceptional proxy mode so
        # evidence boundaries stay visible without repeating the same metadata on every digest row.
        scope = (
            " scope=spatial-proxy"
            if evidence_mode == gameplay_targets.EVIDENCE_ORDINARY_SPATIAL_PROXY
            else ""
        )
        if probe == "wilderness" and not scoped:
            fields = _evidence_fields(
                summary,
                probe,
                (
                    "sample-shape",
                    *(
                        name
                        for name in _ORDINARY_DIGEST_FIELDS["wilderness"]
                        if name != "direct-use"
                    ),
                ),
            )
            overview = compact_fields(summary, fields)
            sections = [f"GAMEPLAY wilderness {overview}".rstrip()]
            direct_use = field(summary, "direct-use")
            if direct_use is not None:
                sections.append(f"GAMEPLAY wilderness-direct-use {direct_use}")
            return "\n".join(sections)
        if probe == "fieldwork":
            experience_fields = _evidence_fields(summary, probe, (
                "sample-shape",
                "exact-local-expedition",
                "generated-exact-local",
                "exact-local-depletion-reroute",
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
                    if name
                    not in {
                        "orders",
                        "knowledge",
                        "pacing-physical",
                        "reuse-physical",
                    }
                )
            knowledge = field(summary, "knowledge")
            if knowledge is not None and " frame=[n:0 " in knowledge:
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
                            "exact-local-expedition",
                            "generated-exact-local",
                            "exact-local-depletion-reroute",
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
                    "survey-campaign",
                    "heavy-tool-market",
                ),
            )
            recovery = compact_fields(summary, ("shortfall-recovery",))
            sections = [f"GAMEPLAY fieldwork{scope} {experience}".rstrip()]
            decision_parts = [part for part in (adaptation, recovery) if part and "gain:0/0" not in part]
            if decision_parts:
                sections.append(f"GAMEPLAY fieldwork-decisions {' '.join(decision_parts)}")
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
        fantasy = compact_fields(summary, ("role", "fantasy-evidence"))
        world_interaction = compact_fields(
            summary,
            (
                "observe-infer",
                "extract",
                "world-feedback",
            ),
        )
        continuity = compact_fields(summary, ("continuity",))
        core = compact_fields(
            summary,
            (
                "bootstrap-boundary",
                "prepare-invest",
                "shared-power",
                "shared-power-projection",
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
            f"GAMEPLAY fantasy {fantasy}".rstrip()
            + "\n"
            + f"GAMEPLAY world-interaction {world_interaction}".rstrip()
            + "\n"
            + f"GAMEPLAY loop {continuity} {core}".rstrip()
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
            "COUNTERFACTUAL probe=agency evidence=counterfactual "
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


def _focused_verbose_gameplay_report(
    stdout: str, seed: str, episode: str | None = None
) -> str:
    """Read one world seed or one explicit episode without mixing unkeyed events.

    All scenarios still run and validate normally. This is output selection, not a reduced
    test workload; global run/context markers remain visible for reproducibility.
    """

    selected = []
    matching_events = 0
    episode_witnesses = 0
    for line in _without_cargo_test_wrapper(stdout).splitlines():
        # A focused *player* replay must not quietly inject evaluator-only geological truth.
        # Full verbose diagnostics are still available when investigating implementation faults.
        if "FIXTURE DIAGNOSTIC" in line:
            continue
        event_seed = field(line, "seed")
        if event_seed is not None:
            if event_seed.startswith("0x") and int(event_seed, 16) == int(seed, 16):
                if episode is not None and field(line, "episode") != episode:
                    continue
                matching_events += 1
                episode_witnesses += int(line.startswith("WILDERNESS EXPERIENCE "))
                selected.append(line)
            continue
        if line.startswith(
            ("SIMULATION TIME ", "PROBE INPUT ", "EVALUATION SCOPE ", "DESIGN TARGET ")
        ):
            selected.append(line)
    if episode is not None and episode_witnesses != 1:
        raise ValueError(
            f"expected one wilderness witness for {seed} episode={episode}, "
            f"found {episode_witnesses}"
        )
    if matching_events == 0:
        raise ValueError(f"no seeded player evidence found for {seed}")
    selected.insert(
        0,
        f"PLAYER REPLAY focus-seed={seed} "
        + (f"episode={episode} " if episode is not None else "")
        + "evidence=seeded-events-only "
        f"events={matching_events} all-samples-executed=true "
        + (
            "continuity=one-wilderness-episode "
            if episode is not None
            else "continuity=not-guaranteed-across-episodes "
        )
        + "diagnostics=excluded "
        "unkeyed-sample-events=omitted",
    )
    return "\n".join(selected)


def concise_gameplay_report(stdout: str, environ=None) -> str:
    """Return a decision-oriented digest; verbose retains every gameplay evidence line."""

    environment = os.environ if environ is None else environ
    if environment.get("DEEP_HEARTH_GAMEPLAY_VERBOSE") is not None or environment.get(
        "DEEP_HEARTH_GAMEPLAY_TRACE"
    ) is not None:
        focused_seed = environment.get("DEEP_HEARTH_GAMEPLAY_FOCUS_SEED")
        if focused_seed is not None:
            return _focused_verbose_gameplay_report(
                stdout, focused_seed, environment.get("DEEP_HEARTH_GAMEPLAY_FOCUS_EPISODE")
            )
        return _without_cargo_test_wrapper(stdout)
    lines = stdout.splitlines()
    selected = [line for line in lines if line.startswith("SIMULATION TIME ")]
    ordinary = ordinary_gameplay_summary(lines)
    controlled = controlled_gameplay_summary(lines)
    _require_summary_coverage(lines, ordinary, controlled)
    scoped_ordinary = len(ordinary) == 1
    ordinary_digests = [
        _digest_summary(summary, scoped=scoped_ordinary) for summary in ordinary
    ]
    specialization = settlement_specialization_summary(lines, include_cadence=False)
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
    # The broad report preserves the lived result without repeating a long receipt.
    # Scoped wilderness exploration retains the full executed meal and finite-source evidence.
    nutrition_witness = next(
        (line for line in lines if line.startswith("WILDERNESS NUTRITION ")),
        None,
    )
    casting_bed_witness = next(
        (line for line in lines if line.startswith("WILDERNESS CASTING BED ")),
        None,
    )
    if nutrition_witness is not None and not scoped_ordinary:
        for index, digest in enumerate(ordinary_digests):
            if digest.startswith("GAMEPLAY wilderness "):
                ordinary_digests[index] = f"{digest} diet=mixed"
                break
    selected.extend(ordinary_digests)
    # Outcome totals conceal the lived cost of a wrong but rational decision. Show the
    # maintained expedition's observation, capital commitment, delayed output, and response
    # next to its aggregate fieldwork metrics, without treating it as a representative seed.
    if any(digest.startswith("GAMEPLAY fieldwork") for digest in ordinary_digests):
        expedition = fieldwork_player_experience(lines)
        if expedition is not None:
            selected.append(expedition)
    # Keep one lived, need-triggered adaptation alongside the opening digest. Aggregate choice
    # counts cannot show whether an initially sensible restraint can later become useful action.
    if any(digest.startswith("GAMEPLAY wilderness ") for digest in ordinary_digests):
        if scoped_ordinary and nutrition_witness is not None:
            selected.append(nutrition_witness)
        if scoped_ordinary and casting_bed_witness is not None:
            selected.append(casting_bed_witness)
        selected.extend(
            line for line in lines if line.startswith("PLAYER ADAPTATION opening=stay-mobile ")
        )
    # The player-loop digest is cross-system evidence. A scoped report intentionally omits
    # unrelated probe families, so synthesizing the loop from partial evidence would fill it with
    # misleading zero/n/a sections. Emit it only when every ordinary probe family is present.
    if len(ordinary) == len(_ORDINARY_DIGEST_FIELDS):
        loop_evidence = player_loop_evidence(lines)
        if loop_evidence is not None:
            selected.append(_digest_summary(loop_evidence))
    selected.extend(_digest_summary(summary) for summary in controlled)
    return "\n".join(selected)
