//! Direct player storage allocation requires free attention and revision-bound admission.

use super::*;

use crate::content::{
    EQUIPMENT_WOODEN_CLUB, FLUID_WATER, FORM_LOG, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::{Mass, Temperature, Volume};
use crate::core::state::{AppState, validate_loaded_state};
use crate::equipment::validate_assemble_equipment;
use crate::fluid::add_fluid_store_with_contents_for_fixture;
use crate::inventory::deposit_lot_for_test;
use crate::logistics::{
    PlayerActiveSelection, validate_pickup_equipment, validate_place_fluid_store,
    validate_set_active_selection,
};
use crate::material::CommodityKey;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::{ViewDirection, VoxelCoord};
use crate::survival::validate_drink;

#[test]
fn player_cannot_allocate_new_ground_storage_while_drinking() {
    let registries = build_registries();
    let mut state = AppState::new();
    let at = VoxelCoord::new(0, 0, 0);
    let source = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(500_000),
        FLUID_WATER,
        Volume::from_microliters(500_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_place_fluid_store(&state, source, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &state, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    let capacity = Mass::from_milligrams(1_000_000);
    let prior = validate_allocate_player_ground_stockpile_at(&state, at, capacity).unwrap();
    let survival_before = state.survival().revision();
    let drink = validate_drink(
        &registries,
        &state,
        source,
        Volume::from_microliters(250_000),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let snapshot = state.clone();
    assert_eq!(
        validate_allocate_player_ground_stockpile_at(&state, at, capacity).err(),
        Some(PlayerGroundStockpileAllocationError::PlayerBusy)
    );
    assert_eq!(
        prior.commit(&mut state).err(),
        Some(
            GroundStockpileAllocationCommitError::StaleSurvivalRevision {
                expected: survival_before,
                actual: snapshot.survival().revision(),
            }
        )
    );
    assert_eq!(state, snapshot);

    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(
        validate_allocate_player_ground_stockpile_at(&replay, at, capacity).err(),
        Some(PlayerGroundStockpileAllocationError::PlayerBusy)
    );
    while state.tick() < drink.completes_at() {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    let storage = validate_allocate_player_ground_stockpile_at(&state, at, capacity)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        state.logistics().stationary_stockpile_position(storage),
        Some(at)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn prepared_weapon_blocks_ground_allocation_and_stale_allocation_token() {
    use crate::combat::{validate_cancel_primitive_preparation, validate_prepare_primitive_strike};

    let registries = build_registries();
    let mut state = AppState::new();
    let at = VoxelCoord::new(0, 0, 0);
    let carried = validate_admit_player(&registries, &state, at)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
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

    let capacity = Mass::from_milligrams(1_000_000);
    let prior = validate_allocate_player_ground_stockpile_at(&state, at, capacity).unwrap();
    let combat_before = state.combat().revision();
    let forward = ViewDirection::new(1_000_000, 0, 0).unwrap();
    let _ = validate_prepare_primitive_strike(&registries, &state, forward)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let snapshot = state.clone();
    assert_eq!(
        validate_allocate_player_ground_stockpile_at(&state, at, capacity).err(),
        Some(PlayerGroundStockpileAllocationError::PreparedStrikeActive)
    );
    assert_eq!(
        prior.commit(&mut state).err(),
        Some(GroundStockpileAllocationCommitError::StaleCombatRevision {
            expected: combat_before,
            actual: snapshot.combat().revision(),
        })
    );
    assert_eq!(state, snapshot);

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for game in [&mut state, &mut replay] {
        assert_eq!(
            validate_allocate_player_ground_stockpile_at(game, at, capacity).err(),
            Some(PlayerGroundStockpileAllocationError::PreparedStrikeActive)
        );
        let _ = validate_cancel_primitive_preparation(game)
            .unwrap()
            .commit(game)
            .unwrap();
        let storage = validate_allocate_player_ground_stockpile_at(game, at, capacity)
            .unwrap()
            .commit(game)
            .unwrap();
        assert_eq!(
            game.logistics().stationary_stockpile_position(storage),
            Some(at)
        );
        assert_eq!(validate_loaded_state(&registries, game), Ok(()));
    }
    assert_eq!(state, replay);
}

#[test]
fn pending_ground_allocation_rejects_new_gathering_work_without_survival_drift() {
    use crate::content::{FORM_LUMP, MATERIAL_STONE, SURFACE_GATHERING_HAND_COLLECT_STONE};
    use crate::material::MaterialComposition;
    use crate::surface::{
        SurfaceGatheringRequest, SurfaceResourceKind, SurfaceResourceSpec,
        insert_surface_resource_for_test, validate_start_surface_gathering,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let at = VoxelCoord::new(0, 0, 0);
    let carried = validate_admit_player(&registries, &state, at)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let resource = insert_surface_resource_for_test(
        &registries,
        &mut state,
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            at,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(100_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap(),
    )
    .unwrap();
    let capacity = Mass::from_milligrams(1_000);
    let stale = validate_allocate_player_ground_stockpile_at(&state, at, capacity).unwrap();
    let work_before = state.player_work().revision();
    let survival_before = state.survival().revision();
    let logistics_before = state.logistics().revision();

    validate_start_surface_gathering(
        &registries,
        &state,
        SurfaceGatheringRequest::new(
            SURFACE_GATHERING_HAND_COLLECT_STONE,
            resource,
            carried,
            Mass::from_milligrams(100),
        ),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(state.survival().revision(), survival_before);
    assert_eq!(state.logistics().revision(), logistics_before);
    let before_rejection = state.clone();
    assert_eq!(
        validate_allocate_player_ground_stockpile_at(&state, at, capacity).err(),
        Some(PlayerGroundStockpileAllocationError::PlayerBusy)
    );
    assert_eq!(
        stale.commit(&mut state).err(),
        Some(
            GroundStockpileAllocationCommitError::StalePlayerWorkRevision {
                expected: work_before,
                actual: before_rejection.player_work().revision(),
            }
        )
    );
    assert_eq!(state, before_rejection);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
