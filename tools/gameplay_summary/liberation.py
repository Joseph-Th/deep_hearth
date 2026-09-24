"""Primitive-liberation summary and foundry-frontier readiness evidence."""

from __future__ import annotations

import re

from .common import physical_duration_span, scaled_span


def _span(values: list[int], unit: str) -> str:
    return f"{min(values)}..{max(values)}{unit}" if values else "n/a"


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
    charge = _numeric_values(witnesses, r"\belectrical-charge=\[(\d+)t")
    melt = _numeric_values(witnesses, r"\bmelt=\[(\d+)t")
    cast = _numeric_values(witnesses, r"\bcast=\[(\d+)t")
    cold_work = _numeric_values(witnesses, r"\bcold-work:(\d+)t")
    total = _numeric_values(witnesses, r"\btotal=(\d+)t/")
    closed = sum(" continuation=closed-loop" in line for line in witnesses)
    return (
        "first-foundry=["
        f"executed:{len(witnesses)} closed-loop:{closed}/{len(witnesses)} "
        f"fabrication:{_span(fabrication, 't')} charge:{_span(charge, 't')} "
        f"melt:{_span(melt, 't')} cast:{_span(cast, 't')} "
        f"ingot-rework:{_span(cold_work, 't')} total:{_span(total, 't')}]"
    )


def _kit_acquisition(lines: list[str]) -> str:
    witnesses = [
        line for line in lines if line.startswith("LIBERATION KIT ACQUISITION ")
    ]
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
        f"executed:{len(witnesses)} "
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
        kit = re.search(r"base-kit=\[executed attention:(\d+)t", line)
        saved = manual_ticks - charge_ticks
        if saved > 0:
            attention_saved.append(saved)
        if " continuity=live-kit-used" in line:
            live_kit_routes += 1
        campaign = re.search(
            r"campaign=\[planned:(\d+)batches .*?"
            r"(?:attention:manual:(\d+)t/powered:(\d+)t "
            r"body:manual:(\d+)nJ/(\d+)uL powered:(\d+)nJ/(\d+)uL )?"
            r"justified:([^\]]+)\]",
            line,
        )
        if campaign is not None:
            planned_campaigns.append(int(campaign.group(1)))
            if campaign.group(2) is not None:
                campaign_manual_attention.append(int(campaign.group(2)))
                campaign_powered_attention.append(int(campaign.group(3)))
                campaign_manual_energy.append(int(campaign.group(4)))
                campaign_manual_hydration.append(int(campaign.group(5)))
                campaign_powered_energy.append(int(campaign.group(6)))
                campaign_powered_hydration.append(int(campaign.group(7)))
            if (
                " continuity=live-kit-used" in line
                and campaign.group(8) == "true"
            ):
                live_kit_justified += 1
        if kit is not None and saved > 0:
            kit_ticks = int(kit.group(1))
            attention_payback_jobs.append(
                (kit_ticks + saved - 1) // saved
            )
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
        f"evidence-mode=[raw-kit-continuity:{live_kit_routes}/{acquisition_witnesses} "
        f"exploratory-preassembled:{preassembled_routes}/{preassembled_routes}] "
        f"disclosed-campaign:{_span(planned_campaigns, 'batches')} "
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
    payback_jobs: list[int] = []
    planned_batches: list[int] = []
    selected_kit = 0
    selected_manual = 0
    for line in lines:
        if not line.startswith("LIBERATION ROUTE TRADEOFF "):
            continue
        manual = re.search(r"manual=\[attention:(\d+)t", line)
        powered = re.search(r"powered=\[elapsed:\d+t charge-attention:(\d+)t", line)
        kit = re.search(r"base-kit=\[executed attention:(\d+)t", line)
        campaign = re.search(r"campaign=\[planned:(\d+)batches", line)
        if None in (manual, powered, kit, campaign):
            continue
        assert manual is not None
        assert powered is not None
        assert kit is not None
        assert campaign is not None
        saved = int(manual.group(1)) - int(powered.group(1))
        if saved <= 0:
            continue
        payback = (int(kit.group(1)) + saved - 1) // saved
        planned = int(campaign.group(1))
        payback_jobs.append(payback)
        planned_batches.append(planned)
        if planned >= payback:
            selected_kit += 1
        else:
            selected_manual += 1
    return (
        "kit-decision=["
        f"attention-payback:{_span(payback_jobs, 'jobs')} "
        f"disclosed-horizon:{_span(planned_batches, 'batches')} "
        f"selected:kit{selected_kit}/manual{selected_manual} "
        "policy=manual-below-payback;kit-at-or-above]"
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
