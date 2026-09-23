"""Power-provider summary for ordinary gameplay evidence."""

from __future__ import annotations

import re

from .common import organic_only, sample_shape, scaled_span


EXECUTED_CYCLE_MARKER = "evidence=[build+charge+productive-discharge+recharge:executed "


def _span(values: list[int], unit: str = "") -> str:
    return f"{min(values)}..{max(values)}{unit}" if values else "n/a"


def _numeric_values(lines: list[str], pattern: str) -> list[int]:
    matcher = re.compile(pattern)
    values: list[int] = []
    for line in lines:
        match = matcher.search(line)
        if match is not None:
            values.append(int(match.group(1)))
    return values


def _selected_project_mass(
    lines: list[str],
    consumer: str,
    selected: str,
) -> list[int]:
    matcher = re.compile(
        rf"project=\[consumer:{re.escape(consumer)} feed:(\d+)mg"
    )
    marker = f"decision=[selected:{selected} "
    values: list[int] = []
    for line in lines:
        if marker not in line:
            continue
        match = matcher.search(line)
        if match is not None:
            values.append(int(match.group(1)))
    return values


def _build_body_values(
    lines: list[str],
    providers: tuple[str, str],
) -> tuple[dict[str, list[int]], dict[str, list[int]]]:
    energy = {provider: [] for provider in providers}
    hydration = {provider: [] for provider in providers}
    for line in lines:
        for provider in providers:
            match = re.search(
                rf"\b{re.escape(provider)}=\[.*?build-body:(\d+)nJ/(\d+)uL",
                line,
            )
            if match is not None:
                energy[provider].append(int(match.group(1)))
                hydration[provider].append(int(match.group(2)))
    return energy, hydration


def _productive_cycle_values(
    lines: list[str],
    consumer: str,
    first: str,
    second: str,
) -> tuple[int, int, list[int], int]:
    executed = 0
    projected = 0
    duration: list[int] = []
    second_recharges = 0
    cycle_pattern = re.compile(
        rf"productive-cycle=\[consumer:{re.escape(consumer)} "
        rf"{re.escape(first)}:(\d+)t {re.escape(second)}:\d+t\]"
    )
    for line in lines:
        if EXECUTED_CYCLE_MARKER in line and f"consumer:{consumer}]" in line:
            executed += 1
        if "long-horizon:projected-canonical" in line:
            projected += 1
        match = cycle_pattern.search(line)
        if match is not None:
            duration.append(int(match.group(1)))
        if len(re.findall(r"\bsecond-charge:\d+t", line)) == 2:
            second_recharges += 1
    return executed, projected, duration, second_recharges


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
    pristine_break_evens = _numeric_values(power, r"pristine-rate-break-even:(\d+)")
    decision_crossovers = _numeric_values(
        power, r"wear-aware-decision-crossover:(\d+)"
    )
    project_mass = _numeric_values(
        power, r"project=\[consumer:stone-crusher feed:(\d+)mg"
    )
    project_work = _numeric_values(power, r"\bwork:(\d+)nJ")
    charge_events = _numeric_values(power, r"charge-events:(\d+)")
    metabolic_wins = 0
    for line in power:
        crank_cost = re.search(r"metabolic-crank:(\d+)nJ", line)
        treadle_cost = re.search(r"metabolic-treadle:(\d+)nJ", line)
        if (
            crank_cost is not None
            and treadle_cost is not None
            and int(treadle_cost.group(1)) < int(crank_cost.group(1))
        ):
            metabolic_wins += 1

    build_body, build_hydration = _build_body_values(power, ("crank", "treadle"))
    lifecycle = _lifecycle_values(power, "crank", "treadle")
    executed_cycles, projected_horizons, consumer_ticks, second_charge_pairs = (
        _productive_cycle_values(power, "stone-crusher", "crank", "treadle")
    )
    crank_load = _selected_project_mass(power, "stone-crusher", "crank")
    treadle_load = _selected_project_mass(power, "stone-crusher", "treadle")
    return (
        f"choice=[crank:{sum('selected:crank' in line for line in power)} "
        f"treadle:{sum('selected:treadle' in line for line in power)}] "
        f"organic-choice=[crank:{sum('selected:crank' in line for line in organic_power)} "
        f"treadle:{sum('selected:treadle' in line for line in organic_power)}] "
        f"project=[crusher-feed:{scaled_span(project_mass, 1_000_000, 'kg')} "
        f"mechanical-work:{scaled_span(project_work, 1_000_000_000_000, 'kJ')} "
        f"charge-events:{_span(charge_events)}] "
        f"decision-crossover-charges={_span(decision_crossovers)} "
        f"pristine-rate-break-even={_span(pristine_break_evens)} "
        f"choice-load=[crank:{scaled_span(crank_load, 1_000_000, 'kg')} "
        f"treadle:{scaled_span(treadle_load, 1_000_000, 'kg')}] "
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
        f"productive-cycle=[consumer:stone-crusher executed:{executed_cycles}/{len(power)} "
        f"consumer-duration:{_span(consumer_ticks, 't')} "
        f"carried-state-recharge:{second_charge_pairs}/{len(power)}] "
        f"evidence-scope=[productive-cycle-executed:{executed_cycles}/{len(power)} "
        f"long-horizon-projected:{projected_horizons}/{len(power)}]"
    )


def _settlement_evidence(settlement: list[str]) -> str:
    organic_settlement = organic_only(settlement)
    project_mass = _numeric_values(
        settlement, r"project=\[consumer:powered-saw feed:(\d+)mg"
    )
    project_work = _numeric_values(settlement, r"\bwork:(\d+)nJ")
    charge_events = _numeric_values(settlement, r"charge-events:(\d+)")
    pristine_break_evens = _numeric_values(
        settlement, r"pristine-rate-break-even:(\d+)charges"
    )
    decision_crossovers = _numeric_values(
        settlement, r"wear-aware-decision-crossover:(\d+)charges"
    )
    lifecycle = _lifecycle_values(settlement, "treadle", "walking-wheel")
    executed_cycles, projected_horizons, consumer_ticks, second_charge_pairs = (
        _productive_cycle_values(
            settlement, "powered-saw", "treadle", "walking"
        )
    )
    treadle_load = _selected_project_mass(settlement, "powered-saw", "treadle")
    walking_load = _selected_project_mass(
        settlement, "powered-saw", "walking-wheel"
    )
    return (
        f"settlement-choice=[treadle:{sum('selected:treadle' in line for line in settlement)} "
        f"walking:{sum('selected:walking-wheel' in line for line in settlement)}] "
        f"organic-settlement-choice=[treadle:{sum('selected:treadle' in line for line in organic_settlement)} "
        f"walking:{sum('selected:walking-wheel' in line for line in organic_settlement)}] "
        f"settlement-project=[lumber-feed:{scaled_span(project_mass, 1_000_000, 'kg')} "
        f"mechanical-work:{scaled_span(project_work, 1_000_000_000_000, 'kJ')} "
        f"charge-events:{_span(charge_events)}] "
        f"settlement-decision-crossover-charges={_span(decision_crossovers)} "
        f"settlement-pristine-rate-break-even={_span(pristine_break_evens)} "
        f"settlement-load=[treadle:{scaled_span(treadle_load, 1_000_000, 'kg')} "
        f"walking:{scaled_span(walking_load, 1_000_000, 'kg')}] "
        f"settlement-lifecycle-body=[treadle-energy:{scaled_span(lifecycle['treadle']['energy'], 1_000_000_000_000, 'kJ')} "
        f"treadle-hydration:{scaled_span(lifecycle['treadle']['hydration'], 1_000, 'mL')} "
        f"walking-energy:{scaled_span(lifecycle['walking-wheel']['energy'], 1_000_000_000_000, 'kJ')} "
        f"walking-hydration:{scaled_span(lifecycle['walking-wheel']['hydration'], 1_000, 'mL')}] "
        f"settlement-lifecycle-end-condition=[treadle:{_span(lifecycle['treadle']['condition'], 'ppm')} "
        f"walking:{_span(lifecycle['walking-wheel']['condition'], 'ppm')}] "
        f"settlement-productive-cycle=[consumer:powered-saw executed:{executed_cycles}/{len(settlement)} "
        f"consumer-duration:{_span(consumer_ticks, 't')} "
        f"carried-state-recharge:{second_charge_pairs}/{len(settlement)}] "
        f"settlement-evidence-scope=[productive-cycle-executed:{executed_cycles}/{len(settlement)} "
        f"long-horizon-projected:{projected_horizons}/{len(settlement)}]"
    )


def power_provider_summary(lines: list[str]) -> str | None:
    power = [line for line in lines if line.startswith("POWER PROVIDER EXPERIENCE ")]
    if not power:
        return None
    settlement = [line for line in lines if line.startswith("POWER SETTLEMENT ")]
    return (
        "ORDINARY SUMMARY probe=power-provider "
        f"samples={len(power)} sample-shape=[{sample_shape(power)}] "
        "workload-source=declared-consumer-project "
        f"{_primitive_evidence(power)} "
        f"{_settlement_evidence(settlement)}"
    )
