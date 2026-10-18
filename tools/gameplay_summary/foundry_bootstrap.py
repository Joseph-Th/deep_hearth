"""First-foundry bootstrap summary after settlement mechanization."""

from __future__ import annotations

import re

from .common import organic_only, sample_shape, scaled_span


def _values(lines: list[str], pattern: str) -> list[int]:
    return [
        int(match.group(1))
        for line in lines
        if (match := re.search(pattern, line)) is not None
    ]


def _span(values: list[int], unit: str) -> str:
    return f"{min(values)}..{max(values)}{unit}" if values else "n/a"


def foundry_bootstrap_summary(lines: list[str]) -> str | None:
    witnesses = [line for line in lines if line.startswith("FIRST FOUNDRY EXPERIENCE ")]
    if not witnesses:
        return None

    builds = [line for line in witnesses if " foundry-build=true " in line]
    deferred = [line for line in witnesses if " foundry-build=false " in line]
    organic_witnesses = organic_only(witnesses)
    organic_builds = [line for line in organic_witnesses if " foundry-build=true " in line]
    organic_deferred = [line for line in organic_witnesses if " foundry-build=false " in line]
    inherited_condition_min = _values(witnesses, r"\binherited-condition=(\d+)\.\.\d+ppm")
    inherited_condition_max = _values(witnesses, r"\binherited-condition=\d+\.\.(\d+)ppm")
    available = _values(witnesses, r"\bremaining-native:(\d+)mg")
    required = _values(witnesses, r"\brequired:(\d+)mg")
    shortfall = _values(witnesses, r"\bshortfall:(\d+)mg")
    organic_shortfall = _values(organic_witnesses, r"\bshortfall:(\d+)mg")
    organic_inherited_min = _values(
        organic_witnesses, r"\binherited-condition=(\d+)\.\.\d+ppm"
    )
    organic_inherited_max = _values(
        organic_witnesses, r"\binherited-condition=\d+\.\.(\d+)ppm"
    )
    shortfall_before = _values(witnesses, r"\bshortfall-before:(\d+)mg")
    capital = _values(witnesses, r"\bfoundry-capital:(\d+)mg")
    ingots = _values(witnesses, r"\bcast-ingots:(\d+)mg")
    fabrication = _values(builds, r"\bfabrication=\[total:(\d+)t/")
    stone_fabrication = _values(builds, r"\bmaterial=\[stone:(\d+)t")
    wood_fabrication = _values(builds, r"\bmaterial=\[stone:\d+t wood:(\d+)t")
    copper_fabrication = _values(
        builds, r"\bmaterial=\[stone:\d+t wood:\d+t copper:(\d+)t"
    )
    workshop_saved = _values(builds, r"\bworkshop-reuse=\[[^\]]*\bsaved:(\d+)t")
    cooldown = _values(builds, r"\bcampaign=\[[^\]]*\bcooldown:(\d+)t")
    autonomous = _values(builds, r"\bcampaign=\[[^\]]*\bautonomous:(\d+)t")
    mold_from = _values(builds, r"\bmold-upgrade=\[(\d+)mg->")
    mold_to = _values(builds, r"\bmold-upgrade=\[\d+mg->(\d+)mg\]")
    settlement_executed = sum(" settlement-cast=[executed:true " in line for line in builds)
    cast_stock = [line for line in lines if line.startswith("FIRST FOUNDRY CAST-STOCK ")]
    if len(cast_stock) != settlement_executed:
        raise ValueError(
            "foundry output lost its executed cast-stock manufacturing continuation"
        )
    helve = [
        line for line in lines
        if line.startswith("FIRST FOUNDRY HELVE REINVESTMENT ")
    ]
    if len(helve) != settlement_executed:
        raise ValueError(
            "first foundry lost the executed finite-timber-to-powered-hammer continuation"
        )
    helve_gather = _values(helve, r"\bgather-attention:(\d+)t")
    helve_board_mass = _values(helve, r"\bboard:(\d+)mg attention:")
    helve_boards = _values(helve, r"\bboard:\d+mg attention:(\d+)t")
    helve_handles = _values(helve, r"\bhandles:400000mg attention:(\d+)t")
    helve_bearing = _values(helve, r"\bcopper-bearing:20000mg attention:(\d+)t")
    helve_charge = _values(helve, r"\bcharge-attention:(\d+)t")
    helve_automated = _values(helve, r"\bautonomous:(\d+)t")
    helve_elapsed = _values(helve, r"\belapsed:(\d+)t")
    helve_manual = _values(helve, r"\bcomparison=\[manual-winding:(\d+)t")
    helve_matched_elapsed = _values(
        helve, r"\bcomparison=\[[^\]]*\bacquired-helve:(\d+)t"
    )
    if (
        any(
            len(values) != len(helve)
            for values in (
                helve_gather, helve_board_mass, helve_boards, helve_handles, helve_bearing,
                helve_charge, helve_automated, helve_elapsed,
                helve_manual, helve_matched_elapsed,
            )
        )
        or any(
            "continuity=one-exact-local-body" not in line
            or "source=[cast:80000mg finite-timber:3000000mg]" not in line
            or "acquired=[timber:3000000mg " not in line
            or "upgraded:treadle-to-helve identity:preserved" not in line
            or "production=[winding:54000mg " not in line
            or "cast-remains:0mg" not in line
            or "electrical=[upgrade:plate-to-wound identity:preserved capacity:" not in line
            or "matter=conserved" not in line
            for line in helve
        )
        or any(not 800_000 <= mass <= 3_000_000 for mass in helve_board_mass)
        or any(ticks <= 0 for ticks in helve_gather + helve_automated + helve_elapsed)
        or any(first != matched for first, matched in zip(helve_elapsed, helve_matched_elapsed))
    ):
        raise ValueError("foundry-to-powered reinvestment lost executed player evidence")
    cast_saw_ticks = _values(cast_stock, r"\bspare-saw-blade:54000mg attention:(\d+)t")
    cast_winding_ticks = _values(cast_stock, r"\bdynamo-winding:54000mg attention:(\d+)t")
    if len(cast_saw_ticks) != len(cast_stock) or len(cast_winding_ticks) != len(cast_stock):
        raise ValueError("foundry cast-stock branch lost measurable fabrication work")
    wound_capacity_before = _values(
        cast_stock, r"buffer-reinvestment=\[base:(\d+)nJ"
    )
    wound_capacity_after = _values(
        cast_stock, r"buffer-reinvestment=\[base:\d+nJ upgraded:(\d+)nJ"
    )
    wound_charge = _values(
        cast_stock, r"buffer-reinvestment=\[[^\]]*\bcharged:(\d+)nJ"
    )
    wound_charge_ticks = _values(
        cast_stock, r"buffer-reinvestment=\[[^\]]*\bcharge-attention:(\d+)t"
    )
    if (
        any(len(values) != len(cast_stock) for values in (
            wound_capacity_before, wound_capacity_after, wound_charge, wound_charge_ticks,
        ))
        or any("retained-identity:true" not in line for line in cast_stock)
        or any(
            "buffer-catalog=[initial:unready after-winding:ready next-rack:unready]"
            not in line
            for line in cast_stock
        )
        or any(
            not 0 < before < charged <= after or ticks <= 0
            for before, after, charged, ticks in zip(
                wound_capacity_before, wound_capacity_after,
                wound_charge, wound_charge_ticks,
            )
        )
    ):
        raise ValueError("cast winding did not establish an executed finite-buffer upgrade")
    settlement_shortfall = _values(
        builds, r"\bsettlement-cast=\[[^\]]*\bsupply-shortfall:(\d+)mg"
    )
    settlement_cooldown = _values(
        builds, r"\bsettlement-cast=\[[^\]]*\bcooldown:(\d+)t"
    )
    build_attention = _values(builds, r"\btotal-player-attention:(\d+)t")
    defer_attention = _values(deferred, r"\bepisode-attention:(\d+)t")
    separate_episode = sum(" continuity=separate-episode " in line for line in witnesses)
    recovery_lines = [line for line in witnesses if " owned-ore-recovery=[" in line]
    recovery_needed = [line for line in recovery_lines if re.search(r"\bshortfall-before:[1-9]\d*mg", line)]
    recovery_executed = [line for line in recovery_lines if " executed:true " in line]
    recovery_closed = [
        line
        for line in recovery_executed
        if re.search(r"\bshortfall:0mg", line) is not None
    ]
    recovery_partial = [
        line
        for line in recovery_executed
        if re.search(r"\bshortfall:[1-9]\d*mg", line) is not None
    ]
    recovery_feed = _values(recovery_executed, r"\bfeed:(\d+)mg")
    recovery_native = _values(recovery_executed, r"\brecovered:(\d+)mg")
    recovery_attention = _values(
        recovery_executed,
        r"\bowned-ore-recovery=\[[^\]]*\battention:(\d+)t",
    )
    recovery_autonomous = _values(
        recovery_executed,
        r"\bowned-ore-recovery=\[[^\]]*\bautonomous:(\d+)t",
    )
    recovery_elapsed = _values(
        recovery_executed,
        r"\bowned-ore-recovery=\[[^\]]*\belapsed:(\d+)t",
    )
    recovery_batches = _values(
        recovery_executed,
        r"\bowned-ore-recovery=\[[^\]]*\bbatches:(\d+)",
    )
    recovery_on_reinforced_crank = sum(
        " owned-ore-recovery=[route:powered-inherited-line provider:copper-reinforced-hand-crank "
        in line
        for line in recovery_executed
    )
    powered_recovery = sum(
        " owned-ore-recovery=[route:powered-inherited-line " in line
        for line in recovery_executed
    )
    electrical_built = sum(
        "electrical-prime-mover=[base:treadle-built conversion:treadle-dynamo]" in line
        for line in builds
    )
    electrical_deferred = sum(
        "electrical-prime-mover=not-built" in line for line in deferred
    )
    organic_condition = (
        f"{min(organic_inherited_min)}..{max(organic_inherited_max)}ppm"
        if organic_inherited_min and organic_inherited_max
        else "n/a"
    )
    recovery_summary = ""
    if shortfall_before:
        recovery_summary = (
            " recovery=["
            f"needed:{len(recovery_needed)} executed:{len(recovery_executed)} "
            f"closed:{len(recovery_closed)} partial:{len(recovery_partial)} "
            f"skipped:{len(recovery_needed) - len(recovery_executed)} "
            f"feed:{scaled_span(recovery_feed, 1_000, 'g')} "
            f"native:{scaled_span(recovery_native, 1_000, 'g')} "
            f"powered:{powered_recovery}/{len(recovery_executed)} "
            f"reinforced-crank:{recovery_on_reinforced_crank}/{len(recovery_executed)} "
            f"attention:{_span(recovery_attention, 't')} "
            f"autonomous:{_span(recovery_autonomous, 't')} "
            f"elapsed:{_span(recovery_elapsed, 't')} "
            f"batches:{_span(recovery_batches, '')}]"
        )

    return (
        "ORDINARY SUMMARY probe=foundry-bootstrap "
        f"samples={len(witnesses)} sample-shape=[{sample_shape(witnesses)}] "
        f"choice=[build:{len(builds)} defer:{len(deferred)}] "
        f"organic-play=[build:{len(organic_builds)} defer:{len(organic_deferred)} "
        f"shortfall:{scaled_span(organic_shortfall, 1_000, 'g')} "
        f"inherited-wear:{organic_condition}] "
        f"inherited-condition=[{min(inherited_condition_min) if inherited_condition_min else 'n/a'}"
        f"..{max(inherited_condition_max) if inherited_condition_max else 'n/a'}ppm] "
        f"copper=[available:{scaled_span(available, 1_000, 'g')} "
        f"threshold:{scaled_span(required, 1_000, 'g')} "
        f"shortfall:{scaled_span(shortfall, 1_000, 'g')}] "
        f"{recovery_summary.strip()} "
        f"investment=[capital:{scaled_span(capital, 1_000, 'g')} "
        f"cast-stock:{scaled_span(ingots, 1_000, 'g')} setup:{_span(fabrication, 't')} "
        f"fabrication=[stone:{_span(stone_fabrication, 't')} "
        f"wood:{_span(wood_fabrication, 't')} copper:{_span(copper_fabrication, 't')}]] "
        f"electrical-transition=[treadle+dynamo:{electrical_built}/{len(builds)} "
        f"deferred-unbuilt:{electrical_deferred}/{len(deferred)}] "
        f"workshop-reuse=[attention-saved:{_span(workshop_saved, 't')}] "
        f"mold=[{scaled_span(mold_from, 1_000, 'g')}->{scaled_span(mold_to, 1_000, 'g')}] "
        f"settlement-batch=[executed:{settlement_executed}/{len(builds)} "
        f"supply-shortfall:{scaled_span(settlement_shortfall, 1_000, 'g')} "
        f"cooldown:{_span(settlement_cooldown, 't')}] "
        f"cast-stock=[executed:{len(cast_stock)}/{settlement_executed} "
        f"selected:blade-54g/{_span(cast_saw_ticks, 't')} "
        f"alternative:winding-54g/{_span(cast_winding_ticks, 't')} "
        "cost:60g-ingot/6g-recoverable-scrap/20g-left] "
        f"owned-helve=[executed:{len(helve)}/{settlement_executed} "
        f"timber:3kg gathered:{_span(helve_gather, 't')} "
        f"fabrication:[boards:{_span(helve_boards, 't')} "
        f"handles:{_span(helve_handles, 't')} "
        f"bearing:{_span(helve_bearing, 't')}] "
        f"charge:{_span(helve_charge, 't')} "
        f"automated:{_span(helve_automated, 't')} "
        f"elapsed:{_span(helve_elapsed, 't')} electrical:plate->wound "
        f"comparison=[manual:{_span(helve_manual, 't')} "
        f"acquired:{_span(helve_elapsed, 't')}]] "
        f"winding-buffer=[upgraded:{len(wound_charge)}/{len(cast_stock)} "
        f"capacity:{scaled_span(wound_capacity_before, 1_000_000_000_000, 'kJ')}"
        f"->{scaled_span(wound_capacity_after, 1_000_000_000_000, 'kJ')} "
        f"two-melt-charge:{scaled_span(wound_charge, 1_000_000_000_000, 'kJ')} "
        f"attention:{_span(wound_charge_ticks, 't')}] "
        f"rhythm=[bootstrap-cooldown:{_span(cooldown, 't')} "
        f"bootstrap-autonomous:{_span(autonomous, 't')}] "
        f"attention=[build:{_span(build_attention, 't')} defer:{_span(defer_attention, 't')}] "
        f"separate-episode={separate_episode}/{len(witnesses)}"
    )
