"""First-foundry bootstrap summary after settlement mechanization."""

from __future__ import annotations

import re

from .common import sample_shape, scaled_span


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
    available = _values(witnesses, r"\bremaining-native:(\d+)mg")
    required = _values(witnesses, r"\brequired:(\d+)mg")
    shortfall = _values(witnesses, r"\bshortfall:(\d+)mg")
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

    return (
        "ORDINARY SUMMARY probe=foundry-bootstrap "
        f"samples={len(witnesses)} sample-shape=[{sample_shape(witnesses)}] "
        f"choice=[build:{len(builds)} defer:{len(deferred)}] "
        f"copper=[available:{scaled_span(available, 1_000, 'g')} "
        f"threshold:{scaled_span(required, 1_000, 'g')} "
        f"shortfall:{scaled_span(shortfall, 1_000, 'g')}] "
        f"investment=[capital:{scaled_span(capital, 1_000, 'g')} "
        f"cast-stock:{scaled_span(ingots, 1_000, 'g')} setup:{_span(fabrication, 't')} "
        f"fabrication=[stone:{_span(stone_fabrication, 't')} "
        f"wood:{_span(wood_fabrication, 't')} copper:{_span(copper_fabrication, 't')}]] "
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
