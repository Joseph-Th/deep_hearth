"""Power-provider summary for ordinary gameplay evidence."""

from __future__ import annotations

import re

from .common import organic_only, sample_shape, scaled_span


EXECUTED_SCOPE_MARKER = (
    "evidence=[build+first-charge:executed "
    "lifecycle:projected-canonical consumer:not-instantiated]"
)


def _span(values: list[int], unit: str = "") -> str:
    return f"{min(values)}..{max(values)}{unit}" if values else "n/a"


def _selected_workload_span(
    candidate_lines: list[str],
    selected: str,
    pattern: str,
) -> str:
    values = []
    for line in candidate_lines:
        decision = re.search(r"\bdecision=\[selected:([^\s\]]+)", line)
        workload = re.search(pattern, line)
        if (
            decision is not None
            and workload is not None
            and decision.group(1) == selected
        ):
            values.append(int(workload.group(1)))
    return _span(values)


def _lifecycle_values(
    lines: list[str],
    first: str,
    second: str,
) -> dict[str, dict[str, list[int]]]:
    values = {
        first: {"energy": [], "hydration": [], "condition": []},
        second: {"energy": [], "hydration": [], "condition": []},
    }
    pattern = re.compile(
        rf"projected-lifecycle=\[{re.escape(first)}:body:(\d+)nJ/(\d+)uL condition:(\d+)ppm "
        rf"{re.escape(second)}:body:(\d+)nJ/(\d+)uL condition:(\d+)ppm\]"
    )
    for line in lines:
        match = pattern.search(line)
        if match is None:
            continue
        for provider, offset in ((first, 1), (second, 4)):
            values[provider]["energy"].append(int(match.group(offset)))
            values[provider]["hydration"].append(int(match.group(offset + 1)))
            values[provider]["condition"].append(int(match.group(offset + 2)))
    return values


def _primitive_evidence(power: list[str]) -> str:
    organic_power = organic_only(power)
    pristine_break_evens = [
        int(match.group(1))
        for line in power
        if (match := re.search(r"pristine-rate-break-even:(\d+)", line)) is not None
    ]
    decision_crossovers = [
        int(match.group(1))
        for line in power
        if (match := re.search(r"wear-aware-decision-crossover:(\d+)", line)) is not None
    ]
    planned_charges = [
        int(match.group(1))
        for line in power
        if (match := re.search(r"planned-charges:(\d+)", line)) is not None
    ]
    metabolic_wins = 0
    build_body = {"crank": [], "treadle": []}
    build_hydration = {"crank": [], "treadle": []}
    for line in power:
        crank_cost = re.search(r"metabolic-crank:(\d+)nJ", line)
        treadle_cost = re.search(r"metabolic-treadle:(\d+)nJ", line)
        if (
            crank_cost is not None
            and treadle_cost is not None
            and int(treadle_cost.group(1)) < int(crank_cost.group(1))
        ):
            metabolic_wins += 1
        for provider in ("crank", "treadle"):
            match = re.search(
                rf"\b{provider}=\[.*?build-body:(\d+)nJ/(\d+)uL", line
            )
            if match is not None:
                build_body[provider].append(int(match.group(1)))
                build_hydration[provider].append(int(match.group(2)))

    lifecycle = _lifecycle_values(power, "crank", "treadle")
    executed_scope = sum(EXECUTED_SCOPE_MARKER in line for line in power)
    return (
        f"choice=[crank:{sum('selected:crank' in line for line in power)} "
        f"treadle:{sum('selected:treadle' in line for line in power)}] "
        f"organic-choice=[crank:{sum('selected:crank' in line for line in organic_power)} "
        f"treadle:{sum('selected:treadle' in line for line in organic_power)}] "
        f"planned-charges={_span(planned_charges)} "
        f"decision-crossover-charges={_span(decision_crossovers)} "
        f"pristine-rate-break-even={_span(pristine_break_evens)} "
        f"choice-load=[crank:{_selected_workload_span(power, 'crank', r'planned-charges:(\d+)')} "
        f"treadle:{_selected_workload_span(power, 'treadle', r'planned-charges:(\d+)')}] "
        f"metabolic-lower-treadle={metabolic_wins} "
        f"build-body=[crank-energy:{scaled_span(build_body['crank'], 1_000_000_000_000, 'kJ')} "
        f"crank-hydration:{scaled_span(build_hydration['crank'], 1_000, 'mL')} "
        f"treadle-energy:{scaled_span(build_body['treadle'], 1_000_000_000_000, 'kJ')} "
        f"treadle-hydration:{scaled_span(build_hydration['treadle'], 1_000, 'mL')}] "
        f"lifecycle-body=[crank-energy:{scaled_span(lifecycle['crank']['energy'], 1_000_000_000_000, 'kJ')} "
        f"crank-hydration:{scaled_span(lifecycle['crank']['hydration'], 1_000, 'mL')} "
        f"treadle-energy:{scaled_span(lifecycle['treadle']['energy'], 1_000_000_000_000, 'kJ')} "
        f"treadle-hydration:{scaled_span(lifecycle['treadle']['hydration'], 1_000, 'mL')}] "
        f"lifecycle-end-condition=[crank:{_span(lifecycle['crank']['condition'], 'ppm')} "
        f"treadle:{_span(lifecycle['treadle']['condition'], 'ppm')}] "
        f"evidence-scope=[first-charge-executed:{executed_scope}/{len(power)} "
        f"repeated-lifecycle-projected:{executed_scope}/{len(power)}]"
    )


def _settlement_evidence(settlement: list[str]) -> str:
    organic_settlement = organic_only(settlement)
    workloads = [
        int(match.group(1))
        for line in settlement
        if (match := re.search(r"planned-charges=(\d+)", line)) is not None
    ]
    pristine_break_evens = [
        int(match.group(1))
        for line in settlement
        if (match := re.search(r"pristine-rate-break-even:(\d+)charges", line)) is not None
    ]
    decision_crossovers = [
        int(match.group(1))
        for line in settlement
        if (match := re.search(r"wear-aware-decision-crossover:(\d+)charges", line)) is not None
    ]
    lifecycle = _lifecycle_values(settlement, "treadle", "walking-wheel")
    executed_scope = sum(EXECUTED_SCOPE_MARKER in line for line in settlement)
    return (
        f"settlement-choice=[treadle:{sum('selected:treadle' in line for line in settlement)} "
        f"walking:{sum('selected:walking-wheel' in line for line in settlement)}] "
        f"organic-settlement-choice=[treadle:{sum('selected:treadle' in line for line in organic_settlement)} "
        f"walking:{sum('selected:walking-wheel' in line for line in organic_settlement)}] "
        f"settlement-planned-charges={_span(workloads)} "
        f"settlement-decision-crossover-charges={_span(decision_crossovers)} "
        f"settlement-pristine-rate-break-even={_span(pristine_break_evens)} "
        f"settlement-load=[treadle:{_selected_workload_span(settlement, 'treadle', r'planned-charges=(\d+)')} "
        f"walking:{_selected_workload_span(settlement, 'walking-wheel', r'planned-charges=(\d+)')}] "
        f"settlement-lifecycle-body=[treadle-energy:{scaled_span(lifecycle['treadle']['energy'], 1_000_000_000_000, 'kJ')} "
        f"treadle-hydration:{scaled_span(lifecycle['treadle']['hydration'], 1_000, 'mL')} "
        f"walking-energy:{scaled_span(lifecycle['walking-wheel']['energy'], 1_000_000_000_000, 'kJ')} "
        f"walking-hydration:{scaled_span(lifecycle['walking-wheel']['hydration'], 1_000, 'mL')}] "
        f"settlement-lifecycle-end-condition=[treadle:{_span(lifecycle['treadle']['condition'], 'ppm')} "
        f"walking:{_span(lifecycle['walking-wheel']['condition'], 'ppm')}] "
        f"settlement-evidence-scope=[first-charge-executed:{executed_scope}/{len(settlement)} "
        f"repeated-lifecycle-projected:{executed_scope}/{len(settlement)}]"
    )


def power_provider_summary(lines: list[str]) -> str | None:
    power = [line for line in lines if line.startswith("POWER PROVIDER EXPERIENCE ")]
    if not power:
        return None
    settlement = [line for line in lines if line.startswith("POWER SETTLEMENT ")]
    return (
        "ORDINARY SUMMARY probe=power-provider "
        f"samples={len(power)} sample-shape=[{sample_shape(power)}] "
        "workload-source=declared-charge-horizon "
        f"{_primitive_evidence(power)} "
        f"{_settlement_evidence(settlement)}"
    )
