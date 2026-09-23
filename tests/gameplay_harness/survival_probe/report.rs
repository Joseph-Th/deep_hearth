//! Aggregated survival probe evaluation and replayable reporting.

use super::*;

fn evaluate_survival_provisioning_probe(registries: &Registries, case: FocusedProbeCase) {
    let seed = case.seed();
    let sample = focused_probe_role_label(case.role());
    let behavior_seed = case
        .behavior_seed()
        .unwrap_or_else(|| panic!("survival probe is missing its actor behavior seed"));
    let world = provisioning_world(registries, seed);
    let protected_food = world.foods[world.witness_index];
    let protected_reserve_mass = world.preserved_reserve_mass;
    let preservation_decision = evaluate_preservation_decision(
        registries,
        seed,
        behavior_seed,
        protected_food,
        protected_reserve_mass,
    );
    let preservation_raw_opportunity = preservation_decision.opportunity.available();
    let preservation_opportunity_label =
        preservation_storage_report_label(registries, preservation_decision.opportunity.origin());
    let preservation_opportunity_mode = preservation_decision.opportunity.mode_label();
    let inherited_preservation_label =
        preservation_storage_report_label(registries, world.inherited_preservation_definition);
    let preservation_raw_summary = preservation_raw_opportunity
        .iter()
        .map(|(commodity, mass)| {
            format!(
                "{}:{}mg",
                preservation_commodity_report_label(registries, *commodity),
                mass.milligrams()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let preservation_frontier_summary = preservation_decision
        .projections
        .iter()
        .map(|projection| {
            format!(
                "{}:{}t/{}mg/{}fresh:{}{}",
                preservation_storage_report_label(registries, projection.definition),
                projection.production_ticks,
                projection.raw_material_mass_mg,
                projection.remaining_fresh_ticks,
                if preservation_decision
                    .physical_frontier
                    .contains(&projection.definition)
                {
                    "P"
                } else {
                    "d"
                },
                if projection.raw_material_mass_mg <= preservation_decision.material_budget_mg {
                    "B"
                } else {
                    "x"
                },
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let attention_investment = preservation_decision.attention;
    let protection_investment = preservation_decision.protection;
    let preservation_infrastructure = preservation_decision.best_enclosure;
    let preservation_commitment = preservation_decision.investment;
    let preservation_no_build = preservation_decision.no_build;
    let protection_attention_delta_ticks = preservation_decision.protection_attention_delta_ticks;
    let protection_raw_delta_mg = preservation_decision.protection_raw_delta_mg;
    let protection_metabolic_delta_nj = preservation_decision.protection_metabolic_delta_nj;
    let protection_hydration_delta_ul = preservation_decision.protection_hydration_delta_ul;
    let protection_freshness_delta_ticks = preservation_decision.protection_freshness_delta_ticks;
    let protection_remaining_fresh_delta_ticks =
        preservation_decision.protection_remaining_fresh_delta_ticks;
    let preservation_return_ppm = preservation_decision.preservation_return_ppm;
    let preservation_attention_value_ppm = preservation_decision.preservation_attention_value_ppm;
    let preservation_minimum_return_ppm = preservation_decision.preservation_minimum_return_ppm;
    let preservation_material_budget_ppm = preservation_decision.material_budget_ppm;
    let preservation_material_budget_mg = preservation_decision.material_budget_mg;
    let preservation_material_budget_eligible_count =
        preservation_decision.material_budget_eligible_count;
    let committed_preservation_label = preservation_commitment
        .map(|definition| preservation_storage_report_label(registries, definition))
        .unwrap_or_else(|| "none".to_string());
    let preservation_commitment_state = if preservation_commitment.is_some() {
        "invested"
    } else {
        "declined"
    };
    let preservation_commitment_reason = if preservation_commitment.is_some() {
        "return-clears-threshold"
    } else {
        "return-does-not-clear-threshold"
    };
    let selected_on_physical_frontier = preservation_decision.selected_on_physical_frontier;
    let selected_within_material_budget = preservation_decision.selected_within_material_budget;
    let selected_preservation_label = preservation_storage_report_label(
        registries,
        preservation_infrastructure.storage_definition,
    );
    let fastest_preservation_label = preservation_storage_report_label(
        registries,
        preservation_infrastructure.fastest_definition,
    );
    let strongest_preservation_label = preservation_storage_report_label(
        registries,
        preservation_infrastructure.strongest_definition,
    );
    let protected_food_label =
        preservation_commodity_report_label(registries, preservation_infrastructure.food_commodity);
    let attention_investment_time =
        format_physical_duration(registries, attention_investment.production_ticks);
    let protection_investment_time =
        format_physical_duration(registries, protection_investment.production_ticks);
    let selected_investment_time =
        format_physical_duration(registries, preservation_infrastructure.production_ticks);
    let protection_attention_delta_time =
        format_physical_duration(registries, protection_attention_delta_ticks);
    let protection_freshness_delta_magnitude =
        u64::try_from(protection_freshness_delta_ticks.unsigned_abs())
            .unwrap_or_else(|_| panic!("bounded preservation freshness delta exceeds u64"));
    let protection_freshness_delta_time = format!(
        "{}{}",
        if protection_freshness_delta_ticks >= 0 {
            "+"
        } else {
            "-"
        },
        format_physical_duration(registries, protection_freshness_delta_magnitude)
    );
    let protection_remaining_delta_magnitude =
        u64::try_from(protection_remaining_fresh_delta_ticks.unsigned_abs())
            .unwrap_or_else(|_| panic!("bounded remaining freshness delta exceeds u64"));
    let protection_remaining_delta_time = format!(
        "{}{}",
        if protection_remaining_fresh_delta_ticks >= 0 {
            "+"
        } else {
            "-"
        },
        format_physical_duration(registries, protection_remaining_delta_magnitude)
    );
    evaluate_survival_pressure_response_probe(registries, seed);
    let work_pressure = evaluate_survival_work_pressure_probe(registries, seed);
    let integrated_work = evaluate_integrated_survival_work_loop(registries, seed, behavior_seed);
    let prospecting_pressure = normalized_deficit_priority(
        work_pressure.prospecting_energy_deficit_ppm,
        work_pressure.prospecting_hydration_deficit_ppm,
    );
    let manual_power_pressure = normalized_deficit_priority(
        work_pressure.manual_power_energy_deficit_ppm,
        work_pressure.manual_power_hydration_deficit_ppm,
    );
    let foods = world.foods.as_slice();
    let provisioning_wait_ticks = world.provisioning_wait_ticks;
    let diet_comparison = evaluate_provisioning_comparison(registries, seed, behavior_seed, &world);
    let compact = diet_comparison.compact;
    let balanced = diet_comparison.balanced;
    let natural_policy = diet_comparison.natural_policy;
    let natural = diet_comparison.natural;
    let available_category_count = diet_comparison.available_category_count;
    let comparison_horizon_ticks = diet_comparison.comparison_horizon_ticks;
    let meal_mass_delta_mg = diet_comparison.meal_mass_delta_mg;
    let water_saved_delta_ul = diet_comparison.water_saved_delta_ul;
    let diet_quality_delta_ppm = diet_comparison.diet_quality_delta_ppm;
    let recovery_rate_delta_ppm_per_tick = diet_comparison.recovery_rate_delta_ppm_per_tick;
    let reserve_recovered = diet_comparison.reserve_recovered;
    let diet_recovery = diet_comparison.recovery;
    let recovery_consequence = if diet_recovery.actionable {
        format!(
            "choice:actionable deprivation:{}t provisioning-horizon:{}t observe:{}t vitality:{}->[compact:{} balanced:{} delta:+{}ppm] diet:[compact:{} balanced:{}ppm]",
            diet_recovery.deprivation_ticks,
            diet_recovery.provisioning_horizon_ticks,
            diet_recovery.observation_ticks,
            diet_recovery.vitality_before_ppm,
            diet_recovery.compact_vitality_after_ppm,
            diet_recovery.balanced_vitality_after_ppm,
            diet_recovery.vitality_advantage_ppm,
            diet_recovery.compact_diet_quality_ppm,
            diet_recovery.balanced_diet_quality_ppm,
        )
    } else {
        "choice:supply-collapsed reason=available-categories-below-authored-diet-set".to_string()
    };
    let choice_state = if diet_comparison.policy_sensitive {
        "policy-sensitive"
    } else {
        "supply-constrained"
    };
    let food_options = food_option_summary(registries, foods);
    let preservation_choice = preservation_decision.comparison();
    let best_enclosure_policy =
        preservation_choice.selection_label(preservation_infrastructure.selection_kind.label());
    let preservation_selection_label = if preservation_commitment.is_some() {
        best_enclosure_policy
    } else {
        "decline"
    };
    let committed_build_ticks =
        preservation_commitment.map_or(0, |_| preservation_infrastructure.production_ticks);
    let committed_raw_mg =
        preservation_commitment.map_or(0, |_| preservation_infrastructure.raw_material_mass_mg);
    let preservation_comparison = preservation_comparison_explanation(preservation_choice, || {
        format!(
            "value=[stronger-return:{preservation_return_ppm}ppm attention-value:{preservation_attention_value_ppm}ppm minimum-build-return:{preservation_minimum_return_ppm}ppm material-budget:{preservation_material_budget_ppm}ppm/{preservation_material_budget_mg}mg] fastest:{fastest_preservation_label}:{}t/{attention_investment_time}:{}ppm strongest:{strongest_preservation_label}:{}t/{protection_investment_time}:{}ppm stronger-tradeoff=[attention:+{protection_attention_delta_ticks}t/+{protection_attention_delta_time} raw:{protection_raw_delta_mg}mg body:{protection_metabolic_delta_nj}nJ/{protection_hydration_delta_ul}uL matched-age:{protection_freshness_delta_ticks:+}t/{protection_freshness_delta_time} remaining-edible:{protection_remaining_fresh_delta_ticks:+}t/{protection_remaining_delta_time}]",
            preservation_infrastructure.fastest_ticks,
            preservation_infrastructure.fastest_preservation_multiplier_ppm,
            preservation_infrastructure.strongest_ticks,
            preservation_infrastructure.strongest_preservation_multiplier_ppm,
        )
    });
    let diet_consequence = diet_comparison_explanation(diet_comparison.policy_sensitive, || {
        format!(
            "diet-delta:{diet_quality_delta_ppm:+}ppm recovery-delta:{recovery_rate_delta_ppm_per_tick:+}ppm/t"
        )
    });
    let diet_counterfactual = diet_comparison_explanation(diet_comparison.policy_sensitive, || {
        format!(
            "matched-counterfactual=[horizon:{comparison_horizon_ticks}t compact-calories:[action:{}t selected:{} meal:{}mg drink:{}uL diet:{}->{}ppm recovery:{}->{}ppm/t] balanced:[action:{}t selected:{} meal:{}mg drink:{}uL diet:{}->{}ppm recovery:{}->{}ppm/t]] tradeoff=[meal-mass-delta:{meal_mass_delta_mg:+}mg water-saved-delta:{water_saved_delta_ul:+}uL diet-quality-delta:{diet_quality_delta_ppm:+}ppm recovery-delta:{recovery_rate_delta_ppm_per_tick:+}ppm/t] recovery-consequence=[{recovery_consequence}]",
            compact.provisioning_elapsed_ticks,
            compact.selected_category_count,
            compact.meal_mass_mg,
            compact.drink_volume_ul,
            compact.diet_quality_before_ppm,
            compact.diet_quality_after_ppm,
            compact.recovery_rate_before_ppm_per_tick,
            compact.recovery_rate_after_ppm_per_tick,
            balanced.provisioning_elapsed_ticks,
            balanced.selected_category_count,
            balanced.meal_mass_mg,
            balanced.drink_volume_ul,
            balanced.diet_quality_before_ppm,
            balanced.diet_quality_after_ppm,
            balanced.recovery_rate_before_ppm_per_tick,
            balanced.recovery_rate_after_ppm_per_tick,
        )
    });
    reviewln!(
        "SURVIVAL EXPERIENCE seed=0x{seed:016X} sample={sample} start={} supply=[foods:{} categories:{}] pressure={} choice=[state:{choice_state} diet:{} meal:{}mg drink:{}uL] inherited-reserve=[storage:{inherited_preservation_label} preservation:{}ppm rotation:consume-ambient-first retained:{}mg age-saved:{}t] separate-investment-scenario=[protected-reserve:{}mg raw-opportunity=[origin:{preservation_opportunity_label} mode:{preservation_opportunity_mode} inputs:{preservation_raw_summary}] storage-policy:{} commitment:{committed_preservation_label} state:{preservation_commitment_state} commitment-reason:{preservation_commitment_reason} minimum-return:{preservation_minimum_return_ppm}ppm committed=[build:{}t raw:{}mg service:0t] no-build-baseline=[{}t fresh:{}t retained-raw:{}mg] best-enclosure-counterfactual=[policy:{best_enclosure_policy} storage:{selected_preservation_label} preservation:{}ppm candidates:{} frontier=[physical:{}/{} budget-eligible:{}/{} selected-physical:{} selected-budget:{}] {preservation_comparison} build:{}t/{} raw:{}mg embodied:{}mg capacity:{}mg utilization:{}ppm dismantle=[{}t body:{}nJ/{}uL returned:{}mg]] consequence=[reserve-improved:{} {diet_consequence} horizon:{}t] lived-wait=[drinks:{} volume:{}uL] work-interlock=[prospecting:{}t cost:{}ppmE/{}ppmH dominant:{} manual-power:{}t cost:{}ppmE/{}ppmH dominant:{} integrated=[hydration-policy:{} drink:{}uL/{}t prospect:{}t opportunity-power:{} reprovision:{}:{}uL/{}t power:{}t stored:{}nJ final-reserve:{}ppmE/{}ppmH warning-safe:{}]]",
        world.start_profile.label(),
        foods.len(),
        available_category_count,
        compact.provisioning_priority.label(),
        natural_policy.label(),
        natural.meal_mass_mg,
        natural.drink_volume_ul,
        world.inherited_preservation_multiplier_ppm,
        compact.retained_preserved_mass_mg,
        compact.preservation_age_saved_ticks,
        protected_reserve_mass.milligrams(),
        preservation_selection_label,
        committed_build_ticks,
        committed_raw_mg,
        preservation_no_build.elapsed_ticks,
        preservation_no_build.remaining_fresh_ticks,
        preservation_no_build.retained_raw_mg,
        preservation_infrastructure.preservation_multiplier_ppm,
        preservation_infrastructure.candidate_count,
        preservation_decision.physical_frontier.len(),
        preservation_decision.projections.len(),
        preservation_material_budget_eligible_count,
        preservation_decision.projections.len(),
        selected_on_physical_frontier,
        selected_within_material_budget,
        preservation_infrastructure.production_ticks,
        selected_investment_time,
        preservation_infrastructure.raw_material_mass_mg,
        preservation_infrastructure.embodied_mass_mg,
        preservation_infrastructure.capacity_mass_mg,
        preservation_decision.capacity_utilization_ppm,
        preservation_infrastructure.dismantle_ticks,
        preservation_infrastructure.dismantle_metabolic_cost_nj,
        preservation_infrastructure.dismantle_hydration_cost_ul,
        preservation_infrastructure.recovered_enclosure_mass_mg,
        reserve_recovered,
        comparison_horizon_ticks,
        diet_comparison.midwait_drink_count,
        diet_comparison.midwait_drink_volume_ul,
        work_pressure.prospecting_ticks,
        work_pressure.prospecting_energy_deficit_ppm,
        work_pressure.prospecting_hydration_deficit_ppm,
        prospecting_pressure.label(),
        work_pressure.manual_power_ticks,
        work_pressure.manual_power_energy_deficit_ppm,
        work_pressure.manual_power_hydration_deficit_ppm,
        manual_power_pressure.label(),
        integrated_work.hydration_policy.label(),
        integrated_work.initial_drink_volume_ul,
        integrated_work.initial_drink_ticks,
        integrated_work.prospecting_ticks,
        integrated_work.power_triggered_by_observation,
        integrated_work.reprovisioned_after_prospecting,
        integrated_work.reprovision_volume_ul,
        integrated_work.reprovision_ticks,
        integrated_work.manual_power_ticks,
        integrated_work.stored_work_nj,
        1_000_000 - integrated_work.energy_deficit_ppm,
        1_000_000 - integrated_work.hydration_deficit_ppm,
        integrated_work.hydration_warning_safe,
    );
    reviewln!(
        "SURVIVAL REVIEW seed=0x{seed:016X} behavior=0x{behavior_seed:016X} sample={sample} role=runtime-experience-after-disclosed-bootstrap fantasy=prepare+provision episode=[start:{} wait:{provisioning_wait_ticks}t lived-wait=[drinks:{} volume:{}uL] available:[foods:{} categories:{} options:{food_options}]] separate-investment-evidence=[policy:{} candidates:{} raw-opportunity=[origin:{preservation_opportunity_label} mode:{preservation_opportunity_mode} inputs:{preservation_raw_summary}] frontier=[{preservation_frontier_summary}] legend=P:physical-frontier,d:dominated,B:within-material-budget,x:over-material-budget commitment:{} committed=[build:{}t raw:{}mg] no-build-baseline=[{}t fresh:{}t retained-raw:{}mg] best-enclosure-counterfactual=[policy:{best_enclosure_policy} storage:{selected_preservation_label} physical:{} within-material-budget:{}]] best-enclosure-execution-and-dismantling-coverage=[food:{protected_food_label} stages:{} route=finite-disclosed-raw-opportunity->manual-production-forest->enclosure production:{}t observation:{}t raw:{}mg embodied:{}mg residual:{}mg capacity:{}mg multiplier:{}ppm witness=[bootstrap-age:{}t ambient:{}:{}t enclosed:{}:{}t remaining:{}t saved:{}t] survival-cost:{}nJ+{}uL dismantle=[{}t body:{}nJ/{}uL returned:{}mg]] activity-pressure=[prospecting:[method:{} region:{}vox {}t] energy:{}ppm hydration:{}ppm dominant:{}; manual-power:{}t energy:{}ppm hydration:{}ppm dominant:{} stored-work:{}nJ; contrast:{}] integrated-work-loop=[start:hydration-warning policy:{} provision:{}t prospect:{}t opportunity-power:{} reprovision:{}:{}t generate:{}t stored:{}nJ final-reserve:{}ppmE/{}ppmH warning-safe:{}] actor-choice=[diet-policy:{} selected:{} meal:{}mg drink:{}uL] diet-evidence=[{diet_counterfactual}] decision-pressure=[energy:{}ppm hydration:{}ppm dominant:{}] inherited-preservation=[definition:{inherited_preservation_label} age-saved:{}t retained:{}mg] reserve-recovered:{}",
        world.start_profile.label(),
        diet_comparison.midwait_drink_count,
        diet_comparison.midwait_drink_volume_ul,
        foods.len(),
        available_category_count,
        preservation_selection_label,
        preservation_infrastructure.candidate_count,
        committed_preservation_label,
        committed_build_ticks,
        committed_raw_mg,
        preservation_no_build.elapsed_ticks,
        preservation_no_build.remaining_fresh_ticks,
        preservation_no_build.retained_raw_mg,
        selected_on_physical_frontier,
        selected_within_material_budget,
        preservation_infrastructure.construction_stages,
        preservation_infrastructure.production_ticks,
        preservation_infrastructure.observation_ticks,
        preservation_infrastructure.raw_material_mass_mg,
        preservation_infrastructure.embodied_mass_mg,
        preservation_infrastructure.residual_mass_mg,
        preservation_infrastructure.capacity_mass_mg,
        preservation_infrastructure.preservation_multiplier_ppm,
        preservation_infrastructure.bootstrap_age_ticks,
        if preservation_infrastructure.ambient_spoiled {
            "spoiled"
        } else {
            "fresh"
        },
        preservation_infrastructure.ambient_age_after_ticks,
        if preservation_infrastructure.enclosed_fresh {
            "fresh"
        } else {
            "spoiled"
        },
        preservation_infrastructure.enclosed_age_after_ticks,
        preservation_infrastructure.enclosed_remaining_fresh_ticks,
        preservation_infrastructure.age_saved_ticks,
        preservation_infrastructure.metabolic_cost_nj,
        preservation_infrastructure.hydration_cost_ul,
        preservation_infrastructure.dismantle_ticks,
        preservation_infrastructure.dismantle_metabolic_cost_nj,
        preservation_infrastructure.dismantle_hydration_cost_ul,
        preservation_infrastructure.recovered_enclosure_mass_mg,
        work_pressure.prospecting_method.value(),
        work_pressure.prospecting_region_voxels,
        work_pressure.prospecting_ticks,
        work_pressure.prospecting_energy_deficit_ppm,
        work_pressure.prospecting_hydration_deficit_ppm,
        prospecting_pressure.label(),
        work_pressure.manual_power_ticks,
        work_pressure.manual_power_energy_deficit_ppm,
        work_pressure.manual_power_hydration_deficit_ppm,
        manual_power_pressure.label(),
        work_pressure.stored_work_nj,
        prospecting_pressure != manual_power_pressure,
        integrated_work.hydration_policy.label(),
        integrated_work.initial_drink_ticks,
        integrated_work.prospecting_ticks,
        integrated_work.power_triggered_by_observation,
        integrated_work.reprovisioned_after_prospecting,
        integrated_work.reprovision_ticks,
        integrated_work.manual_power_ticks,
        integrated_work.stored_work_nj,
        1_000_000 - integrated_work.energy_deficit_ppm,
        1_000_000 - integrated_work.hydration_deficit_ppm,
        integrated_work.hydration_warning_safe,
        natural_policy.label(),
        natural.selected_category_count,
        natural.meal_mass_mg,
        natural.drink_volume_ul,
        compact.energy_deficit_ppm,
        compact.hydration_deficit_ppm,
        compact.provisioning_priority.label(),
        compact.preservation_age_saved_ticks,
        compact.retained_preserved_mass_mg,
        reserve_recovered,
    );
}

pub(in super::super) fn run_survival_provisioning_probe(
    registries: &Registries,
    case: FocusedProbeCase,
) {
    evaluate_survival_provisioning_probe(registries, case);
}
