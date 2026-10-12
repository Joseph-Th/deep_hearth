//! Contracts for local observation, finite gathering, persistence, and exposed-matter age.

use super::*;
use crate::content::{
    FORM_FOOD, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, SURFACE_GATHERING_HAND_COLLECT_STONE,
    SURFACE_GATHERING_HAND_FORAGE_BERRIES, build_registries,
};
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::state::{
    AppState, StateValidationError, apply_clock_advance, validate_loaded_state,
};
use crate::core::time::{SimulationTick, TickSpan};
use crate::inventory::StockpileStorageError;
use crate::labor::PlayerWorkValidationError;
use crate::logistics::{add_traversable_edge_for_test, validate_initialize_player_logistics};
use crate::material::{CommodityKey, MaterialComposition, MaterialPhase};
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::survival::{
    FoodFreshness, Vitality, assess_food_freshness, initialize_player_survival, player_record,
};

const TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn seed_surface(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    position: VoxelCoord,
    commodity: CommodityKey,
    mass: Mass,
) -> SurfaceResourceId {
    let spec = GeneratedSurfaceResourceSpec::new(
        position,
        commodity,
        mass,
        TEMPERATURE,
        MaterialComposition::pure(commodity.material()),
    )
    .unwrap_or_else(|error| panic!("surface test resource specification failed: {error}"));
    insert_generated_surface_resource(registries, state, spec)
        .unwrap_or_else(|error| panic!("surface test resource generation failed: {error}"))
}

#[test]
fn ordinary_surface_gathering_derives_carried_custody() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(100),
    );
    let carried = admit_player(&registries, &mut state, position);

    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE,
        stone,
        Mass::from_milligrams(100),
    )
    .unwrap_or_else(|error| panic!("ordinary surface gathering validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ordinary surface gathering commit failed: {error}"));

    let work = match state.player_work().active() {
        Some(crate::labor::PlayerWork::SurfaceGathering { work }) => work,
        other => panic!("ordinary surface gathering did not own player work: {other:?}"),
    };
    assert_eq!(work.destination(), carried);
    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("ordinary surface gathering tick failed: {error}"));
    assert_eq!(
        outcome
            .surface_gathering()
            .map(|gathered| gathered.destination()),
        Some(carried)
    );
}

#[test]
fn adjacent_surface_observation_requires_persisted_access_and_hides_exact_state() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player_position = VoxelCoord::new(0, 0, 0);
    let reachable_position = VoxelCoord::new(1, 0, 0);
    let blocked_position = VoxelCoord::new(0, 0, 1);
    let reachable = seed_surface(
        &registries,
        &mut state,
        reachable_position,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(123_456),
    );
    let blocked = seed_surface(
        &registries,
        &mut state,
        blocked_position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(654_321),
    );
    assert!(
        available_adjacent_surface_resources(&state)
            .next()
            .is_none()
    );
    let _ = admit_player(&registries, &mut state, player_position);
    assert!(
        available_adjacent_surface_resources(&state)
            .next()
            .is_none(),
        "geometric adjacency without persisted traversal authorization must stay hidden"
    );
    add_traversable_edge_for_test(&mut state, player_position, reachable_position);
    assert_eq!(
        available_adjacent_surface_resources(&state)
            .map(|resource| (
                resource.resource(),
                resource.commodity(),
                resource.position()
            ))
            .collect::<Vec<_>>(),
        vec![(
            reachable,
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
            reachable_position,
        )]
    );
    assert_ne!(reachable, blocked);
}

fn admit_player(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    position: VoxelCoord,
) -> crate::inventory::StockpileId {
    initialize_player_survival(registries, state)
        .unwrap_or_else(|error| panic!("surface test survival initialization failed: {error}"));
    validate_initialize_player_logistics(state, position, Mass::from_milligrams(10_000_000))
        .unwrap_or_else(|error| panic!("surface test logistics initialization failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("surface test logistics commit failed: {error}"))
        .carried_stockpile()
}

fn make_next_tick_fatal(registries: &crate::registry::Registries, state: &mut AppState) {
    let physiology = registries.survival().physiology();
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("surface fatality fixture lost survival player"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            Energy::ZERO,
            player.hydration(),
            Vitality::from_parts_per_million_unchecked(
                physiology.starvation_vitality_loss_ppm_per_tick(),
            ),
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
}

#[test]
fn fatal_tick_cancels_unfinished_surface_gathering_and_releases_capacity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let gathered_mass = Mass::from_milligrams(1_000_000);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        gathered_mass,
    );
    let carried = admit_player(&registries, &mut state, position);
    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE,
        stone,
        gathered_mass,
    )
    .unwrap_or_else(|error| panic!("fatal surface gathering start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fatal surface gathering commit failed: {error}"));
    let work = match state.player_work().active() {
        Some(crate::labor::PlayerWork::SurfaceGathering { work }) => work,
        other => panic!("fatal surface gathering did not own player work: {other:?}"),
    };
    assert!(work.completes_at().value() > state.tick().value() + 1);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.reserved_inbound()),
        Some(gathered_mass)
    );
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal surface gathering tick failed: {error}"));

    assert!(outcome.surface_gathering().is_none());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(
        state
            .surface()
            .get(stone)
            .map(|resource| resource.remaining_mass()),
        Some(gathered_mass),
        "canceled gathering must not consume the surface source"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| (record.stored_mass(), record.reserved_inbound())),
        Some((Mass::ZERO, Mass::ZERO)),
        "canceled gathering must release capacity without materializing matter"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn surface_gathering_due_on_fatal_tick_completes_before_attention_release() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let gathered_mass = Mass::from_milligrams(100);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        gathered_mass,
    );
    let carried = admit_player(&registries, &mut state, position);
    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE,
        stone,
        gathered_mass,
    )
    .unwrap_or_else(|error| panic!("fatal due surface gathering start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fatal due surface gathering commit failed: {error}"));
    let work = match state.player_work().active() {
        Some(crate::labor::PlayerWork::SurfaceGathering { work }) => work,
        other => panic!("fatal due surface gathering did not own player work: {other:?}"),
    };
    assert_eq!(work.completes_at().value(), state.tick().value() + 1);
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal due surface gathering tick failed: {error}"));

    assert_eq!(
        outcome
            .surface_gathering()
            .map(|gathering| gathering.gathered_mass()),
        Some(gathered_mass)
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(
        state
            .surface()
            .get(stone)
            .map(|resource| resource.remaining_mass()),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| (record.stored_mass(), record.reserved_inbound())),
        Some((gathered_mass, Mass::ZERO))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn public_surface_observation_and_start_errors_do_not_reveal_remote_resources() {
    let registries = build_registries();
    let mut state = AppState::new();
    let local_position = VoxelCoord::new(2, 0, -1);
    let remote_position = VoxelCoord::new(8, 0, 4);
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let local = seed_surface(
        &registries,
        &mut state,
        local_position,
        commodity,
        Mass::from_milligrams(100),
    );
    let remote = seed_surface(
        &registries,
        &mut state,
        remote_position,
        commodity,
        Mass::from_milligrams(100),
    );
    let carried = admit_player(&registries, &mut state, local_position);

    let visible = state
        .available_surface_resources()
        .map(SurfaceResourceRecord::id)
        .collect::<Vec<_>>();
    assert_eq!(visible, vec![local]);

    let request = |resource| {
        SurfaceGatheringRequest::new(
            SURFACE_GATHERING_HAND_COLLECT_STONE,
            resource,
            carried,
            Mass::from_milligrams(1),
        )
    };
    assert_eq!(
        validate_start_surface_gathering(&registries, &state, request(remote)).err(),
        Some(SurfaceGatheringError::ResourceUnavailableAtPlayer { resource: remote })
    );
    let unknown = SurfaceResourceId::new(remote.value() + 100);
    assert_eq!(
        validate_start_surface_gathering(&registries, &state, request(unknown)).err(),
        Some(SurfaceGatheringError::ResourceUnavailableAtPlayer { resource: unknown })
    );
}

#[test]
fn surface_gathering_rejects_a_method_authored_for_another_commodity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let berries_commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let berries = seed_surface(
        &registries,
        &mut state,
        position,
        berries_commodity,
        Mass::from_milligrams(100_000),
    );
    let carried = admit_player(&registries, &mut state, position);
    let before = state.clone();

    assert_eq!(
        validate_start_surface_gathering(
            &registries,
            &state,
            SurfaceGatheringRequest::new(
                SURFACE_GATHERING_HAND_COLLECT_STONE,
                berries,
                carried,
                Mass::from_milligrams(100_000),
            ),
        )
        .err(),
        Some(SurfaceGatheringError::CommodityMismatch {
            method: SURFACE_GATHERING_HAND_COLLECT_STONE,
            expected: CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            actual: berries_commodity,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn trusted_load_rejects_surface_gathering_with_a_method_for_another_commodity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let stone_commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        stone_commodity,
        Mass::from_milligrams(100),
    );
    let carried = admit_player(&registries, &mut state, position);
    validate_start_surface_gathering(
        &registries,
        &state,
        SurfaceGatheringRequest::new(
            SURFACE_GATHERING_HAND_COLLECT_STONE,
            stone,
            carried,
            Mass::from_milligrams(100),
        ),
    )
    .unwrap_or_else(|error| panic!("commodity-mismatch load setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("commodity-mismatch load commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("commodity-mismatch load serialization failed: {error}"));
    encoded["state"]["systems"]["player_work"]["active"]["SurfaceGathering"]["work"]["method"] =
        serde_json::json!(SURFACE_GATHERING_HAND_FORAGE_BERRIES.value());
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("commodity-mismatch load decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::SurfaceGatheringCommodityMismatch {
                expected: CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
                actual: stone_commodity,
            }
        )))
    );
}

#[test]
fn trusted_load_rejects_remote_surface_gathering_before_source_state() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(100),
    );
    let carried = admit_player(&registries, &mut state, position);
    validate_start_surface_gathering(
        &registries,
        &state,
        SurfaceGatheringRequest::new(
            SURFACE_GATHERING_HAND_COLLECT_STONE,
            stone,
            carried,
            Mass::from_milligrams(100),
        ),
    )
    .unwrap_or_else(|error| panic!("remote trusted-load gathering setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("remote trusted-load gathering commit failed: {error}"));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("remote trusted-load gathering serialization failed: {error}")
        });
    encoded["state"]["systems"]["logistics"]["player"]["position"] =
        serde_json::json!({"x": 1, "y": 0, "z": 0});
    encoded["state"]["systems"]["player_work"]["active"]["SurfaceGathering"]["work"]["source_mass_before"] =
        serde_json::json!(101_u64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("remote trusted-load gathering decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::SurfaceGatheringPlayerRemote
        )))
    );
}

#[test]
fn gathered_perishable_retains_ambient_surface_age() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let berries = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100),
    );
    apply_clock_advance(&mut state, SimulationTick::new(10));
    let carried = admit_player(&registries, &mut state, position);

    validate_start_surface_gathering(
        &registries,
        &state,
        SurfaceGatheringRequest::new(
            SURFACE_GATHERING_HAND_FORAGE_BERRIES,
            berries,
            carried,
            Mass::from_milligrams(100),
        ),
    )
    .unwrap_or_else(|error| panic!("aged surface-food gathering validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("aged surface-food gathering commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("aged surface-food gathering tick failed: {error}"));

    let lot = state
        .inventory()
        .lot_ids(carried)
        .next()
        .unwrap_or_else(|| panic!("gathered surface food did not enter carried inventory"));
    assert!(matches!(
        assess_food_freshness(&registries, &state, lot),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(11)
    ));
}

#[test]
fn trusted_load_rejects_active_gathering_into_storage_that_no_longer_accepts_source() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(100),
    );
    let carried = admit_player(&registries, &mut state, position);
    validate_start_surface_gathering(
        &registries,
        &state,
        SurfaceGatheringRequest::new(
            SURFACE_GATHERING_HAND_COLLECT_STONE,
            stone,
            carried,
            Mass::from_milligrams(100),
        ),
    )
    .unwrap_or_else(|error| panic!("trusted-load gathering setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("trusted-load gathering setup commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("trusted-load gathering serialization failed: {error}"));
    let storage_profile = &mut encoded["state"]["systems"]["inventory"]["stockpiles"]
        [carried.value().to_string()]["storage_profile"];
    storage_profile["can_store_solid"] = serde_json::json!(false);
    storage_profile["can_store_liquid"] = serde_json::json!(true);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("trusted-load gathering tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::SurfaceGatheringDestinationStorage(
                StockpileStorageError::PhaseNotAccepted {
                    stockpile: carried,
                    phase: MaterialPhase::Solid,
                }
            )
        )))
    );
}
