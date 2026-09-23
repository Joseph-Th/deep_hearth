"""Survival summary for ordinary gameplay evidence."""

from __future__ import annotations

import re

from .common import organic_only, sample_shape, scaled_span


def _span(values: list[int], unit: str = "") -> str:
    return f"{min(values)}..{max(values)}{unit}" if values else "n/a"


def _provisioning_evidence(lines: list[str], survival: list[str]) -> str:
    meal_mass_mg: list[int] = []
    drink_volume_ul: list[int] = []
    for line in survival:
        choice = re.search(
            r"choice=\[[^]]*meal:(\d+)mg drink:(\d+)uL\]",
            line,
        )
        if choice is not None:
            meal_mass_mg.append(int(choice.group(1)))
            drink_volume_ul.append(int(choice.group(2)))

    balanced_meal_extra_mg: list[int] = []
    balanced_diet_quality_gain_ppm: list[int] = []
    balanced_vitality_gain_ppm: list[int] = []
    for line in lines:
        if not line.startswith("SURVIVAL REVIEW "):
            continue
        tradeoff = re.search(
            r"tradeoff=\[meal-mass-delta:\+(\d+)mg .*?"
            r"diet-quality-delta:\+(\d+)ppm",
            line,
        )
        if tradeoff is not None:
            balanced_meal_extra_mg.append(int(tradeoff.group(1)))
            balanced_diet_quality_gain_ppm.append(int(tradeoff.group(2)))
        recovery = re.search(
            r"recovery-consequence=\[choice:actionable .*?"
            r"vitality:\d+->\[compact:\d+ balanced:\d+ delta:\+(\d+)ppm\]",
            line,
        )
        if recovery is not None:
            balanced_vitality_gain_ppm.append(int(recovery.group(1)))

    return (
        "provisioning=["
        f"meal:{scaled_span(meal_mass_mg, 1_000, 'g')} "
        f"drink:{scaled_span(drink_volume_ul, 1_000, 'mL')}] "
        "balanced-diet-counterfactual=["
        f"meal-extra:{scaled_span(balanced_meal_extra_mg, 1_000, 'g')} "
        f"diet-quality-gain:{scaled_span(balanced_diet_quality_gain_ppm, 1, 'ppm')} "
        f"vitality-gain:{scaled_span(balanced_vitality_gain_ppm, 1, 'ppm')}]"
    )


def survival_summary(lines: list[str]) -> str | None:
    survival = [line for line in lines if line.startswith("SURVIVAL EXPERIENCE ")]
    if not survival:
        return None
    count = lambda marker: sum(marker in line for line in survival)
    organic_survival = organic_only(survival)
    candidate_counts = [
        int(match.group(1))
        for line in survival
        if (match := re.search(r"\bcandidates:(\d+)", line)) is not None
    ]
    initial_work_drinks = []
    follow_up_work_drinks = []
    prospecting_ticks = []
    power_ticks = []
    final_hydration_ppm = []
    for line in survival:
        integrated = re.search(
            r"integrated=\[hydration-policy:([^\s]+) drink:(\d+)uL/\d+t "
            r"prospect:(\d+)t .*?reprovision:(?:true|false):(\d+)uL/\d+t "
            r"power:(\d+)t .*?final-reserve:\d+ppmE/(\d+)ppmH",
            line,
        )
        if integrated is not None:
            initial_work_drinks.append(int(integrated.group(2)))
            prospecting_ticks.append(int(integrated.group(3)))
            follow_up_work_drinks.append(int(integrated.group(4)))
            power_ticks.append(int(integrated.group(5)))
            final_hydration_ppm.append(int(integrated.group(6)))
    return (
        "ORDINARY SUMMARY probe=survival "
        f"samples={len(survival)} sample-shape=[{sample_shape(survival)}] "
        f"pressure=[hydration:{count('pressure=hydration')} energy:{count('pressure=energy')}] "
        f"diet=[balanced:{count('diet:balanced-recovery')} compact:{count('diet:compact-calories')}] "
        f"{_provisioning_evidence(lines, survival)} "
        f"preservation-opportunity=[scarce:{count('mode:scarce-timber')} "
        f"choice-rich:{count('mode:choice-rich-timber')} "
        f"alternate:{count('mode:alternate-material')} "
        f"singleton:{sum(value == 1 for value in candidate_counts)} "
        f"multi:{sum(value > 1 for value in candidate_counts)}] "
        f"preservation=[declined:{count('storage-policy:decline')} "
        f"efficient:{count('storage-policy:attention-efficient')} "
        f"singleton:{count('storage-policy:enclosure-singleton')} "
        f"frontier:{count('storage-policy:balanced-frontier')} "
        f"maximum:{count('storage-policy:maximum-protection')}] "
        f"commitment=[cleared:{count('commitment-reason:return-clears-threshold')} "
        f"declined-return:{count('commitment-reason:return-does-not-clear-threshold')}] "
        f"work-interlock=[policy=[task-floor:{count('hydration-policy:task-floor')} "
        f"working-reserve:{count('hydration-policy:working-reserve')}] "
        f"opportunity-power:{count('opportunity-power:true')} "
        f"follow-up-needed:{count('reprovision:true:')} "
        f"single-provision-sufficient:{count('reprovision:false:')}/{len(survival)} "
        f"initial-drink:{scaled_span(initial_work_drinks, 1_000, 'mL')} "
        f"follow-up-drink:{scaled_span(follow_up_work_drinks, 1_000, 'mL')} "
        f"prospect:{_span(prospecting_ticks, 't')} power:{_span(power_ticks, 't')} "
        f"final-hydration:{_span(final_hydration_ppm, 'ppm')} "
        f"warning-safe:{count('warning-safe:true')}/{len(survival)}] "
        f"organic-preservation=[declined:{sum('storage-policy:decline' in line for line in organic_survival)} "
        f"efficient:{sum('storage-policy:attention-efficient' in line for line in organic_survival)} "
        f"singleton:{sum('storage-policy:enclosure-singleton' in line for line in organic_survival)} "
        f"frontier:{sum('storage-policy:balanced-frontier' in line for line in organic_survival)} "
        f"maximum:{sum('storage-policy:maximum-protection' in line for line in organic_survival)}]"
    )
