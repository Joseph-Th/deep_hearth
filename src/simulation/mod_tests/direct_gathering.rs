//! Exact-local actor choice when ordinary surface resources share one position.

use super::*;
use crate::content::{FORM_FOOD, MATERIAL_BERRIES, MATERIAL_WILD_NUTS};
use crate::material::MaterialComposition;
use crate::matter::calculate_matter_accounting;
use crate::surface::{
    SurfaceGatheringError, SurfaceResourceId, SurfaceResourceKind, SurfaceResourceSpec,
    validate_create_surface_resources,
};

#[test]
fn full_carrying_capacity_rejects_explicit_and_empty_hand_gathering_before_queueing() {
    use crate::content::{FORM_LUMP, MATERIAL_STONE};

    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let temperature = Temperature::from_millikelvin(293_150);
    let source_mass = Mass::from_milligrams(1_000_000);
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        position,
        commodity,
        source_mass,
        temperature,
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let creation = validate_create_surface_resources(&registries, &state, [spec]).unwrap();
    let resource = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let mut queued_before_filling = state.clone();
    let capacity = state.inventory().get_stockpile(carried).unwrap().capacity();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        capacity,
        temperature,
    )
    .unwrap();
    let before = state.clone();
    let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
    let requested = Mass::from_milligrams(100_000);

    let explicit = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::gather_surface(
            resource, requested, direction,
        )),
    )
    .unwrap();
    assert_eq!(
        explicit.secondary_action(),
        Some(&SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::DestinationCapacityExceeded {
                stockpile: carried,
                capacity,
                committed: capacity,
                requested,
            }
        ))
    );
    assert!(state.logistics().pending_secondary_use().is_none());
    assert_eq!(state.player_work().active(), None);

    let contextual = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(direction)),
    )
    .unwrap();
    assert_eq!(
        contextual.secondary_action(),
        Some(&SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::DestinationCapacityExceeded {
                stockpile: carried,
                capacity,
                committed: capacity,
                requested: source_mass,
            }
        ))
    );
    assert!(state.logistics().pending_secondary_use().is_none());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(state.inventory(), before.inventory());
    assert_eq!(state.surface(), before.surface());
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        calculate_matter_accounting(&before).unwrap().total()
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );

    // An initially legal buffered request must recheck inventory custody
    // after the pack fills, including after trusted save/reload.
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut queued_before_filling,
        PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::gather_surface(
            resource, requested, direction,
        )),
    )
    .unwrap();
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    deposit_lot_for_test(
        &registries,
        &mut queued_before_filling,
        carried,
        commodity,
        capacity,
        temperature,
    )
    .unwrap();
    let matter_before = calculate_matter_accounting(&queued_before_filling)
        .unwrap()
        .total();
    let serialized =
        serde_json::to_vec(&SaveEnvelope::new(&registries, &queued_before_filling)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let mut due_rejected = false;
    while !queued_before_filling
        .control_tick()
        .is_simulation_boundary()
    {
        let outcome = advance_control_tick(&registries, &mut queued_before_filling).unwrap();
        let replay_outcome = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(outcome, replay_outcome);
        if let Some(SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::DestinationCapacityExceeded {
                stockpile,
                requested: due_mass,
                ..
            },
        )) = outcome.secondary_action()
        {
            assert_eq!((*stockpile, *due_mass), (carried, requested));
            due_rejected = true;
        }
    }
    assert!(
        due_rejected,
        "a stale buffered capacity must fail on its due boundary"
    );
    assert_eq!(queued_before_filling, replay);
    assert!(
        queued_before_filling
            .logistics()
            .pending_secondary_use()
            .is_none()
    );
    assert!(queued_before_filling.player_work().active().is_none());
    assert_eq!(
        calculate_matter_accounting(&queued_before_filling)
            .unwrap()
            .total(),
        matter_before
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &queued_before_filling),
        Ok(())
    );
}

#[test]
fn hot_natural_gathering_is_reported_on_the_input_frame_without_buffering() {
    use crate::content::{FORM_CHIP, MATERIAL_STONE};
    use crate::material::MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE;
    use crate::surface::player_surface_gathering_options;

    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let temperature = Temperature::from_millikelvin(423_150);
    let mass = Mass::from_milligrams(50_000);
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        mass,
        temperature,
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let prepared = validate_create_surface_resources(&registries, &state, [spec]).unwrap();
    let resource = prepared.first_resource_id();
    prepared.commit(&mut state).unwrap();
    validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let options = player_surface_gathering_options(&registries, &state).unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].resource(), resource);
    assert_eq!(options[0].temperature(), temperature);
    let before = calculate_matter_accounting(&state).unwrap().total();

    let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
    let mut explicit = state.clone();
    let result = advance_control_tick_with_direct_input(
        &registries,
        &mut explicit,
        PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::gather_surface(
            resource, mass, direction,
        )),
    )
    .unwrap();
    let expected =
        SecondaryActionOutcome::SurfaceGatheringRejected(SurfaceGatheringError::ResourceTooHot {
            resource,
            temperature,
            maximum: MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
        });
    assert_eq!(result.secondary_action(), Some(&expected));
    assert!(explicit.logistics().pending_secondary_use().is_none());
    assert!(explicit.player_work().active().is_none());

    let result = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(direction)),
    )
    .unwrap();
    assert_eq!(result.secondary_action(), Some(&expected));
    assert!(state.logistics().pending_secondary_use().is_none());
    assert!(state.player_work().active().is_none());
    assert_eq!(state, explicit);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let serialized = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state,
    );
}

#[test]
fn explicit_direct_gathering_resolves_ambiguous_food_and_replays_a_retargeted_press() {
    let registries = build_registries();
    let mut state = AppState::new();
    let at = VoxelCoord::new(0, 0, 0);
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let temperature = Temperature::from_millikelvin(293_150);
    let one_kilo = Mass::from_milligrams(1_000_000);
    let nut_mass = Mass::from_milligrams(200_000);
    let generated = [(berries, one_kilo), (nuts, nut_mass)].map(|(commodity, source_mass)| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            at,
            commodity,
            source_mass,
            temperature,
            MaterialComposition::pure(commodity.material()),
        )
        .unwrap_or_else(|error| panic!("co-located food generation invalid: {error}"))
    });
    validate_create_surface_resources(&registries, &state, generated)
        .unwrap_or_else(|error| panic!("co-located food generation rejected: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("co-located food generation failed: {error}"));
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let carried = validate_admit_player(&registries, &state, at)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let berry_source = state
        .available_surface_resources()
        .find(|source| source.commodity() == berries)
        .unwrap()
        .id();
    let nut_source = state
        .available_surface_resources()
        .find(|source| source.commodity() == nuts)
        .unwrap()
        .id();
    let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
    let zero =
        advance_control_tick_with_direct_input(
            &registries,
            &mut state,
            PlayerDirectControlInput::NEUTRAL.with_secondary(
                SecondaryControlIntent::gather_surface(berry_source, Mass::ZERO, direction),
            ),
        )
        .unwrap();
    assert_eq!(
        zero.secondary_action(),
        Some(&SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::ZeroMass
        ))
    );
    assert!(state.logistics().pending_secondary_use().is_none());
    let missing = SurfaceResourceId::new(u32::MAX);
    let unavailable = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::gather_surface(
            missing,
            Mass::from_milligrams(100_000),
            direction,
        )),
    )
    .unwrap();
    assert_eq!(
        unavailable.secondary_action(),
        Some(&SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::ResourceUnavailableAtPlayer { resource: missing }
        ))
    );
    assert!(state.logistics().pending_secondary_use().is_none());
    // Impossible requests are rejected on the input frame, without first
    // queueing a slow-boundary action and making the player wait for an error.
    let oversized = Mass::from_milligrams(1_000_001);
    let oversized_result =
        advance_control_tick_with_direct_input(
            &registries,
            &mut state,
            PlayerDirectControlInput::NEUTRAL.with_secondary(
                SecondaryControlIntent::gather_surface(berry_source, oversized, direction),
            ),
        )
        .unwrap();
    assert_eq!(
        oversized_result.secondary_action(),
        Some(&SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::BatchTooLarge {
                requested: oversized,
                maximum: one_kilo,
            }
        ))
    );
    assert!(state.logistics().pending_secondary_use().is_none());
    let shortfall = Mass::from_milligrams(200_001);
    let shortfall_result = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::gather_surface(
            nut_source, shortfall, direction,
        )),
    )
    .unwrap();
    assert_eq!(
        shortfall_result.secondary_action(),
        Some(&SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::InsufficientResource {
                requested: shortfall,
                remaining: nut_mass,
            }
        ))
    );
    assert!(state.logistics().pending_secondary_use().is_none());
    let ambiguous = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(direction)),
    )
    .unwrap();
    assert_eq!(
        ambiguous.secondary_action(),
        Some(&SecondaryActionOutcome::AmbiguousLocalSurfaceResources { count: 2 })
    );
    let portion = Mass::from_milligrams(150_000);
    let first =
        advance_control_tick_with_direct_input(
            &registries,
            &mut state,
            PlayerDirectControlInput::NEUTRAL.with_secondary(
                SecondaryControlIntent::gather_surface(berry_source, portion, direction),
            ),
        )
        .unwrap();
    assert_eq!(
        first.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    assert!(first.queued_action_wait_control_ticks().is_some());
    let buffered_revision = state.logistics().revision();
    let held =
        advance_control_tick_with_direct_input(
            &registries,
            &mut state,
            PlayerDirectControlInput::NEUTRAL.with_secondary(
                SecondaryControlIntent::gather_surface(berry_source, portion, direction),
            ),
        )
        .unwrap();
    assert_eq!(
        held.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    assert_eq!(
        state.logistics().revision(),
        buffered_revision,
        "holding the same gathering choice must not repeatedly clear and requeue it"
    );
    // A newer explicit source supersedes the older one, without gathering
    // any berry or spending an arbitrary held-action interval.
    let retarget = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::gather_surface(
            nut_source, portion, direction,
        )),
    )
    .unwrap();
    assert_eq!(
        retarget.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    assert_eq!(
        state
            .logistics()
            .pending_secondary_use()
            .unwrap()
            .surface_request(),
        Some((nut_source, portion))
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    for invalid_field in [0_usize, 1_usize] {
        let mut forged = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
        forged["state"]["systems"]["logistics"]["pending_secondary_use"]["surface_request"]
            [invalid_field] = serde_json::json!(0);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
        assert_eq!(
            decoded.into_state(&registries),
            Err(crate::persistence::LoadError::InvalidState(
                crate::core::state::StateValidationError::Logistics(
                    crate::logistics::LogisticsValidationError::PendingSecondaryUseSurfaceRequestInvalid
                )
            )),
            "trusted load must reject an impossible selected resource ID or mass"
        );
    }
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut replay = loaded.into_state(&registries).unwrap();
    assert_eq!(state, replay);
    let mut started = false;
    while !state.control_tick().is_simulation_boundary() {
        let current = advance_control_tick(&registries, &mut state).unwrap();
        let restored = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(current, restored);
        if let Some(SecondaryActionOutcome::SurfaceGatheringStarted(work)) =
            current.secondary_action()
        {
            assert_eq!(work.resource(), nut_source);
            assert_eq!(work.gathered_mass(), portion);
            started = true;
        } else {
            assert_eq!(current.secondary_action(), None);
        }
    }
    assert!(
        started,
        "queued chosen source must begin exactly on the slow boundary"
    );
    while state.player_work().active().is_some() {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(nuts),
        portion
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(berries),
        Mass::ZERO
    );
    assert_eq!(
        state
            .available_surface_resources()
            .find(|source| source.id() == berry_source)
            .unwrap()
            .remaining_mass(),
        one_kilo
    );
    assert_eq!(
        state
            .available_surface_resources()
            .find(|source| source.id() == nut_source)
            .unwrap()
            .remaining_mass(),
        nut_mass.checked_sub(portion).unwrap()
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn explicit_gathering_rejects_unadmitted_player_without_mutating_resources() {
    let registries = build_registries();
    let mut state = AppState::new();
    let original = state.clone();
    let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
    let outcome = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::gather_surface(
            SurfaceResourceId::new(1),
            Mass::from_milligrams(100_000),
            direction,
        )),
    )
    .unwrap();
    assert_eq!(
        outcome.secondary_action(),
        Some(&SecondaryActionOutcome::PlayerNotAdmitted)
    );
    assert_eq!(state.surface(), original.surface());
    assert_eq!(state.inventory(), original.inventory());
    assert!(state.logistics().pending_secondary_use().is_none());
}

#[test]
fn simultaneous_drop_cancels_a_pending_and_held_explicit_gather() {
    let registries = build_registries();
    let mut state = AppState::new();
    let at = VoxelCoord::new(0, 0, 0);
    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let mass = Mass::from_milligrams(500_000);
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        at,
        commodity,
        mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_BERRIES),
    )
    .unwrap();
    validate_create_surface_resources(&registries, &state, [spec])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let carried = validate_admit_player(&registries, &state, at)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let source = state.available_surface_resources().next().unwrap().id();
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap();
    let action = SecondaryControlIntent::gather_surface(source, mass, direction);
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_secondary(action),
    )
    .unwrap();
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    let drop = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(action)
            .with_drop_active_selection(),
    )
    .unwrap();
    assert_eq!(drop.secondary_action(), None);
    assert!(state.logistics().pending_secondary_use().is_none());
    while !state.control_tick().is_simulation_boundary() {
        let settled = advance_control_tick(&registries, &mut state).unwrap();
        assert_eq!(settled.secondary_action(), None);
    }
    assert!(state.player_work().active().is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(commodity),
        Mass::ZERO
    );
    assert_eq!(
        state
            .available_surface_resources()
            .next()
            .unwrap()
            .remaining_mass(),
        mass
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}
