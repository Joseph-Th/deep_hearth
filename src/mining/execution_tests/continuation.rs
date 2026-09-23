//! Mining target staleness, replay, persistence, and historical-physics contracts.

use super::*;

#[test]
fn resolved_mining_target_survives_unrelated_remote_geological_knowledge() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stale-target knowledge survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("stale-target knowledge destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("stale-target knowledge deposit failed: {error}"));
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("stale-target knowledge deposit disappeared"));
    let target = resolve_mining_target(
        &state,
        MiningTargetRequest::new(
            deposit_record.bounds(),
            deposit_record.commodity().material(),
        ),
    )
    .unwrap_or_else(|error| panic!("stale-target knowledge resolution failed: {error}"));
    let remote = VoxelBounds::new(VoxelCoord::new(100, -8, 0), VoxelCoord::new(101, -7, 1))
        .unwrap_or_else(|error| panic!("stale-target knowledge evidence bounds failed: {error}"));
    let estimate = MaterialAbundanceEstimate::new(MATERIAL_STONE, 1, 1_000_000)
        .unwrap_or_else(|error| panic!("stale-target knowledge estimate failed: {error}"));
    record_prospecting_for_test(
        &registries,
        &mut state,
        ProspectingResolution::new_for_fixture(
            remote,
            GeologicalEvidenceKind::SurfaceExposure,
            vec![estimate],
        ),
    )
    .unwrap_or_else(|error| panic!("stale-target knowledge evidence failed: {error}"));
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
    .unwrap_or_else(|error| panic!("remote knowledge should not stale local target: {error}"));
    assert_eq!(state, before);
}

#[test]
fn resolved_mining_target_is_invalidated_by_new_local_ambiguity() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("ambiguous-target survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("ambiguous-target destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("ambiguous-target deposit failed: {error}"));
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("ambiguous-target deposit disappeared"));
    let target = resolve_mining_target(
        &state,
        MiningTargetRequest::new(
            deposit_record.bounds(),
            deposit_record.commodity().material(),
        ),
    )
    .unwrap_or_else(|error| panic!("ambiguous-target initial resolution failed: {error}"));

    crate::geology::insert_generated_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("ambiguous-target second deposit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        super::validate_start_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            target,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::TargetNoLongerResolved)
    );
    assert_eq!(state, before);
}

#[test]
fn resolved_mining_target_is_invalidated_by_better_local_hardness_evidence() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("hardness-stale target survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("hardness-stale target destination failed: {error}"));
    let deposit = insert_surface_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("hardness-stale target deposit failed: {error}"));
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("hardness-stale target deposit disappeared"));
    let region = deposit_record.bounds();
    let material = deposit_record.commodity().material();
    record_local_hardness_evidence(
        &registries,
        &mut state,
        region,
        material,
        Pressure::from_pascals(300_000_000),
        Pressure::from_pascals(500_000_000),
    );
    let target = resolve_mining_target(&state, MiningTargetRequest::new(region, material))
        .unwrap_or_else(|error| panic!("hardness-stale target resolution failed: {error}"));
    record_local_hardness_evidence(
        &registries,
        &mut state,
        region,
        material,
        Pressure::from_pascals(340_000_000),
        Pressure::from_pascals(360_000_000),
    );
    let current = resolve_mining_target(&state, MiningTargetRequest::new(region, material))
        .unwrap_or_else(|error| panic!("hardness-stale target re-resolution failed: {error}"));
    assert_eq!(current.deposit, target.deposit);
    assert_ne!(current, target);
    let before = state.clone();

    assert_eq!(
        super::validate_start_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            target,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::TargetNoLongerResolved)
    );
    assert_eq!(state, before);
}

#[test]
fn validated_mining_start_is_invalidated_by_new_geological_knowledge() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stale-start knowledge survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("stale-start knowledge destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("stale-start knowledge deposit failed: {error}"));
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("stale-start knowledge deposit disappeared"));
    let deposit_bounds = deposit_record.bounds();
    let deposit_material = deposit_record.commodity().material();
    let target = resolve_mining_target(
        &state,
        MiningTargetRequest::new(deposit_bounds, deposit_material),
    )
    .unwrap_or_else(|error| panic!("stale-start knowledge target resolution failed: {error}"));
    let start = super::validate_start_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("stale-start knowledge mining validation failed: {error}"));
    let contradiction = MaterialAbundanceEstimate::new(MATERIAL_COPPER, 0, 0)
        .unwrap_or_else(|error| panic!("stale-start knowledge estimate failed: {error}"));
    record_prospecting_for_test(
        &registries,
        &mut state,
        ProspectingResolution::new_for_fixture(
            deposit_bounds,
            GeologicalEvidenceKind::SurfaceExposure,
            vec![contradiction],
        ),
    )
    .unwrap_or_else(|error| panic!("stale-start knowledge evidence failed: {error}"));
    let before = state.clone();

    assert_eq!(
        start.commit(&mut state),
        Err(MiningStartCommitError::TargetNoLongerResolved)
    );
    assert_eq!(state, before);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .map(|record| record.remaining_mass()),
        Some(Mass::from_milligrams(1_000_000))
    );
}

#[test]
fn validated_mining_start_rejects_hidden_reserve_change_without_disclosing_amounts() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("reserve-stale mining survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("reserve-stale mining destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("reserve-stale mining deposit failed: {error}"));
    let start = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("reserve-stale mining validation failed: {error}"));
    let geology_revision = state.geology().revision();
    state.geology_state_mut().apply_extraction(
        deposit,
        Mass::from_milligrams(100_000),
        geology_revision + 1,
    );
    let before = state.clone();

    assert_eq!(
        start.commit(&mut state),
        Err(MiningStartCommitError::TargetChanged)
    );
    assert_eq!(state, before);
    assert_eq!(state.player_work().active(), None);
}

#[test]
fn validated_mining_start_is_invalidated_by_better_local_hardness_evidence() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("hardness-stale start survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("hardness-stale start destination failed: {error}"));
    let deposit = insert_surface_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("hardness-stale start deposit failed: {error}"));
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("hardness-stale start deposit disappeared"));
    let region = deposit_record.bounds();
    let material = deposit_record.commodity().material();
    record_local_hardness_evidence(
        &registries,
        &mut state,
        region,
        material,
        Pressure::from_pascals(300_000_000),
        Pressure::from_pascals(500_000_000),
    );
    let target = resolve_mining_target(&state, MiningTargetRequest::new(region, material))
        .unwrap_or_else(|error| panic!("hardness-stale start target failed: {error}"));
    let start = super::validate_start_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("hardness-stale start validation failed: {error}"));
    record_local_hardness_evidence(
        &registries,
        &mut state,
        region,
        material,
        Pressure::from_pascals(340_000_000),
        Pressure::from_pascals(360_000_000),
    );
    let before = state.clone();

    assert_eq!(
        start.commit(&mut state),
        Err(MiningStartCommitError::TargetNoLongerResolved)
    );
    assert_eq!(state, before);
    assert_eq!(state.player_work().active(), None);
}

#[test]
fn validated_mining_start_survives_unrelated_remote_geology_change() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stale-target geology survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("stale-target geology destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("stale-target geology deposit failed: {error}"));
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("stale-target geology deposit disappeared"));
    let target = resolve_mining_target(
        &state,
        MiningTargetRequest::new(
            deposit_record.bounds(),
            deposit_record.commodity().material(),
        ),
    )
    .unwrap_or_else(|error| panic!("stale-target geology resolution failed: {error}"));
    let start = super::validate_start_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("remote-geology mining validation failed: {error}"));
    let remote_bounds = VoxelBounds::new(VoxelCoord::new(100, -8, 0), VoxelCoord::new(101, -7, 1))
        .unwrap_or_else(|error| panic!("stale-target geology deposit bounds failed: {error}"));
    let remote = GeneratedDepositSpec::new(
        remote_bounds,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(300_000),
        Pressure::from_pascals(100_000_000),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap_or_else(|error| panic!("stale-target geology deposit spec failed: {error}"));
    crate::geology::insert_generated_deposit(&registries, &mut state, remote)
        .unwrap_or_else(|error| panic!("stale-target geology mutation failed: {error}"));

    start.commit(&mut state).unwrap_or_else(|error| {
        panic!("remote geology should not stale validated mining: {error}")
    });
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::Mining { .. })
    ));
}

#[test]
fn mining_rejects_work_that_would_continue_after_tool_failure() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("condition-lifetime survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("condition-lifetime destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("condition-lifetime deposit failed: {error}"));
    degrade_equipment_condition_for_test(&mut state, pick, 999_500);
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .unwrap_or_else(|| panic!("condition-lifetime pick disappeared"))
            .condition(),
        Condition::new(500)
            .unwrap_or_else(|error| panic!("condition-lifetime fixture failed: {error}"))
    );
    let before = state.clone();

    assert!(matches!(
        validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100),
        ),
        Err(MiningStartError::ConditionDuration(_))
    ));
    assert_eq!(state, before);
}

#[test]
fn loaded_mining_job_reconstructs_authored_condition_outcome() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining wear-audit survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining wear-audit destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining wear-audit deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining wear-audit start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining wear-audit commit failed: {error}"));
    let required = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("mining wear-audit job disappeared"))
        .equipment_condition_after();
    let forged = Condition::new(required.parts_per_million().saturating_add(1))
        .unwrap_or_else(|error| panic!("mining forged condition failed: {error}"));
    assert_ne!(forged, required);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining wear-audit serialization failed: {error}"));
    encoded["state"]["systems"]["mining"]["jobs"][job.value().to_string()]["resources"]["equipment_condition_after"] =
        serde_json::json!(forged.parts_per_million());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining wear-audit tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::ConditionOutcomeMismatch {
                job,
                stored: forged,
                required,
            }
        )))
    );
}

#[test]
fn loaded_mining_state_rejects_job_map_key_identity_mismatch() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining key-audit survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining key-audit destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining key-audit deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining key-audit start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining key-audit commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining key-audit serialization failed: {error}"));
    let jobs = encoded["state"]["systems"]["mining"]["jobs"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("serialized mining jobs were not an object"));
    let record = jobs
        .remove(&job.value().to_string())
        .unwrap_or_else(|| panic!("serialized mining job disappeared"));
    let forged_key = job.value() + 1;
    assert!(jobs.insert(forged_key.to_string(), record).is_none());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining key-audit tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Mining(
            MiningValidationError::JobIdMismatch {
                key: MiningJobId::new(forged_key),
                record: job,
            }
        )))
    );
}

#[test]
fn loaded_mining_state_rejects_equipment_double_booking_after_index_rebuild() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining double-book survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(200_000))
        .unwrap_or_else(|error| panic!("mining double-book destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining double-book deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining double-book start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining double-book commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining double-book serialization failed: {error}"));
    let second_job = job.value() + 1;
    let mut duplicated =
        encoded["state"]["systems"]["mining"]["jobs"][job.value().to_string()].clone();
    duplicated["identity"]["id"] = serde_json::json!(second_job);
    encoded["state"]["systems"]["mining"]["jobs"][second_job.to_string()] = duplicated;
    encoded["state"]["systems"]["mining"]["next_job_id"] = serde_json::json!(second_job + 1);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining double-book tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Mining(
            MiningValidationError::EquipmentDoubleBooked { equipment: pick }
        )))
    );
}

#[test]
fn deposit_excavation_hardness_is_independent_of_assay_composition() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mixed-hardness survival initialization failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mixed-hardness destination failed: {error}"));
    let bounds = VoxelBounds::new(VoxelCoord::new(12, -8, 0), VoxelCoord::new(13, -7, 1))
        .unwrap_or_else(|error| panic!("mixed-hardness bounds failed: {error}"));
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 999_000),
        CompositionComponent::new(MATERIAL_STONE, 1_000),
    ])
    .unwrap_or_else(|error| panic!("mixed-hardness composition failed: {error}"));
    let deposit = insert_known_deposit(
        &registries,
        &mut state,
        GeneratedDepositSpec::new(
            bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(100_000),
            Temperature::from_millikelvin(300_000),
            Pressure::from_pascals(600_000_000),
            composition,
        )
        .unwrap_or_else(|error| panic!("mixed-hardness deposit fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("mixed-hardness deposit insertion failed: {error}"));

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
    .unwrap_or_else(|| panic!("stone pick unexpectedly ignored deposit excavation hardness"));
    assert_eq!(
        error,
        MiningStartError::ExcavationHardnessEvidenceExceedsCapability {
            observed_upper: Pressure::from_pascals(600_000_000),
            maximum: Pressure::from_pascals(500_000_000),
        }
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("mixed-hardness deposit disappeared"))
            .remaining_mass(),
        Mass::from_milligrams(100_000)
    );
}

#[test]
fn ready_mining_job_keeps_historical_tool_physics_after_tool_upgrade() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining trace survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining trace destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining trace deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining trace start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining trace start commit failed: {error}"));
    let duration = state
        .mining()
        .get_job(job)
        .map(|record| record.completes_at().value() - record.started_at().value())
        .unwrap_or_else(|| panic!("mining trace job disappeared"));
    for _ in 0..duration {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining trace completion failed: {error}"));
    }
    assert!(
        state
            .mining()
            .get_job(job)
            .is_some_and(MiningJobRecord::is_ready_to_claim)
    );

    let reinforcement_source =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
            .unwrap_or_else(|error| panic!("mining trace reinforcement source failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        reinforcement_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("mining trace reinforcement failed: {error}"));
    validate_upgrade_equipment(
        &registries,
        &state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        reinforcement_source,
    )
    .unwrap_or_else(|error| panic!("mining trace upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining trace upgrade commit failed: {error}"));
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.definition()),
        Some(EQUIPMENT_COPPER_REINFORCED_PICK)
    );
    assert_eq!(
        state
            .mining()
            .get_job(job)
            .map(MiningJobRecord::equipment_definition),
        Some(EQUIPMENT_STONE_PICK)
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("mining trace post-upgrade audit failed: {error}"));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining trace serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("mining trace decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("mining trace load failed: {error}"));
    assert_eq!(loaded, state);
}

#[test]
fn loaded_working_mining_job_rejects_forged_source_mass_trace() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining source-trace survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining source-trace destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining source-trace deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining source-trace start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining source-trace commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining source-trace serialization failed: {error}"));
    encoded["state"]["systems"]["mining"]["jobs"][job.value().to_string()]["resources"]["deposit_mass_before"] =
        serde_json::json!(900_000_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining source-trace tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::WorkingDepositMassMismatch {
                job,
                expected: Mass::from_milligrams(900_000),
                actual: Mass::from_milligrams(1_000_000),
            }
        )))
    );
}

#[test]
fn loaded_working_mining_job_rejects_forged_requested_mass() {
    let (registries, mut state, deposit, destination, pick) = unstarted_mining_fixture();
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining requested-mass start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining requested-mass commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining requested-mass serialization failed: {error}"));
    encoded["state"]["systems"]["mining"]["jobs"][job.value().to_string()]["resources"]["requested_mass"] =
        serde_json::json!(0_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining requested-mass tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::ZeroRequestedMass { job }
        )))
    );
}

#[test]
fn unclaimed_output_allows_follow_on_extraction_from_the_same_deposit() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("follow-on mining survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let first_destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
            .unwrap_or_else(|error| panic!("first follow-on mining destination failed: {error}"));
    let second_destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
            .unwrap_or_else(|error| panic!("second follow-on mining destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("follow-on mining deposit failed: {error}"));
    let extraction_mass = Mass::from_milligrams(100_000);

    let first_job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        first_destination,
        pick,
        extraction_mass,
    )
    .unwrap_or_else(|error| panic!("first follow-on mining start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("first follow-on mining commit failed: {error}"));
    let first_duration = state
        .mining()
        .get_job(first_job)
        .map(|record| record.completes_at().value() - record.started_at().value())
        .unwrap_or_else(|| panic!("first follow-on mining job disappeared"));
    for _ in 0..first_duration {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("first follow-on mining tick failed: {error}"));
    }
    assert!(
        state
            .mining()
            .get_job(first_job)
            .is_some_and(MiningJobRecord::is_ready_to_claim)
    );
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .map(|record| record.remaining_mass()),
        Some(Mass::from_milligrams(900_000))
    );

    let second_job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        second_destination,
        pick,
        extraction_mass,
    )
    .unwrap_or_else(|error| panic!("second follow-on mining start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("second follow-on mining commit failed: {error}"));
    let second_duration = state
        .mining()
        .get_job(second_job)
        .map(|record| record.completes_at().value() - record.started_at().value())
        .unwrap_or_else(|| panic!("second follow-on mining job disappeared"));
    for _ in 0..second_duration {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("second follow-on mining tick failed: {error}"));
    }

    for job in [first_job, second_job] {
        assert!(
            state
                .mining()
                .get_job(job)
                .is_some_and(MiningJobRecord::is_ready_to_claim),
            "completed extraction {job:?} must remain claimable after later extraction"
        );
    }
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .map(|record| record.remaining_mass()),
        Some(Mass::from_milligrams(800_000))
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("follow-on mining state audit failed: {error}"));

    let mut forged_mass_history = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| {
            panic!("follow-on mining mass-history serialization failed: {error}")
        });
    forged_mass_history["state"]["systems"]["mining"]["jobs"][second_job.value().to_string()]["resources"]
        ["deposit_mass_before"] = serde_json::json!(950_000_u64);
    let forged_mass_history: LoadedSaveEnvelope = serde_json::from_value(forged_mass_history)
        .unwrap_or_else(|error| panic!("follow-on mining mass-history decode failed: {error}"));
    assert_eq!(
        forged_mass_history.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::DepositHistoryMassIncrease {
                earlier: first_job,
                later: second_job,
                maximum_later_mass: Mass::from_milligrams(900_000),
                later_mass: Mass::from_milligrams(950_000),
            }
        )))
    );

    let first_record = state
        .mining()
        .get_job(first_job)
        .unwrap_or_else(|| panic!("first follow-on mining job disappeared before schedule tamper"));
    let second_record = state.mining().get_job(second_job).unwrap_or_else(|| {
        panic!("second follow-on mining job disappeared before schedule tamper")
    });
    let forged_second_start = SimulationTick::new(
        second_record
            .started_at()
            .value()
            .checked_sub(1)
            .unwrap_or_else(|| panic!("second follow-on mining job unexpectedly starts at zero")),
    );
    let forged_second_completion = SimulationTick::new(
        second_record
            .completes_at()
            .value()
            .checked_sub(1)
            .unwrap_or_else(|| panic!("second follow-on mining completion unexpectedly zero")),
    );
    let mut forged_schedule = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("follow-on mining schedule serialization failed: {error}"));
    forged_schedule["state"]["systems"]["mining"]["jobs"][second_job.value().to_string()]["schedule"]
        ["started_at"] = serde_json::json!(forged_second_start.value());
    forged_schedule["state"]["systems"]["mining"]["jobs"][second_job.value().to_string()]["schedule"]
        ["completes_at"] = serde_json::json!(forged_second_completion.value());
    let forged_schedule: LoadedSaveEnvelope = serde_json::from_value(forged_schedule)
        .unwrap_or_else(|error| panic!("follow-on mining schedule decode failed: {error}"));
    assert_eq!(
        forged_schedule.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::OverlappingRetainedWork {
                earlier: first_job,
                later: second_job,
                earlier_completes: first_record.completes_at(),
                later_starts: forged_second_start,
            }
        )))
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("follow-on mining save failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("follow-on mining decode failed: {error}"));
    let restored = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("follow-on mining load failed: {error}"));
    assert_eq!(restored, state);

    validate_claim_mining_output(&registries, &state, first_job)
        .unwrap_or_else(|error| panic!("older follow-on mining claim failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("older follow-on mining claim commit failed: {error}"));
    validate_claim_mining_output(&registries, &state, second_job)
        .unwrap_or_else(|error| panic!("newer follow-on mining claim failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("newer follow-on mining claim commit failed: {error}"));
    for destination in [first_destination, second_destination] {
        assert_eq!(
            state.inventory().get_stockpile(destination).map(|record| {
                (
                    record.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_ORE)),
                    record.reserved_inbound(),
                )
            }),
            Some((extraction_mass, Mass::ZERO))
        );
    }
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("claimed follow-on mining audit failed: {error}"));
}

#[test]
fn trusted_load_rejects_multiple_working_mining_jobs_before_single_extraction_tick() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("multiple-mining survival setup failed: {error}"));
    let first_pick = assemble_pick_for_test(&registries, &mut state);
    let second_pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(200_000))
        .unwrap_or_else(|error| panic!("multiple-mining destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("multiple-mining deposit failed: {error}"));
    let first_job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        first_pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("multiple-mining canonical start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("multiple-mining canonical commit failed: {error}"));
    let first_record = state
        .mining()
        .get_job(first_job)
        .unwrap_or_else(|| panic!("multiple-mining canonical job disappeared"));
    let first_started_at = first_record.started_at();
    let first_completes_at = first_record.completes_at();

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("multiple-mining serialization failed: {error}"));
    let first_key = first_job.value().to_string();
    let second_job = MiningJobId::new(first_job.value() + 1);
    let second_key = second_job.value().to_string();
    let mut duplicate = encoded["state"]["systems"]["mining"]["jobs"][&first_key].clone();
    duplicate["identity"]["id"] = serde_json::json!(second_job.value());
    duplicate["resources"]["equipment_trace"]["equipment"] = serde_json::json!(second_pick.value());
    encoded["state"]["systems"]["mining"]["jobs"][&second_key] = duplicate;
    encoded["state"]["systems"]["mining"]["next_job_id"] =
        serde_json::json!(second_job.value() + 1);
    encoded["state"]["systems"]["inventory"]["stockpiles"][destination.value().to_string()]["reserved_inbound"] =
        serde_json::json!(200_000_u64);
    let forged: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("multiple-mining forged decode failed: {error}"));

    assert_eq!(
        forged.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::OverlappingRetainedWork {
                earlier: first_job,
                later: second_job,
                earlier_completes: first_completes_at,
                later_starts: first_started_at,
            }
        )))
    );
}

#[test]
fn loaded_ready_mining_job_reconstructs_authored_duration() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining duration-audit survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining duration-audit destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining duration-audit deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining duration-audit start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining duration-audit commit failed: {error}"));
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("mining duration-audit job disappeared"));
    let required = crate::core::time::TickSpan::new(
        record.completes_at().value() - record.started_at().value(),
    );
    for _ in 0..required.value() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining duration-audit completion failed: {error}"));
    }
    let ready = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("ready mining duration-audit job disappeared"));
    assert!(ready.is_ready_to_claim());
    let forged_started_at = ready.started_at().value() + 1;

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining duration-audit serialization failed: {error}"));
    encoded["state"]["systems"]["mining"]["jobs"][job.value().to_string()]["schedule"]["started_at"] =
        serde_json::json!(forged_started_at);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining duration-audit tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::DurationMismatch {
                job,
                stored: required
                    .checked_sub(crate::core::time::TickSpan::new(1))
                    .unwrap_or_else(|| panic!("mining duration fixture must exceed one tick")),
                required,
            }
        )))
    );
}
