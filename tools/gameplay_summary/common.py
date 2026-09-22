"""Shared parsing helpers for gameplay transcript summaries."""

from __future__ import annotations

import re


def field(line: str, name: str) -> str | None:
    """Extract one top-level whitespace-delimited key=value field."""

    match = re.search(rf"(?:^|\s){re.escape(name)}=(\[[^]]*\]|\S+)", line)
    return match.group(1) if match is not None else None


def compact_fields(line: str, names: tuple[str, ...]) -> str:
    return " ".join(
        f"{name}={value}"
        for name in names
        if (value := field(line, name)) is not None
    )


def sample_shape(sample_lines: list[str]) -> str:
    return (
        f"anchor:{sum(' sample=anchor ' in line for line in sample_lines)} "
        f"coverage:{sum(' sample=coverage ' in line for line in sample_lines)} "
        f"organic:{sum(' sample=organic ' in line for line in sample_lines)} "
        f"replay:{sum(' sample=replay ' in line for line in sample_lines)}"
    )


def organic_only(sample_lines: list[str]) -> list[str]:
    return [line for line in sample_lines if " sample=organic " in line]


def scaled_span(values: list[int], divisor: int, unit: str) -> str:
    """Render exact integer evidence in a compact player-scale unit."""

    if not values:
        return "n/a"

    def scaled(value: int) -> str:
        whole, remainder = divmod(value, divisor)
        if remainder == 0:
            return str(whole)
        tenths = (remainder * 10 + divisor // 2) // divisor
        return str(whole + 1) if tenths == 10 else f"{whole}.{tenths}"

    return f"{scaled(min(values))}..{scaled(max(values))}{unit}"


def physical_tick_microseconds(lines: list[str]) -> int | None:
    for line in lines:
        match = re.search(r"^SIMULATION TIME physical-tick-us=(\d+)$", line)
        if match is not None:
            return int(match.group(1))
    return None


def format_physical_duration(ticks: int, tick_microseconds: int) -> str:
    """Mirror the harness' integer-only player-facing duration formatter."""

    microseconds = ticks * tick_microseconds
    if microseconds >= 60_000_000:
        tenths = microseconds // 6_000_000
        return f"{tenths // 10}.{tenths % 10}m"
    tenths = microseconds // 100_000
    return f"{tenths // 10}.{tenths % 10}s"


def physical_duration_span(
    lines: list[str],
    tick_values: list[int],
    fallback: str = "n/a",
) -> str:
    if not tick_values:
        return fallback
    tick_microseconds = physical_tick_microseconds(lines)
    if tick_microseconds is None:
        return fallback
    minimum = format_physical_duration(min(tick_values), tick_microseconds)
    maximum = format_physical_duration(max(tick_values), tick_microseconds)
    minimum_match = re.fullmatch(r"([0-9.]+)([a-z]+)", minimum)
    maximum_match = re.fullmatch(r"([0-9.]+)([a-z]+)", maximum)
    if (
        minimum_match is not None
        and maximum_match is not None
        and minimum_match.group(2) == maximum_match.group(2)
    ):
        return f"{minimum_match.group(1)}..{maximum_match.group(1)}{minimum_match.group(2)}"
    return f"{minimum}..{maximum}"
