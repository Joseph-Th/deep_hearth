"""Fieldwork summary owned separately from the gameplay-report facade."""

from __future__ import annotations

import re

from .common import field, organic_only, physical_duration_span, sample_shape, scaled_span


def _span(values: list[int], unit: str = "t") -> str:
    return f"{min(values)}..{max(values)}{unit}" if values else "n/a"


def _signed_span(values: list[int]) -> str:
    return f"{min(values):+d}..{max(values):+d}t" if values else "n/a"


def _outcome_metrics(fieldwork: list[str]) -> tuple[str, str, int, int]:
    inspections = [
        int(match.group(1))
        for line in fieldwork
        if (match := re.search(r"\bfield-inspections=(\d+)", line)) is not None
    ]
    fulfillment_ppm: list[int] = []
    resource_capped = 0
    organic_resource_capped = 0
    for line in fieldwork:
        requested = re.search(r"\brequested=(\d+)mg", line)
        mined = re.search(r"\bmining=(\d+)mg", line)
        planned = re.search(r"\bplanned-local-work=(\d+)mg", line)
        if requested is not None and mined is not None and int(requested.group(1)) > 0:
            fulfillment_ppm.append(
                int(mined.group(1)) * 1_000_000 // int(requested.group(1))
            )
        if (
            requested is not None
            and planned is not None
            and int(planned.group(1)) < int(requested.group(1))
        ):
            resource_capped += 1
            if " sample=organic " in line:
                organic_resource_capped += 1
    inspection_span = _span(inspections, unit="")
    fulfillment_span = _span(fulfillment_ppm, unit="ppm")
    return inspection_span, fulfillment_span, resource_capped, organic_resource_capped


def _pacing_summary(lines: list[str], fieldwork: list[str]) -> str:
    horizon_by_seed = {}
    for line in fieldwork:
        seed = re.search(r"\bseed=(0x[0-9A-Fa-f]+)", line)
        horizon = re.search(r"\border-horizon=(\S+)", line)
        if seed is not None and horizon is not None:
            horizon_by_seed[seed.group(1).lower()] = horizon.group(1)

    buckets = {
        "first": [],
        "kit": [],
        "search": [],
        "extraction": [],
        "short-first": [],
        "short-search": [],
        "short-extraction": [],
        "project-first": [],
        "project-search": [],
        "project-extraction": [],
        "bulk-first": [],
        "bulk-search": [],
        "bulk-extraction": [],
    }
    for line in lines:
        if not line.startswith("FIELDWORK PACING "):
            continue
        seed = re.search(r"\bseed=(0x[0-9A-Fa-f]+)", line)
        search = re.search(r"\bsearch=(\d+)t/", line)
        sampling_tool = re.search(r"\bsampling-tool=(\d+)t/", line)
        extraction_tool = re.search(r"\bextraction-tool=(\d+)t/", line)
        extraction = re.search(r"\bextraction=(\d+)t/", line)
        episode_end = re.search(r"\bepisode-end=(\d+)t/", line)
        if None in (seed, search, sampling_tool, extraction_tool, extraction, episode_end):
            continue
        assert seed is not None
        assert search is not None
        assert sampling_tool is not None
        assert extraction_tool is not None
        assert extraction is not None
        assert episode_end is not None
        searched_ticks = int(search.group(1))
        kit_ticks = int(sampling_tool.group(1)) + int(extraction_tool.group(1))
        extracted_ticks = int(extraction.group(1))
        total_ticks = int(episode_end.group(1))
        if total_ticks != searched_ticks + kit_ticks + extracted_ticks:
            continue
        buckets["first"].append(total_ticks)
        buckets["kit"].append(kit_ticks)
        buckets["search"].append(searched_ticks)
        buckets["extraction"].append(extracted_ticks)
        horizon = horizon_by_seed.get(seed.group(1).lower())
        if horizon in ("short", "project", "bulk"):
            buckets[f"{horizon}-first"].append(total_ticks)
            buckets[f"{horizon}-search"].append(searched_ticks)
            buckets[f"{horizon}-extraction"].append(extracted_ticks)

    return (
        "pacing=["
        f"first-expedition:{_span(buckets['first'])} "
        f"durable-kit:{_span(buckets['kit'])} "
        f"site-search:{_span(buckets['search'])} "
        f"order-extraction:{_span(buckets['extraction'])} "
        f"short-first:{_span(buckets['short-first'])} "
        f"short-search:{_span(buckets['short-search'])} "
        f"short-extraction:{_span(buckets['short-extraction'])} "
        f"project-first:{_span(buckets['project-first'])} "
        f"project-search:{_span(buckets['project-search'])} "
        f"project-extraction:{_span(buckets['project-extraction'])} "
        f"bulk-first:{_span(buckets['bulk-first'])} "
        f"bulk-search:{_span(buckets['bulk-search'])} "
        f"bulk-extraction:{_span(buckets['bulk-extraction'])}] "
        "pacing-physical=["
        f"first-expedition:{physical_duration_span(lines, buckets['first'])} "
        f"durable-kit:{physical_duration_span(lines, buckets['kit'])} "
        f"site-search:{physical_duration_span(lines, buckets['search'])} "
        f"order-extraction:{physical_duration_span(lines, buckets['extraction'])}]"
    )


def _reuse_summary(lines: list[str]) -> str:
    continuation = [
        line for line in lines if line.startswith("FIELDWORK CONTINUATION ")
    ]
    reusable = [line for line in continuation if " available=true " in line]
    repeat_complete = [line for line in reusable if " stop=order-complete " in line]
    repeat_partial = [line for line in reusable if " stop=order-complete " not in line]
    site_reuse = [line for line in lines if line.startswith("FIELDWORK SITE REUSE ")]
    reusable_sites = [line for line in site_reuse if " available=true " in line]

    def ticks(sample_lines: list[str], pattern: str) -> list[int]:
        return [
            int(match.group(1))
            for line in sample_lines
            if (match := re.search(pattern, line)) is not None
        ]

    repeat_complete_ticks = ticks(repeat_complete, r"\bextraction=(\d+)t/")
    repeat_partial_ticks = ticks(repeat_partial, r"\bextraction=(\d+)t/")
    partial_fulfillment: list[int] = []
    for line in repeat_partial:
        requested = re.search(r"\brequested=(\d+)mg", line)
        extracted = re.search(r"\bextracted=(\d+)mg", line)
        if (
            requested is not None
            and extracted is not None
            and int(requested.group(1)) > 0
        ):
            partial_fulfillment.append(
                int(extracted.group(1)) * 1_000_000 // int(requested.group(1))
            )
    avoided_search = ticks(continuation, r"\bavoided-search=(\d+)t/")
    avoided_kit = ticks(continuation, r"\bavoided-kit=(\d+)t/")
    return (
        "reuse=["
        f"known-site:{len(reusable)}/{len(continuation)} "
        f"matched-order:complete{len(repeat_complete)}/partial{len(repeat_partial)} "
        f"partial-fulfillment:{_span(partial_fulfillment, unit='ppm')} "
        f"repeat-extraction=[complete:{_span(repeat_complete_ticks)} "
        f"partial:{_span(repeat_partial_ticks)}] "
        f"avoided-search:{_span(avoided_search)} "
        f"avoided-kit:{_span(avoided_kit)} "
        f"new-site-with-kit:{len(reusable_sites)}/{len(site_reuse)} "
        f"new-site-search:{_span(ticks(reusable_sites, r'\bsearch=(\d+)t/'))}] "
        "reuse-physical=["
        f"repeat-complete:{physical_duration_span(lines, repeat_complete_ticks)} "
        f"repeat-partial:{physical_duration_span(lines, repeat_partial_ticks)} "
        f"avoided-search:{physical_duration_span(lines, avoided_search)} "
        f"avoided-kit:{physical_duration_span(lines, avoided_kit)}]"
    )


def _depletion_summary(lines: list[str]) -> str:
    depletion = [
        line
        for line in lines
        if line.startswith("FIELDWORK DEPLETION ")
        and not line.startswith("FIELDWORK DEPLETION RECOVERY ")
    ]
    eligible = [line for line in depletion if " eligible=true " in line]

    def values(pattern: str) -> list[int]:
        return [
            int(match.group(1))
            for line in eligible
            if (match := re.search(pattern, line)) is not None
        ]

    completed_orders = values(r"\brepeat-orders=\[complete:(\d+)")
    partial_orders = values(r"\brepeat-orders=\[complete:\d+ partial:(\d+)")
    extracted = values(r"\bextracted=(\d+)mg")
    attention = values(r"\battention=(\d+)t/")
    condition = values(r"\bcondition-after=(\d+)ppm")
    energy = values(r"\bbody=\[energy:(\d+)nJ")
    hydration = values(r"\bhydration:(\d+)uL\]")
    reroutes = [
        line for line in lines if line.startswith("FIELDWORK DEPLETION RECOVERY ")
    ]
    recovery = [line for line in reroutes if " reroute-proved=true " in line]
    blocked = [line for line in reroutes if " reroute-proved=false " in line]
    retool_ticks = [
        int(match.group(1))
        for line in recovery
        if (match := re.search(r"\bretool=(\d+)t", line)) is not None
    ]
    salvaged = sum(" salvage=true " in line for line in recovery)
    ore_recovery_ticks = [
        int(match.group(1))
        for line in recovery
        if (match := re.search(r"\bore-recovery=\[reason:[^\s]+ ticks:(\d+)", line))
        is not None
    ]
    ore_payback = sum(" ore-recovery=[reason:payback " in line for line in recovery)
    ore_required = sum(" ore-recovery=[reason:required-access " in line for line in recovery)
    supply_ended = sum(" supply-ended=true " in line for line in eligible)
    return (
        "known-site-horizon=["
        f"eligible:{len(eligible)}/{len(depletion)} "
        f"supply-ended:{supply_ended} "
        f"horizon-live:{sum(' terminal=horizon-live-target ' in line for line in eligible)} "
        f"reroute-proved:{len(recovery)}/{supply_ended} "
        f"reroute-blocked:{len(blocked)} "
        f"reroute-retooled:{sum(value > 0 for value in retool_ticks)} "
        f"reroute-salvaged:{salvaged} "
        f"reroute-ore-funded:{sum(value > 0 for value in ore_recovery_ticks)}"
        f"(payback:{ore_payback}/access:{ore_required}) "
        f"complete-orders:{_span(completed_orders, unit='')} "
        f"partial-orders:{_span(partial_orders, unit='')} "
        f"extracted:{_span(extracted, unit='mg')} "
        f"attention:{_span(attention)}/{physical_duration_span(lines, attention)} "
        f"body:{scaled_span(energy, 1_000_000_000_000, 'kJ')}/"
        f"{scaled_span(hydration, 1_000, 'mL')} "
        f"condition:{_span(condition, unit='ppm')}]"
    )


def _initial_shortfall_recovery_summary(lines: list[str]) -> str:
    recoveries = [
        line
        for line in lines
        if line.startswith("FIELDWORK INITIAL SHORTFALL RECOVERY ")
    ]

    def values(pattern: str) -> list[int]:
        return [
            int(match.group(1))
            for line in recoveries
            if (match := re.search(pattern, line)) is not None
        ]

    realized_search_deltas: list[int] = []
    realized_total_deltas: list[int] = []
    for line in recoveries:
        realized = re.search(r"\battention-delta:([+-]\d+)t", line)
        if realized is not None:
            realized_search_deltas.append(int(realized.group(1)))
        total = re.search(r"\btotal-attention-delta:([+-]\d+)t", line)
        if total is not None:
            realized_total_deltas.append(int(total.group(1)))

    hardness_changes = values(r"\bhardness-tier-changes:(\d+)")
    tool_builds = values(r"\btool-builds:(\d+)")
    salvage_retools = values(r"\bsalvage-retools:(\d+)")
    blocked_sites = values(r"\bblocked-sites:(\d+)")
    ore_recovery_events = values(r"\bore-recovery-events:(\d+)")
    ore_recovery_required = values(r"\bore-recovery-required-access:(\d+)")
    ore_recovery_payback = values(r"\bore-recovery-payback:(\d+)")
    fulfillment_delta = [
        int(match.group(1))
        for line in recoveries
        if (match := re.search(r"\bfulfillment-delta:([+-]\d+)mg", line)) is not None
    ]

    return (
        "initial-shortfall-campaign=["
        f"cases:{len(recoveries)} "
        f"strategy:point{sum(' strategy=point-search ' in line for line in recoveries)}"
        f"/indexed{sum(' strategy=indexed-channel ' in line for line in recoveries)} "
        f"survey-upgrade:{_span(values(r'\bsurvey-upgrade=(\d+)t'))} "
        f"realized-search=[positive:{sum(value > 0 for value in realized_search_deltas)} "
        f"negative:{sum(value < 0 for value in realized_search_deltas)} "
        f"flat:{sum(value == 0 for value in realized_search_deltas)} "
        f"delta:{_signed_span(realized_search_deltas)}] "
        f"realized-total=[positive:{sum(value > 0 for value in realized_total_deltas)} "
        f"negative:{sum(value < 0 for value in realized_total_deltas)} "
        f"flat:{sum(value == 0 for value in realized_total_deltas)} "
        f"delta:{_signed_span(realized_total_deltas)}] "
        "adaptation=["
        f"geology-changed:{sum(value > 0 for value in hardness_changes)}/{len(recoveries)} "
        f"retooled:{sum(value > 0 for value in tool_builds)}/{len(recoveries)} "
        f"salvaged:{sum(value > 0 for value in salvage_retools)}/{len(recoveries)} "
        f"ore-funded:{sum(value > 0 for value in ore_recovery_events)}/{len(recoveries)}"
        f"(payback:{sum(ore_recovery_payback)}/access:{sum(ore_recovery_required)}) "
        f"blocked-sites:{_span(blocked_sites, unit='')} "
        f"fulfillment-delta:{_signed_span(fulfillment_delta).replace('t', 'mg')}] "
        f"completed:{sum(' terminal=order-complete' in line for line in recoveries)} "
        f"local-area-exhausted:{sum(' terminal=local-search-area-exhausted' in line for line in recoveries)} "
        f"sites:{_span(values(r'\bsites-visited=(\d+)'), unit='')} "
        f"fulfillment:{_span(values(r'\bfulfillment=(\d+)ppm'), unit='ppm')} "
        f"remaining:{_span(values(r'\bremaining=(\d+)mg'), unit='mg')}]"
    )


def _survey_campaign_summary(lines: list[str]) -> str:
    campaigns = [
        line for line in lines if line.startswith("FIELDWORK SURVEY CAMPAIGN ")
    ]
    indexed_campaigns = [
        line for line in campaigns if " selected=indexed-channel " in line
    ]
    indexed_expected_deltas: list[int] = []
    for line in indexed_campaigns:
        projected = re.search(
            r"\bprojected=\[point:(\d+)t indexed:(\d+)t\]", line
        )
        if projected is not None:
            indexed_expected_deltas.append(
                int(projected.group(1)) - int(projected.group(2))
            )
    campaign_horizons = [
        int(match.group(1))
        for line in campaigns
        if (match := re.search(r"\bplanned-sites=(\d+)", line)) is not None
    ]
    indexed_realized_deltas = [
        int(match.group(1))
        for line in indexed_campaigns
        if (match := re.search(r"\battention-delta:([+-]\d+)t", line)) is not None
    ]
    return (
        "survey-campaign=["
        f"point:{sum(' selected=point-search ' in line for line in campaigns)} "
        f"indexed:{len(indexed_campaigns)} "
        f"upgrade-fundable:{sum(' upgrade-available=true ' in line for line in campaigns)}/{len(campaigns)} "
        f"horizons:one{sum(value == 1 for value in campaign_horizons)}"
        f"/two{sum(value == 2 for value in campaign_horizons)}"
        f"/three{sum(value == 3 for value in campaign_horizons)} "
        f"indexed-expected-delta:{_signed_span(indexed_expected_deltas)} "
        "indexed-realized=["
        f"positive:{sum(value > 0 for value in indexed_realized_deltas)} "
        f"negative:{sum(value < 0 for value in indexed_realized_deltas)} "
        f"flat:{sum(value == 0 for value in indexed_realized_deltas)} "
        f"delta:{_signed_span(indexed_realized_deltas)}]]"
    )


def _heavy_tool_market_summary(lines: list[str]) -> str:
    tool_markets = [
        line
        for line in lines
        if line.startswith("FIELDWORK TOOL MARKET ")
        and " phase=acquired-evidence " in line
    ]

    selected = [
        line for line in tool_markets if " heavy-investment=selected" in line
    ]
    deferred = [
        line for line in tool_markets if " heavy-investment=deferred" in line
    ]

    def signed_values(sample_lines: list[str], pattern: str) -> list[int]:
        return [
            int(match.group(1))
            for line in sample_lines
            if (match := re.search(pattern, line)) is not None
        ]

    selected_net_savings = [
        -value
        for value in signed_values(
            selected, r"\bheavy-total-delta=([+-]\d+)t"
        )
    ]
    return (
        "heavy-tool-market=["
        f"selected:{len(selected)} "
        f"deferred:{len(deferred)} "
        f"unavailable:{sum(' heavy-investment=unavailable' in line for line in tool_markets)} "
        "selected-economics=["
        f"prep-extra:{_signed_span(signed_values(selected, r'\bheavy-preparation-extra=([+-]\d+)t'))} "
        f"order-saving:{_signed_span(signed_values(selected, r'\bheavy-order-saving=([+-]\d+)t'))} "
        f"net-saving:{_signed_span(selected_net_savings)}] "
        "deferred-economics=["
        f"prep-extra:{_signed_span(signed_values(deferred, r'\bheavy-preparation-extra=([+-]\d+)t'))} "
        f"order-saving:{_signed_span(signed_values(deferred, r'\bheavy-order-saving=([+-]\d+)t'))} "
        f"net-penalty:{_signed_span(signed_values(deferred, r'\bheavy-total-delta=([+-]\d+)t'))}]]"
    )


def _bulk_crossover_summary(lines: list[str]) -> str:
    crossovers = [
        line for line in lines if line.startswith("FIELDWORK BULK CROSSOVER ")
    ]
    available = [line for line in crossovers if " available=true " in line]
    batches = [
        int(match.group(1))
        for line in available
        if (match := re.search(r"\bbase-batches=(\d+)", line)) is not None
    ]
    orders = [
        int(match.group(1))
        for line in available
        if (match := re.search(r"\border=(\d+)mg", line)) is not None
    ]
    tools = {
        match.group(1)
        for line in available
        if (match := re.search(r"\btool=([^\s]+)", line)) is not None
    }
    return (
        "bulk-crossover=["
        f"available:{len(available)}/{len(crossovers)} "
        f"batches:{_span(batches, unit='')} "
        f"order:{_span(orders, unit='mg')} "
        f"tools:{len(tools)}]"
    )


def fieldwork_summary(lines: list[str]) -> str | None:
    fieldwork = [line for line in lines if line.startswith("FIELDWORK EXPERIENCE ")]
    if not fieldwork:
        return None
    count = lambda marker: sum(marker in line for line in fieldwork)
    count_field = lambda name, value: sum(
        field(line, name) == value for line in fieldwork
    )
    geology_tool_count = lambda geology, tool: sum(
        field(line, "geology") == geology and field(line, "tool") == tool
        for line in fieldwork
    )
    organic_fieldwork = organic_only(fieldwork)
    inspection_span, fulfillment_span, resource_capped, organic_resource_capped = (
        _outcome_metrics(fieldwork)
    )
    return (
        "ORDINARY SUMMARY probe=fieldwork "
        f"samples={len(fieldwork)} sample-shape=[{sample_shape(fieldwork)}] inspections={inspection_span} "
        f"outcomes=[completed:{count('outcome=completed')} "
        f"local-supply-ended:{count('outcome=known-target-supply')} "
        f"fulfillment:{fulfillment_span}] "
        f"organic-outcomes=[completed:{sum('outcome=completed' in line for line in organic_fieldwork)} "
        f"local-supply-ended:{sum('outcome=known-target-supply' in line for line in organic_fieldwork)}] "
        f"reserve-knowledge=[workload-capped:{resource_capped} "
        f"tool-changed:{count('resource-knowledge-effect=changed-tool')}] "
        f"organic-reserve-knowledge=[workload-capped:{organic_resource_capped} "
        f"tool-changed:{sum('resource-knowledge-effect=changed-tool' in line for line in organic_fieldwork)}] "
        f"orders=[short:{count('order-horizon=short')} project:{count('order-horizon=project')} "
        f"bulk:{count('order-horizon=bulk')}] "
        f"{_pacing_summary(lines, fieldwork)} "
        f"{_reuse_summary(lines)} "
        f"{_depletion_summary(lines)} "
        f"{_initial_shortfall_recovery_summary(lines)} "
        f"{_survey_campaign_summary(lines)} "
        f"{_heavy_tool_market_summary(lines)} "
        f"{_bulk_crossover_summary(lines)} "
        f"geology=[soft:{count('geology=quarry-soft')} "
        f"reinforcement:{count('geology=quarry-reinforcement')} "
        f"hard-specialist:{count('geology=hard-pick-specialist')}] "
        f"copper=[available:{count('copper-opportunity=available')} "
        f"absent:{count('copper-opportunity=absent')}] "
        f"tools=[stone-pick:{count_field('tool', 'stone-pick')} "
        f"soft-quarry:{count_field('tool', 'stone-quarry')} "
        f"reinforced-quarry:{count_field('tool', 'copper-reinforced-quarry')} "
        f"hard-pick:{count_field('tool', 'copper-reinforced-hard-pick')}] "
        f"geology-tool=[soft:pick{geology_tool_count('quarry-soft', 'stone-pick')}"
        f"/quarry{geology_tool_count('quarry-soft', 'stone-quarry')}"
        f"/reinforced{geology_tool_count('quarry-soft', 'copper-reinforced-quarry')}"
        f"/hard{geology_tool_count('quarry-soft', 'copper-reinforced-hard-pick')} "
        f"reinforcement:pick{geology_tool_count('quarry-reinforcement', 'stone-pick')}"
        f"/quarry{geology_tool_count('quarry-reinforcement', 'stone-quarry')}"
        f"/reinforced{geology_tool_count('quarry-reinforcement', 'copper-reinforced-quarry')}"
        f"/hard{geology_tool_count('quarry-reinforcement', 'copper-reinforced-hard-pick')} "
        f"hard-specialist:pick{geology_tool_count('hard-pick-specialist', 'stone-pick')}"
        f"/quarry{geology_tool_count('hard-pick-specialist', 'stone-quarry')}"
        f"/reinforced{geology_tool_count('hard-pick-specialist', 'copper-reinforced-quarry')}"
        f"/hard{geology_tool_count('hard-pick-specialist', 'copper-reinforced-hard-pick')}]"
    )
