//! Contracts for local observation, finite gathering, persistence, and exposed-matter age.

use super::*;
use crate::content::{
    FORM_FOOD, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, SURFACE_GATHERING_HAND_SCAVENGE,
    build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, StateValidationError, apply_clock_advance};
use crate::core::time::{SimulationTick, TickSpan};
use crate::inventory::StockpileStorageError;
use crate::labor::PlayerWorkValidationError;
use crate::logistics::validate_initialize_player_logistics;
use crate::material::{CommodityKey, MaterialComposition, MaterialPhase};
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::survival::{FoodFreshness, assess_food_freshness, initialize_player_survival};

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
            SURFACE_GATHERING_HAND_SCAVENGE,
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
            SURFACE_GATHERING_HAND_SCAVENGE,
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
            SURFACE_GATHERING_HAND_SCAVENGE,
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
