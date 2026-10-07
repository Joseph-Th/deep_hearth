"""Concise ordinary-play evidence for settlement investment decisions."""

from __future__ import annotations

import re

from .common import organic_only, sample_shape


def _values(lines: list[str], pattern: str) -> list[int]:
    return [
        int(match.group(1))
        for line in lines
        if (match := re.search(pattern, line)) is not None
    ]


def _span(values: list[int], unit: str) -> str:
    return f"{min(values)}..{max(values)}{unit}" if values else "n/a"


def settlement_summary(lines: list[str]) -> str | None:
    settlement = [
        line for line in lines if line.startswith("SETTLEMENT EXPERIENCE ")
    ]
    if not settlement:
        return None

    direct = sum("choice:frame-saw" in line for line in settlement)
    mechanized = sum("choice:sash-sawmill" in line for line in settlement)
    organic_settlement = organic_only(settlement)
    organic_direct = sum("choice:frame-saw" in line for line in organic_settlement)
    organic_mechanized = sum("choice:sash-sawmill" in line for line in organic_settlement)
    batches = _values(settlement, r"demand=\[batches:(\d+)")
    organic_batches = _values(organic_settlement, r"demand=\[batches:(\d+)")
    baseline = _values(settlement, r"baseline:(\d+)t")
    machine = _values(settlement, r"mechanized:(\d+)t")
    setup = _values(settlement, r"setup:(\d+)t")
    delegated = _values(settlement, r"delegated:(\d+)t")
    initial_charge_events = _values(
        settlement, r"execution=\[[^\]]*charge-events:(\d+)"
    )
    followup_charge_events = _values(
        settlement, r"followup=\[[^\]]*charge-events:(\d+)"
    )
    packed_batches = _values(
        settlement,
        r"decision=\[.*?power-cycle=\[charges:\d+ max-batches-per-charge:(\d+)\].*?policy=",
    )
    policy_returns = _values(settlement, r"minimum-return:(\d+)ppm")
    crossover_values = _values(settlement, r"baseline-crossover:(\d+)")
    crossover_none = sum("baseline-crossover:none" in line for line in settlement)
    frame_start_condition = _values(
        settlement, r"prior-infrastructure=\[frame-saw-condition:(\d+)ppm"
    )
    crank_start_condition = _values(
        settlement, r"crank-condition:(\d+)ppm"
    )
    organic_frame_start_condition = _values(
        organic_settlement, r"prior-infrastructure=\[frame-saw-condition:(\d+)ppm"
    )
    organic_crank_start_condition = _values(
        organic_settlement, r"crank-condition:(\d+)ppm"
    )
    crossover = ""
    if crossover_values or crossover_none:
        crossover = (
            " baseline-crossover=["
            f"found:{len(crossover_values)}/{len(settlement)} "
            f"range:{_span(crossover_values, 'batches')} none:{crossover_none}]"
        )

    storage_experiences = [
        line for line in lines if line.startswith("SETTLEMENT STORAGE EXPERIENCE ")
    ]
    storage_scale = ""
    if storage_experiences:
        assert len(storage_experiences) == 1
        storage = storage_experiences[0]
        stages = re.search(
            r"stages=\[stone:\[capacity:(\d+)nJ embodied:(\d+)mg portfolio-charges:(\d+) max-packed:(\d+)\] "
            r"paired:\[capacity:(\d+)nJ embodied:(\d+)mg portfolio-charges:(\d+) max-packed:(\d+)\] "
            r"bank:\[capacity:(\d+)nJ embodied:(\d+)mg portfolio-charges:(\d+) max-packed:(\d+)\]\]",
            storage,
        )
        if stages is None:
            raise AssertionError("settlement storage experience lost its stage telemetry")
        storage_scale = (
            " storage-scale=["
            f"capacity:{stages.group(1)}->{stages.group(5)}->{stages.group(9)}nJ "
            f"portfolio-charges:{stages.group(3)}->{stages.group(7)}->{stages.group(11)} "
            f"max-packed:{stages.group(4)}->{stages.group(8)}->{stages.group(12)} "
            "identity:additive]"
        )
    builds = sum(" upgraded:true " in line for line in settlement)
    mechanized_lines = [line for line in settlement if "choice:sash-sawmill" in line]
    saved = _values(mechanized_lines, r"margin:\+(\d+)t")
    mechanized_delegated = _values(mechanized_lines, r"delegated:(\d+)t")
    capacity_decisions = sum(" basis:capacity " in line for line in settlement)
    attention_decisions = sum(" basis:attention-return " in line for line in settlement)
    organic_capacity_decisions = sum(
        " basis:capacity " in line for line in organic_settlement
    )
    machine_repeat_drop: list[int] = []
    manual_repeat_growth: list[int] = []
    repeat_delegated: list[int] = []
    repeat_reuse = 0
    repeat_reinvested = 0
    repeat_count = 0
    followup_complete = 0
    followup_partial = 0
    followup_blocked = 0
    followup_capacity_decisions = 0
    followup_attention_decisions = 0
    for line in settlement:
        repeat = re.search(
            r"execution=\[active:(\d+)t .*? upgraded:(true|false).*?\] "
            r"followup=\[demand-batches:(\d+) completed:(\d+) terminal:([^ ]+) "
            r"route:[^ ]+ active:(\d+)t .*? delegated:(\d+)t "
            r"machine-owned-before:(true|false) reinvested:(true|false)",
            line,
        )
        if repeat is None:
            continue
        repeat_count += 1
        first_attention = int(repeat.group(1))
        upgraded = repeat.group(2) == "true"
        demand_batches = int(repeat.group(3))
        completed_batches = int(repeat.group(4))
        terminal = repeat.group(5)
        followup_attention = int(repeat.group(6))
        delegated_ticks = int(repeat.group(7))
        owned_before = repeat.group(8) == "true"
        reinvested = repeat.group(9) == "true"
        followup_capacity_decisions += "followup-reassessment=[basis:capacity " in line
        followup_attention_decisions += (
            "followup-reassessment=[basis:attention-return " in line
        )
        if terminal == "complete":
            assert completed_batches == demand_batches
            followup_complete += 1
        elif terminal == "wear-limited":
            assert 0 < completed_batches < demand_batches
            followup_partial += 1
        elif terminal == "wear-blocked":
            assert completed_batches == 0
            followup_blocked += 1
        else:
            raise AssertionError(f"unknown settlement follow-up terminal {terminal}")
        if upgraded:
            assert owned_before and not reinvested
            repeat_reuse += 1
            if completed_batches == demand_batches:
                machine_repeat_drop.append(first_attention - followup_attention)
            repeat_delegated.append(delegated_ticks)
        elif reinvested:
            assert not owned_before
            repeat_reinvested += 1
            repeat_delegated.append(delegated_ticks)
        else:
            assert not owned_before
            if completed_batches == demand_batches:
                manual_repeat_growth.append(followup_attention - first_attention)
    machine_experiences = [
        line for line in lines if line.startswith("SETTLEMENT MACHINE EXPERIENCE ")
    ]
    machine_families = {
        match.group(1)
        for line in machine_experiences
        if (match := re.search(r"\bfamily=([^ ]+)", line)) is not None
    }
    machine_floors = _values(machine_experiences, r"minimum-attention-return:(\d+)t")
    machine_savings = _values(machine_experiences, r"attention-saved:(\d+)t")
    machine_delegated = _values(machine_experiences, r"delegated:(\d+)t")
    machine_charge_events = _values(
        machine_experiences, r"power-cycle=\[charges:(\d+)"
    )
    machine_packed_batches = _values(
        machine_experiences, r"max-batches-per-charge:(\d+)"
    )
    short_kept = sum("choice:keep-prior" in line for line in machine_experiences)
    project_upgrades = sum("choice:upgrade" in line for line in machine_experiences)
    portfolio = ""
    capital_witnesses = ""
    if machine_experiences:
        portfolio = (
            " portfolio=["
            f"families:{len(machine_families)} short-kept:{short_kept}/{len(machine_experiences)} "
            f"project-upgrade:{project_upgrades}/{len(machine_experiences)} "
            f"return-floor:{_span(machine_floors, 't')} "
            f"attention-saved:{_span(machine_savings, 't')} "
            f"delegated:{_span(machine_delegated, 't')} "
            f"power-cycle=[charges:{_span(machine_charge_events, '')} "
            f"max-batches-per-charge:{_span(machine_packed_batches, '')}]]"
        )
        capital_witnesses = (
            " capital-crossover-witnesses=[scope:separate-executed-projects "
            f"families:{len(machine_families)} short-kept:{short_kept}/{len(machine_experiences)} "
            f"project-upgrade:{project_upgrades}/{len(machine_experiences)} "
            f"return-floor:{_span(machine_floors, 't')} "
            f"attention-saved:{_span(machine_savings, 't')} "
            f"delegated:{_span(machine_delegated, 't')}]"
        )
    delegation_experiences = [
        line for line in lines if line.startswith("SETTLEMENT DELEGATION EXPERIENCE ")
    ]
    delegation_families = {
        match.group(1)
        for line in delegation_experiences
        if (match := re.search(r"\bfamily=([^ ]+)", line)) is not None
    }
    delegation_savings = _values(delegation_experiences, r"active-saving:(\d+)t")
    autonomous = _values(delegation_experiences, r"autonomous:(\d+)t")
    stored_work = ""
    delegation_witnesses = ""
    if delegation_experiences:
        stored_work = (
            " stored-work=["
            f"families:{len(delegation_families)} active-saving:{_span(delegation_savings, 't')} "
            f"autonomous:{_span(autonomous, 't')}]"
        )
        delegation_witnesses = (
            " delegation-witnesses=[scope:separate-executed-projects "
            f"families:{len(delegation_families)} active-saving:{_span(delegation_savings, 't')} "
            f"autonomous:{_span(autonomous, 't')}]"
        )

    witness_scope = ""
    witness_parts = []
    if machine_experiences:
        witness_parts.append(f"capital:{len(machine_families)}f/separate-executed")
    if delegation_experiences:
        witness_parts.append(f"delegation:{len(delegation_families)}f/separate-executed")
    if witness_parts:
        witness_scope = f" witness-scope=[{' '.join(witness_parts)}]"
    followup_payoff = ""
    if repeat_count:
        followup_payoff = (
            " followup-payoff=["
            f"complete:{followup_complete}/{repeat_count} "
            f"wear-limited:{followup_partial}/{repeat_count} "
            f"wear-blocked:{followup_blocked}/{repeat_count} "
            f"owned-machine-reuse:{repeat_reuse}/{repeat_count} "
            f"reinvested:{repeat_reinvested}/{repeat_count} "
            f"machine-active-drop:{_span(machine_repeat_drop, 't')} "
            f"machine-delegated:{_span(repeat_delegated, 't')} "
            f"manual-wear-growth:{_span(manual_repeat_growth, 't')}]"
        )

    return (
        "ORDINARY SUMMARY probe=settlement "
        f"samples={len(settlement)} sample-shape=[{sample_shape(settlement)}] "
        f"choice=[frame:{direct} sawmill:{mechanized}] "
        f"organic-play=[decision=[frame:{organic_direct} sawmill:{organic_mechanized}] "
        f"demand:{_span(organic_batches, 'batches')} "
        f"capacity-driven:{organic_capacity_decisions}/{len(organic_settlement)} "
        f"wear=[saw:{_span(organic_frame_start_condition, 'ppm')} "
        f"crank:{_span(organic_crank_start_condition, 'ppm')}]] "
        f"investment-policy-return={_span(policy_returns, 'ppm')} "
        f"prior-wear=[saw:{_span(frame_start_condition, 'ppm')} "
        f"crank:{_span(crank_start_condition, 'ppm')}] "
        f"demand={min(batches)}..{max(batches)}batches"
        f"{crossover} "
        f"attention=[baseline:{min(baseline)}..{max(baseline)}t "
        f"mechanized:{min(machine)}..{max(machine)}t "
        f"setup:{min(setup)}..{max(setup)}t] "
        f"payoff=[attention-saved:{_span(saved, 't')} "
        f"delegated:{_span(mechanized_delegated, 't')}] "
        f"mechanization=[builds:{builds}/{len(settlement)} "
        f"delegated:{min(delegated)}..{max(delegated)}t "
        f"decision-basis=[attention:{attention_decisions} capacity:{capacity_decisions}]]"
        f" power-cycle=[initial-charge-events:{_span(initial_charge_events, '')} "
        f"followup-charge-events:{_span(followup_charge_events, '')} "
        f"machine-max-batches-per-charge:{_span(packed_batches, '')}]"
        f" followup-basis=[attention:{followup_attention_decisions} capacity:{followup_capacity_decisions}]"
        f"{followup_payoff}{portfolio}{stored_work}{storage_scale}{capital_witnesses}{delegation_witnesses}{witness_scope}"
    )
