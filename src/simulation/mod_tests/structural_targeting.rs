//! Direct interaction chooses real structural matter over overlapping unbuilt plans.

use super::*;
use crate::content::{MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION};
use crate::core::quantity::{Area, Length};
use crate::spatial::VoxelBounds;
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
};

#[test]
fn queued_contextual_use_rejects_sub_voxel_movement_after_save_load() {
    use crate::core::state::validate_loaded_state;
    use crate::logistics::validate_buffer_player_secondary_use;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let (registries, mut state, _) = generated_grounded_player_at(0, 0);
    let _ = advance_control_tick(&registries, &mut state).unwrap();
    let origin = state.logistics().player().unwrap().body().foot_position();
    let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
    validate_buffer_player_secondary_use(&state, direction)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        state
            .logistics()
            .pending_secondary_use()
            .unwrap()
            .position(),
        origin
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let move_input = PlayerDirectControlInput::new(PlayerControlInput::new(
        HorizontalControlInput::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0).unwrap(),
    ));
    assert_eq!(
        advance_control_tick_with_direct_input(&registries, &mut state, move_input).unwrap(),
        advance_control_tick_with_direct_input(&registries, &mut replay, move_input).unwrap()
    );
    let moved = state.logistics().player().unwrap().body().foot_position();
    assert_ne!(
        moved, origin,
        "the player must actually move within the voxel"
    );
    assert_eq!(moved.containing_voxel(), origin.containing_voxel());

    for index in 2..CONTROL_TICKS_PER_SIMULATION_TICK {
        let current = advance_control_tick(&registries, &mut state).unwrap();
        let restored = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(current, restored);
        if index + 1 == CONTROL_TICKS_PER_SIMULATION_TICK {
            assert_eq!(
                current.secondary_action(),
                Some(&SecondaryActionOutcome::BufferedContextChanged)
            );
        } else {
            assert_eq!(current.secondary_action(), None);
        }
    }
    assert_eq!(state, replay);
    assert!(state.logistics().pending_secondary_use().is_none());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn queued_structural_break_rejects_sub_voxel_retargeting_after_save_load() {
    use crate::core::state::validate_loaded_state;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let target = VoxelCoord::new(2, floor_y + 1, 0);
    for x in 1..=2 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, floor_y + 1, 0),
            BlockStateId::AIR,
        );
    }
    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(target, VoxelCoord::new(3, floor_y + 2, 1)).unwrap(),
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(1_000_000),
    )
    .unwrap();
    let panel = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, panel, FORM_LOG);
    let original_structures = state.structures().clone();
    let origin = state.logistics().player().unwrap().body().foot_position();
    let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(direction)),
    )
    .unwrap();
    assert_eq!(
        queued.primary_action(),
        Some(&PrimaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    assert_eq!(
        state
            .logistics()
            .pending_primary_break()
            .unwrap()
            .position(),
        origin
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let move_input = PlayerDirectControlInput::new(PlayerControlInput::new(
        HorizontalControlInput::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0).unwrap(),
    ));
    assert_eq!(
        advance_control_tick_with_direct_input(&registries, &mut state, move_input).unwrap(),
        advance_control_tick_with_direct_input(&registries, &mut restored, move_input).unwrap()
    );
    let moved = state.logistics().player().unwrap().body().foot_position();
    assert_ne!(
        moved, origin,
        "the player must actually move within the voxel"
    );
    assert_eq!(moved.containing_voxel(), origin.containing_voxel());
    assert_eq!(
        state
            .logistics()
            .pending_primary_break()
            .unwrap()
            .position(),
        origin
    );

    for index in 2..CONTROL_TICKS_PER_SIMULATION_TICK {
        let current = advance_control_tick(&registries, &mut state).unwrap();
        let replay = advance_control_tick(&registries, &mut restored).unwrap();
        assert_eq!(current, replay);
        if index + 1 == CONTROL_TICKS_PER_SIMULATION_TICK {
            assert_eq!(
                current.primary_action(),
                Some(&PrimaryActionOutcome::BufferedContextChanged)
            );
        } else {
            assert_eq!(current.primary_action(), None);
        }
    }
    assert_eq!(state, restored);
    assert_eq!(state.structures(), &original_structures);
    assert!(state.logistics().pending_primary_break().is_none());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn combat_input_replaces_buffered_structural_break_without_later_dismantling() {
    use crate::core::state::validate_loaded_state;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let target = VoxelCoord::new(2, floor_y + 1, 0);
    for x in 1..=2 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, floor_y + 1, 0),
            BlockStateId::AIR,
        );
    }
    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(target, VoxelCoord::new(3, floor_y + 2, 1)).unwrap(),
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(1_000_000),
    )
    .unwrap();
    let panel = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, panel, FORM_LOG);
    let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(direction)),
    )
    .unwrap();
    assert_eq!(
        queued.primary_action(),
        Some(&PrimaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    assert_eq!(
        state
            .logistics()
            .pending_primary_break()
            .map(|pending| pending.element()),
        Some(panel)
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let shove =
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::shove(direction));
    let current = advance_control_tick_with_direct_input(&registries, &mut state, shove).unwrap();
    let replay = advance_control_tick_with_direct_input(&registries, &mut restored, shove).unwrap();
    assert_eq!(current, replay);
    assert!(
        matches!(
            current.primary_action(),
            Some(PrimaryActionOutcome::Shove(_) | PrimaryActionOutcome::ShoveRejected(_))
        ),
        "the new combat command must be handled immediately"
    );
    assert!(
        state.logistics().pending_primary_break().is_none(),
        "shoving should cancel an older queued structural break"
    );
    for _ in 2..CONTROL_TICKS_PER_SIMULATION_TICK {
        let current = advance_control_tick(&registries, &mut state).unwrap();
        let replay = advance_control_tick(&registries, &mut restored).unwrap();
        assert_eq!(current, replay);
        assert_eq!(current.primary_action(), None);
    }
    assert_eq!(state, restored);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn direct_targeting_prefers_materialized_member_over_overlapping_inert_plan() {
    // Both a full-footprint panel and a thin supporting member must be
    // selectable without an unrealized plan intercepting the physical target.
    for cross_section in [1_000_000, 5_000] {
        let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
        let target = VoxelCoord::new(2, floor_y + 1, 0);
        for x in 1..=2 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(x, floor_y + 1, 0),
                BlockStateId::AIR,
            );
        }
        let geometry = StructuralElementGeometry::new(
            VoxelBounds::new(target, VoxelCoord::new(3, floor_y + 2, 1)).unwrap(),
            Length::from_micrometers(1_000_000),
            Area::from_square_millimeters(cross_section),
        )
        .unwrap();
        let inert = add_structural_element(
            &registries,
            &mut state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            geometry,
            true,
        )
        .unwrap();
        let materialized = add_structural_element(
            &registries,
            &mut state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            geometry,
            true,
        )
        .unwrap();
        materialize_structural_element_for_test(&registries, &mut state, materialized, FORM_LOG);

        let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
        let outcome = advance_control_tick_with_direct_input(
            &registries,
            &mut state,
            PlayerDirectControlInput::NEUTRAL
                .with_primary(PrimaryControlIntent::break_target(direction)),
        )
        .unwrap();
        assert_eq!(
            outcome.primary_action(),
            Some(&PrimaryActionOutcome::QueuedUntilSimulationBoundary)
        );
        assert_eq!(
            state
                .logistics()
                .pending_primary_break()
                .map(|pending| pending.element()),
            Some(materialized),
            "cross-section {cross_section} must target real matter, not an inert plan"
        );
        assert_ne!(materialized, inert);
        let held = advance_control_tick_with_direct_input(
            &registries,
            &mut state,
            PlayerDirectControlInput::NEUTRAL
                .with_primary(PrimaryControlIntent::break_target(direction)),
        )
        .unwrap();
        assert_eq!(
            held.primary_action(),
            Some(&PrimaryActionOutcome::QueuedUntilSimulationBoundary),
            "holding the same buffered structural break must not return AlreadyPending"
        );
        assert_eq!(
            state
                .logistics()
                .pending_primary_break()
                .map(|pending| pending.element()),
            Some(materialized)
        );
        let opposite = ViewDirection::new(-VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
        let redirected = advance_control_tick_with_direct_input(
            &registries,
            &mut state,
            PlayerDirectControlInput::NEUTRAL
                .with_primary(PrimaryControlIntent::break_target(opposite)),
        )
        .unwrap();
        assert!(
            matches!(
                redirected.primary_action(),
                Some(
                    PrimaryActionOutcome::BlockExcavationStarted(_)
                        | PrimaryActionOutcome::BlockExcavationRejected(_)
                )
            ),
            "new aim must resolve its own target rather than report old buffer"
        );
        assert!(
            state.logistics().pending_primary_break().is_none(),
            "retargeting must cancel the old structural-break request"
        );
    }
}

#[test]
fn inert_front_blueprint_cannot_occlude_reachable_real_structural_member() {
    // Both use the same ray. An unbuilt blueprint must be selectable when
    // alone, but it is not physical cover for an embodied member behind it.
    for with_real_member in [false, true] {
        let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
        let first = VoxelCoord::new(1, floor_y + 1, 0);
        let second = VoxelCoord::new(2, floor_y + 1, 0);
        for target in [first, second] {
            set_generated_block_for_test(registries.world(), &mut state, target, BlockStateId::AIR);
        }
        let add = |state: &mut AppState, target: VoxelCoord| {
            let bounds = VoxelBounds::new(
                target,
                VoxelCoord::new(target.x() + 1, target.y() + 1, target.z() + 1),
            )
            .unwrap();
            let geometry = StructuralElementGeometry::new(
                bounds,
                Length::from_micrometers(1_000_000),
                Area::from_square_millimeters(5_000),
            )
            .unwrap();
            add_structural_element(
                &registries,
                state,
                STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
                MATERIAL_WOOD,
                geometry,
                true,
            )
            .unwrap()
        };
        let blueprint = add(&mut state, first);
        let real = with_real_member.then(|| {
            let element = add(&mut state, second);
            materialize_structural_element_for_test(&registries, &mut state, element, FORM_LOG);
            element
        });
        assert_eq!(
            crate::core::state::validate_loaded_state(&registries, &state),
            Ok(())
        );
        let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
        let serialized = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        let mut reloaded = serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        let intent = PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(direction));
        let response =
            advance_control_tick_with_direct_input(&registries, &mut state, intent).unwrap();
        let replay =
            advance_control_tick_with_direct_input(&registries, &mut reloaded, intent).unwrap();
        assert_eq!(response, replay);
        assert_eq!(
            response.primary_action(),
            Some(&PrimaryActionOutcome::QueuedUntilSimulationBoundary)
        );
        assert_eq!(
            state
                .logistics()
                .pending_primary_break()
                .map(|pending| pending.element()),
            Some(real.unwrap_or(blueprint)),
            "a nonphysical blueprint must not shadow later embodied matter"
        );
        assert_eq!(state, reloaded);
    }
}

#[test]
fn nearby_blueprint_remains_selectable_before_unknown_world_authority() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(14, 0);
    let position = VoxelCoord::new(15, floor_y + 1, 0);
    set_generated_block_for_test(registries.world(), &mut state, position, BlockStateId::AIR);
    let plan = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            VoxelBounds::new(position, VoxelCoord::new(16, floor_y + 2, 1)).unwrap(),
            Length::from_micrometers(1_000_000),
            Area::from_square_millimeters(5_000),
        )
        .unwrap(),
        true,
    )
    .unwrap();
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    // The ray encounters the visible plan before entering an ungenerated chunk.
    let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
    let outcome = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(direction)),
    )
    .unwrap();
    assert_eq!(
        outcome.primary_action(),
        Some(&PrimaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    assert_eq!(
        state
            .logistics()
            .pending_primary_break()
            .map(|pending| pending.element()),
        Some(plan)
    );
}
