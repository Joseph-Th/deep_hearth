//! Workshop scenario and aggregate evidence rendering for exploratory gameplay reports.

use std::collections::BTreeSet;

use deep_hearth::maintenance::MaintenanceBand;

use super::{
    EnergyRecoveryPreference, MaintenancePreference, PowerPreference, ScenarioReport,
    StructuralPreference,
};

fn scenario_outcome(report: &ScenarioReport) -> &'static str {
    if report.structure.structural_stop {
        "structural-stop"
    } else if report.limits.maintenance_stop {
        "maintenance-stop"
    } else if report.limits.energy_stop {
        "energy-stop"
    } else if report.progress.processed_mass == report.progress.target_mass {
        "complete"
    } else {
        "partial"
    }
}

fn scenario_constraint(report: &ScenarioReport) -> &'static str {
    if report.structure.structural_stop {
        "structural-capacity"
    } else if report.limits.maintenance_stop {
        if report.resources.maintenance_stock_remaining.is_zero() {
            "maintenance-supply-exhausted"
        } else {
            "maintenance-safety"
        }
    } else if report.limits.energy_stop {
        if report.limits.manual_recovery_declined {
            "manual-recovery-declined"
        } else if report.limits.manual_recovery_survival_limited {
            "manual-recovery-survival-limited"
        } else {
            "stored-work-insufficient"
        }
    } else {
        "none"
    }
}

fn print_capability_highlight(kind: &str, report: &ScenarioReport) {
    std::println!(
        "CAPABILITY HIGHLIGHT kind={kind} world=0x{:016X} behavior=0x{:016X} pressure=[crusher:{:?} maintenance-units:{} stored-work:small:{}+{}ppm large:{}+{}ppm] policy=[power:{} recovery:{} maintenance:{} structure:{}] decisions=[power-batches:small:{} large:{} basis:policy:{} single-source:{} adaptive:condition:{} stored-work:{} manual-recharges:{} services:{} compact-support:{}] consequence=[processed:{}/{}mg operations:{} bottlenecks:energy:{} throughput:{} balanced:{} relocation:{} suspension:{} stranded:{} elapsed:{}t outcome:{} constraint:{}]",
        report.world_seed,
        report.behavior_seed,
        report.inputs.initial_maintenance_band,
        report.inputs.maintenance_replacement_units,
        report.inputs.small_drive_batch_budget,
        report.inputs.small_drive_partial_batch_ppm,
        report.inputs.large_drive_batch_budget,
        report.inputs.large_drive_partial_batch_ppm,
        report.policy.power_preference.label(),
        report.policy.energy_recovery_preference.label(),
        report.policy.maintenance_preference.label(),
        report.policy.structural_preference.label(),
        report.choices.small_drive_batches,
        report.choices.large_drive_batches,
        report.choices.policy_power_choices,
        report.choices.single_source_power_choices,
        report.progress.condition_adaptive_batch_operations,
        report.progress.energy_adaptive_batch_operations,
        report.choices.manual_recharges,
        report.maintenance.services,
        report.choices.chose_compact_support,
        report.progress.processed_mass.milligrams(),
        report.progress.target_mass.milligrams(),
        report.progress.operations_completed,
        report.limits.energy_bottleneck_batches,
        report.limits.throughput_bottleneck_batches,
        report.limits.balanced_bottleneck_batches,
        report.structure.support_relocation,
        report.structure.production_suspension,
        report.structure.stranded_work_in_process,
        report.resources.elapsed_ticks,
        scenario_outcome(report),
        scenario_constraint(report),
    );
}

fn print_summary_highlights(reports: &[ScenarioReport]) {
    let mut highlighted = BTreeSet::new();
    if let Some(report) = reports.iter().find(|report| {
        report.structure.support_relocation || report.structure.production_suspension
    }) {
        highlighted.insert((report.world_seed, report.behavior_seed));
        print_capability_highlight("world-disruption-recovery", report);
    }
    if let Some(report) = reports
        .iter()
        .filter(|report| {
            report.choices.manual_recharges > 0
                && !highlighted.contains(&(report.world_seed, report.behavior_seed))
        })
        .max_by_key(|report| report.choices.manual_recharges)
    {
        highlighted.insert((report.world_seed, report.behavior_seed));
        print_capability_highlight("manual-energy-recovery", report);
    }
    if let Some(report) = reports
        .iter()
        .filter(|report| {
            report.progress.processed_mass < report.progress.target_mass
                && !highlighted.contains(&(report.world_seed, report.behavior_seed))
        })
        .min_by_key(|report| report.progress.processed_mass.milligrams())
    {
        print_capability_highlight("terminal-constraint", report);
    }
}

fn print_scenario_experiences(reports: &[ScenarioReport]) {
    for report in reports {
        let outcome = scenario_outcome(report);
        let survival_start = if report.inputs.start_at_hydration_warning_boundary {
            "hydration-warning-boundary"
        } else {
            "full-reserve"
        };
        let event_progress = if report.progress.delivery_applied {
            format!(
                "scheduled:{}t reached:true operations-before:{}",
                report.inputs.delivery_at_tick, report.progress.operations_before_delivery
            )
        } else {
            format!(
                "scheduled:{}t reached:false operations-before:n/a",
                report.inputs.delivery_at_tick
            )
        };
        std::println!(
            "CAPABILITY EXPERIENCE world=0x{:016X} behavior=0x{:016X} start=[crusher:{:?} survival:{}] policy=[power:{} recovery:{} maintenance:{} structure:{}] initial=[small:{}+{}ppm nominal-batches large:{}+{}ppm nominal-batches maintenance:{}unit] order=[processed:{}/{}mg operations:{} adaptive:[total:{} condition:{} stored-work:{}] event:[{}]] power=[small:{} large:{} manual-recharges:{} generated:{}nJ manual-ticks:{}] maintenance=[services:{} warning-deferred:{} critical-required:{}] relocation={} suspension={} stranded={} final=[crusher:{}ppm crank:{}ppm small:{}nJ large:{}nJ maintenance:{}mg ticks:{} survival-spent=[energy:{}nJ hydration:{}uL] vitality:{}ppm] outcome={}",
            report.world_seed,
            report.behavior_seed,
            report.inputs.initial_maintenance_band,
            survival_start,
            report.policy.power_preference.label(),
            report.policy.energy_recovery_preference.label(),
            report.policy.maintenance_preference.label(),
            report.policy.structural_preference.label(),
            report.inputs.small_drive_batch_budget,
            report.inputs.small_drive_partial_batch_ppm,
            report.inputs.large_drive_batch_budget,
            report.inputs.large_drive_partial_batch_ppm,
            report.inputs.maintenance_replacement_units,
            report.progress.processed_mass.milligrams(),
            report.progress.target_mass.milligrams(),
            report.progress.operations_completed,
            report.progress.adaptive_batch_operations,
            report.progress.condition_adaptive_batch_operations,
            report.progress.energy_adaptive_batch_operations,
            event_progress,
            report.choices.small_drive_batches,
            report.choices.large_drive_batches,
            report.choices.manual_recharges,
            report.resources.manually_generated_energy.nanojoules(),
            report.resources.manual_power_ticks,
            report.maintenance.services,
            report.maintenance.warning_deferrals,
            report.maintenance.critical_services,
            report.structure.support_relocation,
            report.structure.production_suspension,
            report.structure.stranded_work_in_process,
            report.resources.final_condition_ppm,
            report.resources.final_hand_crank_condition_ppm,
            report.resources.small_drive_remaining.nanojoules(),
            report.resources.large_drive_remaining.nanojoules(),
            report.resources.maintenance_stock_remaining.milligrams(),
            report.resources.elapsed_ticks,
            report.resources.metabolic_energy_spent.nanojoules(),
            report.resources.hydration_spent.microliters(),
            report.resources.final_vitality_ppm,
            outcome,
        );
    }
}

#[derive(Clone, Copy)]
struct HarnessObservationStats {
    controlled_deliveries: usize,
    adaptive_operations: u32,
    manual_recharges: u32,
    relocations: usize,
    suspensions: usize,
    recovered_work_in_process: usize,
    stranded_work_in_process: usize,
    structural_consequences: usize,
    stored_work_pressure: usize,
    body_power_pressure: usize,
    wear_maintenance_pressure: usize,
    structure_production_pressure: usize,
    multi_system_adaptation: usize,
    clean_baselines: usize,
    single_pressure_scenarios: usize,
    maintenance_terminal: usize,
    energy_terminal: usize,
    structural_terminal: usize,
    prework_terminal: usize,
}

fn scenario_pressure_dimensions(report: &ScenarioReport) -> u8 {
    u8::from(
        report.progress.energy_adaptive_batch_operations > 0
            || report.limits.energy_bottleneck_batches > 0
            || report.limits.energy_stop,
    ) + u8::from(
        report.choices.manual_recharges > 0
            || report.limits.manual_recovery_declined
            || report.limits.manual_recovery_survival_limited,
    ) + u8::from(report.limits.maintenance_warning || report.maintenance.services > 0)
        + u8::from(
            report.structure.structural_consequence
                || report.structure.support_relocation
                || report.structure.production_suspension,
        )
}

fn harness_observation_stats(reports: &[ScenarioReport]) -> HarnessObservationStats {
    HarnessObservationStats {
        controlled_deliveries: reports
            .iter()
            .filter(|report| report.progress.delivery_applied)
            .count(),
        adaptive_operations: reports
            .iter()
            .map(|report| u32::from(report.progress.adaptive_batch_operations))
            .sum(),
        manual_recharges: reports
            .iter()
            .map(|report| u32::from(report.choices.manual_recharges))
            .sum(),
        relocations: reports
            .iter()
            .filter(|report| report.structure.support_relocation)
            .count(),
        suspensions: reports
            .iter()
            .filter(|report| report.structure.production_suspension)
            .count(),
        recovered_work_in_process: reports
            .iter()
            .filter(|report| {
                report.structure.production_suspension && !report.structure.stranded_work_in_process
            })
            .count(),
        stranded_work_in_process: reports
            .iter()
            .filter(|report| report.structure.stranded_work_in_process)
            .count(),
        structural_consequences: reports
            .iter()
            .filter(|report| report.structure.structural_consequence)
            .count(),
        stored_work_pressure: reports
            .iter()
            .filter(|report| {
                report.progress.energy_adaptive_batch_operations > 0
                    || report.limits.energy_bottleneck_batches > 0
                    || report.limits.energy_stop
            })
            .count(),
        body_power_pressure: reports
            .iter()
            .filter(|report| {
                report.choices.manual_recharges > 0
                    || report.limits.manual_recovery_declined
                    || report.limits.manual_recovery_survival_limited
            })
            .count(),
        wear_maintenance_pressure: reports
            .iter()
            .filter(|report| report.limits.maintenance_warning || report.maintenance.services > 0)
            .count(),
        structure_production_pressure: reports
            .iter()
            .filter(|report| {
                report.structure.structural_consequence
                    || report.structure.support_relocation
                    || report.structure.production_suspension
            })
            .count(),
        multi_system_adaptation: reports
            .iter()
            .filter(|report| scenario_pressure_dimensions(report) >= 2)
            .count(),
        clean_baselines: reports
            .iter()
            .filter(|report| scenario_pressure_dimensions(report) == 0)
            .count(),
        single_pressure_scenarios: reports
            .iter()
            .filter(|report| scenario_pressure_dimensions(report) == 1)
            .count(),
        maintenance_terminal: reports
            .iter()
            .filter(|report| report.limits.maintenance_stop)
            .count(),
        energy_terminal: reports
            .iter()
            .filter(|report| report.limits.energy_stop)
            .count(),
        structural_terminal: reports
            .iter()
            .filter(|report| report.structure.structural_stop)
            .count(),
        prework_terminal: reports
            .iter()
            .filter(|report| {
                report.progress.operations_completed == 0
                    && (report.limits.maintenance_stop
                        || report.limits.energy_stop
                        || report.structure.structural_stop)
            })
            .count(),
    }
}

fn print_capability_scope(stats: HarnessObservationStats) {
    let observations = [
        ("structural-consequence", stats.structural_consequences > 0),
        ("support-relocation", stats.relocations > 0),
        ("production-suspension", stats.suspensions > 0),
        ("wip-recovery", stats.recovered_work_in_process > 0),
        ("stranded-wip", stats.stranded_work_in_process > 0),
        ("manual-energy-recovery", stats.manual_recharges > 0),
        ("adaptive-batching", stats.adaptive_operations > 0),
        (
            "controlled-supported-stockpile-delivery",
            stats.controlled_deliveries > 0,
        ),
    ];
    let observed = observations
        .iter()
        .filter_map(|(name, observed)| observed.then_some(*name))
        .collect::<Vec<_>>()
        .join(",");
    let unobserved = observations
        .iter()
        .filter_map(|(name, observed)| (!observed).then_some(*name))
        .collect::<Vec<_>>()
        .join(",");
    std::println!(
        "CAPABILITY SCOPE evidence=bootstrapped-industrial surface=[canonical-industrial-comminution,adaptive-batching,manual-energy-recovery,power-choice,wear,maintenance,structural-siting,controlled-supported-stockpile-delivery] observed=[{observed}] unobserved=[{unobserved}] outside-this-workshop-test=[survival,primitive-progression,industrial-ore-preparation,pure-copper-foundry] bootstrap=[industrial-workshop-equipment,industrial-energy-stores,constructed-bays,starting-workshop-matter,preauthorized-controlled-delivery] actor-oracle=none fixture-guard=fail-if-injected-machine-becomes-runtime-acquirable global-runtime-scope=STATUS.md"
    );
}

fn print_experience_review(stats: HarnessObservationStats, scenario_count: usize) {
    std::println!(
        "WORKSHOP EXPERIENCE REVIEW fantasy=operate+adapt-physical-infrastructure sample=pressure-rich+hidden-controlled-delivery reached-events:{}/{} loop=observe-pressure->choose-power/batch/service/site->run->recover pressure-shape=[clean:{} single:{} multi-system:{}] interlocks=[stored-work+throughput:{} body+power:{} wear+maintenance:{} structure+production:{}] terminal=[maintenance:{} energy:{} structural:{} before-first-operation:{}] recovery=[suspensions:{} resumed:{} stranded:{}] agency=matched-policy-counterfactuals-in-AGENCY-SUMMARY dormant=[ore-grade:composition-only-in-this-workshop-scenario;concentration-is-exercised-by-the-separate-ore-probe]",
        stats.controlled_deliveries,
        scenario_count,
        stats.clean_baselines,
        stats.single_pressure_scenarios,
        stats.multi_system_adaptation,
        stats.stored_work_pressure,
        stats.body_power_pressure,
        stats.wear_maintenance_pressure,
        stats.structure_production_pressure,
        stats.maintenance_terminal,
        stats.energy_terminal,
        stats.structural_terminal,
        stats.prework_terminal,
        stats.suspensions,
        stats.recovered_work_in_process,
        stats.stranded_work_in_process,
    );
}

pub(crate) fn print_harness_summary(
    mode: &str,
    reports: &[ScenarioReport],
    include_scenarios: bool,
) {
    assert!(
        !reports.is_empty(),
        "gameplay harness produced no scenario reports"
    );

    let processed_mass_mg: u128 = reports
        .iter()
        .map(|report| u128::from(report.progress.processed_mass.milligrams()))
        .sum();
    let target_mass_mg: u128 = reports
        .iter()
        .map(|report| u128::from(report.progress.target_mass.milligrams()))
        .sum();
    let completed_operations: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.operations_completed))
        .sum();
    let operations_before_delivery: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.operations_before_delivery))
        .sum();
    let controlled_deliveries = reports
        .iter()
        .filter(|report| report.progress.delivery_applied)
        .count();
    let adaptive_operations: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.adaptive_batch_operations))
        .sum();
    let condition_adaptive_operations: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.condition_adaptive_batch_operations))
        .sum();
    let energy_adaptive_operations: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.energy_adaptive_batch_operations))
        .sum();
    let completed_orders = reports
        .iter()
        .filter(|report| report.progress.processed_mass == report.progress.target_mass)
        .count();
    let productive_orders = reports
        .iter()
        .filter(|report| !report.progress.processed_mass.is_zero())
        .count();
    let partial_orders = reports
        .iter()
        .filter(|report| {
            !report.progress.processed_mass.is_zero()
                && report.progress.processed_mass < report.progress.target_mass
        })
        .count();
    let recovered_work_in_process = reports
        .iter()
        .filter(|report| {
            report.structure.production_suspension && !report.structure.stranded_work_in_process
        })
        .count();
    let maintenance_services: u32 = reports
        .iter()
        .map(|report| u32::from(report.maintenance.services))
        .sum();
    let warning_deferrals: u32 = reports
        .iter()
        .map(|report| u32::from(report.maintenance.warning_deferrals))
        .sum();
    let critical_services: u32 = reports
        .iter()
        .map(|report| u32::from(report.maintenance.critical_services))
        .sum();

    let mixed_ore_melt_rejections = reports
        .iter()
        .filter(|report| report.progress.ore_frontier_visible)
        .count();
    let ore_grade_min = reports
        .iter()
        .map(|report| report.inputs.ore_copper_ppm)
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have an ore-grade minimum"));
    let ore_grade_max = reports
        .iter()
        .map(|report| report.inputs.ore_copper_ppm)
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have an ore-grade maximum"));
    let clay_share_min = reports
        .iter()
        .map(|report| report.inputs.gangue_clay_share_ppm)
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have a gangue-clay minimum"));
    let clay_share_max = reports
        .iter()
        .map(|report| report.inputs.gangue_clay_share_ppm)
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have a gangue-clay maximum"));
    let batch_mass_min = reports
        .iter()
        .map(|report| report.inputs.nominal_batch_mass.milligrams())
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have a batch-mass minimum"));
    let batch_mass_max = reports
        .iter()
        .map(|report| report.inputs.nominal_batch_mass.milligrams())
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have a batch-mass maximum"));
    let order_mass_min = reports
        .iter()
        .map(|report| report.inputs.order_mass.milligrams())
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have an order-mass minimum"));
    let order_mass_max = reports
        .iter()
        .map(|report| report.inputs.order_mass.milligrams())
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have an order-mass maximum"));
    let initial_condition_min = reports
        .iter()
        .map(|report| report.inputs.initial_condition_ppm)
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have an initial-condition minimum"));
    let initial_condition_max = reports
        .iter()
        .map(|report| report.inputs.initial_condition_ppm)
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have an initial-condition maximum"));
    let delivery_mass_min = reports
        .iter()
        .map(|report| report.inputs.delivery_mass.milligrams())
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have a delivery-mass minimum"));
    let delivery_mass_max = reports
        .iter()
        .map(|report| report.inputs.delivery_mass.milligrams())
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have a delivery-mass maximum"));
    let delivery_tick_min = reports
        .iter()
        .map(|report| report.inputs.delivery_at_tick)
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have a delivery-tick minimum"));
    let delivery_tick_max = reports
        .iter()
        .map(|report| report.inputs.delivery_at_tick)
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have a delivery-tick maximum"));
    let compact_deliveries = reports
        .iter()
        .filter(|report| report.inputs.delivery_is_compact)
        .count();
    let initial_normal = reports
        .iter()
        .filter(|report| report.inputs.initial_maintenance_band == MaintenanceBand::Normal)
        .count();
    let initial_warning = reports
        .iter()
        .filter(|report| report.inputs.initial_maintenance_band == MaintenanceBand::Warning)
        .count();
    let initial_critical = reports
        .iter()
        .filter(|report| report.inputs.initial_maintenance_band == MaintenanceBand::Critical)
        .count();
    let survival_warning_boundary_starts = reports
        .iter()
        .filter(|report| report.inputs.start_at_hydration_warning_boundary)
        .count();
    let small_drive_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.choices.small_drive_batches))
        .sum();
    let large_drive_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.choices.large_drive_batches))
        .sum();
    let energy_bottleneck_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.limits.energy_bottleneck_batches))
        .sum();
    let throughput_bottleneck_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.limits.throughput_bottleneck_batches))
        .sum();
    let balanced_bottleneck_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.limits.balanced_bottleneck_batches))
        .sum();
    let policy_power_choices: u32 = reports
        .iter()
        .map(|report| u32::from(report.choices.policy_power_choices))
        .sum();
    let single_source_power_choices: u32 = reports
        .iter()
        .map(|report| u32::from(report.choices.single_source_power_choices))
        .sum();
    let manual_recharges: u32 = reports
        .iter()
        .map(|report| u32::from(report.choices.manual_recharges))
        .sum();
    let manually_generated_energy: u128 = reports
        .iter()
        .map(|report| report.resources.manually_generated_energy.nanojoules())
        .sum();
    let manual_power_ticks: u128 = reports
        .iter()
        .map(|report| u128::from(report.resources.manual_power_ticks))
        .sum();
    let manual_metabolic_energy: u128 = reports
        .iter()
        .map(|report| report.resources.manual_power_metabolic_energy.nanojoules())
        .sum();
    let manual_hydration: u128 = reports
        .iter()
        .map(|report| u128::from(report.resources.manual_power_hydration.microliters()))
        .sum();
    let elapsed_ticks_min = reports
        .iter()
        .map(|report| report.resources.elapsed_ticks)
        .min()
        .unwrap_or_else(|| unreachable!("nonempty reports have an elapsed-tick minimum"));
    let elapsed_ticks_max = reports
        .iter()
        .map(|report| report.resources.elapsed_ticks)
        .max()
        .unwrap_or_else(|| unreachable!("nonempty reports have an elapsed-tick maximum"));

    if include_scenarios {
        print_scenario_experiences(reports);
    } else {
        print_summary_highlights(reports);
    }

    std::println!(
        "SAMPLE ore=[grade:{ore_grade_min}..{ore_grade_max}ppm gangue-clay-share:{clay_share_min}..{clay_share_max}ppm nominal-batch:{batch_mass_min}..{batch_mass_max}mg order:{order_mass_min}..{order_mass_max}mg] crusher-condition=[{initial_condition_min}..{initial_condition_max}ppm normal:{initial_normal} warning:{initial_warning} critical:{initial_critical}] survival-start=[hydration-warning-boundary:{survival_warning_boundary_starts} full-reserve:{}] resources=[small-drive:{}..{}+{}..{}ppm nominal-batches large-drive:{}..{}+{}..{}ppm maintenance-units:{}..{}] scheduled-event=[tick:{delivery_tick_min}..{delivery_tick_max}t mass:{delivery_mass_min}..{delivery_mass_max}mg compact:{compact_deliveries} reinforced:{} actor-visibility:hidden]",
        reports.len() - survival_warning_boundary_starts,
        reports
            .iter()
            .map(|report| report.inputs.small_drive_batch_budget)
            .min()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.small_drive_batch_budget)
            .max()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.small_drive_partial_batch_ppm)
            .min()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.small_drive_partial_batch_ppm)
            .max()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.large_drive_batch_budget)
            .min()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.large_drive_batch_budget)
            .max()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.large_drive_partial_batch_ppm)
            .min()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.large_drive_partial_batch_ppm)
            .max()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.maintenance_replacement_units)
            .min()
            .unwrap_or(0),
        reports
            .iter()
            .map(|report| report.inputs.maintenance_replacement_units)
            .max()
            .unwrap_or(0),
        reports.len() - compact_deliveries,
    );
    std::println!(
        "WORKSHOP CAPABILITY mode={mode} scenarios={} orders=[complete:{completed_orders} partial:{partial_orders} productive:{productive_orders}/{}] ore={processed_mass_mg}/{target_mass_mg}mg operations={completed_operations} adaptive=[total:{adaptive_operations} condition:{condition_adaptive_operations} stored-work:{energy_adaptive_operations}] events=[reached:{controlled_deliveries}/{} operations-before-reached:{operations_before_delivery}] stops=[structural:{} maintenance-required:{} energy:{} declined-manual:{} survival-limited-manual:{}] maintenance-blockers=[replacement-supply:{} service-labor:{}] manual-recovery=[charges:{manual_recharges} generated:{manually_generated_energy}nJ ticks:{manual_power_ticks} metabolic:{manual_metabolic_energy}nJ hydration:{manual_hydration}uL] material=[mixed-ore-melt-rejected:{mixed_ore_melt_rejections}/{}]",
        reports.len(),
        reports.len(),
        reports.len(),
        reports
            .iter()
            .filter(|report| report.structure.structural_stop)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.maintenance_stop)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.energy_stop)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.manual_recovery_declined)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.manual_recovery_survival_limited)
            .count(),
        reports
            .iter()
            .filter(|report| {
                report.limits.maintenance_stop && report.maintenance.supply_exhausted
            })
            .count(),
        reports
            .iter()
            .filter(|report| {
                report.limits.maintenance_stop && report.maintenance.labor_unavailable
            })
            .count(),
        reports.len(),
    );
    let observation_stats = harness_observation_stats(reports);
    std::println!(
        "CAPABILITY SYSTEMS policy=[power:reserve:{} speed:{} recovery:protect:{} spend:{} maintenance:warning-demand-aware:{} critical-immediate:{} structure:margin:{} failure-only:{}] machine-work=[small-drive:{small_drive_batches} high-power:{large_drive_batches}] decisions=[power-policy:{policy_power_choices} single-source:{single_source_power_choices} manual-recharges:{manual_recharges} adaptive:[total:{adaptive_operations} condition:{condition_adaptive_operations} stored-work:{energy_adaptive_operations}]] survival=[elapsed:{elapsed_ticks_min}..{elapsed_ticks_max}t manual-metabolic:{manual_metabolic_energy}nJ manual-hydration:{manual_hydration}uL] recovery=[relocations:{} resumed-wip:{recovered_work_in_process} stranded-wip:{} maintenance-services:{maintenance_services} warning-deferred:{warning_deferrals} critical-required:{critical_services}] pressure=[structural:{} maintenance-warning:{}] bottlenecks=[energy-delivery:{energy_bottleneck_batches} throughput:{throughput_bottleneck_batches} balanced:{balanced_bottleneck_batches}]",
        reports
            .iter()
            .filter(|report| report.policy.power_preference == PowerPreference::PreserveReserve)
            .count(),
        reports
            .iter()
            .filter(|report| report.policy.power_preference == PowerPreference::FinishSooner)
            .count(),
        reports
            .iter()
            .filter(|report| {
                report.policy.energy_recovery_preference
                    == EnergyRecoveryPreference::ProtectSurvival
            })
            .count(),
        reports
            .iter()
            .filter(|report| {
                report.policy.energy_recovery_preference
                    == EnergyRecoveryPreference::SpendSurvivalReserve
            })
            .count(),
        reports
            .iter()
            .filter(|report| {
                report.policy.maintenance_preference == MaintenancePreference::ServiceAtWarning
            })
            .count(),
        reports
            .iter()
            .filter(|report| {
                report.policy.maintenance_preference == MaintenancePreference::ServiceAtCritical
            })
            .count(),
        reports
            .iter()
            .filter(|report| {
                report.policy.structural_preference == StructuralPreference::PreserveMargin
            })
            .count(),
        reports
            .iter()
            .filter(|report| {
                report.policy.structural_preference == StructuralPreference::MoveOnlyForFailure
            })
            .count(),
        observation_stats.relocations,
        observation_stats.stranded_work_in_process,
        observation_stats.structural_consequences,
        reports
            .iter()
            .filter(|report| report.limits.maintenance_warning)
            .count(),
    );
    if include_scenarios {
        print_capability_scope(observation_stats);
    }
    print_experience_review(observation_stats, reports.len());
}
