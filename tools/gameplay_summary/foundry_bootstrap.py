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
        f"rhythm=[bootstrap-cooldown:{_span(cooldown, 't')} "
        f"bootstrap-autonomous:{_span(autonomous, 't')}] "
        f"attention=[build:{_span(build_attention, 't')} defer:{_span(defer_attention, 't')}] "
        f"separate-episode={separate_episode}/{len(witnesses)}"
    )
