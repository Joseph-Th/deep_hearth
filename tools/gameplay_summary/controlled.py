"""Controlled-capability summaries for workshop, ore, and foundry probes."""

from __future__ import annotations

import re

from .common import compact_fields


def _workshop_summary(lines: list[str]) -> str | None:
    workshop = next(
        (line for line in lines if line.startswith("WORKSHOP CAPABILITY ")), None
    )
    if workshop is None:
        return None
    detail = compact_fields(
        workshop,
        ("scenarios", "orders", "adaptive", "stops", "maintenance-blockers"),
    )
    experience = next(
        (line for line in lines if line.startswith("WORKSHOP EXPERIENCE REVIEW ")),
        None,
    )
    if experience is None:
        return f"CONTROLLED SUMMARY probe=workshop {detail}".rstrip()

    parts: list[str] = []
    pressure_shape = re.search(
        r"\bpressure-shape=\[clean:(\d+) single:(\d+) multi-system:(\d+)\]",
        experience,
    )
    if pressure_shape is not None:
        parts.append(
            "pressure-shape=["
            f"clean:{pressure_shape.group(1)} single:{pressure_shape.group(2)} "
            f"multi:{pressure_shape.group(3)}]"
        )
    interlocks = re.search(
        r"\binterlocks=\[stored-work\+throughput:(\d+) body\+power:(\d+) "
        r"wear\+maintenance:(\d+) structure\+production:(\d+)\]",
        experience,
    )
    if interlocks is not None:
        parts.append(
            "interlocks=[stored-work:"
            f"{interlocks.group(1)} body-power:{interlocks.group(2)} "
            f"wear-maintenance:{interlocks.group(3)} "
            f"structure-production:{interlocks.group(4)}]"
        )
    recovery = re.search(
        r"\brecovery=\[suspensions:(\d+) resumed:(\d+) stranded:(\d+)\]",
        experience,
    )
    if recovery is not None:
        parts.append(
            "recovery=[suspended:"
            f"{recovery.group(1)} resumed:{recovery.group(2)} "
            f"stranded:{recovery.group(3)}]"
        )
    suffix = f" {' '.join(parts)}" if parts else ""
    return f"CONTROLLED SUMMARY probe=workshop {detail}{suffix}".rstrip()


def _agency_summary(lines: list[str]) -> str | None:
    agency = next((line for line in lines if line.startswith("AGENCY SUMMARY ")), None)
    if agency is None:
        return None
    detail = compact_fields(
        agency,
        ("worlds", "worlds-with-multiple-signatures", "observed-counterfactual-effects"),
    )
    return f"CONTROLLED SUMMARY probe=agency {detail}".rstrip()


def _ore_summary(lines: list[str]) -> str | None:
    ore_lines = [
        line
        for line in lines
        if line.startswith("CAPABILITY ORE_PREP ") or line.startswith("ORE REVIEW ")
    ]
    ore_completed = [line for line in ore_lines if " outcome=completed " in line]
    ore_stopped = [line for line in ore_lines if " outcome=stopped " in line]
    if not ore_completed and not ore_stopped:
        return None
    feed_signatures = {
        line.split(" feed=[", 1)[1].split("]", 1)[0]
        for line in ore_completed
        if " feed=[" in line
    }
    return (
        "ORE CAPABILITY SUMMARY "
        f"samples={len(ore_completed) + len(ore_stopped)} "
        f"completed={len(ore_completed)} stopped={len(ore_stopped)} "
        f"finite-energy-stops={sum('blocker=finite-energy' in line for line in ore_stopped)} "
        f"retryable-energy-stops={sum('blocker=finite-energy' in line and 'retry=stage-input-retained' in line for line in ore_stopped)} "
        f"variable-feed={len(feed_signatures)}"
    )


def _positive_mass_values(lines: list[str], field_name: str) -> list[int]:
    pattern = rf"\b{re.escape(field_name)}=(\d+)mg"
    values = []
    for line in lines:
        match = re.search(pattern, line)
        if match is not None and int(match.group(1)) > 0:
            values.append(int(match.group(1)))
    return values


def _foundry_recovery_counts(foundry: list[str]) -> tuple[int, int, int, int]:
    recovery_casts = len(_positive_mass_values(foundry, "recovery-cast"))
    first_cast_remainders = 0
    recovery_cleared = 0
    molten_stranded = 0
    for line in foundry:
        after_first = re.search(r"\bmolten-after-first=(\d+)mg", line)
        final = re.search(r"\bmolten-final=(\d+)mg", line)
        after_first_mass = int(after_first.group(1)) if after_first is not None else 0
        final_mass = int(final.group(1)) if final is not None else 0
        if after_first_mass > 0:
            first_cast_remainders += 1
            recovery_cleared += final_mass == 0
        molten_stranded += final_mass > 0
    return recovery_casts, first_cast_remainders, recovery_cleared, molten_stranded


def _foundry_summary(lines: list[str]) -> str | None:
    foundry = [
        line
        for line in lines
        if line.startswith("CAPABILITY FOUNDRY ") or line.startswith("FOUNDRY REVIEW ")
    ]
    if not foundry:
        return None

    unmelted_masses = _positive_mass_values(foundry, "unmelted")
    retained_unmelted = sum(
        " feed-retained=true " in line
        and (match := re.search(r"\bunmelted=(\d+)mg", line)) is not None
        and int(match.group(1)) > 0
        for line in foundry
    )
    recovery_casts, remainders, recovery_cleared, stranded = _foundry_recovery_counts(
        foundry
    )
    unmelted_span = (
        f"{min(unmelted_masses)}..{max(unmelted_masses)}mg"
        if unmelted_masses
        else "0mg"
    )
    return (
        "FOUNDRY CAPABILITY SUMMARY "
        f"samples={len(foundry)} "
        f"full={sum(' outcome=full-order-' in line for line in foundry)} "
        f"partial={sum(' outcome=partial-order-' in line for line in foundry)} "
        f"melt-limited={sum('melt-limit=finite-energy' in line for line in foundry)} "
        f"cast-capacity-limited={sum('cast-limit=thermal-sink-capacity' in line for line in foundry)} "
        f"feed-deferred=[orders:{len(unmelted_masses)} retained:{retained_unmelted}/{len(unmelted_masses)} mass:{unmelted_span}] "
        f"cast-recovery=[remainders:{remainders} recovery-casts:{recovery_casts} "
        f"cleared:{recovery_cleared} molten-stranded:{stranded}] "
        f"full-after-cooldown={sum('outcome=full-order-recovered-after-cooldown' in line for line in foundry)}"
    )


def controlled_gameplay_summary(lines: list[str]) -> list[str]:
    """Keep high-value capability evidence visible without dumping capability transcripts."""

    return [
        summary
        for summary in (
            _workshop_summary(lines),
            _agency_summary(lines),
            _ore_summary(lines),
            _foundry_summary(lines),
        )
        if summary is not None
    ]
