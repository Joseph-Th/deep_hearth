//! One hand gesture per 20 Hz sample, including dropping the selected item.

use super::*;

use crate::combat::{PrimitiveAttackError, PrimitivePreparationOutcome};
use crate::content::{
    EQUIPMENT_WOODEN_CLUB, FORM_CHIP, MATERIAL_STONE, STORAGE_ROUGH_TIMBER_FIELD_BOX,
    build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::equipment::validate_assemble_equipment;
use crate::inventory::deposit_lot_for_test;
use crate::logistics::{
    PlayerActiveSelection, validate_pickup_equipment, validate_set_active_selection,
};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick_with_direct_input;
use crate::spatial::{ViewDirection, VoxelCoord};

#[test]
fn combat_windup_preempts_simultaneous_weapon_drop() {
    let registries = build_registries();
    let mut state = AppState::new();
    let here = VoxelCoord::new(0, 0, 0);
    let carried = validate_admit_player(&registries, &state, here)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let assembly = registries
        .equipment()
        .get_equipment(EQUIPMENT_WOODEN_CLUB)
        .unwrap()
        .assembly_profile()
        .unwrap();
    for input in assembly.inputs() {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let club = validate_assemble_equipment(&registries, &state, EQUIPMENT_WOODEN_CLUB, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_pickup_equipment(&registries, &state, club)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::equipment(club)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();

    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let combat_revision = state.combat().revision();
    let condition = state.equipment().get_equipment(club).unwrap().condition();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let direction = ViewDirection::new(1_000_000, 0, 0).unwrap();
    let input = PlayerDirectControlInput::NEUTRAL
        .with_primary(PrimaryControlIntent::prepare_strike(direction))
        .with_drop_active_selection();
    let outcome = advance_control_tick_with_direct_input(&registries, &mut state, input).unwrap();
    assert_eq!(
        outcome,
        advance_control_tick_with_direct_input(&registries, &mut replay, input).unwrap()
    );
    assert!(matches!(
        outcome.primary_action(),
        Some(PrimaryActionOutcome::Preparation(
            PrimitivePreparationOutcome::Prepared { .. }
        ))
    ));
    assert_eq!(outcome.secondary_action(), None);
    assert_eq!(outcome.active_selection_drop(), None);
    assert_eq!(state.combat().revision(), combat_revision + 1);
    assert!(state.combat().has_prepared_strike());
    assert_eq!(
        state.equipment().get_equipment(club).unwrap().condition(),
        condition
    );
    assert!(state.logistics().is_equipment_carried(club));
    assert_eq!(
        state.logistics().active_selection(),
        Some(PlayerActiveSelection::equipment(club))
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(state, replay);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    // Releasing the stance and discarding the weapon are two distinct hand
    // gestures. Both may be requested on one frame, but cannot both commit.
    let cancel_and_drop = PlayerDirectControlInput::NEUTRAL
        .with_primary(PrimaryControlIntent::cancel_strike())
        .with_drop_active_selection();
    let cancelled =
        advance_control_tick_with_direct_input(&registries, &mut state, cancel_and_drop).unwrap();
    assert_eq!(
        cancelled,
        advance_control_tick_with_direct_input(&registries, &mut replay, cancel_and_drop).unwrap()
    );
    assert_eq!(
        cancelled.primary_action(),
        Some(&PrimaryActionOutcome::Preparation(
            PrimitivePreparationOutcome::Cancelled
        ))
    );
    assert_eq!(cancelled.active_selection_drop(), None);
    assert!(!state.combat().has_prepared_strike());
    assert!(state.logistics().is_equipment_carried(club));
    assert_eq!(state, replay);

    // A fresh drop on the next control frame can now change custody.
    let drop = PlayerDirectControlInput::NEUTRAL.with_drop_active_selection();
    let dropped = advance_control_tick_with_direct_input(&registries, &mut state, drop).unwrap();
    assert_eq!(
        dropped,
        advance_control_tick_with_direct_input(&registries, &mut replay, drop).unwrap()
    );
    assert_eq!(
        dropped.active_selection_drop(),
        Some(&ActiveSelectionDropOutcome::EquipmentDropped {
            equipment: club,
            position: here,
        })
    );
    assert!(!state.logistics().is_equipment_carried(club));
    assert_eq!(
        state.equipment().get_equipment(club).unwrap().condition(),
        condition
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(state, replay);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn material_drop_and_primary_shove_obey_distinct_hand_priorities() {
    let registries = build_registries();
    let here = VoxelCoord::new(0, 0, 0);
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let mass = Mass::from_milligrams(100_000);
    let direction = ViewDirection::new(1_000_000, 0, 0).unwrap();
    for secondary in [false, true] {
        let mut state = AppState::new();
        let carried = validate_admit_player(&registries, &state, here)
            .unwrap()
            .commit(&mut state)
            .unwrap()
            .carried_stockpile();
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
        validate_set_active_selection(
            &registries,
            &state,
            Some(PlayerActiveSelection::material(commodity)),
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        let conserved = calculate_matter_accounting(&state).unwrap().total();
        let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        let input = if secondary {
            PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::place_storage(
                STORAGE_ROUGH_TIMBER_FIELD_BOX,
                direction,
            ))
        } else {
            PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::shove(direction))
        }
        .with_drop_active_selection();
        let outcome =
            advance_control_tick_with_direct_input(&registries, &mut state, input).unwrap();
        assert_eq!(
            outcome,
            advance_control_tick_with_direct_input(&registries, &mut replay, input).unwrap()
        );
        if secondary {
            assert_eq!(outcome.primary_action(), None);
        } else {
            assert_eq!(
                outcome.primary_action(),
                Some(&PrimaryActionOutcome::ShoveRejected(
                    PrimitiveAttackError::OccupiedHand
                ))
            );
        }
        assert_eq!(outcome.secondary_action(), None);
        if secondary {
            assert!(matches!(
                outcome.active_selection_drop(),
                Some(ActiveSelectionDropOutcome::MaterialDropped(_))
            ));
        } else {
            assert_eq!(outcome.active_selection_drop(), None);
        }
        assert_eq!(state.player_work().active(), None);
        assert!(!state.logistics().has_pending_direct_action());
        assert_eq!(
            state
                .inventory()
                .get_stockpile(carried)
                .unwrap()
                .stored_mass(),
            if secondary { Mass::ZERO } else { mass }
        );
        assert_eq!(
            calculate_matter_accounting(&state).unwrap().total(),
            conserved
        );
        assert_eq!(state, replay);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
}
