//! Primitive contact uses the authoritative player clock and save graph.

use super::*;
use crate::content::build_registries;
use crate::core::state::validate_loaded_state;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;

fn east() -> ViewDirection {
    ViewDirection::new(1_000_000, 0, 0).unwrap()
}

#[test]
fn unadmitted_player_cannot_strike_or_create_recovery_history() {
    let registries = build_registries();
    let state = AppState::new();
    assert_eq!(
        validate_primitive_shove(&registries, &state, east()).err(),
        Some(PrimitiveAttackError::PlayerNotAdmitted)
    );
    assert_eq!(state.combat().revision(), 0);
}

#[test]
fn recovery_has_a_real_deadline_and_survives_persistence() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let first = validate_primitive_shove(&registries, &state, east());
    assert!(matches!(
        first,
        Err(PrimitiveAttackError::Target(
            PlayerBlockTargetError::WorldAuthorityUnavailable { .. }
        ))
    ));
    // The target fails closed before recovery is consumed.
    assert_eq!(state.combat().revision(), 0);
    state
        .combat_state_mut()
        .apply_attack(0, ControlTick::ZERO, SHOVE_RECOVERY_TICKS);
    assert_eq!(
        validate_primitive_shove(&registries, &state, east()).err(),
        Some(PrimitiveAttackError::Recovering {
            last_at: ControlTick::ZERO
        })
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, loaded);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

fn opening_contact_world() -> (Registries, AppState, VoxelCoord, VoxelCoord, VoxelCoord) {
    use crate::content::BLOCK_STATE_STONE;
    use crate::logistics::add_traversable_edge_for_test;
    use crate::simulation::advance_tick;
    use crate::world::{
        BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(0, 0, 0);
    let junction = VoxelCoord::new(1, 0, 0);
    let tip = VoxelCoord::new(2, 0, 0);
    for (from, to) in [(den, junction), (junction, tip)] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(7),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    for x in 0..=4 {
        let ground = VoxelCoord::new(x, -1, 0);
        set_generated_block_for_test(registries.world(), &mut state, ground, BLOCK_STATE_STONE);
        for y in 0..=2 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(x, y, 0),
                BlockStateId::AIR,
            );
        }
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    for _ in 0..180 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    (registries, state, den, junction, tip)
}

#[test]
fn close_range_shove_startles_a_real_scout_and_retreat_replays_after_save() {
    use crate::simulation::{
        PlayerDirectControlInput, PrimaryActionOutcome, PrimaryControlIntent, advance_control_tick,
        advance_control_tick_with_direct_input,
    };

    let (registries, mut state, den, junction, tip) = opening_contact_world();
    assert_eq!(
        close_contact_at(&state, junction),
        Ok(Some((KoboldRole::Scout, junction)))
    );
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let west = ViewDirection::new(-1_000_000, 0, 0).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    let shove = PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::shove(west));
    for game in [&mut state, &mut replay] {
        let action = advance_control_tick_with_direct_input(&registries, game, shove).unwrap();
        assert!(matches!(
            action.primary_action(),
            Some(PrimaryActionOutcome::Shove(
                PrimitiveAttackOutcome::Startled {
                    resident: KoboldRole::Scout,
                    ..
                }
            ))
        ));
        assert_eq!(game.combat().revision(), 1);
        assert_eq!(validate_loaded_state(&registries, game), Ok(()));
        let repeated = advance_control_tick_with_direct_input(&registries, game, shove).unwrap();
        assert!(matches!(
            repeated.primary_action(),
            Some(PrimaryActionOutcome::ShoveRejected(
                PrimitiveAttackError::Recovering { .. }
            ))
        ));
    }
    assert_eq!(state, replay);
    while state.tick().value() < 185 {
        let left = advance_control_tick(&registries, &mut state).unwrap();
        let right = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(left, right);
    }
    assert_eq!(state, replay);
    assert_eq!(
        crate::kobolds::available_nearby_kobold_scouts(&registries, &state)
            .find(|resident| resident.role() == KoboldRole::Scout)
            .map(|resident| resident.position()),
        Some(den),
        "the struck scout must withdraw through a real patrol edge"
    );
}

/// Assemble the actual authored inputs, then carry and select one equipment instance.
fn arm_player(
    registries: &Registries,
    state: &mut AppState,
    weapon: crate::equipment::EquipmentDefinitionId,
) -> crate::equipment::EquipmentId {
    use crate::core::quantity::Temperature;
    use crate::equipment::validate_assemble_equipment;
    use crate::inventory::deposit_lot_for_test;
    use crate::logistics::{validate_pickup_equipment, validate_set_active_selection};

    let assembly = registries
        .equipment()
        .get_equipment(weapon)
        .unwrap()
        .assembly_profile()
        .unwrap()
        .clone();
    let source = state.logistics().player().unwrap().carried_stockpile();
    for input in assembly.inputs() {
        deposit_lot_for_test(
            registries,
            state,
            source,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let equipment = validate_assemble_equipment(registries, state, weapon, source)
        .unwrap()
        .commit(state)
        .unwrap();
    validate_pickup_equipment(registries, state, equipment)
        .unwrap()
        .commit(state)
        .unwrap();
    validate_set_active_selection(
        registries,
        state,
        Some(PlayerActiveSelection::equipment(equipment)),
    )
    .unwrap()
    .commit(state)
    .unwrap();
    equipment
}

#[test]
fn primitive_club_and_spear_have_real_material_reach_wear_and_recovery() {
    use crate::content::{EQUIPMENT_STONE_SPEAR, EQUIPMENT_WOODEN_CLUB};
    use crate::logistics::validate_set_active_selection;
    use crate::matter::calculate_matter_accounting;
    use crate::simulation::{
        PlayerDirectControlInput, PrimaryActionOutcome, PrimaryControlIntent, advance_control_tick,
        advance_control_tick_with_direct_input,
    };

    let (registries, base, _, junction, _) = opening_contact_world();
    let west = ViewDirection::new(-1_000_000, 0, 0).unwrap();
    let player_start = VoxelCoord::new(4, 0, 0);
    for (kind, expected_hit, wear) in [
        (EQUIPMENT_WOODEN_CLUB, false, 3_000),
        (EQUIPMENT_STONE_SPEAR, true, 4_500),
    ] {
        let mut game = base.clone();
        validate_admit_player(&registries, &game, player_start)
            .unwrap()
            .commit(&mut game)
            .unwrap();
        let weapon = arm_player(&registries, &mut game, kind);
        let assessment = assess_player_melee(&registries, &game).unwrap();
        assert_eq!(assessment.weapon(), Some(weapon));
        assert!(assessment.ready());
        assert_eq!(
            assessment.condition(),
            Some(crate::maintenance::Condition::PRISTINE)
        );
        assert_eq!(
            assessment.reach_millimeters(),
            if expected_hit { 2_750 } else { 1_500 },
        );
        let material_before = calculate_matter_accounting(&game).unwrap().total();
        let condition_before = game.equipment().get_equipment(weapon).unwrap().condition();
        let action =
            PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::strike(west));
        assert!(matches!(
            validate_primitive_shove(&registries, &game, west),
            Err(PrimitiveAttackError::OccupiedHand)
        ));
        let resolved =
            advance_control_tick_with_direct_input(&registries, &mut game, action).unwrap();
        let result = resolved.primary_action();
        if expected_hit {
            assert!(
                matches!(
                    result,
                    Some(PrimaryActionOutcome::Strike(PrimitiveAttackOutcome::Startled {
                        resident: KoboldRole::Scout, position
                    })) if *position == junction
                ),
                "spear contact result: {result:?}"
            );
        } else {
            assert_eq!(
                result,
                Some(&PrimaryActionOutcome::Strike(PrimitiveAttackOutcome::Miss))
            );
        }
        assert_eq!(game.combat().revision(), 1);
        let assessment = assess_player_melee(&registries, &game).unwrap();
        assert_eq!(
            assessment.remaining_recovery_ticks(),
            assessment.recovery_control_ticks(),
        );
        assert_eq!(
            game.equipment()
                .get_equipment(weapon)
                .unwrap()
                .condition()
                .parts_per_million(),
            condition_before.parts_per_million() - wear,
        );
        assert_eq!(
            calculate_matter_accounting(&game).unwrap().total(),
            material_before
        );
        let retry = advance_control_tick_with_direct_input(&registries, &mut game, action).unwrap();
        assert!(matches!(
            retry.primary_action(),
            Some(PrimaryActionOutcome::StrikeRejected(
                PrimitiveAttackError::Recovering { .. }
            ))
        ));
        // Switching back to bare hands cannot cancel the spear/club recovery.
        validate_set_active_selection(&registries, &game, None)
            .unwrap()
            .commit(&mut game)
            .unwrap();
        assert!(matches!(
            validate_primitive_shove(&registries, &game, west),
            Err(PrimitiveAttackError::Recovering { .. })
        ));
        let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(
            &serde_json::to_vec(&SaveEnvelope::new(&registries, &game)).unwrap(),
        )
        .unwrap()
        .into_state(&registries)
        .unwrap();
        assert_eq!(validate_loaded_state(&registries, &game), Ok(()));
        for _ in 0..29 {
            assert_eq!(
                advance_control_tick(&registries, &mut game).unwrap(),
                advance_control_tick(&registries, &mut replay).unwrap(),
            );
        }
        assert_eq!(game, replay);
        assert!(assess_player_melee(&registries, &game).unwrap().ready());
        assert!(validate_primitive_shove(&registries, &game, west).is_ok());
    }
}

#[test]
fn stone_wall_stops_spear_contact_and_failed_commit_spends_nothing() {
    use crate::content::{BLOCK_STATE_STONE, EQUIPMENT_STONE_SPEAR};
    use crate::equipment::EquipmentId;
    use crate::logistics::validate_set_active_selection;
    use crate::world::set_generated_block_for_test;
    let (registries, mut game, _, _, _) = opening_contact_world();
    validate_admit_player(&registries, &game, VoxelCoord::new(4, 0, 0))
        .unwrap()
        .commit(&mut game)
        .unwrap();
    let weapon: EquipmentId = arm_player(&registries, &mut game, EQUIPMENT_STONE_SPEAR);
    let west = ViewDirection::new(-1_000_000, 0, 0).unwrap();
    let pending = validate_primitive_strike(&registries, &game, west).unwrap();
    let condition_before = game.equipment().get_equipment(weapon).unwrap().condition();
    validate_set_active_selection(&registries, &game, None)
        .unwrap()
        .commit(&mut game)
        .unwrap();
    assert_eq!(
        pending.commit(&mut game),
        Err(PrimitiveAttackCommitError::StaleLogisticsRevision)
    );
    assert_eq!(
        game.equipment().get_equipment(weapon).unwrap().condition(),
        condition_before
    );
    assert_eq!(game.combat().revision(), 0);
    validate_set_active_selection(
        &registries,
        &game,
        Some(PlayerActiveSelection::equipment(weapon)),
    )
    .unwrap()
    .commit(&mut game)
    .unwrap();
    set_generated_block_for_test(
        registries.world(),
        &mut game,
        VoxelCoord::new(2, 1, 0),
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        validate_primitive_strike(&registries, &game, west)
            .unwrap()
            .commit(&mut game),
        Ok(PrimitiveAttackOutcome::Miss)
    );
    assert_eq!(validate_loaded_state(&registries, &game), Ok(()));
}

/// A short weapon must not probe an ungenerated chunk outside its own reach.
#[test]
fn melee_miss_ignores_unknown_world_beyond_authored_reach() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
        validate_generate_chunk,
    };
    let registries = build_registries();
    let mut game = AppState::new();
    validate_generate_chunk(
        &registries,
        &game,
        WorldGenerationSeed::new(9),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut game)
    .unwrap();
    for x in 13..=15 {
        set_generated_block_for_test(
            registries.world(),
            &mut game,
            VoxelCoord::new(x, -1, 0),
            BLOCK_STATE_STONE,
        );
        for y in 0..=2 {
            set_generated_block_for_test(
                registries.world(),
                &mut game,
                VoxelCoord::new(x, y, 0),
                BlockStateId::AIR,
            );
        }
    }
    validate_admit_player(&registries, &game, VoxelCoord::new(13, 0, 0))
        .unwrap()
        .commit(&mut game)
        .unwrap();
    assert_eq!(
        validate_primitive_shove(&registries, &game, east())
            .unwrap()
            .commit(&mut game),
        Ok(PrimitiveAttackOutcome::Miss),
    );
    assert_eq!(game.combat().revision(), 1);
    assert_eq!(validate_loaded_state(&registries, &game), Ok(()));
}

#[test]
fn repeated_club_contacts_stagger_scout_then_physical_recovery_replays() {
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::matter::calculate_matter_accounting;
    use crate::simulation::{
        PlayerDirectControlInput, PrimaryActionOutcome, PrimaryControlIntent, advance_control_tick,
        advance_control_tick_with_direct_input,
    };

    let (registries, mut state, den, junction, tip) = opening_contact_world();
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let club = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let initial_mass = calculate_matter_accounting(&state).unwrap().total();
    let west = ViewDirection::new(-1_000_000, 0, 0).unwrap();
    let strike = PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::strike(west));

    for (attack_index, expected_balance) in [720_000, 440_840, 162_520].into_iter().enumerate() {
        // 22 control ticks is the authored club recovery. There is no bypass
        // through fast repeated primary requests.
        if attack_index > 0 {
            for _ in 0..22 {
                let _ = advance_control_tick(&registries, &mut state).unwrap();
            }
        }
        let result =
            advance_control_tick_with_direct_input(&registries, &mut state, strike).unwrap();
        let outcome = result.primary_action().unwrap();
        if attack_index < 2 {
            assert!(
                matches!(
                    outcome,
                    PrimaryActionOutcome::Strike(PrimitiveAttackOutcome::Startled {
                        resident: KoboldRole::Scout, position,
                    }) if *position == junction
                ),
                "impact {}: {outcome:?}",
                attack_index + 1
            );
        } else {
            assert!(
                matches!(
                    outcome,
                    PrimaryActionOutcome::Strike(PrimitiveAttackOutcome::Staggered {
                        resident: KoboldRole::Scout, position, ..
                    }) if *position == junction
                ),
                "stagger outcome: {outcome:?}"
            );
        }
        let observed = crate::kobolds::available_nearby_kobold_scouts(&registries, &state)
            .find(|resident| resident.role() == KoboldRole::Scout)
            .unwrap();
        assert_eq!(
            observed.balance_ppm(),
            Some(expected_balance),
            "impact {}",
            attack_index + 1
        );
        assert_eq!(observed.is_staggered(), attack_index == 2);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial_mass
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(club)
            .unwrap()
            .condition()
            .parts_per_million(),
        991_000
    );

    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    for target_tick in [185_u64, 190_u64] {
        while state.tick().value() < target_tick {
            assert_eq!(
                advance_control_tick(&registries, &mut state).unwrap(),
                advance_control_tick(&registries, &mut replay).unwrap(),
            );
        }
        let observed = crate::kobolds::available_nearby_kobold_scouts(&registries, &state)
            .find(|resident| resident.role() == KoboldRole::Scout)
            .unwrap();
        let expected_position = if target_tick == 185 { junction } else { den };
        assert_eq!(observed.position(), expected_position);
        assert_eq!(observed.is_staggered(), target_tick == 185);
        assert_eq!(state, replay);
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn watchman_can_be_staggered_without_affecting_a_remote_scout() {
    use crate::content::{BLOCK_STATE_STONE, EQUIPMENT_WOODEN_CLUB};
    use crate::simulation::{
        PlayerDirectControlInput, PrimaryActionOutcome, PrimaryControlIntent, advance_control_tick,
        advance_control_tick_with_direct_input,
    };
    use crate::world::{BlockStateId, set_generated_block_for_test};

    let (registries, mut state, den, junction, _) = opening_contact_world();
    let player_at = VoxelCoord::new(0, 0, 1);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(0, -1, 1),
        BLOCK_STATE_STONE,
    );
    for y in 0..=2 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(0, y, 1),
            BlockStateId::AIR,
        );
    }
    validate_admit_player(&registries, &state, player_at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _club = arm_player(&registries, &mut state, EQUIPMENT_WOODEN_CLUB);
    let north = ViewDirection::new(0, 0, -1_000_000).unwrap();
    let strike =
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::strike(north));
    let scout_before = crate::kobolds::available_nearby_kobold_scouts(&registries, &state)
        .find(|resident| resident.role() == KoboldRole::Scout)
        .unwrap()
        .balance_ppm();
    for index in 0..3 {
        if index > 0 {
            for _ in 0..22 {
                let _ = advance_control_tick(&registries, &mut state).unwrap();
            }
        }
        let result =
            advance_control_tick_with_direct_input(&registries, &mut state, strike).unwrap();
        assert!(
            matches!(
                result.primary_action(),
                Some(PrimaryActionOutcome::Strike(
                    PrimitiveAttackOutcome::Startled { resident: KoboldRole::Watchman, position }
                    | PrimitiveAttackOutcome::Staggered { resident: KoboldRole::Watchman, position, .. }
                )) if *position == den
            ),
            "watchman contact: {:?}",
            result.primary_action()
        );
    }
    let seen =
        crate::kobolds::available_nearby_kobold_scouts(&registries, &state).collect::<Vec<_>>();
    assert_eq!(
        seen.iter()
            .find(|resident| resident.role() == KoboldRole::Watchman)
            .unwrap()
            .is_staggered(),
        true,
    );
    assert_eq!(
        seen.iter()
            .find(|resident| resident.role() == KoboldRole::Scout)
            .unwrap()
            .balance_ppm(),
        scout_before
    );
    assert_eq!(
        close_contact_at(&state, junction),
        Ok(Some((KoboldRole::Scout, junction)))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn forged_kobold_balance_cannot_enter_runtime() {
    let (registries, state, ..) = opening_contact_world();
    let base = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    for (value, error) in [
        (1_000_001_u32, "kobold balance exceeds stable footing"),
        (
            800_000_u32,
            "kobold lost balance without recorded local impact",
        ),
    ] {
        let mut forged = base.clone();
        forged["state"]["systems"]["kobolds"]["colony"]["scout_balance_ppm"] =
            serde_json::json!(value);
        let encoded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
        assert_eq!(
            encoded.into_state(&registries),
            Err(crate::persistence::LoadError::InvalidState(
                crate::core::state::StateValidationError::Kobolds(error)
            ))
        );
    }
    let mut unearned = base;
    unearned["state"]["systems"]["kobolds"]["colony"]["scout_shoved_at"] = serde_json::json!(1_u64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(unearned).unwrap();
    assert_eq!(
        decoded.into_state(&registries),
        Err(crate::persistence::LoadError::InvalidState(
            crate::core::state::StateValidationError::Kobolds(
                "kobold impact record lacks authoritative player combat history"
            )
        ))
    );
}

#[test]
fn forged_weapon_recovery_duration_fails_trusted_load() {
    let registries = build_registries();
    let mut game = AppState::new();
    game.combat_state_mut()
        .apply_attack(0, ControlTick::ZERO, 61);
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
