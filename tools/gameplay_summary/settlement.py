"""Concise ordinary-play evidence for settlement investment decisions."""

from __future__ import annotations

import re

from .common import sample_shape


def _values(lines: list[str], pattern: str) -> list[int]:
    return [
        int(match.group(1))
        for line in lines
        if (match := re.search(pattern, line)) is not None
    ]


def _span(values: list[int], unit: str) -> str:
    return f"{min(values)}..{max(values)}{unit}" if values else "n/a"


def settlement_summary(lines: list[str]) -> str | None:
    settlement = [
        line for line in lines if line.startswith("SETTLEMENT EXPERIENCE ")
    ]
    if not settlement:
        return None

    direct = sum("choice:frame-saw" in line for line in settlement)
    mechanized = sum("choice:sash-sawmill" in line for line in settlement)
    batches = _values(settlement, r"demand=\[batches:(\d+)")
    baseline = _values(settlement, r"baseline:(\d+)t")
    machine = _values(settlement, r"mechanized:(\d+)t")
    setup = _values(settlement, r"setup:(\d+)t")
    delegated = _values(settlement, r"delegated:(\d+)t")
    policy_returns = _values(settlement, r"minimum-return:(\d+)ppm")
    builds = sum(" upgraded:true " in line for line in settlement)
    mechanized_lines = [line for line in settlement if "choice:sash-sawmill" in line]
    saved = _values(mechanized_lines, r"margin:\+(\d+)t")
    mechanized_delegated = _values(mechanized_lines, r"delegated:(\d+)t")
    machine_experiences = [
        line for line in lines if line.startswith("SETTLEMENT MACHINE EXPERIENCE ")
    ]
    machine_families = {
        match.group(1)
        for line in machine_experiences
        if (match := re.search(r"\bfamily=([^ ]+)", line)) is not None
    }
    machine_floors = _values(machine_experiences, r"minimum-attention-return:(\d+)t")
    machine_savings = _values(machine_experiences, r"attention-saved:(\d+)t")
    machine_delegated = _values(machine_experiences, r"delegated:(\d+)t")
    short_kept = sum("choice:keep-prior" in line for line in machine_experiences)
    project_upgrades = sum("choice:upgrade" in line for line in machine_experiences)
    portfolio = ""
    if machine_experiences:
        portfolio = (
            " portfolio=["
            f"families:{len(machine_families)} short-kept:{short_kept}/{len(machine_experiences)} "
            f"project-upgrade:{project_upgrades}/{len(machine_experiences)} "
            f"return-floor:{_span(machine_floors, 't')} "
            f"attention-saved:{_span(machine_savings, 't')} "
            f"delegated:{_span(machine_delegated, 't')}]"
        )
    delegation_experiences = [
        line for line in lines if line.startswith("SETTLEMENT DELEGATION EXPERIENCE ")
    ]
    delegation_families = {
        match.group(1)
        for line in delegation_experiences
        if (match := re.search(r"\bfamily=([^ ]+)", line)) is not None
    }
    delegation_savings = _values(delegation_experiences, r"active-saving:(\d+)t")
    autonomous = _values(delegation_experiences, r"autonomous:(\d+)t")
    stored_work = ""
    if delegation_experiences:
        stored_work = (
            " stored-work=["
            f"families:{len(delegation_families)} active-saving:{_span(delegation_savings, 't')} "
            f"autonomous:{_span(autonomous, 't')}]"
        )

    return (
        "ORDINARY SUMMARY probe=settlement "
        f"samples={len(settlement)} sample-shape=[{sample_shape(settlement)}] "
        f"choice=[frame:{direct} sawmill:{mechanized}] "
        f"investment-policy-return={_span(policy_returns, 'ppm')} "
        f"demand={min(batches)}..{max(batches)}batches "
        f"attention=[baseline:{min(baseline)}..{max(baseline)}t "
        f"mechanized:{min(machine)}..{max(machine)}t "
        f"setup:{min(setup)}..{max(setup)}t] "
        f"payoff=[attention-saved:{_span(saved, 't')} "
        f"delegated:{_span(mechanized_delegated, 't')}] "
        f"mechanization=[builds:{builds}/{len(settlement)} "
        f"delegated:{min(delegated)}..{max(delegated)}t]"
        f"{portfolio}{stored_work}"
    )
