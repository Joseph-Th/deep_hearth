//! One complete primitive progression episode executed through canonical runtime actions.

use super::*;

#[path = "episode/discovery.rs"]
mod discovery;
use discovery::{ProgressionDiscovery, ProgressionDiscoveryPlan, discover_primitive_progression};

#[path = "episode/world.rs"]
mod world;
use world::{ProgressionWorldSetup, setup_progression_world};

pub(super) fn run_primitive_progression_case(
    registries: &Registries,
    seed: u64,
    priority: PrimitivePriority,
    deferred_trace_refinement: bool,
    ore_opportunity_batch_budget: u64,
    emit_detail: bool,
) -> PrimitiveProgressionExperience {
    let ProgressionWorldSetup {
        mut state,
        mined_mass,
        soft_ore_deposit_mass,
        hard_ore_deposit_mass,
        ore_copper_ppm,
        hard_ore_copper_ppm,
        trace_copper_ppm,
        raw_surplus,
        stone_pick_batch_limit,
        stone_hardness_limit,
        reinforced_hardness_limit,
        hard_seam_hardness,
        pick_upgrade_native,
        crank_upgrade_native,
        concurrent_soft_mass,
        native_surplus,
        raw,
        shaped,
        ore_storage,
        hard_ore_storage,
        refined_clue_storage,
        native_storage,
        crushed_storage,
        separation_residue_storage,
        visible_clue_requests,
        soft_ore_target,
        hard_ore_target,
        native_target,
        trace_target,
        refined_clue_sample_mass,
    } = setup_progression_world(
        registries,
        seed,
        deferred_trace_refinement,
        ore_opportunity_batch_budget,
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| {
            panic!("primitive progression initial matter audit failed: {error}")
        })
        .total();
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("primitive progression survival state disappeared"));
    let ProgressionDiscovery {
        pick,
        prospecting_ticks,
        regional_recon_ticks,
        regional_upper_bounds_ppm,
        surface_prospecting_ticks,
        clue_count,
        surface_resolved_clues,
        hardness_sampling_ticks,
        mineable_clue_count,
        hardness_blocked_clue_count,
        hard_clue,
        direct_copper_clue,
        bulk_ore_clue,
        stone_mining_ticks,
        direct_copper_mining_ticks,
        direct_second_upgrade_blocked,
        bulk_sample,
        detailed_survey_ticks,
        refined_coarse_lower_ppm,
        refined_coarse_upper_ppm,
        refined_detailed_lower_ppm,
        refined_detailed_upper_ppm,
        refined_sample_copper_ppm,
        refined_sample_is_ore,
        actual_refined_sample_mass,
        refined_clue_mining_ticks,
        refinement_triggered_by_direct_shortage,
        processing_feed_selected_from_bulk,
        information_refinement_required,
    } = discover_primitive_progression(
        registries,
        &mut state,
        ProgressionDiscoveryPlan {
            raw,
            shaped,
            native_storage,
            ore_storage,
            refined_clue_storage,
            visible_clue_requests,
            soft_ore_target,
            hard_ore_target,
            native_target,
            trace_target,
            stone_hardness_limit,
            stone_pick_batch_limit,
            pick_upgrade_native,
            crank_upgrade_native,
            refined_clue_sample_mass,
            mined_mass,
            deferred_trace_refinement,
        },
    );
    let natural_priority = observed_primitive_priority(hard_clue, bulk_sample);
    let primitive_sorting = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("primitive native-copper sorting definition disappeared"));
    let primitive_sorting_recovery_ppm = primitive_sorting.target_recovery_ppm();
    assert!(
        primitive_sorting_recovery_ppm < COMPOSITION_PARTS_PER_MILLION,
        "primitive separator must leave some recoverable copper in its physical residue"
    );
    let soft_separation_feed_mass = primitive_sorting
        .minimum_homogeneous_feed_mass_for_target_recovery(
            crank_upgrade_native,
            bulk_sample.copper_ppm,
        )
        .unwrap_or_else(|| panic!("bulk ore cannot physically recover one crank reinforcement"));
    assert!(
        soft_separation_feed_mass <= mined_mass,
        "inventory-visible bulk ore must contain enough represented copper for the second upgrade"
    );
    let processing_decision_at = state.tick().value();
    // This is disclosed scenario demand, not a post-hoc payback horizon. The actor knows it wants
    // one second reinforcement plus a finite twelve-cycle crushed-feed stockpile before choosing
    // whether the larger processing-line investment is appropriate.
    let manual_stockpile_breaking_ticks =
        project_manual_stockpile_breaking_attention(registries, mined_mass);
    let manual_bridge_projection =
        project_owned_ore_manual_bridge(registries, bulk_sample.copper_ppm, crank_upgrade_native);
    let preaction_manual_processing_attention_ticks = manual_bridge_projection
        .total_attention_ticks
        .checked_add(manual_stockpile_breaking_ticks)
        .unwrap_or_else(|| panic!("primitive pre-action manual workload overflowed"));
    let processing_investment_projection = project_primitive_processing_investment(
        registries,
        &state,
        PrimitiveProcessingInvestmentPlan {
            raw,
            native_storage,
            shaped,
            mined_mass,
            separation_feed_mass: soft_separation_feed_mass,
            seed,
            crank_reinforced_before_charge: priority == PrimitivePriority::CrankFirst,
        },
    );
    assert!(
        processing_investment_projection.conservative_attention_ticks
            < preaction_manual_processing_attention_ticks,
        "the disclosed processing workload must justify mechanization from pre-action evidence: conservative machine upper bound {}t vs hand processing {}t",
        processing_investment_projection.conservative_attention_ticks,
        preaction_manual_processing_attention_ticks
    );
    // At this exact player-visible decision state, ordinary play has two legitimate ways to bridge
    // the missing second reinforcement. The choice above is already frozen from authored planning;
    // replay the manual route only as post-decision evaluator evidence and projection agreement.
    let manual_bridge = evaluate_owned_ore_manual_bridge(
        registries,
        &state,
        OwnedOreManualBridgePlan {
            ore_source: ore_storage,
            crushed_destination: crushed_storage,
            native_destination: hard_ore_storage,
            residue_destination: separation_residue_storage,
            shaped_destination: shaped,
            copper_ppm: bulk_sample.copper_ppm,
            reinforcement_required: crank_upgrade_native,
        },
    );
    assert!(
        manual_bridge.manual_recovery_ppm < manual_bridge.powered_recovery_ppm,
        "primitive mechanization must improve copper recovery over the real same-world hand route"
    );
    assert_eq!(manual_bridge.feed_mass, manual_bridge_projection.feed_mass);
    assert_eq!(
        manual_bridge.total_attention_ticks, manual_bridge_projection.total_attention_ticks,
        "pre-action manual bridge projection must match canonical execution"
    );
    assert_eq!(
        manual_bridge.manual_recovery_ppm,
        manual_bridge_projection.manual_recovery_ppm
    );
    assert_eq!(
        manual_bridge
            .total_attention_ticks
            .checked_sub(manual_bridge_projection.processing_attention_ticks),
        Some(manual_bridge_projection.cold_work_ticks),
        "manual bridge projection must partition processing and cold-work attention exactly"
    );
    let manual_bridge_ready_at = processing_decision_at
        .checked_add(manual_bridge.total_attention_ticks)
        .unwrap_or_else(|| panic!("primitive manual-bridge milestone overflowed"));

    // Replay the manual bootstrap route on a clone; the infrastructure-first episode below is unchanged.
    let mut manual_bootstrap_state = state.clone();
    reinforce_pick(
        registries,
        &mut manual_bootstrap_state,
        raw,
        native_storage,
        shaped,
        pick,
    );
    let manual_bootstrap_pick_ready_ticks = manual_bootstrap_state
        .tick()
        .value()
        .checked_sub(processing_decision_at)
        .unwrap_or_else(|| unreachable!("manual bootstrap pick cannot precede its decision state"));
    let _manual_bootstrap_mining_ticks = mine_and_claim(
        registries,
        &mut manual_bootstrap_state,
        hard_clue.request,
        hard_ore_storage,
        pick,
        mined_mass,
    );
    let manual_bootstrap_hard_sample_ticks = manual_bootstrap_state
        .tick()
        .value()
        .checked_sub(processing_decision_at)
        .unwrap_or_else(|| {
            unreachable!("manual bootstrap hard sample cannot precede its decision state")
        });
    let manual_bootstrap_hard_sample = observe_material_sample(
        &manual_bootstrap_state,
        hard_ore_storage,
        "manual-bootstrap hard-seam ore",
    );
    let manual_bootstrap_selected_hard_feed =
        manual_bootstrap_hard_sample.copper_ppm > bulk_sample.copper_ppm;
    let (manual_bootstrap_feed_source, manual_bootstrap_feed_ppm) =
        if manual_bootstrap_selected_hard_feed {
            (hard_ore_storage, manual_bootstrap_hard_sample.copper_ppm)
        } else {
            (ore_storage, bulk_sample.copper_ppm)
        };
    let manual_bootstrap_separation_feed_mass = primitive_sorting
        .minimum_homogeneous_feed_mass_for_target_recovery(
            crank_upgrade_native,
            manual_bootstrap_feed_ppm,
        )
        .unwrap_or_else(|| {
            panic!("manual bootstrap feed cannot physically recover one crank reinforcement")
        });
    let manual_bootstrap_bridge = run_owned_ore_manual_bridge(
        registries,
        &mut manual_bootstrap_state,
        OwnedOreManualBridgePlan {
            ore_source: manual_bootstrap_feed_source,
            crushed_destination: crushed_storage,
            native_destination: native_storage,
            residue_destination: separation_residue_storage,
            shaped_destination: shaped,
            copper_ppm: manual_bootstrap_feed_ppm,
            reinforcement_required: crank_upgrade_native,
        },
    );
    if manual_bootstrap_selected_hard_feed {
        assert!(
            manual_bootstrap_bridge.feed_mass < manual_bridge.feed_mass,
            "a better observed hard-seam assay must reduce the feed required for the same reinforcement"
        );
        assert!(
            manual_bootstrap_bridge.total_attention_ticks <= manual_bridge.total_attention_ticks,
            "a smaller better-grade manual feed cannot require more hand-processing attention"
        );
    } else {
        assert_eq!(
            manual_bootstrap_bridge.total_attention_ticks, manual_bridge.total_attention_ticks,
            "unchanged feed selection must preserve the matched hand-processing duration"
        );
    }
    let manual_bootstrap_second_ready_ticks = manual_bootstrap_state
        .tick()
        .value()
        .checked_sub(processing_decision_at)
        .unwrap_or_else(|| {
            unreachable!("manual bootstrap second copper cannot precede its decision state")
        });
    let manual_bootstrap_machine = build_primitive_machine(
        registries,
        &mut manual_bootstrap_state,
        PrimitiveMachineBuildPlan {
            raw,
            native_storage,
            shaped,
            mined_mass,
            separation_feed_mass: manual_bootstrap_separation_feed_mass,
            seed,
        },
    );
    reinforce_crank(
        registries,
        &mut manual_bootstrap_state,
        raw,
        native_storage,
        shaped,
        manual_bootstrap_machine.crank,
    );
    let manual_bootstrap_machine = PrimitiveMachine {
        crank_reinforced: true,
        ..manual_bootstrap_machine
    };
    let _manual_bootstrap_machine = charge_primitive_machine(
        registries,
        &mut manual_bootstrap_state,
        manual_bootstrap_machine,
    );
    let manual_bootstrap_machine_ready_ticks = manual_bootstrap_state
        .tick()
        .value()
        .checked_sub(processing_decision_at)
        .unwrap_or_else(|| {
            unreachable!("manual bootstrap machine cannot precede its decision state")
        });
    validate_loaded_state(registries, &manual_bootstrap_state)
        .unwrap_or_else(|error| panic!("manual-bootstrap progression state audit failed: {error}"));

    let mut hard_sample_copper_ppm = None;
    let (
        mut machine,
        mut reinforced_mining_ticks,
        concurrent_work,
        concurrent_task,
        mut pick_upgraded_at,
        mut hard_seam_accessed_at,
        first_upgrade_at,
        hard_ore_before_convergence,
        initial_crank_reinforced,
        selected_processing_feed_copper_ppm,
        selected_processing_feed_is_hard,
        selected_separation_feed_mass,
    ) = match priority {
        PrimitivePriority::PickFirst => {
            reinforce_pick(registries, &mut state, raw, native_storage, shaped, pick);
            let pick_upgraded_at = state.tick().value();
            let reinforced_mining_ticks = mine_and_claim(
                registries,
                &mut state,
                hard_clue.request,
                hard_ore_storage,
                pick,
                mined_mass,
            );
            let hard_seam_accessed_at = state.tick().value();
            let hard_sample = observe_material_sample(&state, hard_ore_storage, "hard-seam ore");
            assert_eq!(hard_sample.commodity.form(), FORM_ORE);
            hard_sample_copper_ppm = Some(hard_sample.copper_ppm);
            let hard_feed_is_better = hard_sample.copper_ppm > bulk_sample.copper_ppm;
            let (
                primary_source,
                selected_processing_feed_copper_ppm,
                selected_processing_feed_is_hard,
                separation_feed_mass,
            ) = if hard_feed_is_better {
                (
                    hard_ore_storage,
                    hard_sample.copper_ppm,
                    true,
                    primitive_sorting
                        .minimum_homogeneous_feed_mass_for_target_recovery(
                            crank_upgrade_native,
                            hard_sample.copper_ppm,
                        )
                        .unwrap_or_else(|| {
                            panic!(
                                "observed hard-seam feed cannot physically recover one crank reinforcement"
                            )
                        }),
                )
            } else {
                (
                    ore_storage,
                    bulk_sample.copper_ppm,
                    false,
                    soft_separation_feed_mass,
                )
            };
            // Pick-first means the scarce copper changes extraction before the player commits
            // hundreds of ticks to infrastructure. Build the processing line only after the
            // reinforced pick has exposed a real hard-seam sample, so the line can be sized from the
            // best feed the actor has actually observed.
            let base_machine = build_primitive_machine(
                registries,
                &mut state,
                PrimitiveMachineBuildPlan {
                    raw,
                    native_storage,
                    shaped,
                    mined_mass,
                    separation_feed_mass,
                    seed,
                },
            );
            let machine = charge_primitive_machine(registries, &mut state, base_machine);
            let concurrent_work = crush_while_mining(
                registries,
                &mut state,
                primary_source,
                crushed_storage,
                machine,
                CrushingBatch {
                    mass: mined_mass,
                    expected_energy: machine.required_energy,
                },
                ConcurrentMiningPlan {
                    target: bulk_ore_clue.request,
                    destination: ore_storage,
                    pick,
                    mass: concurrent_soft_mass,
                },
            )
            .unwrap_or_else(|error| {
                panic!("primitive progression primary crushing failed: {error}")
            });
            (
                machine,
                Some(reinforced_mining_ticks),
                concurrent_work,
                "best-mineable-bulk-copper-with-reinforced-pick",
                Some(pick_upgraded_at),
                Some(hard_seam_accessed_at),
                pick_upgraded_at,
                mined_mass,
                false,
                selected_processing_feed_copper_ppm,
                selected_processing_feed_is_hard,
                separation_feed_mass,
            )
        }
        PrimitivePriority::CrankFirst => {
            // Crank-first deliberately commits to the already-owned bulk feed before buying access
            // to a seam whose acquired grade interval has not yet justified scarce-copper access.
            let mut machine = build_primitive_machine(
                registries,
                &mut state,
                PrimitiveMachineBuildPlan {
                    raw,
                    native_storage,
                    shaped,
                    mined_mass,
                    separation_feed_mass: soft_separation_feed_mass,
                    seed,
                },
            );
            reinforce_crank(
                registries,
                &mut state,
                raw,
                native_storage,
                shaped,
                machine.crank,
            );
            let first_upgrade_at = state.tick().value();
            machine = PrimitiveMachine {
                crank_reinforced: true,
                ..machine
            };
            machine = charge_primitive_machine(registries, &mut state, machine);
            let concurrent_work = crush_while_mining(
                registries,
                &mut state,
                ore_storage,
                crushed_storage,
                machine,
                CrushingBatch {
                    mass: mined_mass,
                    expected_energy: machine.required_energy,
                },
                ConcurrentMiningPlan {
                    target: bulk_ore_clue.request,
                    destination: ore_storage,
                    pick,
                    mass: concurrent_soft_mass,
                },
            )
            .unwrap_or_else(|error| {
                panic!("primitive progression primary crushing failed: {error}")
            });
            (
                machine,
                None,
                concurrent_work,
                "best-mineable-bulk-copper-with-stone-pick",
                None,
                None,
                first_upgrade_at,
                Mass::ZERO,
                true,
                bulk_sample.copper_ppm,
                false,
                soft_separation_feed_mass,
            )
        }
    };

    let first_processed_output_at = concurrent_work
        .machine_started_at
        .checked_add(concurrent_work.crush_ticks)
        .unwrap_or_else(|| panic!("primitive processed-output milestone overflowed"));
    assert!(
        concurrent_work.overlap_ticks > 0
            || matches!(
                concurrent_work.autonomous_stop,
                AutonomousWorkStop::FeedBufferReady | AutonomousWorkStop::FeedBufferCapacity
            ),
        "primary autonomous crushing must expose concurrent work or an already replenished feed buffer"
    );

    let primary_player_free_ticks =
        finish_autonomous_crush(registries, &mut state, concurrent_work);
    let machine_useful_overlap_ticks = concurrent_work.overlap_ticks;
    assert_eq!(
        machine_useful_overlap_ticks.checked_add(primary_player_free_ticks),
        Some(concurrent_work.crush_ticks),
        "primary crusher window must partition into productive overlap and unfilled autonomous time"
    );
    let separation = separate_native_copper(
        registries,
        &mut state,
        PrimitiveSeparationPlan {
            crushed_storage,
            native_storage,
            residue_storage: separation_residue_storage,
            machine,
            feed_mass: selected_separation_feed_mass,
            expected_target: crank_upgrade_native,
        },
    );
    let separation_completed_at = state.tick().value();
    assert!(
        separation_completed_at > first_processed_output_at,
        "second-upgrade copper must come from a real downstream operation after crusher output exists"
    );
    assert_eq!(separation.target_mass, crank_upgrade_native);
    assert!(
        direct_second_upgrade_blocked,
        "processed ore must remain necessary after the direct-copper follow-up action was rejected"
    );

    let second_upgrade_at = match priority {
        PrimitivePriority::PickFirst => {
            reinforce_crank(
                registries,
                &mut state,
                raw,
                native_storage,
                shaped,
                machine.crank,
            );
            let upgraded_at = state.tick().value();
            machine = PrimitiveMachine {
                crank_reinforced: true,
                ..machine
            };
            upgraded_at
        }
        PrimitivePriority::CrankFirst => {
            reinforce_pick(registries, &mut state, raw, native_storage, shaped, pick);
            let upgraded_at = state.tick().value();
            pick_upgraded_at = Some(upgraded_at);
            upgraded_at
        }
    };
    assert!(
        second_upgrade_at > first_upgrade_at,
        "the competing copper upgrades must remain a real sequencing decision"
    );
    assert!(
        second_upgrade_at > separation_completed_at,
        "the second reinforcement must be forged only after processed ore yields its copper input"
    );
    match priority {
        PrimitivePriority::PickFirst => {
            let pick_upgraded_at =
                pick_upgraded_at.unwrap_or_else(|| unreachable!("pick-first upgrades the pick"));
            let reinforced_mining_ticks = reinforced_mining_ticks
                .unwrap_or_else(|| unreachable!("pick-first mines the hard seam"));
            assert!(
                reinforced_mining_ticks < stone_mining_ticks,
                "copper pick reinforcement must save player-attention time on the maintained mining batch"
            );
            assert!(
                pick_upgraded_at < concurrent_work.machine_started_at,
                "pick-first must improve extraction before starting autonomous work"
            );
            assert!(!initial_crank_reinforced && machine.crank_reinforced);
        }
        PrimitivePriority::CrankFirst => {
            assert!(initial_crank_reinforced && machine.crank_reinforced);
            let pick_upgraded_at = pick_upgraded_at.unwrap_or_else(|| {
                panic!("crank-first counterfactual never acquired its second pick upgrade")
            });
            assert!(
                first_processed_output_at < pick_upgraded_at,
                "crank-first counterfactual must produce autonomous output before converging on the pick upgrade"
            );
            let ticks = mine_and_claim(
                registries,
                &mut state,
                hard_clue.request,
                hard_ore_storage,
                pick,
                mined_mass,
            );
            reinforced_mining_ticks = Some(ticks);
            hard_seam_accessed_at = Some(state.tick().value());
            let hard_sample = observe_material_sample(&state, hard_ore_storage, "hard-seam ore");
            assert_eq!(hard_sample.commodity.form(), FORM_ORE);
            hard_sample_copper_ppm = Some(hard_sample.copper_ppm);
        }
    }

    let hard_sample_copper_ppm = hard_sample_copper_ppm
        .unwrap_or_else(|| panic!("primitive progression never observed its accessible hard seam"));
    let post_convergence_mining_target_is_hard = hard_sample_copper_ppm > bulk_sample.copper_ppm;
    let post_convergence_mining_target = if post_convergence_mining_target_is_hard {
        hard_clue.request
    } else {
        bulk_ore_clue.request
    };
    let banked_energy = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| {
            panic!("primitive progression flywheel disappeared after primary crushing")
        });
    let planned_reserve_energy = machine
        .charge_energy
        .checked_sub(machine.required_energy)
        .and_then(|energy| energy.checked_sub(machine.separation_required_energy))
        .unwrap_or_else(|| {
            unreachable!(
                "charge is bounded below by primary crushing plus conservative separation energy"
            )
        });
    let separation_energy_saved = machine
        .separation_required_energy
        .checked_sub(separation.required_energy)
        .unwrap_or_else(|| {
            unreachable!("actual separation energy is bounded by the conservative charge plan")
        });
    let ideal_banked_energy = planned_reserve_energy
        .checked_add(separation_energy_saved)
        .unwrap_or_else(|| panic!("primitive ideal banked energy overflowed"));
    assert!(
        banked_energy < ideal_banked_energy,
        "nonzero flywheel drag must make elapsed time consume some otherwise banked work"
    );
    let flywheel_loss_before_reserve = ideal_banked_energy
        .checked_sub(banked_energy)
        .unwrap_or_else(|| unreachable!("passive loss cannot create stored work"));
    let reserve_recharge_ticks =
        fill_primitive_accumulator(registries, &mut state, machine, planned_reserve_energy)
            .unwrap_or_else(|error| panic!("primitive reserve recharge failed: {error}"));
    let drive_before_reserve = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("primitive flywheel disappeared before reserve crushing"));
    assert!(drive_before_reserve >= planned_reserve_energy);
    let reserve_work = crush_while_mining(
        registries,
        &mut state,
        ore_storage,
        crushed_storage,
        machine,
        CrushingBatch {
            mass: machine.reserve_mass,
            expected_energy: planned_reserve_energy,
        },
        ConcurrentMiningPlan {
            target: post_convergence_mining_target,
            destination: ore_storage,
            pick,
            mass: mined_mass,
        },
    )
    .unwrap_or_else(|error| panic!("primitive progression reserve crushing failed: {error}"));
    let reserve_player_free_ticks = finish_autonomous_crush(registries, &mut state, reserve_work);
    let reserve_useful_overlap_ticks = reserve_work.overlap_ticks;
    assert_eq!(
        reserve_useful_overlap_ticks.checked_add(reserve_player_free_ticks),
        Some(reserve_work.crush_ticks),
        "reserve crusher window must partition into productive overlap and unfilled autonomous time"
    );
    let drive_after_reserve = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| {
            panic!("primitive progression flywheel disappeared after reserve crushing")
        });
    assert!(
        drive_after_reserve
            <= drive_before_reserve
                .checked_sub(planned_reserve_energy)
                .unwrap_or_else(|| unreachable!("validated reserve energy exceeds stored work")),
        "follow-up work and passive drag must not create residual flywheel energy"
    );
    let required_steady_state_productive_ticks = machine
        .automation_preparation_ticks
        .saturating_sub(machine_useful_overlap_ticks)
        .saturating_sub(reserve_useful_overlap_ticks);
    // Compare the actual investment goal against speculative stockpiling from this same
    // observable state. The existing buffer may already fund upgrades; machine activity alone
    // is not a reason to postpone them. This is completion-cost evidence, not a policy oracle.
    let demand_decision_at = state.tick().value();
    let survival_at_decision = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("selected progression decision survival state disappeared"));
    let reinvestment_plan = MatureReinvestmentPlan {
        raw,
        shaped,
        ore_storage,
        crushed_storage,
        native_storage,
        residue_storage: separation_residue_storage,
        machine,
        pick,
        mining_target: post_convergence_mining_target,
        primary_batch_mass: mined_mass,
        separation_feed_mass: selected_separation_feed_mass,
        reinforcement_mass: crank_upgrade_native,
    };
    // Freeze the authored three-upgrade goal and observed feed sizing before either continuation runs.
    let mut stockpiling_state = state.clone();
    let reinvestment = run_mature_reinvestment(registries, &mut state, reinvestment_plan);
    let selected_survival = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("selected progression survival state disappeared"));
    let selected_end = PrimitiveSelectedEnd {
        decision_at: demand_decision_at,
        completed_at: state.tick().value(),
        crushed_mass: state
            .inventory()
            .get_stockpile(crushed_storage)
            .unwrap_or_else(|| panic!("selected crushed stockpile disappeared"))
            .stored_mass(),
        native_copper: state
            .inventory()
            .get_stockpile(native_storage)
            .unwrap_or_else(|| panic!("selected native stockpile disappeared"))
            .get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)),
        pick_condition_ppm: state
            .equipment()
            .get_equipment(pick)
            .unwrap_or_else(|| panic!("selected pick disappeared"))
            .condition()
            .parts_per_million(),
        crusher_reinforced: state
            .equipment()
            .get_equipment(machine.crusher)
            .unwrap_or_else(|| panic!("selected crusher disappeared"))
            .definition()
            == EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        separator_reinforced: state
            .equipment()
            .get_equipment(machine.separator)
            .unwrap_or_else(|| panic!("selected separator disappeared"))
            .definition()
            == EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        drive_reinforced: state
            .energy()
            .get_store(machine.drive)
            .unwrap_or_else(|| panic!("selected drive disappeared"))
            .definition()
            == ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        metabolic_energy_spent_nj: survival_at_decision.metabolic_energy().nanojoules()
            - selected_survival.metabolic_energy().nanojoules(),
        hydration_spent_ul: survival_at_decision.hydration().microliters()
            - selected_survival.hydration().microliters(),
    };
    if let PrimitiveReinvestmentOutcome::Completed(work) = &reinvestment {
        assert_eq!(
            duration(demand_decision_at, state.tick().value()),
            work.elapsed_ticks
        );
        assert!(
            selected_end.crusher_reinforced
                && selected_end.separator_reinforced
                && selected_end.drive_reinforced
        );
    }
    let steady_state = run_steady_state_crushing(
        registries,
        &mut stockpiling_state,
        SteadyStateCrushingPlan {
            ore_storage,
            crushed_storage,
            machine,
            concurrent: ConcurrentMiningPlan {
                target: post_convergence_mining_target,
                destination: ore_storage,
                pick,
                mass: mined_mass,
            },
            raw,
            native_storage,
            shaped,
            required_productive_ticks: required_steady_state_productive_ticks,
        },
    );
    // Only the coverage branch forces post-order component replacement. The selected
    // continuation above retains its worn pick and pays no unnecessary service cost.
    let component_service = service_reinforced_pick(
        registries,
        &mut stockpiling_state,
        raw,
        native_storage,
        shaped,
        pick,
        steady_state.maintenance_preparation_ticks,
    );
    assert!(
        steady_state.maintenance_preparation_overlap_ticks > 0,
        "delegated crusher time must overlap real maintenance preparation"
    );
    let stockpiling_delay_ticks = stockpiling_state.tick().value() - demand_decision_at;
    let stockpiling_reinvestment =
        evaluate_mature_reinvestment(registries, &stockpiling_state, reinvestment_plan);
    // Metrics below describe this coverage endpoint; primary continuation values live in selected_end.
    let state = stockpiling_state;
    let drive_remaining = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| {
            panic!("primitive progression flywheel disappeared after repeated crushing")
        });
    assert!(drive_remaining <= machine.drive_capacity);
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("primitive progression final survival state disappeared"));
    let total_ore_reserve = soft_ore_deposit_mass
        .checked_add(hard_ore_deposit_mass)
        .unwrap_or_else(|| panic!("primitive progression combined ore reserve overflowed"));
    let post_convergence_mined = reserve_work
        .mined_mass
        .checked_add(steady_state.mined_mass)
        .unwrap_or_else(|| panic!("primitive post-convergence mining accounting overflowed"));
    let hard_ore_mined = mined_mass
        .checked_add(if post_convergence_mining_target_is_hard {
            post_convergence_mined
        } else {
            Mass::ZERO
        })
        .unwrap_or_else(|| panic!("primitive hard-ore accounting overflowed"));
    let total_ore_mined = mined_mass
        .checked_add(concurrent_work.mined_mass)
        .and_then(|mass| mass.checked_add(mined_mass))
        .and_then(|mass| mass.checked_add(post_convergence_mined))
        .unwrap_or_else(|| panic!("primitive total-ore accounting overflowed"));
    let unmined_ore_reserve = total_ore_reserve
        .checked_sub(total_ore_mined)
        .unwrap_or_else(|| unreachable!("ore world fixture exceeds the actor's actual extraction"));
    assert!(!unmined_ore_reserve.is_zero() && !native_surplus.is_zero());
    assert!(survival_after.metabolic_energy() < survival_before.metabolic_energy());
    assert!(survival_after.hydration() < survival_before.hydration());
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "primitive progression final matter audit failed: {error}"
            ))
            .total(),
        matter_before
    );
    let processed_mass = mined_mass
        .checked_add(machine.reserve_mass)
        .and_then(|mass| {
            mass.checked_add(multiply_mass(
                mined_mass,
                steady_state.cycles,
                "steady-state processed mass",
            ))
        })
        .unwrap_or_else(|| panic!("primitive progression processed mass overflowed"));
    let remaining_crushed_mass = processed_mass
        .checked_sub(separation.feed_mass)
        .unwrap_or_else(|| unreachable!("separation feed is bounded by primary crushed output"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(crushed_storage)
            .unwrap_or_else(|| panic!("primitive progression crushed storage disappeared"))
            .stored_mass(),
        remaining_crushed_mass
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(separation_residue_storage)
            .unwrap_or_else(|| panic!("primitive separation residue storage disappeared"))
            .stored_mass(),
        separation.residue_mass
    );
    assert_eq!(state.player_work().active(), None);
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("primitive progression persistence audit failed: {error}"));

    let drive_mass = state
        .energy()
        .get_store(machine.drive)
        .unwrap_or_else(|| panic!("primitive progression constructed drive disappeared"))
        .embodied_mass();
    let crusher_mass = state
        .equipment()
        .get_equipment(machine.crusher)
        .unwrap_or_else(|| panic!("primitive progression constructed crusher disappeared"))
        .embodied_mass();
    let separator_mass = state
        .equipment()
        .get_equipment(machine.separator)
        .unwrap_or_else(|| panic!("primitive progression constructed separator disappeared"))
        .embodied_mass();
    let final_pick_condition_ppm = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("primitive progression final pick disappeared"))
        .condition()
        .parts_per_million();
    let metabolic_energy_spent_nj = survival_before
        .metabolic_energy()
        .nanojoules()
        .checked_sub(survival_after.metabolic_energy().nanojoules())
        .unwrap_or_else(|| panic!("primitive progression cannot create metabolic reserve"));
    let hydration_spent_ul = survival_before
        .hydration()
        .microliters()
        .checked_sub(survival_after.hydration().microliters())
        .unwrap_or_else(|| panic!("primitive progression cannot create hydration reserve"));
    let physiology = registries.survival().physiology();
    let total_machine_work_ticks = concurrent_work
        .crush_ticks
        .checked_add(reserve_work.crush_ticks)
        .and_then(|ticks| ticks.checked_add(steady_state.machine_ticks))
        .unwrap_or_else(|| panic!("primitive autonomous-work duration overflowed"));
    let total_useful_overlap_ticks = machine_useful_overlap_ticks
        .checked_add(reserve_useful_overlap_ticks)
        .and_then(|ticks| ticks.checked_add(steady_state.useful_overlap_ticks))
        .unwrap_or_else(|| panic!("primitive useful-overlap duration overflowed"));
    let total_player_free_ticks = primary_player_free_ticks
        .checked_add(reserve_player_free_ticks)
        .and_then(|ticks| ticks.checked_add(steady_state.player_free_ticks))
        .unwrap_or_else(|| panic!("primitive player-free autonomous duration overflowed"));
    let total_charge_ticks = machine
        .charge_ticks
        .checked_add(separation.charge_ticks)
        .and_then(|ticks| ticks.checked_add(reserve_recharge_ticks))
        .and_then(|ticks| ticks.checked_add(steady_state.charge_ticks))
        .unwrap_or_else(|| panic!("primitive charging attention overflowed"));
    let actual_machine_assembly_ticks = machine
        .processing_line_preparation_ticks
        .checked_sub(machine.charge_ticks)
        .unwrap_or_else(|| unreachable!("machine charge is part of processing-line preparation"));
    assert_eq!(
        actual_machine_assembly_ticks, processing_investment_projection.assembly_attention_ticks,
        "pre-action processing-line assembly projection must match canonical execution"
    );
    assert!(
        machine.charge_ticks <= processing_investment_projection.initial_charge_ticks,
        "observed feed refinement cannot make the real initial charge exceed the conservative pre-action projection"
    );
    let manual_disclosed_work_attention = manual_bridge
        .total_attention_ticks
        .checked_add(manual_stockpile_breaking_ticks)
        .unwrap_or_else(|| panic!("primitive manual disclosed-work attention overflowed"));
    let mechanized_stockpile_player_ticks = machine
        .processing_line_preparation_ticks
        .checked_add(total_charge_ticks)
        .unwrap_or_else(|| panic!("primitive mechanized disclosed-work attention overflowed"));
    assert!(
        mechanized_stockpile_player_ticks < manual_disclosed_work_attention,
        "the disclosed twelve-cycle stockpile order no longer justifies infrastructure-first processing: machine player attention {mechanized_stockpile_player_ticks}t vs manual {manual_disclosed_work_attention}t"
    );
    assert!(
        mechanized_stockpile_player_ticks
            <= processing_investment_projection.conservative_attention_ticks,
        "real mechanized workload exceeded its conservative pre-action attention bound"
    );
    let experience = PrimitiveProgressionExperience {
        natural_priority,
        prospecting_ticks,
        regional_recon_ticks,
        regional_upper_bounds_ppm,
        surface_prospecting_ticks,
        hardness_sampling_ticks,
        detailed_survey_ticks,
        surface_clue_count: u8::try_from(clue_count)
            .unwrap_or_else(|_| unreachable!("primitive clue count fits u8")),
        surface_resolved_clue_count: surface_resolved_clues,
        information_refinement_required,
        refinement_triggered_by_direct_shortage,
        refined_coarse_lower_ppm,
        refined_coarse_upper_ppm,
        refined_detailed_lower_ppm,
        refined_detailed_upper_ppm,
        refined_sample_copper_ppm,
        refined_sample_is_ore,
        stone_mineable_clue_count: u8::try_from(mineable_clue_count)
            .unwrap_or_else(|_| unreachable!("primitive mineable clue count fits u8")),
        hardness_blocked_clue_count: u8::try_from(hardness_blocked_clue_count)
            .unwrap_or_else(|_| unreachable!("primitive blocked clue count fits u8")),
        direct_copper_evidence_lower_ppm: direct_copper_clue.lower_ppm,
        direct_copper_evidence_upper_ppm: direct_copper_clue.upper_ppm,
        bulk_ore_evidence_lower_ppm: bulk_ore_clue.lower_ppm,
        bulk_ore_evidence_upper_ppm: bulk_ore_clue.upper_ppm,
        hard_ore_evidence_lower_ppm: hard_clue.lower_ppm,
        hard_ore_evidence_upper_ppm: hard_clue.upper_ppm,
        bulk_sample_copper_ppm: bulk_sample.copper_ppm,
        processing_decision_at,
        manual_bridge_ready_at,
        manual_bridge_feed_mass: manual_bridge.feed_mass,
        manual_bridge_attention_ticks: manual_bridge.total_attention_ticks,
        preaction_manual_processing_attention_ticks,
        preaction_mechanized_attention_upper_ticks: processing_investment_projection
            .conservative_attention_ticks,
        preaction_machine_assembly_ticks: processing_investment_projection.assembly_attention_ticks,
        preaction_machine_initial_charge_ticks: processing_investment_projection
            .initial_charge_ticks,
        preaction_machine_repeated_charge_ticks: processing_investment_projection
            .repeated_charge_ticks,
        manual_stockpile_breaking_ticks,
        mechanized_stockpile_player_ticks,
        manual_bridge_recovery_ppm: manual_bridge.manual_recovery_ppm,
        manual_bootstrap_pick_ready_ticks,
        manual_bootstrap_hard_sample_ticks,
        manual_bootstrap_second_ready_ticks,
        manual_bootstrap_machine_ready_ticks,
        manual_bootstrap_selected_hard_feed,
        manual_bridge_metabolic_cost_nj: manual_bridge.metabolic_cost_nj,
        manual_bridge_hydration_cost_ul: manual_bridge.hydration_cost_ul,
        selected_processing_feed_copper_ppm,
        selected_processing_feed_is_hard,
        processing_feed_selected_from_bulk,
        post_convergence_mining_target_is_hard,
        refined_clue_sample_mass: actual_refined_sample_mass,
        refined_clue_mining_ticks,
        primary_batch_mass: mined_mass,
        first_upgrade_at,
        second_upgrade_at,
        pick_upgraded_at,
        hard_seam_accessed_at,
        machine_started_at: concurrent_work.machine_started_at,
        automation_preparation_ticks: machine.automation_preparation_ticks,
        separator_preparation_ticks: machine.separator_preparation_ticks,
        processing_line_preparation_ticks: machine.processing_line_preparation_ticks,
        processing_line_preparation_metabolic_cost_nj: machine.preparation_metabolic_cost_nj,
        processing_line_preparation_hydration_cost_ul: machine.preparation_hydration_cost_ul,
        overlap_setup_equivalent_cycles: steady_state.overlap_setup_equivalent_cycle,
        steady_state_cycles: steady_state.cycles,
        steady_state_stop: steady_state.stop,
        final_crusher_condition_ppm: steady_state.terminal_crusher_condition_ppm,
        initial_full_charge_ticks: machine.full_charge_ticks,
        first_processed_output_at,
        elapsed_ticks: state.tick().value(),
        soft_ore_mining_ticks: stone_mining_ticks,
        reinforced_mining_ticks,
        charge_ticks: total_charge_ticks,
        machine_work_ticks: total_machine_work_ticks,
        reserve_batch_mass: machine.reserve_mass,
        reserve_machine_work_ticks: reserve_work.crush_ticks,
        overlap_ticks: concurrent_work.overlap_ticks,
        machine_useful_overlap_ticks: total_useful_overlap_ticks,
        reserve_useful_overlap_ticks,
        machine_player_free_ticks: total_player_free_ticks,
        primary_autonomous_stop: concurrent_work.autonomous_stop,
        reserve_autonomous_stop: reserve_work.autonomous_stop,
        primary_mining_jobs: concurrent_work.mining_jobs,
        reserve_mining_jobs: reserve_work.mining_jobs,
        steady_mining_jobs: steady_state.mining_jobs,
        steady_feed_buffer_limited_cycles: steady_state.feed_buffer_limited_cycles,
        separation_feed_mass: separation.feed_mass,
        recovered_copper_mass: separation.target_mass,
        separation_required_energy: separation.required_energy,
        flywheel_loss_before_reserve,
        reserve_recharge_ticks,
        separation_ticks: separation.ticks,
        separation_completed_at,
        processed_output_enabled_second_upgrade: separation.target_mass == crank_upgrade_native
            && direct_second_upgrade_blocked
            && second_upgrade_at > separation_completed_at,
        hard_ore_mined,
        hard_ore_before_convergence,
        total_ore_mined,
        direct_second_upgrade_blocked,
        initial_crank_reinforced,
        crank_reinforced: machine.crank_reinforced,
        maintenance_material_preparation_ticks: component_service.preparation_ticks,
        maintenance_preparation_overlap_ticks: steady_state.maintenance_preparation_overlap_ticks,
        component_service_ticks: component_service.service_ticks,
        component_service_mass: component_service.material_mass,
        component_service_condition_before_ppm: component_service.condition_before_ppm,
        component_service_preserved_reinforcement: component_service.preserved_reinforcement,
        final_pick_condition_ppm,
        metabolic_energy_spent_nj,
        hydration_spent_ul,
        reinvestment,
        stockpiling_reinvestment,
        stockpiling_delay_ticks,
        selected_end,
    };
    let (first_upgrade, second_upgrade) = match priority {
        PrimitivePriority::PickFirst => ("pick", "hand-crank"),
        PrimitivePriority::CrankFirst => ("hand-crank", "pick"),
    };
    let pick_milestone = pick_upgraded_at
        .map(|tick| format!("{tick}t"))
        .unwrap_or_else(|| "not-acquired".to_string());
    let hard_seam_milestone = hard_seam_accessed_at
        .map(|tick| format!("{tick}t"))
        .unwrap_or_else(|| "locked".to_string());
    let selected_processing_feed = if selected_processing_feed_is_hard {
        "hard-seam"
    } else {
        "bulk"
    };

    if emit_detail && priority == natural_priority {
        let information_path = if information_refinement_required {
            "deferred-survey"
        } else {
            "surface-resolved"
        };
        println!(
            "PLAYABLE PROGRESSION seed=0x{seed:016X} branch={} local-copper-policy=pick-first-vs-crank world-bootstrap=[raw-gathered-matter-surplus:{}mg,visible-regional-geological-clue-zones+local-follow-up-regions,empty-storage] discovery=[path:{information_path} regional-recon:{}t regional-upper:[{},{}]ppm local-inspection:{}t hardness-sampling:{}t clues:{} coarse-resolved:{} refinement-triggered-by-direct-shortage:{} deferred-refinement:{}t alternative-bounds:{}..{}->{}..{}ppm alternative-sample:{}mg/{}t sample-observed:{} sample-grade:{}ppm bulk-grade:{}ppm evidence-persisted:true evidence-gated-target-resolution:true hidden-deposit-id:unavailable-to-actor] episode-scope=[current-primitive-route-actions-useful] canonical=recon-regional-clue-zones->prioritize-local-inspection->physically-sample-resolved-targets->classify-tool-fit-from-acquired-hardness->mine-best-bulk-feed->confirm-known-hardness-gate->mine-strongest-copper-clue->observe-native-metal->spend-local-copper-parcel-on-pick->sample-hard-seam->reassess-feed->build-processing-line->charge+autonomous-crush+mine-while-waiting->separate-crushed-ore->forge-crank-upgrade->repeat fantasy=read-world->infer-affordances->respond-to-constraints-with-information->survive->craft-tools->turn-scarce-matter-into-new-access->store-work->delegate-repetition->convert-processed-matter-into-next-capability",
            priority.label(),
            raw_surplus.milligrams(),
            regional_recon_ticks,
            regional_upper_bounds_ppm[0],
            regional_upper_bounds_ppm[1],
            surface_prospecting_ticks,
            hardness_sampling_ticks,
            clue_count,
            surface_resolved_clues,
            refinement_triggered_by_direct_shortage,
            detailed_survey_ticks,
            refined_coarse_lower_ppm,
            refined_coarse_upper_ppm,
            refined_detailed_lower_ppm,
            refined_detailed_upper_ppm,
            actual_refined_sample_mass.milligrams(),
            refined_clue_mining_ticks,
            refined_sample_is_ore,
            refined_sample_copper_ppm,
            bulk_sample.copper_ppm,
        );
        println!(
            "PROGRESSION DECISION observed-affordances=[sampled-mineable:{} hardness-blocked:{} strongest-copper:{}..{}ppm bulk-clue:{}..{}ppm strongest-output:native-metal direct-follow-up:insufficient-target-mass initial-processing-choice:[bulk:{}ppm alternative-sample:{}ppm sampled:{} selected:bulk] post-investment-feed:[source:{} grade:{}ppm]] sequence=[first:{}:{}mg@{}t second:{}:{}mg@{}t separated-copper:{}mg@{}t] milestones=[pick-upgrade:{} hard-access:{} machine-start:{}t first-crushed-output:{}t] tool-limits=[stone:{}Pa reinforced:{}Pa blocker-known-from-acquired-sample:true]",
            mineable_clue_count,
            hardness_blocked_clue_count,
            direct_copper_clue.lower_ppm,
            direct_copper_clue.upper_ppm,
            bulk_ore_clue.lower_ppm,
            bulk_ore_clue.upper_ppm,
            bulk_sample.copper_ppm,
            refined_sample_copper_ppm,
            refined_sample_is_ore,
            selected_processing_feed,
            selected_processing_feed_copper_ppm,
            first_upgrade,
            pick_upgrade_native.milligrams(),
            first_upgrade_at,
            second_upgrade,
            crank_upgrade_native.milligrams(),
            second_upgrade_at,
            separation.target_mass.milligrams(),
            separation_completed_at,
            pick_milestone,
            hard_seam_milestone,
            concurrent_work.machine_started_at,
            first_processed_output_at,
            stone_hardness_limit.pascals(),
            reinforced_hardness_limit.pascals(),
        );
        println!(
            "PROGRESSION SYSTEMS knowledge=[surface:{}t hardness-sampling:{}t deferred-refinement:{}t refined-extraction:{}mg/{}t] ore=[batch:{}mg stone-mining:{}t reinforced-mining:{:?} concurrent-bulk:{}mg total-mined:{}mg hard-before-convergence:{}mg hard-mined:{}mg remaining:{}mg] copper=[strongest-clue-mining:{}t direct-invested:{}mg direct-follow-up-blocked:{} separation-feed:{}mg recovered:{}mg residue:{}mg separation:{}t] infrastructure=[drive:{}mg crusher:{}mg separator:{}mg automation-preparation:{}t separator-preparation:{}t full-line-preparation:{}t] stored-work=[fill:{}ppm initial-charge:{}nJ primary-crush:{}nJ separation-plan:{}nJ separation-actual:{}nJ passive-loss-before-reserve:{}nJ reserve-recharge:{}t banked:{}nJ follow-up:{}mg:{}t steady-cycles:{} steady-stop:{} crusher-condition:{}ppm overlap-setup-equivalent:{:?} steady-charge:{}t final:{}nJ] charge=[crank-reinforced-initial:{} final:{} full-accumulator:{}t initial:{}t total:{}t] mechanization=[primary:{}t concurrent-plan:{} work:{}t jobs:{} mined:{}mg stop:{} initial-overlap:{}t primary-feed-replenishment-overlap:{}t primary-unfilled:{}t reserve:{}t reserve-mining:{}t/{}jobs stop:{} reserve-feed-replenishment-overlap:{}t reserve-unfilled:{}t steady-machine:{}t steady-mining:{}jobs buffer-limited:{}cycles steady-feed-replenishment-overlap:{}t steady-unfilled:{}t total-feed-replenishment-overlap:{}t total-unfilled:{}t crushed-total:{}mg crushed-remaining:{}mg] durability=[pick-service:condition:{}->{}ppm component:{}mg prep:{}t service:{}t reinforcement-preserved:{}] survival=[spent:{}nJ/{}uL remaining:{}nJ/{}uL warning:{}nJ/{}uL state:{:?}/{:?} elapsed:{}t] matter=conserved",
            surface_prospecting_ticks,
            hardness_sampling_ticks,
            detailed_survey_ticks,
            refined_clue_sample_mass.milligrams(),
            refined_clue_mining_ticks,
            mined_mass.milligrams(),
            stone_mining_ticks,
            reinforced_mining_ticks,
            concurrent_soft_mass.milligrams(),
            total_ore_mined.milligrams(),
            hard_ore_before_convergence.milligrams(),
            hard_ore_mined.milligrams(),
            unmined_ore_reserve.milligrams(),
            direct_copper_mining_ticks,
            pick_upgrade_native.milligrams(),
            direct_second_upgrade_blocked,
            separation.feed_mass.milligrams(),
            separation.target_mass.milligrams(),
            separation.residue_mass.milligrams(),
            separation.ticks,
            drive_mass.milligrams(),
            crusher_mass.milligrams(),
            separator_mass.milligrams(),
            machine.automation_preparation_ticks,
            machine.separator_preparation_ticks,
            machine.processing_line_preparation_ticks,
            machine.charge_fill_ppm,
            machine.charge_energy.nanojoules(),
            machine.required_energy.nanojoules(),
            machine.separation_required_energy.nanojoules(),
            separation.required_energy.nanojoules(),
            flywheel_loss_before_reserve.nanojoules(),
            reserve_recharge_ticks,
            banked_energy.nanojoules(),
            machine.reserve_mass.milligrams(),
            reserve_work.crush_ticks,
            steady_state.cycles,
            steady_state.stop.label(),
            steady_state.terminal_crusher_condition_ppm,
            steady_state.overlap_setup_equivalent_cycle,
            steady_state.charge_ticks,
            drive_remaining.nanojoules(),
            initial_crank_reinforced,
            machine.crank_reinforced,
            machine.full_charge_ticks,
            machine.charge_ticks,
            total_charge_ticks,
            concurrent_work.crush_ticks,
            concurrent_task,
            concurrent_work.player_work_ticks,
            concurrent_work.mining_jobs,
            concurrent_work.mined_mass.milligrams(),
            concurrent_work.autonomous_stop.label(),
            concurrent_work.overlap_ticks,
            machine_useful_overlap_ticks,
            primary_player_free_ticks,
            reserve_work.crush_ticks,
            reserve_work.player_work_ticks,
            reserve_work.mining_jobs,
            reserve_work.autonomous_stop.label(),
            reserve_useful_overlap_ticks,
            reserve_player_free_ticks,
            steady_state.machine_ticks,
            steady_state.mining_jobs,
            steady_state.feed_buffer_limited_cycles,
            steady_state.useful_overlap_ticks,
            steady_state.player_free_ticks,
            total_useful_overlap_ticks,
            total_player_free_ticks,
            processed_mass.milligrams(),
            remaining_crushed_mass.milligrams(),
            component_service.condition_before_ppm,
            final_pick_condition_ppm,
            component_service.material_mass.milligrams(),
            component_service.preparation_ticks,
            component_service.service_ticks,
            component_service.preserved_reinforcement,
            metabolic_energy_spent_nj,
            hydration_spent_ul,
            survival_after.metabolic_energy().nanojoules(),
            survival_after.hydration().microliters(),
            physiology.hungry_below().nanojoules(),
            physiology.thirsty_below().microliters(),
            survival_after.hunger(),
            survival_after.hydration_state(),
            state.tick().value(),
        );
        println!(
            "PROGRESSION CONTROLLER-DIAGNOSTIC hidden-world=[bulk-grade:{}ppm hard-grade:{}ppm refined-grade:{}ppm blocked-target-hardness:{}Pa direct-unmined-surplus:{}mg] note=diagnostic-only-not-actor-input",
            ore_copper_ppm,
            hard_ore_copper_ppm,
            trace_copper_ppm,
            hard_seam_hardness.pascals(),
            native_surplus.milligrams(),
        );
    }
    experience
}
