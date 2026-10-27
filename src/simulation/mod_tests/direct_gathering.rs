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
