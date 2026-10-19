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
    let (registries, state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
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
    let (registries, mut state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
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
fn knap_assemble_mine_loop_auto_settles_conserves_exclusivity_and_persists() {
    let registries = build_registries();
    let mut state = AppState::new();

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
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining copper deposit insertion failed: {error}"));
    admit_known_mining_player_at_deposit(
        &registries,
        &mut state,
        deposit,
        &[stone_source, shaped, ore_destination],
        &[],
    );

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
    let final_tick = final_tick
        .as_ref()
        .unwrap_or_else(|| panic!("mining work produced no tick outcome"));
    let mining_output = final_tick
        .mining_output()
        .unwrap_or_else(|| panic!("mining completion produced no automatic output landing"));
    assert_eq!(mining_output.job(), job);
    assert_eq!(
        mining_output.output().mass(),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(state.player_work().active(), None);
    assert!(state.mining().get_job(job).is_none());
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
            .unwrap_or_else(|| panic!("mining destination disappeared after completion"))
            .stored_mass(),
        Mass::from_milligrams(100_000)
    );
    let settled_energy = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("settled mining energy accounting failed: {error}"));
    assert_eq!(settled_energy.total(), Some(energy_before_mining));
    assert_eq!(
        settled_energy.mining_material_thermal(),
        crate::energy::PreciseEnergy::ZERO,
        "automatic settlement must transfer completed ore thermal ownership out of mining"
    );
    let completion_tick = state.tick();
    let destination = state
        .inventory()
        .get_stockpile(ore_destination)
        .unwrap_or_else(|| panic!("mining destination disappeared after automatic settlement"));
    assert_eq!(
        destination.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_ORE)),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(destination.reserved_inbound(), Mass::ZERO);
    let mined_lot = state
        .inventory()
        .lot_ids(ore_destination)
        .next()
        .and_then(|lot| state.inventory().get_lot(lot))
        .unwrap_or_else(|| panic!("automatically settled mining output lot disappeared"));
    assert_eq!(
        mined_lot.created_at(),
        completion_tick,
        "automatic settlement must preserve physical extraction time as provenance"
    );
    assert_eq!(
        mined_lot.latest_created_at(),
        completion_tick,
        "automatic settlement must not rewrite output provenance"
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
fn awaiting_mining_output_waits_for_destination_support_recovery() {
    let (registries, mut state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
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
    let support = complete_mining_into_blocked_settlement_state(
        &registries,
        &mut state,
        job,
        destination,
        "mining support recovery",
    );
    assert!(
        state
            .mining()
            .get_job(job)
            .is_some_and(MiningJobRecord::is_awaiting_settlement)
    );

    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("mining support-recovery unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining support-recovery unmount commit failed: {error}"));
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );
    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining support-recovery retry tick failed: {error}"));
    assert_eq!(
        outcome.mining_output().map(|output| output.job()),
        Some(job)
    );
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

#[test]
fn blocked_older_mining_output_does_not_starve_independent_settlement() {
    let (registries, mut state, deposit, first_destination, pick) =
        ordinary_unstarted_mining_fixture();
    let mass = Mass::from_milligrams(100_000);
    let first = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        first_destination,
        pick,
        mass,
    )
    .unwrap_or_else(|error| panic!("first mining job admission failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("first mining job commit failed: {error}"));
    let _first_support = complete_mining_into_blocked_settlement_state(
        &registries,
        &mut state,
        first,
        first_destination,
        "older blocked mining output",
    );

    let second_destination = add_solid_stockpile_for_test(&mut state, mass)
        .unwrap_or_else(|error| panic!("second mining destination failed: {error}"));
    let position = state
        .logistics()
        .player()
        .map(|player| player.position())
        .unwrap_or_else(|| panic!("mining player disappeared"));
    validate_place_ground_stockpile(&state, second_destination, position)
        .unwrap_or_else(|error| panic!("second mining destination placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("second mining destination placement commit failed: {error}")
        });
    let second = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        second_destination,
        pick,
        mass,
    )
    .unwrap_or_else(|error| panic!("second mining job admission failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("second mining job commit failed: {error}"));
    let _second_support = complete_mining_into_blocked_settlement_state(
        &registries,
        &mut state,
        second,
        second_destination,
        "newer blocked mining output",
    );
    assert!(
        state
            .mining()
            .get_job(first)
            .is_some_and(MiningJobRecord::is_awaiting_settlement)
    );
    assert!(
        state
            .mining()
            .get_job(second)
            .is_some_and(MiningJobRecord::is_awaiting_settlement)
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("two-job matter baseline failed: {error}"))
        .total();
    let _ = validate_unmount_stockpile(&registries, &state, second_destination)
        .unwrap_or_else(|error| panic!("second destination unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("second destination unmount commit failed: {error}"));

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("independent mining settlement failed: {error}"));
    assert_eq!(
        outcome.mining_output().map(|receipt| receipt.job()),
        Some(second)
    );
    assert!(
        state
            .mining()
            .get_job(first)
            .is_some_and(MiningJobRecord::is_awaiting_settlement),
        "first blocked output must retain its reserved custody"
    );
    assert!(state.mining().get_job(second).is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(first_destination)
            .map(|pile| pile.reserved_inbound()),
        Some(mass)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(second_destination)
            .map(|pile| (pile.stored_mass(), pile.reserved_inbound())),
        Some((mass, Mass::ZERO))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("independent settlement matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("two-job continuation save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved)
        .unwrap_or_else(|error| panic!("two-job continuation decode failed: {error}"));
    let mut restored = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("two-job continuation validation failed: {error}"));
    assert_eq!(restored, state);
    let _ = validate_unmount_stockpile(&registries, &restored, first_destination)
        .unwrap_or_else(|error| panic!("first destination recovery failed: {error}"))
        .commit(&mut restored)
        .unwrap_or_else(|error| panic!("first destination recovery commit failed: {error}"));
    let recovered = advance_tick(&registries, &mut restored)
        .unwrap_or_else(|error| panic!("first output recovery tick failed: {error}"));
    assert_eq!(
        recovered.mining_output().map(|receipt| receipt.job()),
        Some(first)
    );
    assert!(restored.mining().get_job(first).is_none());
    assert_eq!(
        calculate_matter_accounting(&restored)
            .unwrap_or_else(|error| panic!("two-job recovery matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &restored), Ok(()));
}

fn blocked_unsettled_mining_with_local_stone(
    context: &'static str,
) -> (
    Registries,
    AppState,
    MiningJobId,
    StockpileId,
    StructuralElementId,
    crate::surface::SurfaceResourceId,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("{context} mining destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("{context} mining deposit failed: {error}"));
    let position = admit_known_mining_player_at_deposit(
        &registries,
        &mut state,
        deposit,
        &[destination],
        &[pick],
    );
    let stone = insert_surface_resource_for_test(
        &registries,
        &mut state,
        SurfaceResourceSpec::new(
            crate::surface::SurfaceResourceKind::Opportunity,
            position,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(500_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap_or_else(|error| panic!("{context} surface stone spec failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("{context} surface stone insertion failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("{context} mining start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("{context} mining start commit failed: {error}"));
    let support = complete_mining_into_blocked_settlement_state(
        &registries,
        &mut state,
        job,
        destination,
        context,
    );
    (registries, state, job, destination, support, stone)
}

#[test]
fn mining_settlement_retry_defers_to_due_surface_gathering_inventory_mutation() {
    let (registries, mut state, job, destination, _support, stone) =
        blocked_unsettled_mining_with_local_stone("mining retry versus gathering completion");
    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE,
        stone,
        Mass::from_milligrams(500_000),
    )
    .unwrap_or_else(|error| panic!("surface gathering start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("surface gathering start commit failed: {error}"));
    let completes_at = match state.player_work().active() {
        Some(PlayerWork::SurfaceGathering { work }) => work.completes_at(),
        other => panic!("surface gathering did not own player work: {other:?}"),
    };
    while state.tick().value() + 1 < completes_at.value() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("surface gathering setup tick failed: {error}"));
        assert_eq!(outcome.mining_output(), None);
    }
    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("mining retry destination unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining retry destination unmount commit failed: {error}"));

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("shared inventory completion tick failed: {error}"));
    assert!(outcome.surface_gathering().is_some());
    assert_eq!(outcome.mining_output(), None);
    assert!(
        state
            .mining()
            .get_job(job)
            .is_some_and(MiningJobRecord::is_awaiting_settlement)
    );

    let retry = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("deferred mining retry tick failed: {error}"));
    assert_eq!(retry.mining_output().map(|output| output.job()), Some(job));
    assert!(state.mining().get_job(job).is_none());
}

#[test]
fn mining_settlement_retry_defers_to_fatal_surface_gathering_cancellation() {
    let (registries, mut state, job, destination, _support, stone) =
        blocked_unsettled_mining_with_local_stone("mining retry versus gathering cancellation");
    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE,
        stone,
        Mass::from_milligrams(500_000),
    )
    .unwrap_or_else(|error| panic!("cancelable surface gathering start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("cancelable surface gathering commit failed: {error}"));
    let completes_at = match state.player_work().active() {
        Some(PlayerWork::SurfaceGathering { work }) => work.completes_at(),
        other => panic!("cancelable surface gathering did not own player work: {other:?}"),
    };
    assert!(
        completes_at.value() > state.tick().value() + 1,
        "fatal cancellation proof requires gathering unfinished after the next tick"
    );
    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("fatal retry destination unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("fatal retry destination unmount commit failed: {error}"));
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal cancellation tick failed: {error}"));
    assert_eq!(outcome.mining_output(), None);
    assert_eq!(state.player_work().active(), None);
    assert!(
        state
            .mining()
            .get_job(job)
            .is_some_and(MiningJobRecord::is_awaiting_settlement)
    );

    let retry = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("post-fatal mining retry tick failed: {error}"));
    assert_eq!(retry.mining_output().map(|output| output.job()), Some(job));
    assert!(state.mining().get_job(job).is_none());
}

#[cfg(feature = "test-soak")]
#[path = "gameplay/soak.rs"]
mod soak;
