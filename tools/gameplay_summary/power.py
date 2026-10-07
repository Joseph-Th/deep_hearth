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


def _counterfactual_provider_attention(line: str) -> dict[str, int]:
    """Return every executed provider arm reported by one played project."""

    return {
        provider: int(attention)
        for provider, attention in re.findall(
            r"\b([a-z0-9-]+)-active-attention:(\d+)t",
            line,
        )
    }


def _project_choice_counts(lines: list[str], era: str) -> dict[str, int]:
    """Count played provider choices without maintaining a second provider catalog."""

    counts: dict[str, int] = {}
    for line in lines:
        if not line.startswith("POWER PROJECT EXPERIENCE ") or f" era={era} " not in line:
            continue
        for provider in _counterfactual_provider_attention(line):
            counts.setdefault(provider, 0)
        selected = re.search(r"\bselected=([^\s]+)", line)
        if selected is not None:
            provider = selected.group(1)
            counts[provider] = counts.get(provider, 0) + 1
    return counts


def _frontier_thresholds(lines: list[str], provider: str) -> list[int]:
    thresholds: list[int] = []
    for line in lines:
        match = re.search(r"\bmarket-frontier:([a-z0-9,:-]+)", line)
        if match is None:
            continue
        for entry in match.group(1).split(","):
            charge, separator, candidate = entry.partition(":")
            if separator and candidate == provider and charge.isdecimal():
                thresholds.append(int(charge))
                break
    return thresholds


def _choice_counts_text(counts: dict[str, int]) -> str:
    selected = [(provider, count) for provider, count in counts.items() if count > 0]
    return " ".join(f"{provider}:{count}" for provider, count in selected) or "none"


def _provider_market_text(counts: dict[str, int]) -> str:
    """Expose provider labels observed in selected play or fixed reference counterfactuals."""

    return ",".join(counts) or "none"


def _settlement_frontier_evidence(
    lines: list[str], candidate_counts: dict[str, int]
) -> tuple[str, str]:
    by_policy: dict[str, list[str]] = {}
    frontier_union: set[str] = set()
    for line in lines:
        policy = re.search(r"\bcopper-policy:([^\s]+)", line)
        frontier = re.search(r"\bmarket-frontier:([a-z0-9,:-]+)", line)
        if policy is None or frontier is None:
            continue
        choices = [entry.partition(":")[2] for entry in frontier.group(1).split(",")]
        choices = [choice for choice in choices if choice]
        frontier_union.update(choices)
        existing = by_policy.setdefault(policy.group(1), [])
        if not existing:
            existing.extend(choices)
        elif existing != choices:
            existing[:] = ["varies-with-policy"]
    frontier_text = " ".join(
        f"{policy}:{'>'.join(choices)}" for policy, choices in sorted(by_policy.items())
    ) or "none"
    off_frontier = [candidate for candidate in candidate_counts if candidate not in frontier_union]
    return frontier_text, ",".join(off_frontier) or "none"


def _market_regime_counts(lines: list[str]) -> list[int]:
    counts: list[int] = []
    for line in lines:
        match = re.search(r"\bmarket-frontier:([a-z0-9,:-]+)", line)
        if match is not None:
            counts.append(len(match.group(1).split(",")))
    return counts


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
        "reference_attention_delta": [],
        "limited_batches": [],
        "attention": [],
        "services": [],
        "maintenance_preparation_ticks": [],
        "service_ticks": [],
        "maintenance_active_share_percent": [],
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
        "reference_attention_delta": r"selected-vs-reference-attention-delta:([+-]?\d+)t",
        "pristine_charge_events": r"pristine-charge-events:(\d+)",
        "charge_events": r"executed=\[charge-events:(\d+)",
        "limited_batches": r"survival-limited-batches:(\d+)",
        "attention": r"active-attention:(\d+)t",
        "services": r"maintenance=\[services:(\d+)",
        "maintenance_preparation_ticks": r"maintenance=\[services:\d+ preparation:(\d+)t",
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

        active_attention = re.search(r"active-attention:(\d+)t", line)
        maintenance = re.search(
            r"maintenance=\[services:\d+ preparation:(\d+)t service:(\d+)t", line
        )
        if active_attention is not None and maintenance is not None:
            active_ticks = int(active_attention.group(1))
            preparation_ticks, service_ticks = map(int, maintenance.groups())
            if active_ticks > 0:
                maintenance_ticks = preparation_ticks + service_ticks
                values["maintenance_active_share_percent"].append(
                    (maintenance_ticks * 100 + active_ticks // 2) // active_ticks
                )

    values["samples"] = [len(selected)]
    return values


def _organic_survival_evidence(projects: list[str], era: str) -> str:
    selected = [
        line
        for line in projects
        if line.startswith("POWER PROJECT EXPERIENCE ")
        and f" era={era} " in line
        and " sample=organic " in line
    ]
    starts = {
        "full": 0,
        "hunger": 0,
        "hydration": 0,
    }
    labels = {
        "full-reserve": "full",
        "hunger-warning-boundary": "hunger",
        "hydration-warning-boundary": "hydration",
    }
    for line in selected:
        match = re.search(r"\bsurvival-start=([^\s]+)", line)
        if match is not None and match.group(1) in labels:
            starts[labels[match.group(1)]] += 1
    lived = _project_experience(selected, era)
    provisioned = sum(stops > 0 for stops in lived["provisioning_stops"])
    return (
        f"[start=[full:{starts['full']} hunger:{starts['hunger']} hydration:{starts['hydration']}] "
        f"provisioned:{provisioned}/{len(selected)} "
        f"stops:{_span(lived['provisioning_stops'])} "
        f"drinks:{_span(lived['drink_actions'])} meals:{_span(lived['meal_actions'])} "
        f"limited-batches:{_span(lived['limited_batches'])}]"
    )


def _primitive_evidence(power: list[str], projects: list[str]) -> str:
    choice_counts = _project_choice_counts(projects, "primitive")
    organic_power = organic_only(power)
    organic_project_lines = [
        line
        for line in projects
        if line.startswith("POWER PROJECT EXPERIENCE ")
        and " era=primitive " in line
        and " sample=organic " in line
    ]
    organic_choice_counts = _project_choice_counts(organic_project_lines, "primitive")
    organic_routine = sum(
        " workload-source=routine-stockpile " in line for line in organic_power
    )
    organic_bulk = sum(
        " workload-source=bulk-fieldwork-ore " in line for line in organic_power
    )
    policy_returns = _numeric_values(power, r"minimum-return:(\d+)ppm")
    pristine_break_evens = _numeric_values(power, r"pristine-rate-break-even:(\d+)")
    minimum_attention_return = _numeric_values(
        power, r"minimum-attention-return:(\d+)t"
    )
    market_regimes = _market_regime_counts(power)
    treadle_thresholds = _frontier_thresholds(power, "treadle")
    walking_thresholds = _frontier_thresholds(power, "walking-wheel")
    project_mass = _numeric_values(
        power, r"project=\[consumer:stone-crusher feed:(\d+)mg"
    )
    project_work = _numeric_values(power, r"\bwork:(\d+)nJ")
    declared_charge_events = _numeric_values(power, r"declared-charge-events:(\d+)")
    consumer_projected_batches = _numeric_values(
        power, r"consumer-projected-batches:(\d+)"
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
    reference_attention_matches = sum(
        delta == 0 for delta in lived["reference_attention_delta"]
    )
    return (
        f"choice=[{_choice_counts_text(choice_counts)}] "
        f"market=[{_provider_market_text(choice_counts)}] "
        f"organic-choice=[{_choice_counts_text(organic_choice_counts)}] "
        f"organic-workload=[routine:{organic_routine} bulk:{organic_bulk}] "
        f"organic-survival={_organic_survival_evidence(projects, 'primitive')} "
        f"project=[crusher-feed:{scaled_span(project_mass, 1_000_000, 'kg')} "
        f"mechanical-work:{scaled_span(project_work, 1_000_000_000_000, 'kJ')} "
        f"declared-charge-events:{_span(declared_charge_events)} "
        f"consumer-projected-batches:{_span(consumer_projected_batches)}] "
        f"project-experience=[charges:{_span(lived['charge_events'])} "
        f"services:{_span(lived['services'])} "
        f"feed:{scaled_span(project_mass, 1_000_000, 'kg')} "
        f"provisioning-stops:{_span(lived['provisioning_stops'])} "
        f"drinks:{_span(lived['drink_actions'])} meals:{_span(lived['meal_actions'])} "
        f"unplanned-extra-charges:{_span(lived['unplanned_extra_charge_events'])}] "
        f"reference-comparison=[selected-equals-best:{reference_attention_matches}/{lived_samples} "
        f"attention-delta:{_span(lived['reference_attention_delta'], 't')}] "
        f"lifecycle-obligations=[services:{_span(lived['services'])} "
        f"prep:{_span(lived['maintenance_preparation_ticks'], 't')} "
        f"share:{_span(lived['maintenance_active_share_percent'], '%')} "
        f"provisioning:{_span(lived['provisioning_stops'])} "
        f"drinks:{_span(lived['drink_actions'])} meals:{_span(lived['meal_actions'])}] "
        f"market-regimes={_span(market_regimes)} "
        f"primitive-scale=[charges:{_span(declared_charge_events)} "
        f"treadle:{_span(treadle_thresholds)} wheel:{_span(walking_thresholds)}] "
        f"pristine-rate-break-even={_span(pristine_break_evens)} "
        f"investment-policy-return={_span(policy_returns, 'ppm')} "
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
        f"selected-vs-reference-attention-delta:{_span(lived['reference_attention_delta'], 't')} "
        f"charge-events:{_span(lived['charge_events'])} "
        f"consumer-projected-charge-events:{_span(lived['projected_charge_events'])} "
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
    choice_counts = _project_choice_counts(projects, "settlement")
    organic_project_lines = [
        line
        for line in projects
        if line.startswith("POWER PROJECT EXPERIENCE ")
        and " era=settlement " in line
        and " sample=organic " in line
    ]
    organic_choice_counts = _project_choice_counts(organic_project_lines, "settlement")
    frontier_text, off_frontier = _settlement_frontier_evidence(
        settlement, choice_counts
    )
    policy_returns = _numeric_values(settlement, r"minimum-return:(\d+)ppm")
    project_mass = _numeric_values(
        settlement, r"project=\[consumer:powered-saw feed:(\d+)mg"
    )
    project_work = _numeric_values(settlement, r"\bwork:(\d+)nJ")
    charge_events = _numeric_values(settlement, r"charge-events:(\d+)")
    treadle_thresholds = _frontier_thresholds(settlement, "treadle")
    walking_thresholds = _frontier_thresholds(settlement, "walking-wheel")
    pristine_break_evens = _numeric_values(
        settlement, r"pristine-rate-break-even:(\d+)charges"
    )
    market_regimes = _market_regime_counts(settlement)
    spend_copper = sum(" copper-policy:spend-available " in line for line in settlement)
    preserve_copper = sum(
        " copper-policy:preserve-for-other-uses " in line for line in settlement
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
        "settlement-context=[later-workshop timber-bank] "
        f"settlement-choice=[{_choice_counts_text(choice_counts)}] "
        f"settlement-candidates=[{_provider_market_text(choice_counts)}] "
        f"settlement-frontier=[{frontier_text}] "
        f"settlement-off-frontier=[{off_frontier}] "
        f"organic-settlement-choice=[{_choice_counts_text(organic_choice_counts)}] "
        f"organic-settlement-survival={_organic_survival_evidence(projects, 'settlement')} "
        f"settlement-project=[lumber-feed:{scaled_span(project_mass, 1_000_000, 'kg')} "
        f"mechanical-work:{scaled_span(project_work, 1_000_000_000_000, 'kJ')} "
        f"charge-events:{_span(charge_events)}] "
        f"settlement-project-experience=[charges:{_span(lived['charge_events'])} "
        f"services:{_span(lived['services'])} "
        f"feed:{scaled_span(project_mass, 1_000_000, 'kg')} "
        f"provisioning-stops:{_span(lived['provisioning_stops'])} "
        f"drinks:{_span(lived['drink_actions'])} meals:{_span(lived['meal_actions'])} "
        f"unplanned-extra-charges:{_span(lived['unplanned_extra_charge_events'])}] "
        f"settlement-lifecycle-obligations=[services:{_span(lived['services'])} "
        f"prep:{_span(lived['maintenance_preparation_ticks'], 't')} "
        f"share:{_span(lived['maintenance_active_share_percent'], '%')} "
        f"provisioning:{_span(lived['provisioning_stops'])} "
        f"drinks:{_span(lived['drink_actions'])} meals:{_span(lived['meal_actions'])}] "
        f"settlement-copper-policy=[spend:{spend_copper} preserve:{preserve_copper}] "
        f"settlement-scale=[charges:{_span(charge_events)} "
        f"treadle:{_span(treadle_thresholds)} wheel:{_span(walking_thresholds)}] "
        f"settlement-regimes={_span(market_regimes)} "
        f"settlement-pristine-rate-break-even={_span(pristine_break_evens)} "
        f"settlement-investment-policy-return={_span(policy_returns, 'ppm')} "
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
        f"selected-vs-reference-attention-delta:{_span(lived['reference_attention_delta'], 't')} "
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
        f"{_primitive_evidence(power, projects)} "
        f"{_settlement_evidence(settlement, projects)}"
    )
