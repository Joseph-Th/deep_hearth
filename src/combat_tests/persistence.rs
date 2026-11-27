//! Trusted-load validation of combat histories and prepared-weapon references.

use super::*;

#[test]
fn trusted_combat_recovery_requires_reachable_authored_attack_history() {
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::simulation::advance_control_tick;

    let (registries, mut state, _, _, tip) = opening_contact_world_after(0);
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let club = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let _ = validate_prepare_primitive_strike(&registries, &state, east())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..preparation::PREPARATION_TICKS {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    let _ = validate_release_prepared_strike(&registries, &state, east())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let profile = registries
        .equipment()
        .get_equipment(state.equipment().get_equipment(club).unwrap().definition())
        .unwrap()
        .melee_profile()
        .unwrap();
    assert_eq!(
        state.combat().recovery_control_ticks,
        profile.recovery_control_ticks() + preparation::PREPARED_RECOVERY_BONUS_TICKS
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let base = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_value::<LoadedSaveEnvelope>(base.clone())
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);

    // A duration inside the broad profile bounds is not necessarily authored.
    // A prepared strike also cannot precede its windup or omit its revision.
    for (key, value) in [
        ("recovery_control_ticks", serde_json::json!(9)),
        ("last_attack_at", serde_json::json!(0)),
        ("revision", serde_json::json!(1)),
    ] {
        let mut forged = base.clone();
        forged["state"]["systems"]["combat"][key] = value;
        let loaded = serde_json::from_value::<LoadedSaveEnvelope>(forged).unwrap();
        assert_eq!(
            loaded.into_state(&registries),
            Err(crate::persistence::LoadError::InvalidState(
                crate::core::state::StateValidationError::Combat(
                    "combat recovery record conflicts with control history"
                )
            )),
            "accepted forged {key}"
        );
    }
}

#[test]
fn trusted_load_rejects_prepared_strike_with_real_non_melee_equipment() {
    use crate::content::{EQUIPMENT_STONE_PICK, EQUIPMENT_WOODEN_CLUB};

    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let pick = arm_player(&registries, &mut state, EQUIPMENT_STONE_PICK);
    let club = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let prepared = validate_prepare_primitive_strike(&registries, &state, east())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(matches!(
        prepared,
        PrimitivePreparationOutcome::Prepared { .. }
    ));
    assert_eq!(
        state.combat().prepared.map(|strike| strike.weapon),
        Some(club)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let mut forged = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    forged["state"]["systems"]["combat"]["prepared"]["weapon"] =
        serde_json::to_value(pick).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
    assert_eq!(
        loaded.into_state(&registries),
        Err(crate::persistence::LoadError::InvalidState(
            crate::core::state::StateValidationError::Combat(
                "prepared strike references equipment without a melee profile"
            )
        ))
    );
}

#[test]
fn trusted_preparation_requires_the_original_usable_carried_weapon() {
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::core::state::StateValidationError;
    use crate::equipment::degrade_equipment_condition_for_test;
    use crate::logistics::{validate_drop_equipment, validate_set_active_selection};
    use crate::persistence::LoadError;

    let (registries, mut state, _, _, tip) = opening_contact_world();
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let club = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let _ = validate_prepare_primitive_strike(&registries, &state, east())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_set_active_selection(&registries, &state, None)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let prepared = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_value::<LoadedSaveEnvelope>(prepared.clone())
            .unwrap()
            .into_state(&registries),
        Ok(state.clone()),
        "switching quick slots is legal while retaining a carried weapon"
    );
    let combat = prepared["state"]["systems"]["combat"].clone();
    let _ = validate_cancel_primitive_preparation(&state)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    // Each forged stance references genuine, otherwise valid equipment custody.
    let mut dropped = state.clone();
    validate_drop_equipment(&registries, &dropped, club)
        .unwrap()
        .commit(&mut dropped)
        .unwrap();
    let mut broken = state.clone();
    degrade_equipment_condition_for_test(&mut broken, club, 1_000_000);
    assert_eq!(
        broken.equipment().get_equipment(club).unwrap().condition(),
        Condition::FAILED
    );
    for (candidate, reason) in [
        (dropped, "prepared melee weapon is not carried"),
        (broken, "prepared melee weapon is broken"),
    ] {
        assert_eq!(validate_loaded_state(&registries, &candidate), Ok(()));
        let mut forged = serde_json::to_value(SaveEnvelope::new(&registries, &candidate)).unwrap();
        forged["state"]["systems"]["combat"] = combat.clone();
        let loaded = serde_json::from_value::<LoadedSaveEnvelope>(forged).unwrap();
        assert_eq!(
            loaded.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::Combat(
                reason
            )))
        );
    }

    // The weapon cannot have been raised before its actual creation tick.
    let mut forged = prepared;
    forged["state"]["systems"]["combat"]["prepared"]["started_at"] = serde_json::json!(0);
    assert!(
        state
            .equipment()
            .get_equipment(club)
            .unwrap()
            .created_at()
            .value()
            > 0
    );
    assert_eq!(
        serde_json::from_value::<LoadedSaveEnvelope>(forged)
            .unwrap()
            .into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Combat(
            "prepared melee weapon did not exist at windup"
        )))
    );
}

#[test]
fn trusted_load_rejects_combat_history_without_player_admission() {
    let registries = build_registries();
    let state = AppState::new();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    // Two combat revisions could represent a completed preparation/cancellation,
    // but neither operation can occur without an admitted player.
    let mut forged = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    forged["state"]["systems"]["combat"]["revision"] = serde_json::json!(2);
    let loaded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
    assert_eq!(
        loaded.into_state(&registries),
        Err(crate::persistence::LoadError::InvalidState(
            crate::core::state::StateValidationError::Combat(
                "combat history requires an admitted player"
            )
        ))
    );
}

#[test]
fn forged_preparation_facing_and_future_start_fail_trusted_load() {
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    let (registries, mut state, _, _, tip) = opening_contact_world();
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let west = ViewDirection::new(-1_000_000, 0, 0).unwrap();
    let _ = validate_prepare_primitive_strike(&registries, &state, west)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let base = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let scenarios = [
        (
            "aim",
            serde_json::json!([0, 0, 0]),
            "prepared combat stance contradicts actor clock or facing",
        ),
        (
            "started_at",
            serde_json::json!(state.control_tick().value() + 1),
            "prepared combat stance contradicts actor clock or facing",
        ),
        (
            "weapon",
            serde_json::json!(999_999_u32),
            "prepared melee weapon has no equipment identity",
        ),
    ];
    for (key, value, reason) in scenarios {
        let mut forged = base.clone();
        forged["state"]["systems"]["combat"]["prepared"][key] = value;
        let loaded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
        assert_eq!(
            loaded.into_state(&registries),
            Err(crate::persistence::LoadError::InvalidState(
                crate::core::state::StateValidationError::Combat(reason)
            ))
        );
    }
}

#[test]
fn forged_unmatched_preparation_revision_is_rejected_before_any_attack() {
    let registries = build_registries();
    let state = AppState::new();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    encoded["state"]["systems"]["combat"]["revision"] = serde_json::json!(1_u64);
    let loaded: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        loaded.into_state(&registries),
        Err(crate::persistence::LoadError::InvalidState(
            crate::core::state::StateValidationError::Combat(
                "combat recovery record conflicts with control history"
            )
        ))
    );
}

#[test]
fn forged_weapon_recovery_duration_fails_trusted_load() {
    let registries = build_registries();
    let mut game = AppState::new();
    game.combat_state_mut().apply_attack(
        0,
        ControlTick::ZERO,
        61 + preparation::PREPARED_RECOVERY_BONUS_TICKS,
    );
    assert_eq!(
        validate_loaded_state(&registries, &game),
        Err(crate::core::state::StateValidationError::Combat(
            "combat recovery record conflicts with control history",
        )),
    );
}

#[test]
fn saved_future_combat_history_is_rejected() {
    let registries = build_registries();
    let mut state = AppState::new();
    state
        .combat_state_mut()
        .apply_attack(0, ControlTick::new(1), SHOVE_RECOVERY_TICKS);
    assert!(validate_loaded_state(&registries, &state).is_err());
}

#[test]
fn loaded_prepared_strike_must_reserve_its_fatal_cleanup_revision() {
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::core::state::StateValidationError;

    let (registries, mut state, _, _, tip) = opening_contact_world_after(0);
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let _ = validate_prepare_primitive_strike(&registries, &state, east())
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(state.combat().has_prepared_strike());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let baseline = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    // Preparation can never legitimately leave zero combat revisions for
    // cancellation or fatality. Both require a future committed owner step.
    let mut forged = baseline.clone();
    forged["state"]["systems"]["combat"]["revision"] = serde_json::json!(u64::MAX);
    let invalid = serde_json::from_value::<LoadedSaveEnvelope>(forged)
        .unwrap()
        .into_state(&registries);
    assert_eq!(
        invalid,
        Err(crate::persistence::LoadError::InvalidState(
            StateValidationError::Combat(
                "prepared combat stance has no revision space for cancellation"
            )
        ))
    );

    // The highest admissible odd prepared revision retains enough headroom
    // for cancellation, and the resulting state remains a trusted save.
    let mut limiting = baseline;
    limiting["state"]["systems"]["combat"]["revision"] = serde_json::json!(u64::MAX - 2);
    let mut recovered = serde_json::from_value::<LoadedSaveEnvelope>(limiting)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    // The same last legal prepared revision must also tolerate a *fatal*
    // slow tick: the survival owner clears it without overflowing combat.
    let mut fatal = recovered.clone();
    let before = *fatal.survival().player().unwrap();
    let survival_revision = fatal.survival().revision();
    fatal.survival_state_mut().apply_player(
        survival_revision,
        survival_revision + 1,
        crate::survival::player_record(
            Energy::ZERO,
            before.hydration(),
            crate::survival::Vitality::from_parts_per_million_unchecked(
                registries
                    .survival()
                    .physiology()
                    .starvation_vitality_loss_ppm_per_tick(),
            ),
            before.nutrition(),
            before.vitality_recovery_remainder(),
        ),
    );
    assert_eq!(validate_loaded_state(&registries, &fatal), Ok(()));
    for _ in 0..crate::core::time::CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = crate::simulation::advance_control_tick(&registries, &mut fatal).unwrap();
    }
    assert_eq!(
        fatal.survival().player().unwrap().vitality(),
        crate::survival::Vitality::ZERO
    );
    assert!(fatal.combat().prepared.is_none());
    assert_eq!(fatal.combat().revision(), u64::MAX - 1);
    assert_eq!(validate_loaded_state(&registries, &fatal), Ok(()));
    let persisted_fatal = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &fatal)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    assert_eq!(persisted_fatal, fatal);

    assert_eq!(
        validate_cancel_primitive_preparation(&recovered)
            .unwrap()
            .commit(&mut recovered)
            .unwrap(),
        PrimitivePreparationOutcome::Cancelled
    );
    assert_eq!(recovered.combat().revision(), u64::MAX - 1);
    assert_eq!(validate_loaded_state(&registries, &recovered), Ok(()));
}
