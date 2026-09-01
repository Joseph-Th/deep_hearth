//! Owns workshop scenario orchestration, bounded matrix execution, and reporting.

use super::*;

struct WorkshopEpisode {
    variation: ScenarioVariation,
    state: AppState,
    ids: WorkshopIds,
    delivery_authorization: Option<ControlledMaterialDelivery>,
    initial_survival: deep_hearth::survival::SurvivalAssessment,
    initial_matter: deep_hearth::core::quantity::AggregateMass,
    initial_ore_composition: deep_hearth::material::MaterialComposition,
    thresholds: deep_hearth::maintenance::MaintenanceThresholds,
    current_support: StructuralElementId,
    alternate_support: StructuralElementId,
    report: ScenarioReport,
}

fn prepare_episode(registries: &Registries, variation: ScenarioVariation) -> WorkshopEpisode {
    let mut variation = variation;
    let (mut state, ids, delivery_authorization) = setup_workshop(registries, variation);
    let initial_survival = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("workshop player survival state disappeared after setup"));
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| {
            panic!("gameplay harness initial matter accounting failed: {error}")
        })
        .total();
    let initial_ore_composition = state
        .inventory()
        .get_lot(ids.ore_lot)
        .unwrap_or_else(|| panic!("workshop input ore lot disappeared after setup"))
        .composition()
        .clone();
    let crusher_definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .unwrap_or_else(|| panic!("canonical crusher definition disappeared"));
    let thresholds = crusher_definition.maintenance_thresholds();
    let initial_band = thresholds.classify(variation.crusher.initial_crusher_condition);
    let mut report = ScenarioReport::new(variation, initial_band);
    // Initial critical condition is resolved before support siting and controlled-event scheduling.
    // This is intentionally separate from the recurring pre-batch maintenance gate below because
    // moving it into that loop changes observable scenario ordering and terminal-event timing.
    if initial_band == MaintenanceBand::Critical {
        report.limits.maintenance_warning = true;
        println!(
            "  initial maintenance gate: crusher begins at {}ppm/{initial_band:?}; service before planning powered work",
            variation
                .crusher
                .initial_crusher_condition
                .parts_per_million(),
        );
        if service_crusher(registries, &mut state, ids, &mut report.maintenance)
            == MaintenanceAttempt::SupplyExhausted
        {
            report.limits.maintenance_stop = true;
            println!(
                "  initial maintenance gate: no replacement stock is available; the work order cannot start"
            );
        }
    }
    let maintenance_profile = crusher_definition
        .maintenance_profile()
        .unwrap_or_else(|| panic!("canonical crusher maintenance profile disappeared"));
    let compact_mount =
        validate_mount_equipment(registries, &state, ids.crusher, ids.compact_support)
            .unwrap_or_else(|error| panic!("compact bay mount prediction failed: {error}"));
    let reinforced_mount =
        validate_mount_equipment(registries, &state, ids.crusher, ids.reinforced_support)
            .unwrap_or_else(|error| panic!("reinforced bay mount prediction failed: {error}"));
    let compact_assessment =
        structural_assessment(compact_mount.structural_analysis(), ids.compact_support);
    let reinforced_assessment = structural_assessment(
        reinforced_mount.structural_analysis(),
        ids.reinforced_support,
    );
    println!(
        "  support options: compact={}; reinforced={} (reinforced stored cargo={}mg)",
        structural_label(compact_assessment),
        structural_label(reinforced_assessment),
        variation.structure.reinforced_background_mass.milligrams(),
    );
    assert_ne!(
        compact_assessment.stage(),
        StructuralStage::Failed,
        "gameplay scenario must offer a legal compact crusher siting option"
    );
    assert_ne!(
        reinforced_assessment.stage(),
        StructuralStage::Failed,
        "gameplay scenario must offer a legal reinforced crusher siting option"
    );
    assert!(
        compact_assessment.utilization_ppm()
            <= u128::from(variation.structure.compact_target_utilization_ppm),
        "gameplay compact-support fixture drifted above its production structural-utilization target"
    );
    assert!(
        reinforced_assessment.utilization_ppm()
            <= u128::from(variation.structure.reinforced_target_utilization_ppm),
        "gameplay reinforced-support fixture drifted above its production structural-utilization target"
    );
    let compact_is_better = (
        stage_rank(compact_assessment.stage()),
        compact_assessment.utilization_ppm(),
    ) < (
        stage_rank(reinforced_assessment.stage()),
        reinforced_assessment.utilization_ppm(),
    );
    let (current_support, alternate_support, selected_mount, support_name) = if compact_is_better {
        report.choices.chose_compact_support = true;
        (
            ids.compact_support,
            ids.reinforced_support,
            compact_mount,
            "compact clear bay",
        )
    } else {
        (
            ids.reinforced_support,
            ids.compact_support,
            reinforced_mount,
            "reinforced occupied bay",
        )
    };
    let reason = "player chooses the best currently observable structural margin";
    println!("  decision: mount crusher on {support_name}; {reason}");
    let _ = selected_mount
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selected crusher mount failed: {error}"));

    if report.limits.maintenance_stop {
        variation.delivery.delivery_at_tick = state
            .tick()
            .value()
            .checked_add(1)
            .unwrap_or_else(|| panic!("terminal workshop event tick overflowed"));
    } else {
        schedule_controlled_delivery_event(registries, &state, ids, &mut variation);
    }
    report.inputs.delivery_at_tick = variation.delivery.delivery_at_tick;
    let delivery_target = if variation.delivery.destination_is_compact {
        "compact"
    } else {
        "reinforced"
    };
    println!(
        "\nSCENARIO world=0x{:016X} behavior=0x{:016X} ore=[copper:{}ppm gangue-clay-share:{}ppm] order={}mg nominal_batch={}mg crusher={}ppm controller_event=[tick:{} mass:{}mg target:{} actor_visibility:hidden] policy=[power:{} recovery:{} maintenance:{} structure:{}] stored_work=[small:{}+{}ppm nominal-batches, high-power:{}+{}ppm nominal-batches] maintenance=[units:{} replacement:{}mg target:{}ppm]",
        variation.world_seed,
        variation.behavior_seed,
        variation.ore.ore_copper_ppm,
        variation.ore.gangue_clay_share_ppm,
        variation.ore.order_mass.milligrams(),
        variation.ore.nominal_batch_mass.milligrams(),
        variation
            .crusher
            .initial_crusher_condition
            .parts_per_million(),
        variation.delivery.delivery_at_tick,
        variation.delivery.mass.milligrams(),
        delivery_target,
        variation.policy.power_preference.label(),
        variation.policy.energy_recovery_preference.label(),
        variation.policy.maintenance_preference.label(),
        variation.policy.structural_preference.label(),
        variation.crusher.small_drive_batch_budget,
        variation.crusher.small_drive_partial_batch_ppm,
        variation.crusher.large_drive_batch_budget,
        variation.crusher.large_drive_partial_batch_ppm,
        variation.crusher.maintenance_replacement_units,
        maintenance_profile
            .full_service_replacement_mass()
            .milligrams(),
        maintenance_profile.restored_condition().parts_per_million(),
    );
    println!(
        "  objective: complete the ore work order using observable workshop state; react to the controlled delivery only after it occurs"
    );

    WorkshopEpisode {
        variation,
        state,
        ids,
        delivery_authorization,
        initial_survival,
        initial_matter,
        initial_ore_composition,
        thresholds,
        current_support,
        alternate_support,
        report,
    }
}

struct SelectedBatch {
    mass: Mass,
    option: CrushOption,
    reason: &'static str,
    choice_basis: PowerChoiceBasis,
    adaptive: bool,
    condition_adaptive: bool,
    energy_adaptive: bool,
}

enum BatchSelection {
    Ready(SelectedBatch),
    Stop,
}

struct BatchSelectionContext<'a> {
    variation: ScenarioVariation,
    state: &'a mut AppState,
    ids: WorkshopIds,
    delivery_authorization: &'a mut Option<ControlledMaterialDelivery>,
    thresholds: deep_hearth::maintenance::MaintenanceThresholds,
    current_support: &'a mut StructuralElementId,
    alternate_support: &'a mut StructuralElementId,
    report: &'a mut ScenarioReport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreBatchTransition {
    Proceed,
    Retry,
    Stop,
}

fn handle_pre_batch_maintenance(
    registries: &Registries,
    context: &mut BatchSelectionContext<'_>,
) -> PreBatchTransition {
    let current_condition = context
        .state
        .equipment()
        .get_equipment(context.ids.crusher)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("crusher disappeared during gameplay harness"));
    let band = context.thresholds.classify(current_condition);
    if band != MaintenanceBand::Normal && !context.report.limits.maintenance_warning {
        context.report.limits.maintenance_warning = true;
        println!(
            "  maintenance transition: condition={}ppm band={band:?}",
            current_condition.parts_per_million()
        );
    }
    if band == MaintenanceBand::Warning
        && context.variation.policy.maintenance_preference
            == MaintenancePreference::ServiceAtWarning
        && !context.report.maintenance.supply_exhausted
    {
        println!(
            "  decision: service crusher in warning condition because player policy favors preventive maintenance"
        );
        match service_crusher(
            registries,
            &mut *context.state,
            context.ids,
            &mut context.report.maintenance,
        ) {
            MaintenanceAttempt::Serviced => return PreBatchTransition::Retry,
            MaintenanceAttempt::SupplyExhausted => {
                println!(
                    "  maintenance policy: preventive service is unavailable; continue legal work until condition or another constraint forces a stop"
                );
            }
        }
    }
    if band != MaintenanceBand::Critical {
        return PreBatchTransition::Proceed;
    }

    println!("  decision: service crusher before more work because current condition is critical");
    match service_crusher(
        registries,
        &mut *context.state,
        context.ids,
        &mut context.report.maintenance,
    ) {
        MaintenanceAttempt::Serviced => PreBatchTransition::Retry,
        MaintenanceAttempt::SupplyExhausted => {
            context.report.limits.maintenance_stop = true;
            println!(
                "  decision: stop crushing; replacement stock is exhausted and the crusher remains critical"
            );
            PreBatchTransition::Stop
        }
    }
}

fn handle_maintenance_blocked_plan(
    registries: &Registries,
    context: &mut BatchSelectionContext<'_>,
) -> PreBatchTransition {
    println!(
        "  decision: service crusher because no positive powered batch is legal within the remaining condition lifetime and maintenance safety margin"
    );
    match service_crusher(
        registries,
        &mut *context.state,
        context.ids,
        &mut context.report.maintenance,
    ) {
        MaintenanceAttempt::Serviced => PreBatchTransition::Retry,
        MaintenanceAttempt::SupplyExhausted => {
            context.report.limits.maintenance_stop = true;
            println!(
                "  decision: stop crushing; replacement stock is exhausted and even the smallest powered batch is outside the crusher's remaining safe working envelope"
            );
            PreBatchTransition::Stop
        }
    }
}

fn manual_recovery_adaptation_reason(constraint: Option<ManualRecoveryConstraint>) -> &'static str {
    match constraint {
        Some(ManualRecoveryConstraint::SurvivalPolicy) => {
            "a larger charging commitment would cross the protected hunger or thirst reserve"
        }
        Some(ManualRecoveryConstraint::SurvivalReserve) => {
            "a larger charging commitment exceeds current physiological reserves"
        }
        Some(ManualRecoveryConstraint::EquipmentCondition) => {
            "the hand crank cannot sustain a larger charging commitment within its remaining condition lifetime"
        }
        Some(ManualRecoveryConstraint::StorageCapacity) => {
            "the selected drive cannot accept enough additional work for a larger batch"
        }
        None => "a larger charging commitment is not currently executable",
    }
}

fn attempt_manual_recovery(
    registries: &Registries,
    context: &mut BatchSelectionContext<'_>,
    planned_mass: Mass,
) -> PreBatchTransition {
    match largest_manual_recovery(
        registries,
        &*context.state,
        context.ids,
        planned_mass,
        context.variation.policy.energy_recovery_preference,
    ) {
        ManualRecoverySearch::Available {
            mass,
            option,
            adaptive_constraint,
        } => {
            if mass < planned_mass {
                let reason = manual_recovery_adaptation_reason(adaptive_constraint);
                println!(
                    "  manual recovery adapts the next operation from {}mg to {}mg because {reason}",
                    planned_mass.milligrams(),
                    mass.milligrams(),
                );
            }
            let mut controller = ControlledDeliveryRuntime {
                delivery: context.variation.delivery,
                authorization: &mut *context.delivery_authorization,
            };
            let mut actor = ScenarioActorRuntime::new(
                context.variation.policy,
                context.variation.ore.nominal_batch_mass,
                &mut *context.current_support,
                &mut *context.alternate_support,
                ScenarioActorReport {
                    structure: &mut context.report.structure,
                    choices: &mut context.report.choices,
                    progress: &mut context.report.progress,
                    resources: &mut context.report.resources,
                },
            );
            execute_manual_recovery(
                registries,
                &mut *context.state,
                context.ids,
                *option,
                &mut controller,
                &mut actor,
            );
            if context.report.structure.structural_stop {
                PreBatchTransition::Stop
            } else {
                PreBatchTransition::Retry
            }
        }
        ManualRecoverySearch::DeclinedForSurvival => {
            context.report.limits.manual_recovery_declined = true;
            println!(
                "  manual recovery declined: even the smallest useful charging commitment would cross the player's hunger or thirst warning reserve"
            );
            stop_for_energy_shortage(context)
        }
        ManualRecoverySearch::SurvivalLimited => {
            context.report.limits.manual_recovery_survival_limited = true;
            println!(
                "  manual recovery unavailable: the player lacks enough physiological reserve for another useful charging commitment"
            );
            stop_for_energy_shortage(context)
        }
        ManualRecoverySearch::EquipmentLimited => {
            println!(
                "  manual recovery unavailable: the hand crank cannot complete even the smallest useful charging commitment within its remaining condition lifetime"
            );
            stop_for_energy_shortage(context)
        }
        ManualRecoverySearch::StorageLimited => {
            println!(
                "  manual recovery unavailable: neither mechanical drive can accept the work needed for even the smallest useful recovered batch"
            );
            stop_for_energy_shortage(context)
        }
    }
}

fn stop_for_energy_shortage(context: &mut BatchSelectionContext<'_>) -> PreBatchTransition {
    if context.report.structure.structural_stop {
        return PreBatchTransition::Stop;
    }
    context.report.limits.energy_stop = true;
    let reason = if context.report.limits.manual_recovery_declined {
        "player preserves survival reserve"
    } else if context.report.limits.manual_recovery_survival_limited {
        "player lacks the physiological reserve to generate the missing work"
    } else {
        "stored work is insufficient and the manual fallback cannot supply the deficit"
    };
    println!("  decision: stop crushing; {reason}");
    PreBatchTransition::Stop
}

fn choose_powered_batch(
    context: &mut BatchSelectionContext<'_>,
    planned_mass: Mass,
    plan: CrushBatchPlan,
) -> SelectedBatch {
    let resolved_mass = plan.mass;
    let small = plan.small;
    let large = plan.large;
    let adaptive = resolved_mass < planned_mass;
    if adaptive {
        println!(
            "  adaptive batching: planned={}mg -> executable={}mg constraints=[equipment-capacity:{} condition-lifetime:{} maintenance-safety:{} stored-work:{}]",
            planned_mass.milligrams(),
            resolved_mass.milligrams(),
            plan.equipment_capacity_limited,
            plan.condition_lifetime_limited,
            plan.maintenance_limited,
            plan.energy_limited,
        );
    }
    if let Some(option) = &small {
        print_crush_option(option, context.thresholds);
    }
    if let Some(option) = &large {
        print_crush_option(option, context.thresholds);
    } else if !context.report.choices.large_drive_exhausted {
        context.report.choices.large_drive_exhausted = true;
        println!("  power reserve: high-power drive cannot supply the current planned mass");
    }
    let (option, reason, choice_basis) = choose_crush_option(
        small,
        large,
        CrushChoiceContext {
            thresholds: context.thresholds,
            preference: context.variation.policy.power_preference,
        },
    );
    SelectedBatch {
        mass: resolved_mass,
        option,
        reason,
        choice_basis,
        adaptive,
        condition_adaptive: plan.equipment_capacity_limited
            || plan.condition_lifetime_limited
            || plan.maintenance_limited,
        energy_adaptive: plan.energy_limited,
    }
}

fn select_next_batch(
    registries: &Registries,
    mut context: BatchSelectionContext<'_>,
) -> BatchSelection {
    let transition_budget = u64::from(context.variation.crusher.maintenance_replacement_units)
        .checked_add(3)
        .unwrap_or_else(|| panic!("workshop pre-batch transition budget overflowed"));

    for _ in 0..transition_budget {
        match handle_pre_batch_maintenance(registries, &mut context) {
            PreBatchTransition::Proceed => {}
            PreBatchTransition::Retry => continue,
            PreBatchTransition::Stop => return BatchSelection::Stop,
        }

        let remaining = context
            .report
            .progress
            .target_mass
            .checked_sub(context.report.progress.processed_mass)
            .unwrap_or_else(|| panic!("workshop processed mass exceeded its work order"));
        let planned_mass = Mass::from_milligrams(
            remaining
                .milligrams()
                .min(context.variation.ore.nominal_batch_mass.milligrams()),
        );
        match largest_safe_powered_crush_batch(
            registries,
            &*context.state,
            context.ids,
            planned_mass,
            context.thresholds,
        ) {
            CrushBatchSearch::Available(plan) => {
                return BatchSelection::Ready(choose_powered_batch(
                    &mut context,
                    planned_mass,
                    *plan,
                ));
            }
            CrushBatchSearch::MaintenanceBlocked => {
                match handle_maintenance_blocked_plan(registries, &mut context) {
                    PreBatchTransition::Retry => continue,
                    PreBatchTransition::Stop => return BatchSelection::Stop,
                    PreBatchTransition::Proceed => {
                        unreachable!("maintenance-block handling either services or stops")
                    }
                }
            }
            CrushBatchSearch::EnergyUnavailable => {
                match attempt_manual_recovery(registries, &mut context, planned_mass) {
                    PreBatchTransition::Retry => continue,
                    PreBatchTransition::Stop => return BatchSelection::Stop,
                    PreBatchTransition::Proceed => {
                        unreachable!("manual recovery either changes state or stops")
                    }
                }
            }
        }
    }

    panic!(
        "workshop actor exhausted its bounded pre-batch transition budget without selecting work or reaching a terminal state"
    );
}

fn select_episode_batch(registries: &Registries, episode: &mut WorkshopEpisode) -> BatchSelection {
    select_next_batch(
        registries,
        BatchSelectionContext {
            variation: episode.variation,
            state: &mut episode.state,
            ids: episode.ids,
            delivery_authorization: &mut episode.delivery_authorization,
            thresholds: episode.thresholds,
            current_support: &mut episode.current_support,
            alternate_support: &mut episode.alternate_support,
            report: &mut episode.report,
        },
    )
}

fn apply_due_delivery(registries: &Registries, episode: &mut WorkshopEpisode) -> bool {
    if episode.report.progress.delivery_applied {
        return false;
    }
    let current_tick = episode.state.tick().value();
    let delivery_tick = episode.variation.delivery.delivery_at_tick;
    assert!(
        current_tick <= delivery_tick,
        "controlled event tick was passed without being applied"
    );
    if current_tick != delivery_tick {
        return false;
    }

    let mut controller = ControlledDeliveryRuntime {
        delivery: episode.variation.delivery,
        authorization: &mut episode.delivery_authorization,
    };
    let mut actor = ScenarioActorRuntime::new(
        episode.variation.policy,
        episode.variation.ore.nominal_batch_mass,
        &mut episode.current_support,
        &mut episode.alternate_support,
        ScenarioActorReport {
            structure: &mut episode.report.structure,
            choices: &mut episode.report.choices,
            progress: &mut episode.report.progress,
            resources: &mut episode.report.resources,
        },
    );
    apply_delivery_and_adapt(
        registries,
        &mut episode.state,
        episode.ids,
        &mut controller,
        &mut actor,
    );
    true
}

fn record_completed_batch(
    report: &mut ScenarioReport,
    mass: Mass,
    adaptive: bool,
    condition_adaptive: bool,
    energy_adaptive: bool,
) {
    report.progress.processed_mass = report
        .progress
        .processed_mass
        .checked_add(mass)
        .unwrap_or_else(|| panic!("workshop processed-mass accounting overflowed"));
    report.progress.operations_completed = report
        .progress
        .operations_completed
        .checked_add(1)
        .unwrap_or_else(|| panic!("workshop operation count overflowed"));
    if !adaptive {
        return;
    }
    report.progress.adaptive_batch_operations = report
        .progress
        .adaptive_batch_operations
        .checked_add(1)
        .unwrap_or_else(|| panic!("adaptive-batch count overflowed"));
    if condition_adaptive {
        report.progress.condition_adaptive_batch_operations = report
            .progress
            .condition_adaptive_batch_operations
            .checked_add(1)
            .unwrap_or_else(|| panic!("condition-adaptive count overflowed"));
    }
    if energy_adaptive {
        report.progress.energy_adaptive_batch_operations = report
            .progress
            .energy_adaptive_batch_operations
            .checked_add(1)
            .unwrap_or_else(|| panic!("energy-adaptive count overflowed"));
    }
}

fn record_batch_bottleneck(report: &mut ScenarioReport, bottleneck: PoweredOreBottleneck) {
    match bottleneck {
        PoweredOreBottleneck::Throughput => report.limits.throughput_bottleneck_batches += 1,
        PoweredOreBottleneck::EnergyDelivery => report.limits.energy_bottleneck_batches += 1,
        PoweredOreBottleneck::Balanced => report.limits.balanced_bottleneck_batches += 1,
    }
}

fn execute_selected_batch(
    registries: &Registries,
    episode: &mut WorkshopEpisode,
    batch: SelectedBatch,
) -> bool {
    let SelectedBatch {
        mass,
        option,
        reason,
        choice_basis,
        adaptive,
        condition_adaptive,
        energy_adaptive,
    } = batch;
    match choice_basis {
        PowerChoiceBasis::Policy => episode.report.choices.policy_power_choices += 1,
        PowerChoiceBasis::SingleSource => episode.report.choices.single_source_power_choices += 1,
    }
    println!("  decision: use {} drive because {reason}", option.name);
    if option.store == episode.ids.small_drive {
        episode.report.choices.small_drive_batches += 1;
    } else if option.store == episode.ids.large_drive {
        episode.report.choices.large_drive_batches += 1;
    }
    let next_operation = episode.report.progress.operations_completed + 1;
    let outcome = {
        let mut controller = ControlledDeliveryRuntime {
            delivery: episode.variation.delivery,
            authorization: &mut episode.delivery_authorization,
        };
        let mut actor = ScenarioActorRuntime::new(
            episode.variation.policy,
            episode.variation.ore.nominal_batch_mass,
            &mut episode.current_support,
            &mut episode.alternate_support,
            ScenarioActorReport {
                structure: &mut episode.report.structure,
                choices: &mut episode.report.choices,
                progress: &mut episode.report.progress,
                resources: &mut episode.report.resources,
            },
        );
        crush_batch(
            registries,
            &mut episode.state,
            episode.ids,
            CrushBatchExecution {
                mass,
                option,
                batch_index: next_operation,
            },
            &mut controller,
            &mut actor,
        )
    };
    if outcome.completed {
        record_completed_batch(
            &mut episode.report,
            mass,
            adaptive,
            condition_adaptive,
            energy_adaptive,
        );
    }
    record_batch_bottleneck(&mut episode.report, outcome.bottleneck);
    outcome.completed
}

fn observe_episode(
    registries: &Registries,
    episode: &mut WorkshopEpisode,
    observation_horizon: Option<u64>,
) {
    if !episode.report.progress.delivery_applied {
        println!(
            "  controlled event: not reached before the actor's work-order episode ended at tick={} (scheduled tick={})",
            episode.state.tick().value(),
            episode.variation.delivery.delivery_at_tick,
        );
    }
    episode.report.resources.episode_end_tick = episode.state.tick().value();
    let Some(observation_horizon) = observation_horizon else {
        return;
    };
    assert!(
        observation_horizon >= episode.state.tick().value(),
        "agency observation horizon must not precede the actor episode end"
    );
    if !episode.report.progress.delivery_applied
        && episode.variation.delivery.delivery_at_tick <= observation_horizon
    {
        assert!(
            episode.state.tick().value() <= episode.variation.delivery.delivery_at_tick,
            "actor episode passed the controlled event without applying it"
        );
        if episode.state.tick().value() < episode.variation.delivery.delivery_at_tick {
            let wait_ticks =
                episode.variation.delivery.delivery_at_tick - episode.state.tick().value();
            advance_idle_ticks(
                registries,
                &mut episode.state,
                wait_ticks,
                "post-episode controlled-event wait",
            );
        }
        let mut controller = ControlledDeliveryRuntime {
            delivery: episode.variation.delivery,
            authorization: &mut episode.delivery_authorization,
        };
        let mut actor = ScenarioActorRuntime::new(
            episode.variation.policy,
            episode.variation.ore.nominal_batch_mass,
            &mut episode.current_support,
            &mut episode.alternate_support,
            ScenarioActorReport {
                structure: &mut episode.report.structure,
                choices: &mut episode.report.choices,
                progress: &mut episode.report.progress,
                resources: &mut episode.report.resources,
            },
        );
        let _ = apply_delivery(
            registries,
            &mut episode.state,
            episode.ids,
            &mut controller,
            &mut actor,
        );
        println!(
            "  evaluator: controlled event applied during post-episode observation; actor remains inactive"
        );
    }
    if episode.state.tick().value() < observation_horizon {
        let wait_ticks = observation_horizon - episode.state.tick().value();
        advance_idle_ticks(
            registries,
            &mut episode.state,
            wait_ticks,
            "agency observation horizon",
        );
    }
    assert_eq!(
        episode.state.tick().value(),
        observation_horizon,
        "agency branch must finish at its shared observation horizon"
    );
}

fn finalize_episode(registries: &Registries, episode: WorkshopEpisode) -> ScenarioReport {
    super::finalize::finalize_scenario(
        registries,
        &episode.state,
        super::finalize::ScenarioAuditInput {
            ids: episode.ids,
            current_support: episode.current_support,
            initial_matter: episode.initial_matter,
            initial_survival: episode.initial_survival,
            initial_ore_composition: &episode.initial_ore_composition,
        },
        episode.report,
    )
}

pub(super) fn run_scenario(
    registries: &Registries,
    variation: ScenarioVariation,
    observation_horizon: Option<u64>,
) -> ScenarioReport {
    let mut episode = prepare_episode(registries, variation);

    while episode.report.progress.processed_mass < episode.report.progress.target_mass {
        if episode.report.structure.structural_stop {
            println!(
                "  decision: stop crushing; the delivered stored-matter load left no support that can carry the machine"
            );
            break;
        }
        if episode.report.limits.maintenance_stop {
            println!(
                "  decision: stop crushing; the crusher is critical and replacement stock is unavailable"
            );
            break;
        }
        if apply_due_delivery(registries, &mut episode) {
            continue;
        }

        let batch = match select_episode_batch(registries, &mut episode) {
            BatchSelection::Ready(batch) => batch,
            BatchSelection::Stop => break,
        };
        if !execute_selected_batch(registries, &mut episode, batch) {
            break;
        }
        let _ = apply_due_delivery(registries, &mut episode);
    }

    observe_episode(registries, &mut episode, observation_horizon);
    finalize_episode(registries, episode)
}

/// Runs the bootstrapped industrial workshop capability matrix.
pub(super) fn run_gameplay_harness(mode: ScenarioPlanMode) {
    let registries = build_registries();
    let verbose = has_verbose_output();
    let scenario_raw = env::var("DEEP_HEARTH_GAMEPLAY_SEEDS").ok();
    let variation_raw = env::var("DEEP_HEARTH_GAMEPLAY_VARIATION_SEED").ok();
    let behavior_raw = env::var("DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED").ok();
    let mode_salt = match mode {
        ScenarioPlanMode::Gate => 0x4741_5445_5EED_2026_u64,
        ScenarioPlanMode::Explore => 0x4558_504C_5EED_2026_u64,
    };
    let default_world_root = fresh_root(MAINTAINED_VARIATION_ROOT ^ mode_salt);
    let default_behavior_root =
        fresh_root(MAINTAINED_BEHAVIOR_ROOT ^ mode_salt.rotate_left(17) ^ 0xB3A4_7102_5EED_2026);
    let plan = scenario_seeds_from(
        mode,
        scenario_raw.as_deref(),
        variation_raw.as_deref(),
        behavior_raw.as_deref(),
        default_world_root,
        default_behavior_root,
    )
    .unwrap_or_else(|error| panic!("gameplay harness configuration failed: {error:?}"));
    std::println!(
        "HARNESS INPUT plan={} anchors={} variation={} custom={} world_root={} behavior_root={} replay={}",
        plan.source_label(),
        plan.anchor_seed_count(),
        plan.variation_seed_count(),
        plan.custom_seed_count(),
        plan.variation_label(),
        plan.behavior_label(),
        plan.replay_label(),
    );
    print_content_summary(&registries, verbose || mode == ScenarioPlanMode::Explore);
    std::println!(
        "EVIDENCE CONTRACT runtime-experience-after-disclosed-bootstrap=[survival,primitive-progression] controlled-capability-probes=[industrial-workshop,industrial-ore-preparation,pure-copper-foundry] setup-shortcuts=disclosed-and-fixture-guarded actor-observation=runtime-public-state-only catalog=registry-derived authored-edges=not-end-to-end-proof global-reachability-authority=STATUS.md"
    );
    if verbose {
        std::println!(
            "EVIDENCE INTERPRETATION runtime-experience-probes=normal-resolvers+validators+commits+ticks-after-disclosed-starting-world-setup controlled-probes=same-runtime-operations-on-unreachable-preinstalled-capabilities actor-hidden=[deposit-identity,future-controlled-event] variation=maintained-regressions+fresh-replayable-organic-worlds detailed-outcomes=PROGRESSION-REVIEW+SURVIVAL-REVIEW+WORKSHOP-CAPABILITY+ORE-REVIEW+FOUNDRY-REVIEW"
        );
    }
    println!(
        "\n=== DEEP HEARTH INDUSTRIAL WORKSHOP CAPABILITY MATRIX: {} scenario(s), registry schema {} ===",
        plan.cases().len(),
        registries.schema_version().value(),
    );
    println!(
        "CAPABILITY SETUP: industrial machines and industrial energy stores with no current ordinary acquisition path, plus starting matter, structural bays, background cargo, and one single-use delivery authorization, are arranged before the actor starts. Fixture guards fail if any injected machine/store later gains a direct authored acquisition/assembly edge. The controlled event consumes its authorization through canonical inventory validation/commit; the actor receives no future event tick or target."
    );
    println!(
        "CAPABILITY FANTASY: given an already-acquired industrial workshop, operate it by reading structural margin, uneven stored work, machine condition, material state, and personal survival reserve. Use residual work instead of discarding it, fall back to direct labor when worth the bodily cost, and recover when the world changes."
    );
    println!(
        "CAPABILITY LOOP: each workshop has a total ore work order rather than a required fixed batch count. Bounded cases vary uneven finite stored work, replacement stock, condition, support state, and player priorities. The actor uses canonical projections to resize operations, choose power, decide whether manual generation is survivable, and react after one hidden preauthorized supported-stockpile event changes the world. This does not imply industrial acquisition, generation, or logistics. Separate probes exercise runtime survival/progression actions after controlled bootstrap and bootstrapped ore-preparation/foundry capabilities."
    );

    let reports: Vec<_> = plan
        .cases()
        .iter()
        .copied()
        .map(|case| {
            ScenarioVariation::from_seeds(
                &registries,
                case.world_seed,
                case.behavior_seed,
                case.anchor,
            )
        })
        .map(|variation| run_scenario(&registries, variation, None))
        .collect();
    assert_scenario_contracts(&reports);
    let anchor_reports = plan
        .cases()
        .iter()
        .zip(&reports)
        .filter_map(|(case, report)| case.anchor.map(|anchor| (anchor, *report)))
        .collect::<Vec<_>>();
    if !anchor_reports.is_empty() {
        assert_anchor_diversity(&anchor_reports);
    }
    let evidence_mode = match mode {
        ScenarioPlanMode::Gate => "gate+organic",
        ScenarioPlanMode::Explore => "exploratory",
    };
    print_harness_summary(evidence_mode, &reports, verbose);
}
