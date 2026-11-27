//! Prepared-strike windup, cancellation, release, exertion, and fatal interruption.

use super::*;

#[test]
fn prepared_club_strike_waits_for_windup_commits_live_contact_and_replays() {
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::matter::calculate_matter_accounting;
    use crate::simulation::{
        PlayerDirectControlInput, PrimaryActionOutcome, PrimaryControlIntent, advance_control_tick,
        advance_control_tick_with_direct_input,
    };
    let (registries, mut state, _, junction, tip) = opening_contact_world();
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let club = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let west = ViewDirection::new(-1_000_000, 0, 0).unwrap();
    let east = east();
    let mass = calculate_matter_accounting(&state).unwrap().total();
    let begin =
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::prepare_strike(west));
    let start = advance_control_tick_with_direct_input(&registries, &mut state, begin).unwrap();
    let ready_at = match start.primary_action() {
        Some(PrimaryActionOutcome::Preparation(PrimitivePreparationOutcome::Prepared {
            ready_at,
        })) => *ready_at,
        other => panic!("prepared stance rejected: {other:?}"),
    };
    assert_eq!(ready_at.value(), state.control_tick().value() + 10);
    assert_eq!(state.combat().revision(), 1);
    assert_eq!(
        state.equipment().get_equipment(club).unwrap().condition(),
        Condition::PRISTINE
    );
    let assessment = assess_player_melee(&registries, &state).unwrap();
    assert!(!assessment.ready());
    assert!(!assessment.prepared_release_ready());
    assert_eq!(assessment.preparation_remaining_ticks(), Some(10));
    assert_eq!(assessment.recovery_control_ticks(), 34);
    let quick = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::strike(west)),
    )
    .unwrap();
    assert_eq!(
        quick.primary_action(),
        Some(&PrimaryActionOutcome::StrikeRejected(
            PrimitiveAttackError::AlreadyPreparing
        ))
    );
    let wrong_aim = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::release_strike(east)),
    )
    .unwrap();
    assert_eq!(
        wrong_aim.primary_action(),
        Some(&PrimaryActionOutcome::PreparedStrikeRejected(
            PrimitiveAttackError::PreparationAimChanged
        ))
    );
    let early = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::release_strike(west)),
    )
    .unwrap();
    assert_eq!(
        early.primary_action(),
        Some(&PrimaryActionOutcome::PreparedStrikeRejected(
            PrimitiveAttackError::PreparationNotReady { ready_at }
        ))
    );
    assert_eq!(
        state.equipment().get_equipment(club).unwrap().condition(),
        Condition::PRISTINE
    );
    let mut loaded = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    while state.control_tick().value() + 1 < ready_at.value() {
        assert_eq!(
            advance_control_tick(&registries, &mut state).unwrap(),
            advance_control_tick(&registries, &mut loaded).unwrap()
        );
    }
    for game in [&mut state, &mut loaded] {
        let before = assess_player_melee(&registries, game).unwrap();
        assert_eq!(before.preparation_remaining_ticks(), Some(1));
        assert!(!before.prepared_release_ready());
        let _ = advance_control_tick(&registries, game).unwrap();
        let ready = assess_player_melee(&registries, game).unwrap();
        assert_eq!(ready.preparation_remaining_ticks(), Some(0));
        assert!(ready.prepared_release_ready());
        assert!(!ready.ready());
        crate::logistics::validate_set_active_selection(&registries, game, None)
            .unwrap()
            .commit(game)
            .unwrap();
        let unarmed = assess_player_melee(&registries, game).unwrap();
        assert_eq!(unarmed.preparation_remaining_ticks(), Some(0));
        assert!(!unarmed.prepared_release_ready());
        crate::logistics::validate_set_active_selection(
            &registries,
            game,
            Some(PlayerActiveSelection::Equipment(club)),
        )
        .unwrap()
        .commit(game)
        .unwrap();
        assert!(
            assess_player_melee(&registries, game)
                .unwrap()
                .prepared_release_ready()
        );
        assert_eq!(validate_loaded_state(&registries, game), Ok(()));
    }
    let release =
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::release_strike(west));
    for game in [&mut state, &mut loaded] {
        let struck = advance_control_tick_with_direct_input(&registries, game, release).unwrap();
        assert!(
            matches!(struck.primary_action(),
            Some(PrimaryActionOutcome::PreparedStrike(
                PrimitiveAttackOutcome::Startled {
                    resident: KoboldRole::Scout, position
                }
            )) if *position == junction),
            "unexpected contact: {:?}",
            struck.primary_action()
        );
        assert_eq!(game.combat().revision(), 2);
        assert_eq!(game.combat().prepared, None);
        assert_eq!(
            game.equipment()
                .get_equipment(club)
                .unwrap()
                .condition()
                .parts_per_million(),
            994_000
        );
        assert_eq!(calculate_matter_accounting(game).unwrap().total(), mass);
        let scout = crate::kobolds::available_nearby_kobold_scouts(&registries, game)
            .find(|r| r.role() == KoboldRole::Scout)
            .unwrap();
        assert_eq!(scout.balance_ppm(), Some(580_000));
        assert_eq!(
            assess_player_melee(&registries, game)
                .unwrap()
                .remaining_recovery_ticks(),
            34
        );
        assert_eq!(
            assess_player_melee(&registries, game)
                .unwrap()
                .recovery_control_ticks(),
            34
        );
        assert_eq!(validate_loaded_state(&registries, game), Ok(()));
    }
    assert_eq!(state, loaded);
}

#[test]
fn prepared_attack_exertion_rejects_unrepresentable_costs() {
    let energy = SurvivalExertion::new(
        Energy::from_nanojoules(u128::MAX),
        Volume::from_microliters(1),
    );
    let hydration = SurvivalExertion::new(
        Energy::from_nanojoules(1),
        Volume::from_microliters(u64::MAX),
    );
    assert_eq!(
        prepared_attack_exertion(energy),
        Err(PrimitiveAttackError::ExertionOverflow)
    );
    assert_eq!(
        prepared_attack_exertion(hydration),
        Err(PrimitiveAttackError::ExertionOverflow)
    );
    assert_eq!(
        prepared_attack_exertion(SHOVE_EXERTION),
        Ok(SurvivalExertion::new(
            Energy::from_nanojoules(180_000_000_000),
            Volume::from_microliters(12),
        ))
    );
}

#[test]
fn cancelling_preparation_and_changed_weapon_cannot_conjure_an_impact() {
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::logistics::validate_set_active_selection;
    use crate::simulation::{
        PlayerDirectControlInput, PrimaryActionOutcome, PrimaryControlIntent,
        advance_control_tick_with_direct_input,
    };
    let (registries, mut state, _, _, tip) = opening_contact_world();
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let club = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let west = ViewDirection::new(-1_000_000, 0, 0).unwrap();
    let run = |state: &mut AppState, primary| {
        advance_control_tick_with_direct_input(
            &registries,
            state,
            PlayerDirectControlInput::NEUTRAL.with_primary(primary),
        )
        .unwrap()
        .primary_action()
        .cloned()
    };
    assert!(matches!(
        run(&mut state, PrimaryControlIntent::prepare_strike(west)),
        Some(PrimaryActionOutcome::Preparation(
            PrimitivePreparationOutcome::Prepared { .. }
        ))
    ));
    validate_set_active_selection(&registries, &state, None)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        run(&mut state, PrimaryControlIntent::release_strike(west)),
        Some(PrimaryActionOutcome::PreparedStrikeRejected(
            PrimitiveAttackError::PreparationWeaponChanged
        ))
    );
    assert_eq!(
        run(&mut state, PrimaryControlIntent::cancel_strike()),
        Some(PrimaryActionOutcome::Preparation(
            PrimitivePreparationOutcome::Cancelled
        ))
    );
    assert_eq!(
        state.equipment().get_equipment(club).unwrap().condition(),
        Condition::PRISTINE
    );
    assert_eq!(state.combat().last_attack_at, None);
    assert_eq!(state.combat().prepared, None);
    assert_eq!(
        run(&mut state, PrimaryControlIntent::release_strike(west)),
        Some(PrimaryActionOutcome::PreparedStrikeRejected(
            PrimitiveAttackError::NotPreparing
        ))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn changed_world_invalidates_validated_prepared_contact_without_partial_mutation() {
    use crate::content::{BLOCK_STATE_STONE, EQUIPMENT_STONE_SPEAR};
    use crate::world::set_generated_block_for_test;
    let (registries, mut state, _, _, _) = opening_contact_world();
    validate_admit_player(&registries, &state, VoxelCoord::new(4, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let club = arm_player(&registries, &mut state, EQUIPMENT_STONE_SPEAR);
    let west = ViewDirection::new(-1_000_000, 0, 0).unwrap();
    let _ = validate_prepare_primitive_strike(&registries, &state, west)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..10 {
        let _ = crate::simulation::advance_control_tick(&registries, &mut state).unwrap();
    }
    let pending = validate_release_prepared_strike(&registries, &state, west).unwrap();
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(2, 1, 0),
        BLOCK_STATE_STONE,
    );
    let before = state.clone();
    assert_eq!(
        pending.commit(&mut state),
        Err(PrimitiveAttackCommitError::StaleWorldRevision)
    );
    assert_eq!(state, before);
    assert_eq!(
        state.equipment().get_equipment(club).unwrap().condition(),
        Condition::PRISTINE
    );
    assert_eq!(
        validate_release_prepared_strike(&registries, &state, west)
            .unwrap()
            .commit(&mut state),
        Ok(PrimitiveAttackOutcome::Miss)
    );
    assert_eq!(state.combat().prepared, None);
}

#[test]
fn fatal_slow_tick_cancels_prepared_strike_without_wear_or_afterlife_attack() {
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::core::quantity::Energy;
    use crate::simulation::advance_control_tick;
    use crate::survival::{Vitality, player_record};

    let (registries, mut state, _, _, tip) = opening_contact_world();
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let club = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let west = ViewDirection::new(-1_000_000, 0, 0).unwrap();
    let _ = validate_prepare_primitive_strike(&registries, &state, west)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let current = *state.survival().player().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            Energy::ZERO,
            current.hydration(),
            Vitality::from_parts_per_million_unchecked(
                registries
                    .survival()
                    .physiology()
                    .starvation_vitality_loss_ppm_per_tick(),
            ),
            current.nutrition(),
            current.vitality_recovery_remainder(),
        ),
    );
    for _ in 0..72 {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.survival().player().unwrap().vitality(),
        Vitality::ZERO
    );
    assert_eq!(state.combat().prepared, None);
    assert_eq!(state.combat().last_attack_at, None);
    assert_eq!(
        state.equipment().get_equipment(club).unwrap().condition(),
        Condition::PRISTINE
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
