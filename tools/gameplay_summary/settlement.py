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
    )
