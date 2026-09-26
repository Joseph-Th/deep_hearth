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
        if "comparator-lifecycle:projected-canonical" in line:
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
        rf"projected-provider-lifecycle=\[{re.escape(first)}:body:(\d+)nJ/(\d+)uL condition:(\d+)ppm "
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


def _project_experience(lines: list[str], era: str) -> dict[str, list[int]]:
    selected = [
        line
        for line in lines
        if line.startswith("POWER PROJECT EXPERIENCE ") and f" era={era} " in line
    ]
    values = {
        "pristine_charge_events": [],
        "projected_charge_events": [],
        "projected_services": [],
        "charge_events": [],
        "wear_projected_extra_charge_events": [],
        "unplanned_extra_charge_events": [],
        "attention_regret": [],
        "limited_batches": [],
        "attention": [],
        "services": [],
        "service_ticks": [],
        "provisioning_stops": [],
        "provisioning_attention": [],
        "drink_actions": [],
        "meal_actions": [],
        "elapsed": [],
        "food": [],
        "preservation": [],
        "water": [],
    }
    patterns = {
        "pristine_charge_events": r"pristine-charge-events:(\d+)",
        "charge_events": r"executed=\[charge-events:(\d+)",
        "limited_batches": r"survival-limited-batches:(\d+)",
        "attention": r"active-attention:(\d+)t",
        "services": r"maintenance=\[services:(\d+)",
        "service_ticks": r"\bservice:(\d+)t",
        "provisioning_stops": r"provisioning=\[stops:(\d+)",
        "provisioning_attention": r"provisioning=\[stops:\d+ attention:(\d+)t",
        "drink_actions": r"provisioning=\[.*?drinks:(\d+)",
        "meal_actions": r"provisioning=\[.*?meals:(\d+)",
        "elapsed": r"\belapsed:(\d+)t",
        "food": r"project-cache=\[food:(\d+)mg",
        "preservation": r"preservation:(\d+)ppm",
        "water": r"preservation:\d+ppm water:(\d+)uL",
    }
    for line in selected:
        for key, pattern in patterns.items():
            match = re.search(pattern, line)
            if match is not None:
                values[key].append(int(match.group(1)))
        pristine = re.search(r"pristine-charge-events:(\d+)", line)
        executed = re.search(r"executed=\[charge-events:(\d+)", line)
        projected = re.search(r"consumer-projected-charge-events:(\d+)", line)
        projected_services = re.search(r"consumer-projected-services:(\d+)", line)
        if pristine is not None and executed is not None:
            pristine_count = int(pristine.group(1))
            projected_count = (
                int(projected.group(1)) if projected is not None else pristine_count
            )
            executed_count = int(executed.group(1))
            values["projected_charge_events"].append(projected_count)
            values["wear_projected_extra_charge_events"].append(
                projected_count - pristine_count
            )
            values["unplanned_extra_charge_events"].append(
                executed_count - projected_count
            )
        if projected_services is not None:
            values["projected_services"].append(int(projected_services.group(1)))

        selected_provider = re.search(r"\bselected=([^\s]+)", line)
        if era == "primitive":
            counterfactual = re.search(
                r"crank-active-attention:(\d+)t treadle-active-attention:(\d+)t",
                line,
            )
            provider_attention = (
                {"crank": int(counterfactual.group(1)), "treadle": int(counterfactual.group(2))}
                if counterfactual is not None
                else None
            )
        else:
            counterfactual = re.search(
                r"treadle-active-attention:(\d+)t walking-active-attention:(\d+)t",
                line,
            )
            provider_attention = (
                {
                    "treadle": int(counterfactual.group(1)),
                    "walking-wheel": int(counterfactual.group(2)),
                }
                if counterfactual is not None
                else None
            )
        if selected_provider is not None and provider_attention is not None:
            chosen = provider_attention[selected_provider.group(1)]
            values["attention_regret"].append(chosen - min(provider_attention.values()))
    values["selected_agrees"] = [
        1 if "selected-agrees:true" in line else 0 for line in selected
    ]
    values["samples"] = [len(selected)]
    return values


def _primitive_evidence(power: list[str], projects: list[str]) -> str:
    organic_power = organic_only(power)
    pristine_break_evens = _numeric_values(power, r"pristine-rate-break-even:(\d+)")
    minimum_attention_return = _numeric_values(
        power, r"minimum-attention-return:(\d+)t"
    )
    decision_crossovers = _numeric_values(
        power, r"wear-aware-decision-crossover:(\d+)"
    )
    project_mass = _numeric_values(
        power, r"project=\[consumer:stone-crusher feed:(\d+)mg"
    )
    project_work = _numeric_values(power, r"\bwork:(\d+)nJ")
    buffer_lower_bound_charges = _numeric_values(
        power, r"buffer-lower-bound-charges:(\d+)"
    )
    consumer_projected_charges = _numeric_values(
        power, r"consumer-projected-charges:(\d+)"
    )
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
    lived = _project_experience(projects, "primitive")
    lived_samples = lived["samples"][0]
    return (
        f"choice=[crank:{sum('selected:crank' in line for line in power)} "
        f"treadle:{sum('selected:treadle' in line for line in power)}] "
        f"organic-choice=[crank:{sum('selected:crank' in line for line in organic_power)} "
        f"treadle:{sum('selected:treadle' in line for line in organic_power)}] "
        f"project=[crusher-feed:{scaled_span(project_mass, 1_000_000, 'kg')} "
        f"mechanical-work:{scaled_span(project_work, 1_000_000_000_000, 'kJ')} "
        f"buffer-lower-bound-charges:{_span(buffer_lower_bound_charges)} "
        f"consumer-projected-charges:{_span(consumer_projected_charges)}] "
        f"project-experience=[charges:{_span(lived['charge_events'])} "
        f"services:{_span(lived['services'])} "
        f"feed:{scaled_span(project_mass, 1_000_000, 'kg')} "
        f"provisioning-stops:{_span(lived['provisioning_stops'])} "
        f"drinks:{_span(lived['drink_actions'])} meals:{_span(lived['meal_actions'])} "
        f"unplanned-extra-charges:{_span(lived['unplanned_extra_charge_events'])}] "
        f"decision-crossover-charges={_span(decision_crossovers)} "
        f"pristine-rate-break-even={_span(pristine_break_evens)} "
        f"minimum-investment-return={_span(minimum_attention_return, 't')} "
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
        f"lived-project=[executed:{lived_samples}/{len(power)} "
        f"choice-agrees:{sum(lived['selected_agrees'])}/{lived_samples} "
        f"attention-regret:{_span(lived['attention_regret'], 't')} "
        f"charge-events:{_span(lived['charge_events'])} "
        f"consumer-projected-charges:{_span(lived['projected_charge_events'])} "
        f"wear-projected-extra-charges:{_span(lived['wear_projected_extra_charge_events'])} "
        f"unplanned-extra-charges:{_span(lived['unplanned_extra_charge_events'])} "
        f"projected-services:{_span(lived['projected_services'])} "
        f"survival-limited-batches:{_span(lived['limited_batches'])} "
        f"active-attention:{_span(lived['attention'], 't')} "
        f"services:{_span(lived['services'])} service-time:{_span(lived['service_ticks'], 't')} "
        f"provisioning-stops:{_span(lived['provisioning_stops'])} "
        f"provisioning-attention:{_span(lived['provisioning_attention'], 't')} "
        f"break-actions=[drinks:{_span(lived['drink_actions'])} meals:{_span(lived['meal_actions'])}] "
        f"elapsed:{_span(lived['elapsed'], 't')} "
        f"cache-food:{scaled_span(lived['food'], 1_000_000, 'kg')} "
        f"cache-preservation:{_span(lived['preservation'], 'ppm')} "
        f"cache-water:{scaled_span(lived['water'], 1_000_000, 'L')}] "
        f"evidence-scope=[productive-cycle-executed:{executed_cycles}/{len(power)} "
        f"full-project-executed:{lived_samples}/{len(power)} "
        f"provider-lifecycle-projected:{projected_horizons}/{len(power)}]"
    )


def _settlement_evidence(settlement: list[str], projects: list[str]) -> str:
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
    lived = _project_experience(projects, "settlement")
    lived_samples = lived["samples"][0]
    return (
        f"settlement-choice=[treadle:{sum('selected:treadle' in line for line in settlement)} "
        f"walking:{sum('selected:walking-wheel' in line for line in settlement)}] "
        f"organic-settlement-choice=[treadle:{sum('selected:treadle' in line for line in organic_settlement)} "
        f"walking:{sum('selected:walking-wheel' in line for line in organic_settlement)}] "
        f"settlement-project=[lumber-feed:{scaled_span(project_mass, 1_000_000, 'kg')} "
        f"mechanical-work:{scaled_span(project_work, 1_000_000_000_000, 'kJ')} "
        f"charge-events:{_span(charge_events)}] "
        f"settlement-project-experience=[charges:{_span(lived['charge_events'])} "
        f"services:{_span(lived['services'])} "
        f"feed:{scaled_span(project_mass, 1_000_000, 'kg')} "
        f"provisioning-stops:{_span(lived['provisioning_stops'])} "
        f"drinks:{_span(lived['drink_actions'])} meals:{_span(lived['meal_actions'])} "
        f"unplanned-extra-charges:{_span(lived['unplanned_extra_charge_events'])}] "
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
        f"settlement-lived-project=[executed:{lived_samples}/{len(settlement)} "
        f"choice-agrees:{sum(lived['selected_agrees'])}/{lived_samples} "
        f"attention-regret:{_span(lived['attention_regret'], 't')} "
        f"charge-events:{_span(lived['charge_events'])} "
        f"unplanned-extra-charges:{_span(lived['unplanned_extra_charge_events'])} "
        f"survival-limited-batches:{_span(lived['limited_batches'])} "
        f"active-attention:{_span(lived['attention'], 't')} "
        f"services:{_span(lived['services'])} service-time:{_span(lived['service_ticks'], 't')} "
        f"provisioning-stops:{_span(lived['provisioning_stops'])} "
        f"provisioning-attention:{_span(lived['provisioning_attention'], 't')} "
        f"break-actions=[drinks:{_span(lived['drink_actions'])} meals:{_span(lived['meal_actions'])}] "
        f"elapsed:{_span(lived['elapsed'], 't')} "
        f"cache-food:{scaled_span(lived['food'], 1_000_000, 'kg')} "
        f"cache-preservation:{_span(lived['preservation'], 'ppm')} "
        f"cache-water:{scaled_span(lived['water'], 1_000_000, 'L')}] "
        f"settlement-evidence-scope=[productive-cycle-executed:{executed_cycles}/{len(settlement)} "
        f"full-project-executed:{lived_samples}/{len(settlement)} "
        f"provider-lifecycle-projected:{projected_horizons}/{len(settlement)}]"
    )


def power_provider_summary(lines: list[str]) -> str | None:
    power = [line for line in lines if line.startswith("POWER PROVIDER EXPERIENCE ")]
    if not power:
        return None
    settlement = [line for line in lines if line.startswith("POWER SETTLEMENT ")]
    projects = [line for line in lines if line.startswith("POWER PROJECT EXPERIENCE ")]
    return (
        "ORDINARY SUMMARY probe=power-provider "
        f"samples={len(power)} sample-shape=[{sample_shape(power)}] "
        "workload-source=declared-consumer-project "
        f"{_primitive_evidence(power, projects)} "
        f"{_settlement_evidence(settlement, projects)}"
    )
