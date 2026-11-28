//! Admission-local geological evidence and retained mining target replay.

use super::*;

#[test]
fn trusted_load_requires_acquired_mining_locality_and_evidence() {
    let (registries, mut state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
    let start = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap();
    let job = start.commit(&mut state).unwrap();
    let baseline = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_value::<LoadedSaveEnvelope>(baseline.clone())
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state
    );
    // All the physical source, tool, output and schedule facts remain valid,
    // but this snapshot lacks any acquired authorization for extraction.
    let mut unknown = baseline.clone();
    unknown["state"]["systems"]["geological_knowledge"] =
        serde_json::to_value(crate::geology::GeologicalKnowledgeState::new()).unwrap();
    assert_eq!(
        serde_json::from_value::<LoadedSaveEnvelope>(unknown)
            .unwrap()
            .into_state(&registries)
            .err(),
        Some(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::TargetEvidenceMismatch { job }
        ))),
        "mining cannot start without acquired evidence"
    );

    // Move the entire local work site to another voxel inside the same body.
    // That voxel has no acquired observations, and normal admission rejects it.
    let bounds = state.geology().get_deposit(deposit).unwrap().bounds();
    let at = bounds.min();
    let other = VoxelCoord::new(at.x() + 1, at.y(), at.z());
    assert!(bounds.has_voxel(other));
    let target =
        resolve_mining_target(&state, MiningTargetRequest::new(bounds, MATERIAL_COPPER)).unwrap();
    assert!(!target.region().has_voxel(other));
    let mut relocated = baseline.clone();
    let logistics = &mut relocated["state"]["systems"]["logistics"];
    logistics["player"]["body"]["foot_position"] = serde_json::json!({
        "x_nanometers": other.x() * 1_000_000_000 + 500_000_000,
        "y_nanometers": other.y() * 1_000_000_000,
        "z_nanometers": other.z() * 1_000_000_000 + 500_000_000
    });
    logistics["equipment_locations"][pick.value().to_string()] =
        serde_json::to_value(other).unwrap();
    logistics["stockpile_locations"][destination.value().to_string()] =
        serde_json::to_value(other).unwrap();
    assert_eq!(
        serde_json::from_value::<LoadedSaveEnvelope>(relocated.clone())
            .unwrap()
            .into_state(&registries)
            .err(),
        Some(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::WorkingPlayerOutsideTarget {
                job,
                player_position: other,
                bounds: target.region()
            }
        ))),
        "mining cannot leave the acquired voxel"
    );
    // Editing the durable target as well cannot manufacture acquired authority.
    let remote_region = VoxelBounds::new(
        other,
        VoxelCoord::new(other.x() + 1, other.y() + 1, other.z() + 1),
    )
    .unwrap();
    relocated["state"]["systems"]["mining"]["jobs"][job.value().to_string()]["identity"]["target_region"] =
        serde_json::to_value(remote_region).unwrap();
    assert_eq!(
        serde_json::from_value::<LoadedSaveEnvelope>(relocated)
            .unwrap()
            .into_state(&registries)
            .err(),
        Some(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::TargetEvidenceMismatch { job }
        )))
    );
    let mut later = state.clone();
    let _ = advance_tick(&registries, &mut later).unwrap();
    assert!(later.mining().get_job(job).unwrap().is_working());
    let mut later_evidence = serde_json::to_value(SaveEnvelope::new(&registries, &later)).unwrap();
    for observation in later_evidence["state"]["systems"]["geological_knowledge"]["observations"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        observation["observed_at"] = serde_json::json!(later.tick().value());
    }
    assert_eq!(
        serde_json::from_value::<LoadedSaveEnvelope>(later_evidence)
            .unwrap()
            .into_state(&registries)
            .err(),
        Some(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::TargetEvidenceMismatch { job }
        ))),
        "a later observation cannot authorize an earlier start"
    );

    let mut replay = serde_json::from_value::<LoadedSaveEnvelope>(baseline)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let total = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(
        complete_mining_job(&registries, &mut state, job, "authorized mine"),
        complete_mining_job(&registries, &mut replay, job, "reloaded authorized mine")
    );
    assert_eq!(state, replay);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
}

#[test]
fn retained_mining_output_replays_admission_before_depletion_and_later_survey() {
    use crate::geology::{FieldProspectingRequest, validate_start_field_prospecting};

    let registries = build_registries();
    let mut state = AppState::new();
    let mass = Mass::from_milligrams(100_000);
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, mass).unwrap();
    let deposit = crate::geology::validate_generate_geological_deposit(
        &registries,
        &state,
        deposit_spec_with_mass(mass),
    )
    .unwrap()
    .apply(&mut state);
    let at = admit_known_mining_player_at_deposit(
        &registries,
        &mut state,
        deposit,
        &[destination],
        &[pick],
    );
    let region = VoxelBounds::new(at, VoxelCoord::new(at.x() + 1, at.y() + 1, at.z() + 1)).unwrap();
    let request = MiningTargetRequest::new(region, MATERIAL_COPPER);
    assert!(resolve_mining_target(&state, request).is_err());
    let survey = |state: &mut AppState| {
        let validated = validate_start_field_prospecting(
            &registries,
            state,
            FieldProspectingRequest::new(PROSPECTING_FIELD_INSPECTION, region, MATERIAL_COPPER),
        )
        .unwrap();
        let work = validated.work();
        validated.commit(state).unwrap();
        while state.tick() < work.completes_at() {
            let _ = advance_tick(&registries, state).unwrap();
        }
    };
    survey(&mut state);
    let target = resolve_mining_target(&state, request).unwrap();
    let job = validate_start_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        pick,
        mass,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    complete_mining_into_blocked_settlement_state(
        &registries,
        &mut state,
        job,
        destination,
        "depleted target history",
    );
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap()
            .remaining_mass(),
        Mass::ZERO
    );
    assert!(
        state
            .mining()
            .get_job(job)
            .unwrap()
            .is_awaiting_settlement()
    );
    let total = calculate_matter_accounting(&state).unwrap().total();
    survey(&mut state);
    assert!(resolve_mining_target(&state, request).is_err());
    assert!(
        state
            .mining()
            .get_job(job)
            .unwrap()
            .is_awaiting_settlement()
    );
    assert_eq!(state.geological_knowledge().observations().count(), 2);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);
    for game in [&mut state, &mut replay] {
        let _ = validate_unmount_stockpile(&registries, game, destination)
            .unwrap()
            .commit(game)
            .unwrap();
        let receipt = advance_tick(&registries, game).unwrap();
        assert_eq!(receipt.mining_output().unwrap().job(), job);
        assert!(game.mining().get_job(job).is_none());
        assert_eq!(
            game.inventory()
                .get_stockpile(destination)
                .unwrap()
                .stored_mass(),
            mass
        );
        assert_eq!(calculate_matter_accounting(game).unwrap().total(), total);
        assert_eq!(validate_loaded_state(&registries, game), Ok(()));
    }
    assert_eq!(state, replay);
}
