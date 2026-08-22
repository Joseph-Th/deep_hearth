//! Workshop scenario orchestration, maintained matrix execution, and reporting.

use super::*;

pub(super) fn run_scenario(
    registries: &Registries,
    mut variation: ScenarioVariation,
) -> ScenarioReport {
    let (mut state, ids, mut delivery_authorization) = setup_workshop(registries, variation);
    let initial_survival = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("workshop player survival state disappeared after setup"));
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| {
            panic!("gameplay harness initial matter accounting failed: {error}")
        })
        .total();
    let crusher_definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .unwrap_or_else(|| panic!("canonical crusher definition disappeared"));
    let thresholds = crusher_definition.maintenance_thresholds();
    let initial_band = thresholds.classify(variation.crusher.initial_crusher_condition);
    let mut report = ScenarioReport::new(variation, initial_band);
    if initial_band == MaintenanceBand::Critical {
        report.limits.maintenance_warning = true;
        println!(
            "  initial maintenance gate: crusher begins at {}ppm/{initial_band:?}; service before planning powered work",
            variation
                .crusher
                .initial_crusher_condition
                .parts_per_million(),
        );
        if service_crusher(registries, &mut state, ids, &mut report)
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
    let compact_is_better = (
        stage_rank(compact_assessment.stage()),
        compact_assessment.utilization_ppm(),
    ) < (
        stage_rank(reinforced_assessment.stage()),
        reinforced_assessment.utilization_ppm(),
    );
    let choose_compact = compact_is_better;
    let (mut current_support, mut alternate_support, selected_mount, support_name) =
        if choose_compact {
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
    selected_mount
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selected crusher mount failed: {error}"));

    schedule_controlled_delivery_event(registries, &state, ids, &mut variation);
    let delivery_target = if variation.delivery.destination_is_compact {
        "compact"
    } else {
        "reinforced"
    };
    println!(
        "\nSCENARIO world=0x{:016X} behavior=0x{:016X} ore={}ppm Cu order={}mg nominal_batch={}mg crusher={}ppm controller_event=[tick:{} mass:{}mg target:{} actor_visibility:hidden] policy=[power:{} recovery:{} maintenance:{} structure:{}] stored_work=[small:{}+{}ppm nominal-batches, high-power:{}+{}ppm nominal-batches] maintenance=[units:{} replacement:{}mg target:{}ppm]",
        variation.world_seed,
        variation.behavior_seed,
        variation.ore.ore_copper_ppm,
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
        maintenance_profile.replacement_mass().milligrams(),
        maintenance_profile.restored_condition().parts_per_million(),
    );
    println!(
        "  objective: complete the ore work order using observable workshop state; react to the controlled delivery only after it occurs"
    );

    'work_order: while report.progress.processed_mass < report.progress.target_mass {
        if report.structure.structural_stop {
            println!(
                "  decision: stop crushing; the delivered stored-matter load left no support that can carry the machine"
            );
            break;
        }
        if report.limits.maintenance_stop {
            println!(
                "  decision: stop crushing; the crusher is critical and replacement stock is unavailable"
            );
            break;
        }
        if !report.progress.delivery_applied {
            assert!(
                state.tick().value() <= variation.delivery.delivery_at_tick,
                "controlled event tick was passed without being applied"
            );
            if state.tick().value() == variation.delivery.delivery_at_tick {
                let mut controller = ControlledDeliveryRuntime {
                    delivery: variation.delivery,
                    authorization: &mut delivery_authorization,
                };
                let mut actor = ScenarioActorRuntime::new(
                    variation.policy,
                    variation.ore.nominal_batch_mass,
                    &mut current_support,
                    &mut alternate_support,
                    &mut report,
                );
                apply_delivery_and_adapt(registries, &mut state, ids, &mut controller, &mut actor);
                continue;
            }
        }

        let (
            batch_mass,
            selected,
            reason,
            choice_basis,
            adaptive_batch,
            condition_adaptive,
            energy_adaptive,
        ) = loop {
            let current_condition = state
                .equipment()
                .get_equipment(ids.crusher)
                .map(|record| record.condition())
                .unwrap_or_else(|| panic!("crusher disappeared during gameplay harness"));
            let band = thresholds.classify(current_condition);
            if band != MaintenanceBand::Normal && !report.limits.maintenance_warning {
                report.limits.maintenance_warning = true;
                println!(
                    "  maintenance transition: condition={}ppm band={band:?}",
                    current_condition.parts_per_million()
                );
            }
            if band == MaintenanceBand::Warning
                && variation.policy.maintenance_preference
                    == MaintenancePreference::ServiceAtWarning
                && !report.maintenance.supply_exhausted
            {
                println!(
                    "  decision: service crusher in warning condition because player policy favors preventive maintenance"
                );
                match service_crusher(registries, &mut state, ids, &mut report) {
                    MaintenanceAttempt::Serviced => continue,
                    MaintenanceAttempt::SupplyExhausted => {
                        println!(
                            "  maintenance policy: preventive service is unavailable; continue legal work until condition or another constraint forces a stop"
                        );
                    }
                }
            }
            if band == MaintenanceBand::Critical {
                println!(
                    "  decision: service crusher before more work because current condition is critical"
                );
                match service_crusher(registries, &mut state, ids, &mut report) {
                    MaintenanceAttempt::Serviced => continue,
                    MaintenanceAttempt::SupplyExhausted => {
                        report.limits.maintenance_stop = true;
                        println!(
                            "  decision: stop crushing; replacement stock is exhausted and the crusher remains critical"
                        );
                        break 'work_order;
                    }
                }
            }

            let remaining = report
                .progress
                .target_mass
                .checked_sub(report.progress.processed_mass)
                .unwrap_or_else(|| panic!("workshop processed mass exceeded its work order"));
            let planned_mass = Mass::from_milligrams(
                remaining
                    .milligrams()
                    .min(variation.ore.nominal_batch_mass.milligrams()),
            );
            let condition_limit = current_crusher_batch_limit(registries, &state, ids);
            if condition_limit.is_zero() {
                report.limits.maintenance_stop = true;
                println!(
                    "  decision: stop crushing; current crusher condition leaves no usable batch capacity"
                );
                break 'work_order;
            }
            let desired_mass =
                Mass::from_milligrams(planned_mass.milligrams().min(condition_limit.milligrams()));
            let condition_capacity_limited = desired_mass < planned_mass;
            let plan = match largest_safe_powered_crush_batch(
                registries,
                &state,
                ids,
                desired_mass,
                thresholds,
            ) {
                CrushBatchSearch::Available(plan) => plan,
                CrushBatchSearch::MaintenanceBlocked => {
                    println!(
                        "  decision: service crusher because no positive powered batch can preserve non-critical condition"
                    );
                    match service_crusher(registries, &mut state, ids, &mut report) {
                        MaintenanceAttempt::Serviced => continue,
                        MaintenanceAttempt::SupplyExhausted => {
                            report.limits.maintenance_stop = true;
                            println!(
                                "  decision: stop crushing; replacement stock is exhausted and even the smallest powered batch would enter critical condition"
                            );
                            break 'work_order;
                        }
                    }
                }
                CrushBatchSearch::EnergyUnavailable => {
                    match largest_manual_recovery(
                        registries,
                        &state,
                        ids,
                        desired_mass,
                        variation.policy.energy_recovery_preference,
                    ) {
                        ManualRecoverySearch::Available { mass, option } => {
                            if mass < desired_mass {
                                println!(
                                    "  manual recovery adapts the next operation from {}mg to {}mg because a larger charging commitment is not currently survivable",
                                    desired_mass.milligrams(),
                                    mass.milligrams(),
                                );
                            }
                            let mut controller = ControlledDeliveryRuntime {
                                delivery: variation.delivery,
                                authorization: &mut delivery_authorization,
                            };
                            let mut actor = ScenarioActorRuntime::new(
                                variation.policy,
                                variation.ore.nominal_batch_mass,
                                &mut current_support,
                                &mut alternate_support,
                                &mut report,
                            );
                            execute_manual_recovery(
                                registries,
                                &mut state,
                                ids,
                                *option,
                                &mut controller,
                                &mut actor,
                            );
                            if report.structure.structural_stop {
                                break 'work_order;
                            }
                            continue;
                        }
                        ManualRecoverySearch::DeclinedForSurvival => {
                            report.limits.manual_recovery_declined = true;
                            println!(
                                "  manual recovery declined: even the smallest useful charging commitment would cross the player's hunger or thirst warning reserve"
                            );
                        }
                        ManualRecoverySearch::SurvivalLimited => {
                            report.limits.manual_recovery_survival_limited = true;
                            println!(
                                "  manual recovery unavailable: the player lacks enough physiological reserve for another useful charging commitment"
                            );
                        }
                        ManualRecoverySearch::EquipmentUnavailable => {
                            println!(
                                "  manual recovery unavailable: hand-crank condition has reduced usable power to zero"
                            );
                        }
                    }
                    if report.structure.structural_stop {
                        break 'work_order;
                    }
                    report.limits.energy_stop = true;
                    let reason = if report.limits.manual_recovery_declined {
                        "player preserves survival reserve"
                    } else if report.limits.manual_recovery_survival_limited {
                        "player lacks the physiological reserve to generate the missing work"
                    } else {
                        "stored work is insufficient and the manual fallback cannot supply the deficit"
                    };
                    println!("  decision: stop crushing; {reason}");
                    break 'work_order;
                }
            };
            let resolved_mass = plan.mass;
            let small = plan.small;
            let large = plan.large;
            let adaptive_batch = resolved_mass < planned_mass;
            if adaptive_batch {
                println!(
                    "  adaptive batching: planned={}mg -> executable={}mg constraints=[condition-capacity:{} maintenance-safety:{} stored-work:{}]",
                    planned_mass.milligrams(),
                    resolved_mass.milligrams(),
                    condition_capacity_limited,
                    plan.maintenance_limited,
                    plan.energy_limited,
                );
            }
            if let Some(option) = &small {
                print_crush_option(option, thresholds);
            }
            if let Some(option) = &large {
                print_crush_option(option, thresholds);
            } else if !report.choices.large_drive_exhausted {
                report.choices.large_drive_exhausted = true;
                println!(
                    "  power reserve: high-power drive cannot supply the current planned mass"
                );
            }
            match choose_crush_option(
                small,
                large,
                CrushChoiceContext {
                    thresholds,
                    preference: variation.policy.power_preference,
                },
            ) {
                Ok((selected, reason, choice_basis)) => {
                    break (
                        resolved_mass,
                        selected,
                        reason,
                        choice_basis,
                        adaptive_batch,
                        condition_capacity_limited || plan.maintenance_limited,
                        plan.energy_limited,
                    );
                }
                Err(CrushStopReason::EnergyUnavailable) => {
                    unreachable!("safe powered batch returned without a viable energy option")
                }
            }
        };
        match choice_basis {
            PowerChoiceBasis::Policy => report.choices.policy_power_choices += 1,
            PowerChoiceBasis::SingleSource => report.choices.single_source_power_choices += 1,
        }
        println!("  decision: use {} drive because {reason}", selected.name);
        if selected.store == ids.small_drive {
            report.choices.small_drive_batches += 1;
        } else if selected.store == ids.large_drive {
            report.choices.large_drive_batches += 1;
        }
        let next_operation = report.progress.operations_completed + 1;
        let mut controller = ControlledDeliveryRuntime {
            delivery: variation.delivery,
            authorization: &mut delivery_authorization,
        };
        let mut actor = ScenarioActorRuntime::new(
            variation.policy,
            variation.ore.nominal_batch_mass,
            &mut current_support,
            &mut alternate_support,
            &mut report,
        );
        let outcome = crush_batch(
            registries,
            &mut state,
            ids,
            CrushBatchExecution {
                mass: batch_mass,
                option: selected,
                batch_index: next_operation,
            },
            &mut controller,
            &mut actor,
        );
        if outcome.completed {
            report.progress.processed_mass = report
                .progress
                .processed_mass
                .checked_add(batch_mass)
                .unwrap_or_else(|| panic!("workshop processed-mass accounting overflowed"));
            report.progress.operations_completed = report
                .progress
                .operations_completed
                .checked_add(1)
                .unwrap_or_else(|| panic!("workshop operation count overflowed"));
            if adaptive_batch {
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
        }
        match outcome.bottleneck {
            ComminutionBottleneck::Throughput => {
                report.limits.throughput_bottleneck_batches += 1;
            }
            ComminutionBottleneck::EnergyDelivery => {
                report.limits.energy_bottleneck_batches += 1;
            }
            ComminutionBottleneck::Balanced => {
                report.limits.balanced_bottleneck_batches += 1;
            }
        }
        if !outcome.completed {
            break;
        }
        if !report.progress.delivery_applied
            && state.tick().value() >= variation.delivery.delivery_at_tick
        {
            let mut controller = ControlledDeliveryRuntime {
                delivery: variation.delivery,
                authorization: &mut delivery_authorization,
            };
            let mut actor = ScenarioActorRuntime::new(
                variation.policy,
                variation.ore.nominal_batch_mass,
                &mut current_support,
                &mut alternate_support,
                &mut report,
            );
            apply_delivery_and_adapt(registries, &mut state, ids, &mut controller, &mut actor);
        }
    }
    if !report.progress.delivery_applied {
        let current_tick = state.tick().value();
        if current_tick < variation.delivery.delivery_at_tick {
            println!(
                "  timeline: scenario controller advances from tick={current_tick} to controlled delivery at tick={}",
                variation.delivery.delivery_at_tick
            );
            finish_operation(
                registries,
                &mut state,
                TickSpan::new(variation.delivery.delivery_at_tick - current_tick),
            );
        }
        let mut controller = ControlledDeliveryRuntime {
            delivery: variation.delivery,
            authorization: &mut delivery_authorization,
        };
        let mut actor = ScenarioActorRuntime::new(
            variation.policy,
            variation.ore.nominal_batch_mass,
            &mut current_support,
            &mut alternate_support,
            &mut report,
        );
        apply_delivery_and_adapt(registries, &mut state, ids, &mut controller, &mut actor);
    }
    let final_condition = state
        .equipment()
        .get_equipment(ids.crusher)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("crusher disappeared after gameplay harness crushing"));
    if thresholds.classify(final_condition) != MaintenanceBand::Normal {
        report.limits.maintenance_warning = true;
    }
    let final_hand_crank_condition = state
        .equipment()
        .get_equipment(ids.hand_crank)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("workshop hand crank disappeared"));

    let crushed_mass = state
        .inventory()
        .get_stockpile(ids.crushed_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("crushed storage disappeared"));
    if crushed_mass.is_zero() {
        println!(
            "  material: no crushed output was produced; selected ore remains in source storage or conserved work-in-process"
        );
        println!(
            "  process frontier: mixed-ore melt rejection is not probed because this scenario produced no crushed lot"
        );
    } else {
        let crushed_lots = state
            .inventory()
            .lot_ids(ids.crushed_storage)
            .collect::<Vec<_>>();
        assert!(
            !crushed_lots.is_empty(),
            "positive crushed storage mass must have at least one owned output lot"
        );
        let expected_composition = MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_COPPER, variation.ore.ore_copper_ppm),
            CompositionComponent::new(MATERIAL_STONE, 1_000_000 - variation.ore.ore_copper_ppm),
        ])
        .unwrap_or_else(|error| panic!("workshop expected crushed composition failed: {error}"));
        let first_record = state
            .inventory()
            .get_lot(crushed_lots[0])
            .unwrap_or_else(|| panic!("crushed output lot disappeared"));
        let particle_distribution = first_record
            .particle_size_distribution()
            .unwrap_or_else(|| panic!("canonical crushed ore lost particle-size state"));
        let particle_envelope = first_record
            .particle_size()
            .unwrap_or_else(|| panic!("canonical crushed ore lost particle-size envelope"));
        let contained_copper_floor = crushed_lots
            .iter()
            .map(|lot| {
                let record = state
                    .inventory()
                    .get_lot(*lot)
                    .unwrap_or_else(|| panic!("crushed output lot disappeared"));
                assert_eq!(
                    record.composition(),
                    &expected_composition,
                    "every crushed batch must preserve the work-order ore composition"
                );
                assert_eq!(
                    record.particle_size_distribution(),
                    Some(particle_distribution),
                    "every crushed batch from the same authored process must carry the same particle-size state"
                );
                record
                    .composition()
                    .constituent_mass_floor(record.mass(), MATERIAL_COPPER)
            })
            .try_fold(Mass::ZERO, |total, mass| total.checked_add(mass))
            .unwrap_or_else(|| panic!("workshop contained-copper accounting overflowed"));
        println!(
            "  material: crushed={}mg lots={} composition={}ppm Cu / {}ppm gangue contained_copper_floor={}mg particle_classes={} envelope={}..={}um",
            crushed_mass.milligrams(),
            crushed_lots.len(),
            variation.ore.ore_copper_ppm,
            1_000_000 - variation.ore.ore_copper_ppm,
            contained_copper_floor.milligrams(),
            particle_distribution.classes().len(),
            particle_envelope.minimum_diameter().micrometers(),
            particle_envelope.maximum_diameter().micrometers(),
        );
        println!(
            "  value state: ore grade changes conserved contained copper, but it cannot yet change a downstream production choice because concentration/smelting is unresolved"
        );
        if particle_distribution.classes().len() == 1 {
            println!(
                "  preparation state: crusher output is one unresolved size class; a screen cut through that class cannot claim a fabricated yield"
            );
        }
        report.progress.ore_frontier_visible = crushed_lots.iter().all(|lot| {
            let mixed_selection = [MaterialLotSelection::new(*lot, Mass::from_milligrams(1))];
            matches!(
                resolve_melting_process(
                    registries,
                    &state,
                    MeltingRequest::new(
                        PROCESS_MELT_PURE_COPPER,
                        ids.crushed_storage,
                        &mixed_selection,
                        ids.furnace,
                        ids.electrical_buffer,
                    ),
                ),
                Err(MeltingResolutionError::Batch(
                    MeltingBatchError::ImpureInput {
                        commodity: _commodity,
                    }
                ))
            )
        });
        println!(
            "  process frontier: crushed mixed ore cannot enter pure-copper melting={} (concentration/smelting remains the missing bridge)",
            report.progress.ore_frontier_visible
        );
    }

    assert_eq!(
        calculate_matter_accounting(&state).map(|accounting| accounting.total()),
        Ok(initial_matter),
        "gameplay workshop must conserve matter across production, relocation, and maintenance"
    );
    assert_eq!(validate_loaded_state(registries, &state), Ok(()));
    let small_remaining = state
        .energy()
        .get_store(ids.small_drive)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("small mechanical drive disappeared"));
    let large_remaining = state
        .energy()
        .get_store(ids.large_drive)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("large mechanical drive disappeared"));
    let active_support = state
        .structures()
        .get_element(current_support)
        .unwrap_or_else(|| panic!("active workshop support disappeared"));
    let maintenance_remaining = state
        .inventory()
        .get_stockpile(ids.maintenance_source)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("maintenance replacement stockpile disappeared"));
    let survival = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("workshop player survival state disappeared"));
    let metabolic_energy_spent = initial_survival
        .metabolic_energy()
        .checked_sub(survival.metabolic_energy())
        .unwrap_or_else(|| panic!("workshop metabolic reserve exceeded its scenario start"));
    let hydration_spent = initial_survival
        .hydration()
        .checked_sub(survival.hydration())
        .unwrap_or_else(|| panic!("workshop hydration reserve exceeded its scenario start"));
    report.resources.final_condition_ppm = final_condition.parts_per_million();
    report.resources.small_drive_remaining = small_remaining;
    report.resources.large_drive_remaining = large_remaining;
    report.resources.maintenance_stock_remaining = maintenance_remaining;
    report.resources.elapsed_ticks = state.tick().value();
    report.resources.metabolic_energy_spent = metabolic_energy_spent;
    report.resources.hydration_spent = hydration_spent;
    report.resources.final_vitality_ppm = survival.vitality().parts_per_million();
    report.resources.final_hand_crank_condition_ppm =
        final_hand_crank_condition.parts_per_million();
    println!(
        "  outcome: ore={}/{}mg operations={} adaptive=[total:{} condition:{} stored-work:{}] before_event={} choices=[policy:{} single-source:{} manual-recharges:{}] suspended={} stranded_wip={} equipment=[crusher:{}ppm/{:?} crank:{}ppm] maintenance=[services:{} spent:{}mg remaining:{}mg] mechanical_reserve=[small:{}nJ high-power:{}nJ] manual_generation=[energy:{}nJ ticks:{} body:{}nJ/{}uL] survival=[total-energy:-{}nJ total-hydration:-{}uL vitality:{}ppm] active_support={:?}/cracked:{} ticks={}",
        report.progress.processed_mass.milligrams(),
        report.progress.target_mass.milligrams(),
        report.progress.operations_completed,
        report.progress.adaptive_batch_operations,
        report.progress.condition_adaptive_batch_operations,
        report.progress.energy_adaptive_batch_operations,
        report.progress.operations_before_delivery,
        report.choices.policy_power_choices,
        report.choices.single_source_power_choices,
        report.choices.manual_recharges,
        report.structure.production_suspension,
        report.structure.stranded_work_in_process,
        final_condition.parts_per_million(),
        thresholds.classify(final_condition),
        final_hand_crank_condition.parts_per_million(),
        report.maintenance.services,
        report.maintenance.replacement_spent.milligrams(),
        maintenance_remaining.milligrams(),
        small_remaining.nanojoules(),
        large_remaining.nanojoules(),
        report.resources.manually_generated_energy.nanojoules(),
        report.resources.manual_power_ticks,
        report.resources.manual_power_metabolic_energy.nanojoules(),
        report.resources.manual_power_hydration.microliters(),
        metabolic_energy_spent.nanojoules(),
        hydration_spent.microliters(),
        survival.vitality().parts_per_million(),
        active_support.lifecycle(),
        active_support.is_cracked(),
        state.tick().value(),
    );
    println!(
        "  report: structural_change={} damage_debt={} support_block={} relocation={} structural_stop={} production_suspension={} stranded_wip={} machine_ops=[small:{} large:{}] manual_recharges={} power_choices=[policy:{} single-source:{}] bottlenecks=[energy:{} throughput:{} balanced:{}] maintenance_warning={} maintenance_services={} maintenance_supply_exhausted={} stops=[maintenance:{} energy:{} recovery_declined:{} recovery_survival_limited:{}] ore_frontier={}",
        report.structure.structural_consequence,
        report.structure.structural_damage_debt,
        report.structure.support_failure_blocked_production,
        report.structure.support_relocation,
        report.structure.structural_stop,
        report.structure.production_suspension,
        report.structure.stranded_work_in_process,
        report.choices.small_drive_batches,
        report.choices.large_drive_batches,
        report.choices.manual_recharges,
        report.choices.policy_power_choices,
        report.choices.single_source_power_choices,
        report.limits.energy_bottleneck_batches,
        report.limits.throughput_bottleneck_batches,
        report.limits.balanced_bottleneck_batches,
        report.limits.maintenance_warning,
        report.maintenance.services,
        report.maintenance.supply_exhausted,
        report.limits.maintenance_stop,
        report.limits.energy_stop,
        report.limits.manual_recovery_declined,
        report.limits.manual_recovery_survival_limited,
        report.progress.ore_frontier_visible,
    );
    report
}

/// Runs the bootstrapped industrial workshop capability matrix.
pub(super) fn run_gameplay_harness(mode: ScenarioPlanMode) {
    let registries = build_registries();
    let scenario_raw = env::var("DEEP_HEARTH_GAMEPLAY_SEEDS").ok();
    let variation_raw = env::var("DEEP_HEARTH_GAMEPLAY_VARIATION_SEED").ok();
    let behavior_raw = env::var("DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED").ok();
    let (default_world_root, default_behavior_root) = match mode {
        ScenarioPlanMode::Gate => (MAINTAINED_VARIATION_ROOT, MAINTAINED_BEHAVIOR_ROOT),
        ScenarioPlanMode::Explore => {
            let mode_salt = 0x4558_504C_5EED_2026;
            (
                fresh_root(MAINTAINED_VARIATION_ROOT ^ mode_salt),
                fresh_root(
                    MAINTAINED_BEHAVIOR_ROOT ^ mode_salt.rotate_left(17) ^ 0xB3A4_7102_5EED_2026,
                ),
            )
        }
    };
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
    print_content_summary(&registries, has_verbose_output());
    std::println!(
        "PLAYABILITY runtime-actions-after-controlled-bootstrap=[survival-provisioning,manual-shaping,equipment-assembly+upgrade,hand-mining,material-backed-flywheel-construction,survival-costed-manual-power,primitive-autonomous-crushing] bootstrap-assumptions=[starting-authored-food+drink+storage-profile,raw-gathered-matter,preauthorized-mining-site-identities] discovery=not-implemented capability-only=[industrial-workshop,industrial-ore-preparation,pure-copper-foundry] missing-bridge=[industrial-acquisition,industrial-power-generation,mixed-ore-concentration/smelting]"
    );
    std::println!(
        "PLAYER LOOP runtime-after-bootstrap=[survive->shape-tools->mine->reinforce->store-work->mechanize] capability-workshop=[site-machine->process-total-mass->adapt-batch-to-condition+stored-work->choose-power->hand-charge-or-protect-survival->react-to-world-load->maintain-or-relocate->iterate] utility=[survival-reserve,machine-condition,structural-margin,stored-work,time]"
    );
    println!(
        "\n=== DEEP HEARTH INDUSTRIAL WORKSHOP CAPABILITY MATRIX: {} scenario(s), registry schema {} ===",
        plan.cases().len(),
        registries.schema_version().value(),
    );
    println!(
        "CAPABILITY SETUP: industrial machines and industrial energy stores with no current runtime acquisition path, plus starting matter, structural bays, background cargo, and one single-use delivery authorization, are arranged before the actor starts. Fixture guards fail if any injected machine/store later gains a runtime acquisition path. The controlled event consumes its authorization through canonical inventory validation/commit; the actor receives no future event tick or target."
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
        .map(|variation| run_scenario(&registries, variation))
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
        ScenarioPlanMode::Gate => "controlled",
        ScenarioPlanMode::Explore => "exploratory",
    };
    print_harness_summary(evidence_mode, &reports);
}
