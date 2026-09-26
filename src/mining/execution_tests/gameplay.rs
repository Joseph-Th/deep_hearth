//! Mining capability, survival, conservation, integration, and soak contracts.

use super::*;

#[test]
fn stone_pick_refuses_acquired_hardness_above_authored_capability() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("hardness survival initialization failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("hardness destination failed: {error}"));
    let bounds = VoxelBounds::new(VoxelCoord::new(8, -8, 0), VoxelCoord::new(9, -7, 1))
        .unwrap_or_else(|error| panic!("hardness bounds failed: {error}"));
    let deposit = insert_known_deposit(
        &registries,
        &mut state,
        GeneratedDepositSpec::new(
            bounds,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(100_000),
            Temperature::from_millikelvin(300_000),
            Pressure::from_pascals(700_000_000),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap_or_else(|error| panic!("hardness deposit fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("hardness deposit insertion failed: {error}"));

    let error = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .err()
    .unwrap_or_else(|| panic!("stone pick unexpectedly mined deposit above its hardness"));
    assert_eq!(
        error,
        MiningStartError::ExcavationHardnessEvidenceExceedsCapability {
            observed_upper: Pressure::from_pascals(700_000_000),
            maximum: Pressure::from_pascals(500_000_000),
        }
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("hardness deposit disappeared"))
            .remaining_mass(),
        Mass::from_milligrams(100_000)
    );
}

#[test]
fn visible_localized_target_can_be_tried_without_sampling_hardness_first() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("direct-attempt survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("direct-attempt destination failed: {error}"));
    let bounds = VoxelBounds::new(VoxelCoord::new(8, -8, 0), VoxelCoord::new(9, -7, 1))
        .unwrap_or_else(|error| panic!("direct-attempt bounds failed: {error}"));
    let _deposit = insert_surface_known_deposit(
        &registries,
        &mut state,
        GeneratedDepositSpec::new(
            bounds,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(100_000),
            Temperature::from_millikelvin(300_000),
            Pressure::from_pascals(400_000_000),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap_or_else(|error| panic!("direct-attempt deposit fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("direct-attempt deposit insertion failed: {error}"));
    let target = resolve_mining_target(&state, MiningTargetRequest::new(bounds, MATERIAL_STONE))
        .unwrap_or_else(|error| panic!("direct-attempt target failed: {error}"));
    assert_eq!(target.excavation_hardness(), None);
    let before = state.clone();

    let _validated = super::validate_start_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| {
        panic!("visible target should permit a direct mining attempt: {error}")
    });
    assert_eq!(
        state, before,
        "read-only direct-attempt admission must not mutate state"
    );
}

#[test]
fn unsampled_hard_target_reports_failed_tool_without_revealing_hidden_resistance() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("resistant-target survival setup failed: {error}"));
    let stone_pick = assemble_pick_for_test(&registries, &mut state);
    let reinforced_pick = assemble_reinforced_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("resistant-target destination failed: {error}"));
    let bounds = VoxelBounds::new(VoxelCoord::new(8, -8, 0), VoxelCoord::new(9, -7, 1))
        .unwrap_or_else(|error| panic!("resistant-target bounds failed: {error}"));
    let _deposit = insert_surface_known_deposit(
        &registries,
        &mut state,
        GeneratedDepositSpec::new(
            bounds,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(100_000),
            Temperature::from_millikelvin(300_000),
            Pressure::from_pascals(700_000_000),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap_or_else(|error| panic!("resistant-target deposit fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("resistant-target deposit insertion failed: {error}"));
    let target = resolve_mining_target(&state, MiningTargetRequest::new(bounds, MATERIAL_STONE))
        .unwrap_or_else(|error| panic!("resistant-target resolution failed: {error}"));
    assert_eq!(target.excavation_hardness(), None);
    let before = state.clone();

    let error = super::validate_start_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        stone_pick,
        Mass::from_milligrams(100_000),
    )
    .err()
    .unwrap_or_else(|| panic!("stone pick unexpectedly mined unsampled hard target"));
    assert_eq!(
        error,
        MiningStartError::TargetResistsEquipment {
            maximum: Pressure::from_pascals(500_000_000),
        }
    );
    assert!(
        !error.to_string().contains("700000000"),
        "failed direct attempt must not reveal exact hidden target hardness"
    );
    assert_eq!(state, before);

    let _validated = super::validate_start_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        reinforced_pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| {
        panic!("strong enough tool should mine the same visible target: {error}")
    });
}

#[test]
fn mining_requires_enough_hydration_reserve_to_finish() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining reserve survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining reserve destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining reserve deposit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining reserve serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["hydration"] = serde_json::json!(1_u64);
    let loaded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining low-hydration decode failed: {error}"));
    let low_reserve = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("mining low-hydration load failed: {error}"));
    let before = low_reserve.clone();

    assert!(matches!(
        validate_known_mining(
            &registries,
            &low_reserve,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        ),
        Err(MiningStartError::Work(
            PlayerWorkStartError::InsufficientHydration { .. }
        ))
    ));
    assert_eq!(low_reserve, before);
}

#[test]
fn active_mining_save_requires_enough_hydration_to_finish_remaining_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining save reserve survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining save reserve destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining save reserve deposit failed: {error}"));
    let token = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining save reserve start failed: {error}"));
    let job = token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining save reserve commit failed: {error}"));
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("mining save reserve job disappeared"));
    let remaining = record
        .completes_at()
        .checked_duration_since(state.tick())
        .unwrap_or_else(|| panic!("mining completion precedes current tick"));
    let exertion = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("mining save reserve method disappeared"))
        .exertion();
    let required = calculate_player_work_resource_budget(
        registries.survival().physiology(),
        exertion,
        remaining,
    )
    .unwrap_or_else(|error| panic!("mining save reserve budget failed: {error:?}"))
    .hydration();
    assert!(required > Volume::from_microliters(1));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining save reserve serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["hydration"] = serde_json::json!(1_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining save reserve decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::InsufficientHydration {
                available: Volume::from_microliters(1),
                required,
            }
        )))
    );
}

#[test]
fn copper_reinforcement_turns_cold_worked_native_metal_into_more_capable_extraction() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("reinforced mining survival setup failed: {error}"));
    let stone_pick = assemble_pick_for_test(&registries, &mut state);
    let reinforced_pick = assemble_reinforced_pick_for_test(&registries, &mut state);
    let reinforced_record = state
        .equipment()
        .get_equipment(reinforced_pick)
        .unwrap_or_else(|| panic!("reinforced pick disappeared after assembly"));
    assert_eq!(
        reinforced_record.embodied_mass(),
        Mass::from_milligrams(1_020_000)
    );
    assert!(reinforced_record.embodied_material().iter().any(|trace| {
        trace.profile().commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)
            && trace.mass() == Mass::from_milligrams(20_000)
    }));

    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(300_000))
        .unwrap_or_else(|error| panic!("reinforced mining destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("reinforced mining deposit failed: {error}"));
    let requested = Mass::from_milligrams(250_000);

    assert_eq!(
        validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            stone_pick,
            requested,
        )
        .err(),
        Some(MiningStartError::BatchTooLarge {
            maximum: Mass::from_milligrams(200_000),
            requested,
        })
    );

    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        reinforced_pick,
        requested,
    )
    .unwrap_or_else(|error| panic!("reinforced pick mining validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("reinforced pick mining commit failed: {error}"));
    let job_record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("reinforced mining job disappeared"));
    assert_eq!(
        job_record.completes_at().value() - job_record.started_at().value(),
        3
    );
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("reinforced mining deposit disappeared"))
            .remaining_mass(),
        Mass::from_milligrams(1_000_000)
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("reinforced mining state audit failed: {error}"));
}

#[test]
fn missing_mining_capability_reports_the_exact_authored_requirement() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("missing-capability survival setup failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("missing-capability destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("missing-capability deposit failed: {error}"));
    let hand_crank = assemble_hand_crank_for_test(&registries, &mut state);
    let expected_capability = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("hand-pick mining method disappeared"))
        .mass_flow_capability();

    let error = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        hand_crank,
        Mass::from_milligrams(1),
    )
    .err()
    .unwrap_or_else(|| panic!("hand crank unexpectedly satisfied hand-mining capabilities"));

    assert_eq!(
        error,
        MiningStartError::MissingCapability {
            capability: expected_capability,
        }
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("missing-capability deposit disappeared"))
            .remaining_mass(),
        Mass::from_milligrams(1_000_000)
    );
}

#[test]
fn knap_assemble_mine_claim_loop_is_conserved_exclusive_and_persistent() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining survival initialization failed: {error}"));

    let stone_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(3_000_000))
        .unwrap_or_else(|error| panic!("mining primitive-material source failed: {error}"));
    let shaped = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("mining shaped stockpile failed: {error}"));
    let ore_destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
            .unwrap_or_else(|error| panic!("mining ore destination failed: {error}"));
    let stone = deposit_lot_for_test(
        &registries,
        &mut state,
        stone_source,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(2_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("mining stone ingress failed: {error}"));
    let wood = deposit_lot_for_test(
        &registries,
        &mut state,
        stone_source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("mining handle wood ingress failed: {error}"));

    validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            stone_source,
            MaterialLotSelection::new(stone, Mass::from_milligrams(1_000_000)),
            shaped,
        ),
    )
    .unwrap_or_else(|error| panic!("mining knapping start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining knapping commit failed: {error}"));
    for _ in 0..40 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining knapping tick failed: {error}"));
    }
    validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_SHAPE_WOOD_HANDLE,
            stone_source,
            MaterialLotSelection::new(wood, Mass::from_milligrams(1_000_000)),
            shaped,
        ),
    )
    .unwrap_or_else(|error| panic!("mining handle shaping start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining handle shaping commit failed: {error}"));
    for _ in 0..40 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining handle shaping tick failed: {error}"));
    }

    let energy_before_assembly = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("pre-assembly energy accounting failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("pre-assembly energy total overflowed"));
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, shaped)
        .unwrap_or_else(|error| panic!("stone pick assembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stone pick assembly commit failed: {error}"));
    let energy_after_assembly = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("post-assembly energy accounting failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("post-assembly energy total overflowed"));
    assert_eq!(energy_after_assembly, energy_before_assembly);
    let pick_record = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("assembled stone pick disappeared"));
    assert_eq!(
        pick_record.embodied_mass(),
        Mass::from_milligrams(1_000_000)
    );
    assert_eq!(pick_record.embodied_material().len(), 2);
    assert!(pick_record.embodied_material().iter().any(|trace| {
        trace.profile().commodity() == CommodityKey::new(MATERIAL_STONE, FORM_TOOL)
            && trace.mass() == Mass::from_milligrams(800_000)
    }));
    assert!(pick_record.embodied_material().iter().any(|trace| {
        trace.profile().commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE)
            && trace.mass() == Mass::from_milligrams(200_000)
    }));

    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining copper deposit insertion failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("mining initial matter accounting failed: {error}"))
        .total();
    let energy_before_mining = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("mining initial energy accounting failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("mining initial energy total overflowed"));
    let survival_before_mining = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("mining survival state disappeared before work"));
    let pick_condition_before = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("mining pick disappeared before work"))
        .condition();

    let mining = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        ore_destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining start validation failed: {error}"));
    let job = mining
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining start commit failed: {error}"));
    let job_record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("mining job disappeared after start"));
    let pick_condition_after = job_record.equipment_condition_after();
    let mining_duration = job_record.completes_at().value() - job_record.started_at().value();
    assert!(pick_condition_after < pick_condition_before);
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .unwrap_or_else(|| panic!("mining pick disappeared after start"))
            .condition(),
        pick_condition_before
    );
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("mining deposit disappeared"))
            .remaining_mass(),
        Mass::from_milligrams(1_000_000)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ore_destination)
            .unwrap_or_else(|| panic!("mining destination disappeared"))
            .reserved_inbound(),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("mining WIP accounting failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("mining WIP energy accounting failed: {error}"))
            .total(),
        Some(energy_before_mining)
    );
    assert_eq!(
        state.player_work().active(),
        Some(PlayerWork::Mining { job })
    );

    let craft_error = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            stone_source,
            MaterialLotSelection::new(stone, Mass::from_milligrams(1_000_000)),
            shaped,
        ),
    )
    .err()
    .unwrap_or_else(|| panic!("manual crafting unexpectedly started during mining"));
    assert_eq!(
        craft_error,
        StartManualCraftError::Work(PlayerWorkStartError::Busy {
            active: Box::new(PlayerWork::Mining { job }),
        })
    );

    let mut final_tick = None;
    for _ in 0..mining_duration {
        final_tick = Some(
            advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("mining work tick failed: {error}")),
        );
    }
    assert_eq!(
        final_tick
            .as_ref()
            .unwrap_or_else(|| panic!("mining work produced no tick outcome"))
            .ready_mining_jobs(),
        &[job]
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("mining deposit disappeared after work"))
            .remaining_mass(),
        Mass::from_milligrams(900_000)
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .unwrap_or_else(|| panic!("mining pick disappeared after work"))
            .condition(),
        pick_condition_after
    );
    let survival_after_mining = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("mining survival state disappeared after work"));
    let physiology = registries.survival().physiology();
    let exertion = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("hand mining method disappeared"))
        .exertion();
    assert_eq!(
        survival_before_mining.metabolic_energy().nanojoules()
            - survival_after_mining.metabolic_energy().nanojoules(),
        (physiology.basal_energy_cost_per_tick().nanojoules()
            + exertion.energy_cost_per_tick().nanojoules())
            * u128::from(mining_duration)
    );
    assert_eq!(
        survival_before_mining.hydration().microliters()
            - survival_after_mining.hydration().microliters(),
        (physiology.hydration_loss_per_tick().microliters()
            + exertion.hydration_loss_per_tick().microliters())
            * mining_duration
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ore_destination)
            .unwrap_or_else(|| panic!("mining destination disappeared before claim"))
            .stored_mass(),
        Mass::ZERO
    );
    let ready_energy = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("ready mining energy accounting failed: {error}"));
    assert_eq!(ready_energy.total(), Some(energy_before_mining));
    assert!(
        !ready_energy.mining_material_thermal().is_zero(),
        "extracted ore must retain explicit thermal ownership while waiting to be claimed"
    );
    let completion_tick = state.tick();
    for _ in 0..3 {
        let delayed = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("delayed mining-claim tick failed: {error}"));
        assert!(delayed.ready_mining_jobs().is_empty());
        assert!(
            state
                .mining()
                .get_job(job)
                .is_some_and(MiningJobRecord::is_ready_to_claim),
            "completed mining output must remain durably mining-owned until claim"
        );
        assert_eq!(state.player_work().active(), None);
        assert_eq!(
            state
                .inventory()
                .get_stockpile(ore_destination)
                .map(|stockpile| stockpile.reserved_inbound()),
            Some(Mass::from_milligrams(100_000)),
            "delayed mining output must retain its destination capacity reservation"
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("delayed mining matter audit failed: {error}"))
                .total(),
            matter_before
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("delayed mining state audit failed: {error}"));
    }

    let delayed_encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("delayed mining save serialization failed: {error}"));
    let delayed_loaded: LoadedSaveEnvelope = serde_json::from_value(delayed_encoded)
        .unwrap_or_else(|error| panic!("delayed mining save decode failed: {error}"));
    let delayed_restored = delayed_loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("delayed mining save validation failed: {error}"));
    assert_eq!(delayed_restored, state);

    validate_claim_mining_output(&registries, &state, job)
        .unwrap_or_else(|error| panic!("mining claim validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining claim commit failed: {error}"));
    let destination = state
        .inventory()
        .get_stockpile(ore_destination)
        .unwrap_or_else(|| panic!("mining destination disappeared after claim"));
    assert_eq!(
        destination.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_ORE)),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(destination.reserved_inbound(), Mass::ZERO);
    let claimed_lot = state
        .inventory()
        .lot_ids(ore_destination)
        .next()
        .and_then(|lot| state.inventory().get_lot(lot))
        .unwrap_or_else(|| panic!("claimed mining output lot disappeared"));
    assert_eq!(
        claimed_lot.created_at(),
        completion_tick,
        "delayed claim must preserve physical extraction time as provenance"
    );
    assert_eq!(
        claimed_lot.latest_created_at(),
        completion_tick,
        "delayed claim must not rewrite output provenance to the later claim tick"
    );
    assert_eq!(
        claimed_lot
            .storage_history()
            .project(state.tick(), AMBIENT_PRESERVATION_MULTIPLIER_PPM,),
        Some(3 * STORAGE_AGE_PARTS_PER_TICK),
        "unclaimed output must accumulate ambient storage exposure before inventory admission"
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("claimed mining energy ownership audit failed: {error}"))
            .mining_material_thermal(),
        crate::energy::PreciseEnergy::ZERO,
        "claim must transfer all ready ore thermal ownership out of mining"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("mining final matter accounting failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("mining final energy accounting failed: {error}"))
            .total(),
        Some(energy_before_mining)
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("mining final state audit failed: {error}"));

    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining save serialization failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining save decode failed: {error}"));
    let restored = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("mining save validation failed: {error}"));
    assert_eq!(restored, state);
}

#[test]
fn ready_mining_output_waits_for_destination_support_recovery() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining support-recovery survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining support-recovery destination failed: {error}"));
    let support = active_stockpile_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("mining support-recovery mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining support-recovery mount commit failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining support-recovery deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining support-recovery start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining support-recovery start commit failed: {error}"));
    let duration = state
        .mining()
        .get_job(job)
        .and_then(|record| {
            record
                .completes_at()
                .value()
                .checked_sub(record.started_at().value())
        })
        .unwrap_or_else(|| panic!("mining support-recovery duration was invalid"));
    for _ in 0..duration {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining support-recovery work tick failed: {error}"));
    }
    assert!(
        state
            .mining()
            .get_job(job)
            .is_some_and(MiningJobRecord::is_ready_to_claim)
    );

    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("mining support-recovery overload failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining support-recovery overload commit failed: {error}"));
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );
    let blocked = state.clone();
    assert!(matches!(
        validate_claim_mining_output(&registries, &state, job),
        Err(MiningClaimError::StructuralLoad(
            StockpileStructuralLoadError::SupportNotActiveForIncrease {
                stockpile,
                element,
                lifecycle: StructuralLifecycle::Failed,
            }
        )) if stockpile == destination && element == support
    ));
    assert_eq!(state, blocked);

    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("mining support-recovery unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining support-recovery unmount commit failed: {error}"));
    validate_claim_mining_output(&registries, &state, job)
        .unwrap_or_else(|error| panic!("mining support-recovery claim failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining support-recovery claim commit failed: {error}"));
    assert!(state.mining().get_job(job).is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|record| (record.stored_mass(), record.reserved_inbound())),
        Some((Mass::from_milligrams(100_000), Mass::ZERO))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[cfg(feature = "test-soak")]
#[path = "gameplay/soak.rs"]
mod soak;
