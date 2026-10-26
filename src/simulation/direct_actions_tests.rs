//! A held work command is quiet, but explicit combat intents must report why they cannot fire.

use super::*;
use crate::combat::PrimitiveAttackError;
use crate::content::{
    FORM_CHIP, MATERIAL_STONE, SURFACE_GATHERING_HAND_COLLECT_STONE_CHIPS, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::inventory::PlayerStoragePlacementError;
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{PrimaryActionOutcome, advance_control_tick_with_direct_input};
use crate::spatial::{ViewDirection, VoxelCoord};
use crate::surface::{
    SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources,
    validate_start_player_surface_gathering,
};

#[test]
fn busy_player_combat_controls_report_rejections_without_interrupting_active_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    let here = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, here)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let source = validate_create_surface_resources(
        &registries,
        &state,
        [SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            here,
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(50_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()],
    )
    .unwrap();
    let resource = source.first_resource_id();
    source.commit(&mut state).unwrap();
    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE_CHIPS,
        resource,
        Mass::from_milligrams(50_000),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let started = state.player_work().active();
    let combat_revision = state.combat().revision();
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let available = state
        .surface()
        .available_resources_at(Some(here))
        .find(|record| record.id() == resource)
        .unwrap()
        .remaining_mass();
    let direction = ViewDirection::new(1_000_000, 0, 0).unwrap();
    let intents = [
        (
            PrimaryControlIntent::shove(direction),
            PrimaryActionOutcome::ShoveRejected(PrimitiveAttackError::PlayerBusy),
        ),
        (
            PrimaryControlIntent::strike(direction),
            PrimaryActionOutcome::StrikeRejected(PrimitiveAttackError::PlayerBusy),
        ),
        (
            PrimaryControlIntent::prepare_strike(direction),
            PrimaryActionOutcome::PreparationRejected(PrimitiveAttackError::PlayerBusy),
        ),
        (
            PrimaryControlIntent::release_strike(direction),
            PrimaryActionOutcome::PreparedStrikeRejected(PrimitiveAttackError::NotPreparing),
        ),
        (
            PrimaryControlIntent::cancel_strike(),
            PrimaryActionOutcome::PreparationRejected(PrimitiveAttackError::PlayerBusy),
        ),
    ];

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for (intent, rejection) in intents {
        let command = PlayerDirectControlInput::NEUTRAL.with_primary(intent);
        let current =
            advance_control_tick_with_direct_input(&registries, &mut state, command).unwrap();
        let restored =
            advance_control_tick_with_direct_input(&registries, &mut replay, command).unwrap();
        assert_eq!(current, restored);
        assert_eq!(current.primary_action(), Some(&rejection));
        assert_eq!(state.player_work().active(), started);
        assert_eq!(state.combat().revision(), combat_revision);
        assert_eq!(
            calculate_matter_accounting(&state).unwrap().total(),
            matter_before
        );
        assert_eq!(
            state
                .surface()
                .available_resources_at(Some(here))
                .find(|record| record.id() == resource)
                .unwrap()
                .remaining_mass(),
            available
        );
        assert_eq!(state, replay);
    }

    for (secondary, expected) in [
        (SecondaryControlIntent::use_active(direction), None),
        (
            SecondaryControlIntent::place_storage(
                crate::content::STORAGE_ROUGH_TIMBER_FIELD_BOX,
                direction,
            ),
            Some(SecondaryActionOutcome::StoragePlacementRejected {
                definition: crate::content::STORAGE_ROUGH_TIMBER_FIELD_BOX,
                error: PlayerStoragePlacementError::PlayerBusy,
            }),
        ),
    ] {
        let command = PlayerDirectControlInput::NEUTRAL.with_secondary(secondary);
        let current =
            advance_control_tick_with_direct_input(&registries, &mut state, command).unwrap();
        let restored =
            advance_control_tick_with_direct_input(&registries, &mut replay, command).unwrap();
        assert_eq!(current, restored);
        assert_eq!(
            current.secondary_action(),
            expected.as_ref(),
            "held contextual use stays one-shot, but explicit storage placement must report busy"
        );
        assert_eq!(state.player_work().active(), started);
        assert_eq!(state.combat().revision(), combat_revision);
        assert_eq!(
            calculate_matter_accounting(&state).unwrap().total(),
            matter_before
        );
        assert_eq!(state, replay);
    }

    let break_while_busy = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(direction)),
    )
    .unwrap();
    assert_eq!(
        break_while_busy.primary_action(),
        None,
        "held excavation remains quiet until the current work releases"
    );
    assert_eq!(state.player_work().active(), started);
    let replayed_break = advance_control_tick_with_direct_input(
        &registries,
        &mut replay,
        PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(direction)),
    )
    .unwrap();
    assert_eq!(break_while_busy, replayed_break);
    while state.player_work().active().is_some() {
        let current = crate::simulation::advance_control_tick(&registries, &mut state).unwrap();
        let restored = crate::simulation::advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(current, restored);
        assert_eq!(
            current.primary_action(),
            None,
            "rejected attacks must not queue"
        );
    }
    assert_eq!(state.combat().revision(), combat_revision);
    assert_eq!(state, replay);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    // The busy response is not a queued placement and must disappear after work releases.
    let place =
        PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::place_storage(
            crate::content::STORAGE_ROUGH_TIMBER_FIELD_BOX,
            direction,
        ));
    let current = advance_control_tick_with_direct_input(&registries, &mut state, place).unwrap();
    let restored = advance_control_tick_with_direct_input(&registries, &mut replay, place).unwrap();
    assert_eq!(current, restored);
    assert!(current.secondary_action().is_some());
    assert!(!matches!(
        current.secondary_action(),
        Some(SecondaryActionOutcome::StoragePlacementRejected {
            error: PlayerStoragePlacementError::PlayerBusy,
            ..
        })
    ));
    assert_eq!(state, replay);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
}
