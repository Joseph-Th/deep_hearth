//! One complete primitive progression episode executed through canonical runtime actions.

use super::*;

pub(super) fn run_primitive_progression_case(
    registries: &Registries,
    seed: u64,
    priority: PrimitivePriority,
    emit_detail: bool,
) -> PrimitiveProgressionExperience {
    let mined_mass = progression_mining_mass(registries, seed);
    let two_mining_batches = mined_mass
        .checked_add(mined_mass)
        .unwrap_or_else(|| panic!("primitive progression ore fixture mass overflowed"));
    let three_mining_batches = two_mining_batches
        .checked_add(mined_mass)
        .unwrap_or_else(|| panic!("primitive progression ore fixture mass overflowed"));
    let ore_storage_capacity = three_mining_batches;
    let crushed_storage_capacity = multiply_mass(
        mined_mass,
        MAX_STEADY_STATE_CRUSH_CYCLES + 2,
        "crushed-storage capacity",
    );
    let soft_ore_surplus = Mass::from_milligrams(
        1 + mix64(seed ^ 0x534F_4654_5F4F_5245) % mined_mass.milligrams().max(1),
    );
    let hard_ore_surplus = Mass::from_milligrams(
        mined_mass.milligrams().div_ceil(2)
            + mix64(seed ^ 0x4841_5244_5F4F_5245) % (mined_mass.milligrams() + 1),
    );
    let soft_ore_deposit_mass = three_mining_batches
        .checked_add(soft_ore_surplus)
        .unwrap_or_else(|| panic!("primitive progression soft-ore reserve mass overflowed"));
    let hard_ore_deposit_mass = multiply_mass(
        mined_mass,
        MAX_STEADY_STATE_CRUSH_CYCLES + 2,
        "hard-ore reserve",
    )
    .checked_add(hard_ore_surplus)
    .unwrap_or_else(|| panic!("primitive progression hard-ore reserve mass overflowed"));
    let ore_copper_ppm = 450_000 + (mix64(seed ^ 0x5052_4F47_4752_4144) % 300_001) as u32;
    let hard_ore_copper_ppm = 800_000 + (mix64(seed ^ 0x4841_5244_5F47_5244) % 100_001) as u32;
    assert!(
        hard_ore_copper_ppm > ore_copper_ppm,
        "hard-seam progression fixture must reward improved extraction capability with richer processable ore"
    );
    let trace_copper_ppm = 50_000 + (mix64(seed ^ 0x5452_4143_455F_4752) % 40_001) as u32;
    let refined_clue_sample_mass = Mass::from_milligrams(10_000);
    let PrimitiveMaterialPlan {
        raw_inputs,
        raw_capacity,
        shaped_capacity,
        native_copper: total_native_copper,
    } = primitive_material_plan(registries);
    let raw_seed_inputs = raw_inputs
        .into_iter()
        .enumerate()
        .map(|(index, (commodity, required))| {
            let maximum_extra = required.milligrams().div_ceil(2).max(1);
            let extra = Mass::from_milligrams(
                1 + mix64(seed ^ 0x5241_575F_5355_5250 ^ index as u64) % maximum_extra,
            );
            let seeded = required
                .checked_add(extra)
                .unwrap_or_else(|| panic!("primitive progression raw-material surplus overflowed"));
            (commodity, seeded)
        })
        .collect::<Vec<_>>();
    let raw_seed_capacity = raw_seed_inputs
        .iter()
        .try_fold(Mass::ZERO, |total, (_, mass)| total.checked_add(*mass))
        .unwrap_or_else(|| panic!("primitive progression raw-material capacity overflowed"));
    let raw_surplus = raw_seed_capacity
        .checked_sub(raw_capacity)
        .unwrap_or_else(|| unreachable!("seeded raw material includes every required input"));
    let stone_pick_batch_limit = stone_pick_mining_batch_limit(registries);
    let (stone_hardness_limit, reinforced_hardness_limit, hard_seam_hardness) =
        mining_hardness_limits(registries);
    let native_seam_hardness =
        Pressure::from_pascals(stone_hardness_limit.pascals().div_ceil(2).max(1));
    let pick_upgrade_native =
        native_input_for_upgrade(registries, EQUIPMENT_COPPER_REINFORCED_PICK);
    let crank_upgrade_native =
        native_input_for_upgrade(registries, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK);
    assert_eq!(
        pick_upgrade_native, crank_upgrade_native,
        "primitive competing copper upgrades must require the same scarce native-copper investment"
    );
    let two_upgrade_native = pick_upgrade_native
        .checked_add(crank_upgrade_native)
        .unwrap_or_else(|| panic!("primitive two-upgrade native-copper requirement overflowed"));
    assert_eq!(
        two_upgrade_native, total_native_copper,
        "primitive material plan must provision both sequential copper upgrades"
    );
    assert!(
        pick_upgrade_native.milligrams() > 1,
        "primitive scarce-copper episode requires a nontrivial upgrade parcel"
    );
    let concurrent_soft_mass = pick_upgrade_native;
    assert!(concurrent_soft_mass <= stone_pick_batch_limit);
    let native_surplus = Mass::from_milligrams(
        1 + mix64(seed ^ 0x4E41_5449_5645_5355) % (pick_upgrade_native.milligrams() - 1),
    );
    let native_deposit_mass = pick_upgrade_native
        .checked_add(native_surplus)
        .unwrap_or_else(|| panic!("primitive progression native-copper reserve overflowed"));

    let mut state = AppState::new(WorldSeed::new(seed));
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("primitive progression survival setup failed: {error}"));
    let raw = add_solid_stockpile(&mut state, raw_seed_capacity);
    let shaped = add_solid_stockpile(&mut state, shaped_capacity);
    let ore_storage = add_solid_stockpile(&mut state, ore_storage_capacity);
    let hard_ore_storage = add_solid_stockpile(&mut state, mined_mass);
    let refined_clue_storage = add_solid_stockpile(&mut state, refined_clue_sample_mass);
    let native_storage = add_solid_stockpile(&mut state, total_native_copper);
    let crushed_storage = add_solid_stockpile(&mut state, crushed_storage_capacity);
    let separation_residue_storage = add_solid_stockpile(&mut state, mined_mass);
    for (commodity, mass) in raw_seed_inputs {
        seed_lot(
            registries,
            &mut state,
            raw,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    let soft_ore_bounds = VoxelBounds::new(VoxelCoord::new(0, -4, 0), VoxelCoord::new(1, -3, 1))
        .unwrap_or_else(|error| panic!("primitive progression soft-ore bounds failed: {error}"));
    let ore_composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, ore_copper_ppm),
        CompositionComponent::new(MATERIAL_STONE, 1_000_000 - ore_copper_ppm),
    ])
    .unwrap_or_else(|error| panic!("primitive progression ore composition failed: {error}"));
    let hard_ore_composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, hard_ore_copper_ppm),
        CompositionComponent::new(MATERIAL_STONE, 1_000_000 - hard_ore_copper_ppm),
    ])
    .unwrap_or_else(|error| panic!("primitive progression hard-ore composition failed: {error}"));
    let _ = seed_geological_deposit(
        registries,
        &mut state,
        geological_deposit_spec(
            soft_ore_bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            soft_ore_deposit_mass,
            ROOM_TEMPERATURE,
            stone_hardness_limit,
            ore_composition.clone(),
        ),
    );
    let soft_ore_target = MiningTargetRequest::new(soft_ore_bounds, MATERIAL_COPPER);
    let hard_ore_bounds = VoxelBounds::new(VoxelCoord::new(2, -4, 0), VoxelCoord::new(3, -3, 1))
        .unwrap_or_else(|error| panic!("primitive progression hard-ore bounds failed: {error}"));
    let _ = seed_geological_deposit(
        registries,
        &mut state,
        geological_deposit_spec(
            hard_ore_bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            hard_ore_deposit_mass,
            ROOM_TEMPERATURE,
            hard_seam_hardness,
            hard_ore_composition,
        ),
    );
    let hard_ore_target = MiningTargetRequest::new(hard_ore_bounds, MATERIAL_COPPER);
    let native_bounds = VoxelBounds::new(VoxelCoord::new(4, -4, 0), VoxelCoord::new(5, -3, 1))
        .unwrap_or_else(|error| panic!("primitive native-copper bounds failed: {error}"));
    let _ = seed_geological_deposit(
        registries,
        &mut state,
        geological_deposit_spec(
            native_bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            native_deposit_mass,
            ROOM_TEMPERATURE,
            native_seam_hardness,
            MaterialComposition::pure(MATERIAL_COPPER),
        ),
    );
    let native_target = MiningTargetRequest::new(native_bounds, MATERIAL_COPPER);
    let trace_bounds = VoxelBounds::new(VoxelCoord::new(6, -4, 0), VoxelCoord::new(7, -3, 1))
        .unwrap_or_else(|error| panic!("primitive trace-copper bounds failed: {error}"));
    let trace_composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, trace_copper_ppm),
        CompositionComponent::new(MATERIAL_STONE, 1_000_000 - trace_copper_ppm),
    ])
    .unwrap_or_else(|error| panic!("primitive trace-copper composition failed: {error}"));
    let _ = seed_geological_deposit(
        registries,
        &mut state,
        geological_deposit_spec(
            trace_bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(100_000),
            ROOM_TEMPERATURE,
            stone_hardness_limit,
            trace_composition,
        ),
    );
    let trace_target = MiningTargetRequest::new(trace_bounds, MATERIAL_COPPER);
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| {
            panic!("primitive progression initial matter audit failed: {error}")
        })
        .total();
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("primitive progression survival state disappeared"));
    let clue_requests = [
        soft_ore_target,
        hard_ore_target,
        native_target,
        trace_target,
    ];
    for request in clue_requests {
        assert_eq!(
            resolve_mining_target(&state, request),
            Err(MiningTargetResolutionError::NoEvidence {
                material: MATERIAL_COPPER,
                region: request.region(),
            }),
            "hidden geological truth must not authorize mining before the player performs prospecting"
        );
    }
    let surface_prospecting_ticks = clue_requests
        .into_iter()
        .try_fold(0_u64, |total, region| {
            total.checked_add(inspect_local_copper_evidence(
                registries,
                &mut state,
                PROSPECTING_FIELD_INSPECTION,
                region.region(),
            ))
        })
        .unwrap_or_else(|| panic!("primitive progression surface-prospecting duration overflowed"));
    let mut surface_resolved_clues = 0_u8;
    let mut surface_clues = Vec::new();
    let mut refinement = None;
    for request in clue_requests {
        let (lower_ppm, upper_ppm) = observed_copper_bounds(&state, request);
        match resolve_mining_target(&state, request) {
            Ok(target) => {
                assert_eq!(target.region(), request.region());
                assert_eq!(target.material(), MATERIAL_COPPER);
                surface_clues.push(ObservedCopperClue {
                    request,
                    lower_ppm,
                    upper_ppm,
                });
                surface_resolved_clues = surface_resolved_clues
                    .checked_add(1)
                    .unwrap_or_else(|| panic!("surface clue count overflowed"));
            }
            Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region,
            }) if region == request.region() && lower_ppm == 0 => {
                assert!(
                    refinement.is_none(),
                    "primitive progression should expose one information-escalation decision at a time"
                );
                refinement = Some((request, lower_ppm, upper_ppm));
            }
            Err(error) => panic!("unexpected surface prospecting outcome: {error}"),
        }
    }
    assert_eq!(
        surface_resolved_clues, 3,
        "cheap inspection should immediately resolve most local copper clues"
    );
    let (refinement_request, refined_coarse_lower_ppm, refined_coarse_upper_ppm) = refinement
        .unwrap_or_else(|| panic!("primitive progression lost its ambiguous low-grade clue"));
    let information_refinement_required = true;
    assert_eq!(
        surface_prospecting_ticks,
        registries
            .labor()
            .get_prospecting(PROSPECTING_FIELD_INSPECTION)
            .map(|definition| definition.duration().value() * 4)
            .unwrap_or_else(|| panic!(
                "primitive progression field-inspection definition disappeared"
            )),
        "primitive progression must pay authored surface-inspection time for every visible clue region"
    );

    craft_for_profile(
        registries,
        &mut state,
        raw,
        native_storage,
        shaped,
        equipment_assembly_profile(registries, EQUIPMENT_STONE_PICK),
    );
    let pick = validate_assemble_equipment(registries, &state, EQUIPMENT_STONE_PICK, shaped)
        .unwrap_or_else(|error| panic!("primitive progression pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("primitive progression pick assembly commit failed: {error}")
        });
    let mut mineable_clues = Vec::new();
    let mut hardness_blocked_clues = Vec::new();
    for clue in surface_clues {
        match preview_stone_pick_mining(
            registries,
            &state,
            clue,
            ore_storage,
            pick,
            refined_clue_sample_mass,
        ) {
            Ok(()) => mineable_clues.push(clue),
            Err(MiningStartError::TargetTooHard { maximum }) => {
                assert_eq!(maximum, stone_hardness_limit);
                hardness_blocked_clues.push(clue);
            }
            Err(error) => panic!("unexpected observed mining affordance blocker: {error}"),
        }
    }
    assert_eq!(mineable_clues.len(), 2);
    assert_eq!(hardness_blocked_clues.len(), 1);
    let hard_clue = hardness_blocked_clues[0];
    let direct_copper_clue = strongest_observed_copper_clue(mineable_clues.iter().copied());
    let bulk_ore_clue = strongest_observed_copper_clue(
        mineable_clues
            .iter()
            .copied()
            .filter(|clue| clue.request != direct_copper_clue.request),
    );
    assert_eq!(
        refinement_request, trace_target,
        "controller fixture no longer produces the intended low-grade information escalation"
    );
    assert_eq!(
        direct_copper_clue.request, native_target,
        "strongest player-visible copper evidence no longer points at the direct-copper occurrence"
    );
    assert_eq!(
        bulk_ore_clue.request, soft_ore_target,
        "best remaining mineable copper evidence no longer points at the bulk processing feed"
    );
    assert_eq!(
        hard_clue.request, hard_ore_target,
        "canonical stone-pick preview no longer discovers the intended hardness gate"
    );
    let stone_mining_ticks = mine_and_claim(
        registries,
        &mut state,
        bulk_ore_clue.request,
        ore_storage,
        pick,
        mined_mass,
    );
    let blocked_hard_target = resolve_progression_mining_target(&state, hard_clue.request);
    assert_eq!(
        validate_start_mining(
            registries,
            &state,
            MINING_METHOD_HAND_PICK,
            blocked_hard_target,
            ore_storage,
            pick,
            mined_mass,
        )
        .err(),
        Some(MiningStartError::TargetTooHard {
            maximum: stone_hardness_limit,
        }),
        "the known hard seam must be a real blocked affordance before pick reinforcement"
    );
    let direct_copper_mining_ticks = mine_total_and_claim(
        registries,
        &mut state,
        direct_copper_clue.request,
        native_storage,
        pick,
        pick_upgrade_native,
        stone_pick_batch_limit,
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(native_storage)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL))
            }),
        Some(pick_upgrade_native),
        "the actor's strongest copper clue must reveal directly usable native metal only after extraction"
    );
    let direct_second_upgrade_blocked = matches!(
        validate_start_mining(
            registries,
            &state,
            MINING_METHOD_HAND_PICK,
            resolve_progression_mining_target(&state, direct_copper_clue.request),
            native_storage,
            pick,
            crank_upgrade_native,
        ),
        Err(MiningStartError::InsufficientTargetMass { requested })
            if requested == crank_upgrade_native
    );
    assert!(
        direct_second_upgrade_blocked,
        "the player must learn through the canonical mining action that the promising direct-copper occurrence cannot fund both upgrades"
    );
    let direct_supply_blocked_at = state.tick().value();
    assert!(
        matches!(
            resolve_mining_target(&state, refinement_request),
            Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget { .. })
        ),
        "the actor must defer unresolved geological work until the direct-copper shortage makes another occurrence relevant"
    );
    let refinement_started_at = state.tick().value();
    assert_eq!(
        refinement_started_at, direct_supply_blocked_at,
        "the actor should revisit unresolved evidence as the immediate response to exhausting the clear direct-copper option"
    );
    let detailed_survey_ticks = inspect_local_copper_evidence(
        registries,
        &mut state,
        PROSPECTING_DETAILED_FIELD_SURVEY,
        refinement_request.region(),
    );
    let refined_clue = observed_resolved_copper_clue(&state, refinement_request);
    assert!(
        refined_clue.lower_ppm > refined_coarse_lower_ppm
            && refined_clue.upper_ppm < refined_coarse_upper_ppm,
        "detailed survey must materially narrow the deferred ambiguous clue"
    );
    assert_eq!(
        detailed_survey_ticks,
        registries
            .labor()
            .get_prospecting(PROSPECTING_DETAILED_FIELD_SURVEY)
            .map(|definition| definition.duration().value())
            .unwrap_or_else(|| panic!(
                "primitive progression detailed-survey definition disappeared"
            )),
        "deferred ambiguity recovery must pay the authored refinement cost"
    );
    let refined_clue_mining_ticks = mine_and_claim(
        registries,
        &mut state,
        refined_clue.request,
        refined_clue_storage,
        pick,
        refined_clue_sample_mass,
    );
    let refined_sample =
        observe_single_material_sample(&state, refined_clue_storage, "refined clue");
    let bulk_sample = observe_single_material_sample(&state, ore_storage, "bulk ore");
    assert_eq!(
        refined_sample.commodity.form(),
        FORM_ORE,
        "the refined alternative must reveal its physical form only after extraction"
    );
    assert_eq!(
        bulk_sample.commodity.form(),
        FORM_ORE,
        "the maintained bulk occurrence must reveal processable ore after extraction"
    );
    assert!(
        bulk_sample.copper_ppm > refined_sample.copper_ppm,
        "the actor should choose the richer already-mined bulk ore after the deferred sample rules out another direct native source"
    );
    let processing_feed_selected_from_bulk = bulk_sample.commodity.form() == FORM_ORE
        && refined_sample.commodity.form() == FORM_ORE
        && bulk_sample.copper_ppm > refined_sample.copper_ppm;
    let soft_separation_feed_mass =
        feed_mass_for_exact_constituent(crank_upgrade_native, bulk_sample.copper_ppm);
    assert!(
        soft_separation_feed_mass <= mined_mass,
        "inventory-visible bulk ore must contain enough represented copper for the second upgrade"
    );
    let prospecting_ticks = surface_prospecting_ticks
        .checked_add(detailed_survey_ticks)
        .unwrap_or_else(|| panic!("primitive progression prospecting duration overflowed"));
    let refinement_triggered_by_direct_shortage = refinement_started_at == direct_supply_blocked_at;
    let base_machine = build_primitive_machine(
        registries,
        &mut state,
        raw,
        native_storage,
        shaped,
        mined_mass,
        soft_separation_feed_mass,
        seed,
    );
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
        PrimitivePriority::ExtractionFirst => {
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
            let hard_sample =
                observe_single_material_sample(&state, hard_ore_storage, "hard-seam ore");
            assert_eq!(hard_sample.commodity.form(), FORM_ORE);
            assert!(
                hard_sample.copper_ppm > bulk_sample.copper_ppm,
                "reinforced-pick access should reveal a richer processing opportunity than the easy seam"
            );
            let separation_feed_mass =
                feed_mass_for_exact_constituent(crank_upgrade_native, hard_sample.copper_ppm);
            let machine = charge_primitive_machine(registries, &mut state, base_machine);
            let concurrent_work = crush_while_mining(
                registries,
                &mut state,
                hard_ore_storage,
                crushed_storage,
                machine,
                mined_mass,
                machine.required_energy,
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
                hard_sample.copper_ppm,
                true,
                separation_feed_mass,
            )
        }
        PrimitivePriority::MechanizationFirst => {
            let mut machine = base_machine;
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
                mined_mass,
                machine.required_energy,
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
        state.tick().value() < first_processed_output_at,
        "primary autonomous crushing must leave player time for additional world interaction before output is ready"
    );

    let primary_player_free_ticks =
        finish_autonomous_crush(registries, &mut state, concurrent_work);
    let machine_useful_overlap_ticks = concurrent_work
        .crush_ticks
        .checked_sub(primary_player_free_ticks)
        .unwrap_or_else(|| {
            panic!("primitive machine player-free time exceeded active process time")
        });
    let separation = separate_native_copper(
        registries,
        &mut state,
        crushed_storage,
        native_storage,
        separation_residue_storage,
        machine,
        selected_separation_feed_mass,
        crank_upgrade_native,
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
        PrimitivePriority::ExtractionFirst => {
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
        PrimitivePriority::MechanizationFirst => {
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
        PrimitivePriority::ExtractionFirst => {
            let pick_upgraded_at = pick_upgraded_at
                .unwrap_or_else(|| unreachable!("extraction-first upgrades the pick"));
            let reinforced_mining_ticks = reinforced_mining_ticks
                .unwrap_or_else(|| unreachable!("extraction-first mines the hard seam"));
            assert!(
                reinforced_mining_ticks < stone_mining_ticks,
                "copper pick reinforcement must save player-attention time on the maintained mining batch"
            );
            assert!(
                pick_upgraded_at < concurrent_work.machine_started_at,
                "extraction-first priority must improve extraction before starting autonomous work"
            );
            assert!(!initial_crank_reinforced && machine.crank_reinforced);
        }
        PrimitivePriority::MechanizationFirst => {
            assert!(initial_crank_reinforced && machine.crank_reinforced);
            let pick_upgraded_at = pick_upgraded_at.unwrap_or_else(|| {
                panic!("mechanization-first never acquired its second pick upgrade")
            });
            assert!(
                first_processed_output_at < pick_upgraded_at,
                "mechanization-first must produce autonomous output before converging on the pick upgrade"
            );
            let ticks = mine_and_claim(
                registries,
                &mut state,
                hard_clue.request,
                ore_storage,
                pick,
                mined_mass,
            );
            reinforced_mining_ticks = Some(ticks);
            hard_seam_accessed_at = Some(state.tick().value());
        }
    }

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
    assert_eq!(
        banked_energy.checked_sub(planned_reserve_energy),
        Some(separation_energy_saved),
        "higher-grade feed must remain visible as conserved stored work after the same conservative charge"
    );
    let reserve_work = crush_while_mining(
        registries,
        &mut state,
        ore_storage,
        crushed_storage,
        machine,
        machine.reserve_mass,
        planned_reserve_energy,
        ConcurrentMiningPlan {
            target: hard_clue.request,
            destination: ore_storage,
            pick,
            mass: mined_mass,
        },
    )
    .unwrap_or_else(|error| panic!("primitive progression reserve crushing failed: {error}"));
    let reserve_player_free_ticks = finish_autonomous_crush(registries, &mut state, reserve_work);
    let reserve_useful_overlap_ticks = reserve_work
        .crush_ticks
        .checked_sub(reserve_player_free_ticks)
        .unwrap_or_else(|| panic!("reserve crusher player-free time exceeded active process time"));
    assert!(
        reserve_useful_overlap_ticks > 0,
        "banked mechanical work must support another cycle while the player performs useful extraction"
    );
    let drive_after_reserve = state
        .energy()
        .get_store(machine.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| {
            panic!("primitive progression flywheel disappeared after reserve crushing")
        });
    assert_eq!(
        drive_after_reserve, separation_energy_saved,
        "material-quality savings must survive the fixed follow-up crusher workload as reusable stored work"
    );
    let required_steady_state_free_ticks = machine
        .automation_preparation_ticks
        .saturating_sub(primary_player_free_ticks)
        .saturating_sub(reserve_player_free_ticks);
    let steady_state = run_steady_state_crushing(
        registries,
        &mut state,
        ore_storage,
        crushed_storage,
        machine,
        hard_clue.request,
        pick,
        mined_mass,
        required_steady_state_free_ticks,
    );
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
    let steady_mined = multiply_mass(mined_mass, steady_state.cycles, "steady-state mined");
    let hard_ore_mined = two_mining_batches
        .checked_add(steady_mined)
        .unwrap_or_else(|| panic!("primitive hard-ore accounting overflowed"));
    let total_ore_mined = three_mining_batches
        .checked_add(concurrent_soft_mass)
        .and_then(|mass| mass.checked_add(steady_mined))
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
        .and_then(|mass| mass.checked_add(steady_mined))
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
    let metabolic_energy_spent_nj = survival_before.metabolic_energy().nanojoules()
        - survival_after.metabolic_energy().nanojoules();
    let hydration_spent_ul =
        survival_before.hydration().microliters() - survival_after.hydration().microliters();
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
        .checked_add(steady_state.charge_ticks)
        .unwrap_or_else(|| panic!("primitive charging attention overflowed"));
    let experience = PrimitiveProgressionExperience {
        priority,
        prospecting_ticks,
        surface_prospecting_ticks,
        detailed_survey_ticks,
        surface_clue_count: u8::try_from(clue_requests.len())
            .unwrap_or_else(|_| unreachable!("primitive clue count fits u8")),
        surface_resolved_clue_count: surface_resolved_clues,
        information_refinement_required,
        refinement_triggered_by_direct_shortage,
        refined_coarse_lower_ppm,
        refined_coarse_upper_ppm,
        refined_detailed_lower_ppm: refined_clue.lower_ppm,
        refined_detailed_upper_ppm: refined_clue.upper_ppm,
        refined_sample_copper_ppm: refined_sample.copper_ppm,
        refined_sample_is_ore: refined_sample.commodity.form() == FORM_ORE,
        stone_mineable_clue_count: u8::try_from(mineable_clues.len())
            .unwrap_or_else(|_| unreachable!("primitive mineable clue count fits u8")),
        hardness_blocked_clue_count: u8::try_from(hardness_blocked_clues.len())
            .unwrap_or_else(|_| unreachable!("primitive blocked clue count fits u8")),
        direct_copper_evidence_lower_ppm: direct_copper_clue.lower_ppm,
        direct_copper_evidence_upper_ppm: direct_copper_clue.upper_ppm,
        bulk_ore_evidence_lower_ppm: bulk_ore_clue.lower_ppm,
        bulk_ore_evidence_upper_ppm: bulk_ore_clue.upper_ppm,
        bulk_sample_copper_ppm: bulk_sample.copper_ppm,
        selected_processing_feed_copper_ppm,
        selected_processing_feed_is_hard,
        processing_feed_selected_from_bulk,
        refined_clue_sample_mass,
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
        attention_payback_cycles: steady_state.payback_cycle,
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
        reserve_machine_work_ticks: reserve_work.crush_ticks,
        overlap_ticks: concurrent_work.overlap_ticks,
        machine_useful_overlap_ticks: total_useful_overlap_ticks,
        reserve_useful_overlap_ticks,
        machine_player_free_ticks: total_player_free_ticks,
        separation_feed_mass: separation.feed_mass,
        recovered_copper_mass: separation.target_mass,
        separation_required_energy: separation.required_energy,
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
        final_pick_condition_ppm,
        metabolic_energy_spent_nj,
        hydration_spent_ul,
    };
    let (first_upgrade, second_upgrade) = match priority {
        PrimitivePriority::ExtractionFirst => ("pick", "hand-crank"),
        PrimitivePriority::MechanizationFirst => ("hand-crank", "pick"),
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

    if emit_detail && std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        std::println!(
            "PLAYABLE PROGRESSION seed=0x{seed:016X} priority={} world-bootstrap=[raw-gathered-matter-surplus:{}mg,visible-local-geological-clue-regions,empty-storage] discovery=[surface-inspection:{}t clues:{} coarse-resolved:{} unresolved-deferred:true refinement-triggered-by-direct-shortage:{} detailed-survey:{}t refined-bounds:{}..{}->{}..{}ppm refined-sample:{}mg/{}t sample-form:ore sample-grade:{}ppm bulk-grade:{}ppm evidence-persisted:true evidence-gated-target-resolution:true hidden-deposit-id:unavailable-to-actor] episode-scope=[all-authored-manual-actions-useful] canonical=inspect-local-clues->act-on-resolved-evidence+defer-unresolved-clue->shape+assemble-pick->preview-resolved-mining-affordances->mine-best-bulk-feed->encounter-hardness-gate->mine-strongest-copper-clue->observe-native-metal->attempt-second-direct-parcel->observe-insufficient-target-mass->return-to-unresolved-clue->detailed-survey->extract-sample->observe-ore+lower-grade->choose-richer-current-feed->build-processing-line->choose-first-copper-upgrade:[pick|crank]->exercise-affordance+reassess-feed->charge+autonomous-crush+mine-while-waiting->separate-crushed-ore->forge-second-upgrade->converge->repeat fantasy=read-world->infer-affordances->respond-to-constraints-with-information->survive->craft-tools->sequence-competing-investments->turn-investment-into-affordance->store-work->delegate-repetition->convert-processed-matter-into-next-capability",
            priority.label(),
            raw_surplus.milligrams(),
            surface_prospecting_ticks,
            clue_requests.len(),
            surface_resolved_clues,
            refinement_triggered_by_direct_shortage,
            detailed_survey_ticks,
            refined_coarse_lower_ppm,
            refined_coarse_upper_ppm,
            refined_clue.lower_ppm,
            refined_clue.upper_ppm,
            refined_clue_sample_mass.milligrams(),
            refined_clue_mining_ticks,
            refined_sample.copper_ppm,
            bulk_sample.copper_ppm,
        );
        std::println!(
            "PROGRESSION DECISION observed-affordances=[surface-mineable:{} hardness-blocked:{} strongest-copper:{}..{}ppm bulk-clue:{}..{}ppm strongest-output:native-metal direct-follow-up:insufficient-target-mass initial-processing-choice:[bulk:{}ppm deferred-sample:{}ppm selected:bulk] post-investment-feed:[source:{} grade:{}ppm]] sequence=[first:{}:{}mg@{}t second:{}:{}mg@{}t separated-copper:{}mg@{}t] milestones=[pick-upgrade:{} hard-access:{} machine-start:{}t first-crushed-output:{}t] tool-limits=[stone:{}Pa reinforced:{}Pa blocker-discovered-by-validator:true]",
            mineable_clues.len(),
            hardness_blocked_clues.len(),
            direct_copper_clue.lower_ppm,
            direct_copper_clue.upper_ppm,
            bulk_ore_clue.lower_ppm,
            bulk_ore_clue.upper_ppm,
            bulk_sample.copper_ppm,
            refined_sample.copper_ppm,
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
        std::println!(
            "PROGRESSION SYSTEMS knowledge=[surface:{}t refinement:{}t refined-extraction:{}mg/{}t] ore=[batch:{}mg stone-mining:{}t reinforced-mining:{:?} concurrent-bulk:{}mg total-mined:{}mg hard-before-convergence:{}mg hard-mined:{}mg remaining:{}mg] copper=[strongest-clue-mining:{}t direct-invested:{}mg direct-follow-up-blocked:{} separation-feed:{}mg recovered:{}mg residue:{}mg separation:{}t] infrastructure=[drive:{}mg crusher:{}mg separator:{}mg automation-preparation:{}t separator-preparation:{}t full-line-preparation:{}t] stored-work=[fill:{}ppm initial-charge:{}nJ primary-crush:{}nJ separation-plan:{}nJ separation-actual:{}nJ banked:{}nJ follow-up:{}mg:{}t steady-cycles:{} steady-stop:{} crusher-condition:{}ppm automation-attention-break-even:{:?} steady-charge:{}t final:{}nJ] charge=[crank-reinforced-initial:{} final:{} full-accumulator:{}t initial:{}t total:{}t] mechanization=[primary:{}t concurrent-task:{}:{}t initial-overlap:{}t primary-productive-overlap:{}t primary-player-free:{}t reserve:{}t reserve-mining:{}t reserve-productive-overlap:{}t reserve-player-free:{}t steady-machine:{}t steady-productive-overlap:{}t steady-player-free:{}t total-productive-overlap:{}t total-player-free:{}t crushed-total:{}mg crushed-remaining:{}mg] durability=[pick:{}ppm] survival=[spent:{}nJ/{}uL remaining:{}nJ/{}uL warning:{}nJ/{}uL state:{:?}/{:?} elapsed:{}t] matter=conserved",
            surface_prospecting_ticks,
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
            banked_energy.nanojoules(),
            machine.reserve_mass.milligrams(),
            reserve_work.crush_ticks,
            steady_state.cycles,
            steady_state.stop.label(),
            steady_state.terminal_crusher_condition_ppm,
            steady_state.payback_cycle,
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
            concurrent_work.overlap_ticks,
            machine_useful_overlap_ticks,
            primary_player_free_ticks,
            reserve_work.crush_ticks,
            reserve_work.player_work_ticks,
            reserve_useful_overlap_ticks,
            reserve_player_free_ticks,
            steady_state.machine_ticks,
            steady_state.useful_overlap_ticks,
            steady_state.player_free_ticks,
            total_useful_overlap_ticks,
            total_player_free_ticks,
            processed_mass.milligrams(),
            remaining_crushed_mass.milligrams(),
            final_pick_condition_ppm,
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
        std::println!(
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
