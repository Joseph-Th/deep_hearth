"""Primitive-liberation summary and foundry-frontier readiness evidence."""

from __future__ import annotations

import re

from .common import physical_duration_span, scaled_span


def _span(values: list[int], unit: str) -> str:
    return f"{min(values)}..{max(values)}{unit}" if values else "n/a"


def _signed_span(values: list[int], unit: str) -> str:
    return (
        f"{min(values):+d}..{max(values):+d}{unit}"
        if values
        else "n/a"
    )


def _numeric_values(lines: list[str], pattern: str) -> list[int]:
    return [
        int(match.group(1))
        for line in lines
        if (match := re.search(pattern, line)) is not None
    ]


def _scavenger_marginal(lines: list[str]) -> tuple[str, str]:
    attention: list[int] = []
    native: list[int] = []
    for line in lines:
        if not line.startswith("LIBERATION COST "):
            continue
        match = re.search(
            r"\bscavenger-marginal=\[attention:(\d+)t native:(\d+)mg\]",
            line,
        )
        if match is not None:
            attention.append(int(match.group(1)))
            native.append(int(match.group(2)))
    return _span(attention, "t"), _span(native, "mg")


def _frontier_evidence(lines: list[str]) -> tuple[str, str]:
    frontiers = [
        match.group(1)
        for line in lines
        if line.startswith("LIBERATION FRONTIER ")
        and (match := re.search(r"\bremaining-frontier=(\S+)", line)) is not None
    ]
    frontier_lines = [
        line
        for line in lines
        if line.startswith("LIBERATION FRONTIER ")
        and not line.startswith("LIBERATION FRONTIER CAPABILITY ")
    ]
    frontier = (
        frontiers[0]
        if frontiers and len(set(frontiers)) == 1
        else "mixed-or-unknown"
    )
    count = len(frontier_lines)
    readiness = (
        "industrial-foundry-readiness=["
        f"furnace-assembly-edge:{sum('assembly-edge=[furnace:true' in line for line in frontier_lines)}/{count} "
        f"mold-assembly-edge:{sum(re.search(r'assembly-edge=\[[^]]*\bmold:true', line) is not None for line in frontier_lines)}/{count} "
        f"electrical-buffer-assembly-edge:{sum('electrical-buffer:true' in line for line in frontier_lines)}/{count} "
        f"thermal-sink-assembly-edge:{sum('thermal-sink:true' in line for line in frontier_lines)}/{count} "
        f"manual-electrical-generation:{sum('manual-electrical-generation:true' in line for line in frontier_lines)}/{count} "
        f"support-required:{sum('support-required=[furnace:true mold:true]' in line for line in frontier_lines)}/{count}]"
    )
    manual_power: list[int] = []
    furnace_power: list[int] = []
    power_gap: list[int] = []
    for line in frontier_lines:
        scale = re.search(
            r"energy-scale=\[manual-electrical-max:(\d+)uW "
            r"industrial-furnace-transfer-ceiling:(\d+)uW ceiling-ratio:(\d+)x "
            r"melting-carrier:([^\s\]]+) conversion-path:([^\s\]]+)\]",
            line,
        )
        if scale is None:
            continue
        manual_power.append(int(scale.group(1)))
        furnace_power.append(int(scale.group(2)))
        power_gap.append(int(scale.group(3)))
    energy_frontier = (
        "industrial-foundry-frontier=["
        f"manual-electrical-max:{scaled_span(manual_power, 1_000_000, 'W')} "
        f"industrial-furnace-transfer-ceiling:{scaled_span(furnace_power, 1_000_000, 'W')} "
        f"ceiling-ratio:{_span(power_gap, 'x')} "
        f"electrical-melting:{sum('melting-carrier:Electrical' in line for line in frontier_lines)}/{count} "
        f"conversion-path-present:{sum('conversion-path:present' in line for line in frontier_lines)}/{count}]"
    )
    return frontier, f"{readiness} {energy_frontier}"


def _first_foundry(lines: list[str]) -> str:
    witnesses = [line for line in lines if line.startswith("FIRST FOUNDRY EXPERIENCE ")]
    fabrication = _numeric_values(witnesses, r"\bfabrication=(\d+)t/")
    direct_native = _numeric_values(witnesses, r"\bdirect-native:(\d+)t")
    native_fulfillment = _numeric_values(
        witnesses,
        r"\bdirect-native:\d+t reinforcement:\d+mg fulfillment:(\d+)ppm",
    )
    cold_rework = _numeric_values(witnesses, r"\bcold-rework:(\d+)t")
    cold_fulfillment = _numeric_values(
        witnesses,
        r"\bcold-rework:\d+t reinforcement:\d+mg chips:\d+mg fulfillment:(\d+)ppm",
    )
    foundry_fulfillment = _numeric_values(
        witnesses,
        r"\bfoundry-active:\d+t reinforcement:\d+mg chips:\d+mg fulfillment:(\d+)ppm",
    )
    foundry_active = _numeric_values(witnesses, r"\bfoundry-active:(\d+)t")
    attention_delta = [
        int(match.group(1))
        for line in witnesses
        if (
            match := re.search(r"\battention-delta:([+-]\d+)t", line)
        )
        is not None
    ]
    useful_gain = _numeric_values(witnesses, r"\buseful-gain:\+(\d+)mg")
    foundry_deferred = sum(" foundry-deferred:true " in line for line in witnesses)
    scarcity_foundry = sum(
        " scarcity-choice=[" in line and " selection:foundry " in line
        for line in witnesses
    )
    scarcity_shortfall = _numeric_values(witnesses, r"\bshortfall:(\d+)mg")
    treadle_upgrade = sum(
        " dynamo-path=treadle-additive-upgrade " in line for line in witnesses
    )
    return (
        "first-foundry=["
        f"defer:{foundry_deferred}/{len(witnesses)} scarcity-select:{scarcity_foundry}/{len(witnesses)} "
        f"scarcity-shortfall:{scaled_span(scarcity_shortfall, 1_000, 'g')} "
        f"native:{_span(direct_native, 't')}/"
        f"{scaled_span(native_fulfillment, 10_000, '%')} setup:{_span(fabrication, 't')} "
        f"upgrade:{treadle_upgrade}/{len(witnesses)} recovery:{_span(cold_rework, 't')}/"
        f"{scaled_span(cold_fulfillment, 10_000, '%')}->{_span(foundry_active, 't')}/"
        f"{scaled_span(foundry_fulfillment, 10_000, '%')} "
        f"gain:{scaled_span(useful_gain, 1_000, 'g')}/{_signed_span(attention_delta, 't')}]"
    )


def _kit_acquisition(lines: list[str]) -> str:
    witnesses = [
        line for line in lines if line.startswith("LIBERATION KIT ACQUISITION ")
    ]
    routes = [line for line in lines if line.startswith("LIBERATION ROUTE TRADEOFF ")]
    live_kit_routes = sum(" continuity=live-kit-used" in line for line in routes)
    preassembled_routes = sum(
        " continuity=controlled-preassembled-kit" in line for line in routes
    )
    fixture_sources = sum(
        " raw-origin=pre-admission-fixture " in line for line in witnesses
    )
    runtime_pickups = sum(" pickup=same-voxel-runtime " in line for line in witnesses)
    world_gathering = sum(" world-gathering-proved=true " in line for line in witnesses)
    stone: list[int] = []
    wood: list[int] = []
    total: list[int] = []
    attention: list[int] = []
    metabolic: list[int] = []
    hydration: list[int] = []
    for line in witnesses:
        raw = re.search(
            r"raw=\[stone:(\d+)mg wood:(\d+)mg total:(\d+)mg\]",
            line,
        )
        body = re.search(r"attention:(\d+)t body=(\d+)nJ/(\d+)uL", line)
        if raw is not None:
            stone.append(int(raw.group(1)))
            wood.append(int(raw.group(2)))
            total.append(int(raw.group(3)))
        if body is not None:
            attention.append(int(body.group(1)))
            metabolic.append(int(body.group(2)))
            hydration.append(int(body.group(3)))
    return (
        "kit-acquisition=["
        f"executed:{len(witnesses)} live-routes:{live_kit_routes} preassembled:{preassembled_routes} "
        f"source=[fixture:{fixture_sources}/{len(witnesses)} "
        f"pickup-runtime:{runtime_pickups}/{len(witnesses)} "
        f"world-gathering:{world_gathering}/{len(witnesses)}] "
        f"raw:stone-{scaled_span(stone, 1_000_000, 'kg')}"
        f"/wood-{scaled_span(wood, 1_000_000, 'kg')}"
        f"/total-{scaled_span(total, 1_000_000, 'kg')} "
        f"attention:{_span(attention, 't')} "
        f"physical:{physical_duration_span(lines, attention)} "
        f"body:{scaled_span(metabolic, 1_000_000_000_000, 'kJ')}/"
        f"{scaled_span(hydration, 1_000, 'mL')}]"
    )


def _route_tradeoff(lines: list[str]) -> str:
    routes = [line for line in lines if line.startswith("LIBERATION ROUTE TRADEOFF ")]
    manual_attention: list[int] = []
    manual_native: list[int] = []
    powered_elapsed: list[int] = []
    powered_charge_attention: list[int] = []
    powered_native: list[int] = []
    attention_payback_jobs: list[int] = []
    attention_saved: list[int] = []
    planned_campaigns: list[int] = []
    executed_campaigns: list[int] = []
    live_kit_justified = 0
    live_kit_routes = 0
    campaign_manual_attention: list[int] = []
    campaign_powered_attention: list[int] = []
    campaign_manual_energy: list[int] = []
    campaign_powered_energy: list[int] = []
    campaign_manual_hydration: list[int] = []
    campaign_powered_hydration: list[int] = []
    for line in routes:
        manual = re.search(
            r"manual=\[attention:(\d+)t native:(\d+)mg recovery:\d+ppm",
            line,
        )
        powered = re.search(
            r"powered=\[elapsed:(\d+)t charge-attention:(\d+)t native:(\d+)mg\]",
            line,
        )
        if manual is None or powered is None:
            continue
        manual_ticks = int(manual.group(1))
        manual_mass = int(manual.group(2))
        elapsed_ticks = int(powered.group(1))
        charge_ticks = int(powered.group(2))
        powered_mass = int(powered.group(3))
        manual_attention.append(manual_ticks)
        manual_native.append(manual_mass)
        powered_elapsed.append(elapsed_ticks)
        powered_charge_attention.append(charge_ticks)
        powered_native.append(powered_mass)
        saved = manual_ticks - charge_ticks
        if saved > 0:
            attention_saved.append(saved)
        if " continuity=live-kit-used" in line:
            live_kit_routes += 1
        planned = re.search(r"campaign=\[planned:(\d+)batches", line)
        executed = re.search(r"\bexecuted:(\d+)\b", line)
        payback = re.search(r"\bkit-payback:(\d+)batches\b", line)
        campaign_attention = re.search(
            r"\battention:manual:(\d+)t/powered:(\d+)t\b", line
        )
        campaign_body = re.search(
            r"\bbody:manual:(\d+)nJ/(\d+)uL powered:(\d+)nJ/(\d+)uL\b",
            line,
        )
        if planned is not None:
            planned_campaigns.append(int(planned.group(1)))
        if executed is not None and " continuity=live-kit-used" in line:
            executed_campaigns.append(int(executed.group(1)))
        if payback is not None and " continuity=live-kit-used" in line:
            attention_payback_jobs.append(int(payback.group(1)))
        if campaign_attention is not None:
            campaign_manual_attention.append(int(campaign_attention.group(1)))
            campaign_powered_attention.append(int(campaign_attention.group(2)))
        if campaign_body is not None:
            campaign_manual_energy.append(int(campaign_body.group(1)))
            campaign_manual_hydration.append(int(campaign_body.group(2)))
            campaign_powered_energy.append(int(campaign_body.group(3)))
            campaign_powered_hydration.append(int(campaign_body.group(4)))
        if " continuity=live-kit-used" in line and " justified:true" in line:
            live_kit_justified += 1
    native_gain = [
        powered - manual
        for powered, manual in zip(powered_native, manual_native, strict=True)
    ]
    campaign_attention_saved = [
        manual - powered
        for manual, powered in zip(campaign_manual_attention, campaign_powered_attention, strict=True)
    ]
    acquisition_witnesses = sum(
        line.startswith("LIBERATION KIT ACQUISITION ") for line in lines
    )
    preassembled_routes = len(routes) - live_kit_routes
    return (
        "route-tradeoff=["
        f"samples:{len(manual_attention)}/{len(routes)} "
        f"manual-attention:{_span(manual_attention, 't')} "
        f"powered-charge-attention:{_span(powered_charge_attention, 't')} "
        f"attention-saved:{_span(attention_saved, 't')}/batch "
        f"powered-elapsed:{_span(powered_elapsed, 't')} "
        f"native-gain:{_span(native_gain, 'mg')} "
        f"evidence-mode=[raw-kit-continuity:{live_kit_routes} "
        f"acquisition-witnesses:{acquisition_witnesses} "
        f"controlled-preassembled:{preassembled_routes}] "
        f"disclosed-campaign:{_span(planned_campaigns, 'batches')} "
        f"executed-campaign:{_span(executed_campaigns, 'batches')} "
        f"live-kit-justified:{live_kit_justified}/{live_kit_routes} "
        f"campaign-attention=[manual:{_span(campaign_manual_attention, 't')} "
        f"powered:{_span(campaign_powered_attention, 't')} "
        f"saved:{_span(campaign_attention_saved, 't')}] "
        f"campaign-body=[manual:{scaled_span(campaign_manual_energy, 1_000_000_000_000, 'kJ')}/"
        f"{scaled_span(campaign_manual_hydration, 1_000, 'mL')} "
        f"powered:{scaled_span(campaign_powered_energy, 1_000_000_000_000, 'kJ')}/"
        f"{scaled_span(campaign_powered_hydration, 1_000, 'mL')}] "
        f"kit-attention-payback:{_span(attention_payback_jobs, 'jobs')}]"
    )


def _kit_decision(lines: list[str]) -> str:
    routes = [line for line in lines if line.startswith("LIBERATION ROUTE TRADEOFF ")]
    payback_jobs: list[int] = []
    planned_batches: list[int] = []
    selected_kit = 0
    selected_manual = 0
    for line in routes:
        campaign = re.search(r"campaign=\[planned:(\d+)batches", line)
        payback = re.search(r"\bkit-payback:(\d+)batches\b", line)
        executed = re.search(r"\bexecuted:(\d+)\b", line)
        if campaign is None or payback is None or executed is None:
            continue
        planned = int(campaign.group(1))
        actual_payback = int(payback.group(1))
        payback_jobs.append(actual_payback)
        planned_batches.append(planned)
        if planned >= actual_payback and int(executed.group(1)) == planned:
            selected_kit += 1
        else:
            selected_manual += 1
    return (
        "kit-decision=["
        f"attention-payback:{_span(payback_jobs, 'jobs')} "
        f"disclosed-horizon:{_span(planned_batches, 'batches')} "
        f"selected:kit{selected_kit}/manual{selected_manual} "
        "policy=manual-below-payback;kit-at-or-above "
        f"evaluated:{len(payback_jobs)}/{len(routes)} "
        f"preassembled:{sum(' continuity=controlled-preassembled-kit' in line for line in routes)}]"
    )


def liberation_summary(lines: list[str]) -> str | None:
    liberation = [
        line for line in lines if line.startswith("LIBERATION FRONTIER CAPABILITY ")
    ]
    if not liberation:
        return None

    grade_span = _span(
        _numeric_values(liberation, r"\bfinal:\d+mg/(\d+)ppm"),
        "ppm",
    )
    scavenged_span = _span(
        _numeric_values(liberation, r"\bscavenger-recovered:(\d+)mg"),
        "mg",
    )
    native_span = _span(
        _numeric_values(liberation, r"\bnative-copper=(\d+)mg"),
        "mg",
    )
    marginal_attention, marginal_native = _scavenger_marginal(lines)
    frontier, foundry_readiness = _frontier_evidence(lines)
    cleanup_executed = sum("cleanup-executed=true" in line for line in liberation)
    usable_sink = sum(
        "reason=required-native-copper-conversion" in line for line in liberation
    )
    return (
        "ORDINARY SUMMARY probe=primitive-liberation "
        f"samples={len(liberation)} cleanup-executed={cleanup_executed}/{len(liberation)} "
        f"final-concentrate-grade={grade_span} native-copper={native_span} "
        f"scavenger-copper={scavenged_span} "
        f"scavenger-marginal=[attention:{marginal_attention} native:{marginal_native}] "
        f"{_kit_acquisition(lines)} "
        f"{_kit_decision(lines)} "
        f"{_first_foundry(lines)} "
        f"{_route_tradeoff(lines)} "
        f"conserved={sum('matter=conserved' in line for line in liberation)} "
        f"ordinary-loop=[concentrate-reachable:{len(liberation)}/{len(liberation)} "
        f"usable-native-sink:{usable_sink}/{len(liberation)}] "
        f"remaining-frontier={frontier} {foundry_readiness}"
    )
