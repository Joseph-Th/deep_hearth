//! Contract tests for direct food and fluid consumption.

use super::*;
use crate::content::{
    FLUID_WATER, FORM_CHEST_BODY, FORM_FOOD, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_GRAIN,
    MATERIAL_MEAT, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_KNAP_STONE_TOOL,
    STORAGE_TIMBER_PROVISIONS_CHEST, build_registries,
};
use crate::core::quantity::{AggregateMass, AggregateVolume, Energy, Mass, Temperature, Volume};
use crate::core::state::{
    AppState, StateValidationError, apply_clock_advance, validate_loaded_state,
};
use crate::core::time::{ControlTick, SimulationTick, TickSpan};
use crate::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use crate::fluid::{add_fluid_store_with_contents_for_fixture, calculate_fluid_volume_accounting};
use crate::inventory::{
    MaterialLotSelection, StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
    deposit_lot_for_test, validate_build_storage_enclosure, validate_material_relocation_for_test,
};
use crate::labor::{PlayerWork, PlayerWorkValidationError};
use crate::logistics::{
    PlayerFluidStoreAccessError, PlayerStockpileAccessError, add_traversable_edge_for_test,
    validate_initialize_player_logistics, validate_place_fluid_store,
    validate_place_ground_stockpile,
};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::registry::Registries;
use crate::simulation::{advance_control_tick, advance_tick};
use crate::spatial::VoxelCoord;
use crate::survival::{
    FoodCategory, NUTRITION_PARTS_PER_MILLION, NutritionReserves, SurvivalValidationError,
    Vitality, assess_survival, initialize_player_survival, player_record,
};

fn initialize_and_spend_reserves(registries: &Registries, state: &mut AppState) {
    initialize_player_survival(registries, state)
        .unwrap_or_else(|error| panic!("survival initialization failed: {error}"));
    spend_initial_reserves(registries, state);
}

fn admit_and_spend_reserves(registries: &Registries, state: &mut AppState, position: VoxelCoord) {
    validate_admit_player(registries, state, position)
        .unwrap_or_else(|error| panic!("survival persistence player admission failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("survival persistence player admission commit failed: {error}")
        });
    spend_initial_reserves(registries, state);
}

fn spend_initial_reserves(registries: &Registries, state: &mut AppState) {
    for _ in 0..5 {
        let _ = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("survival reserve-spend tick failed: {error}"));
    }
}

#[test]
fn dead_player_cannot_see_corpse_location_drink_sources() {
    use crate::player::validate_respawn_player;
    use crate::survival::player_record_with_thermal_comfort;

    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let serving = minimum_drink_volume(&registries);
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        serving,
        FLUID_WATER,
        serving,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_place_fluid_store(&state, store, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        available_local_drink_sources(&registries, &state)
            .map(|source| source.store())
            .collect::<Vec<_>>(),
        vec![store]
    );
    let before = *state.survival().player().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            before.metabolic_energy(),
            before.hydration(),
            Vitality::ZERO,
            before.nutrition(),
            before.vitality_recovery_remainder(),
            before.thermal_comfort_ppm(),
        ),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        available_local_drink_sources(&registries, &state).count(),
        0
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(
        available_local_drink_sources(&registries, &loaded).count(),
        0
    );
    let _ = validate_respawn_player(&registries, &state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = validate_respawn_player(&registries, &loaded)
        .unwrap()
        .commit(&mut loaded)
        .unwrap();
    assert_eq!(loaded, state);
    assert_eq!(
        available_local_drink_sources(&registries, &state)
            .map(|source| source.store())
            .collect::<Vec<_>>(),
        vec![store]
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn local_drink_source_observation_exposes_only_currently_usable_colocated_fluid() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player_position = VoxelCoord::new(0, 0, 0);
    let serving = minimum_drink_volume(&registries);
    let local = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        serving,
        FLUID_WATER,
        serving,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("local drink-source fixture failed: {error}"));
    let hot = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        serving,
        FLUID_WATER,
        serving,
        Temperature::from_millikelvin(340_000),
    )
    .unwrap_or_else(|error| panic!("hot drink-source fixture failed: {error}"));
    let remote = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        serving,
        FLUID_WATER,
        serving,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("remote drink-source fixture failed: {error}"));
    validate_place_fluid_store(&state, local, player_position)
        .unwrap_or_else(|error| panic!("local drink-source placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("local drink-source placement commit failed: {error}"));
    validate_place_fluid_store(&state, hot, player_position)
        .unwrap_or_else(|error| panic!("hot drink-source placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("hot drink-source placement commit failed: {error}"));
    validate_place_fluid_store(&state, remote, VoxelCoord::new(1, 0, 0))
        .unwrap_or_else(|error| panic!("remote drink-source placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote drink-source placement commit failed: {error}"));

    assert!(
        available_local_drink_sources(&registries, &state)
            .next()
            .is_none()
    );
    assert!(
        available_adjacent_drink_sources(&registries, &state)
            .next()
            .is_none(),
        "adjacent drink-source observation requires admitted logistics"
    );

    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("drink-source actor admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("drink-source actor admission commit failed: {error}"));
    assert!(
        available_adjacent_drink_sources(&registries, &state)
            .next()
            .is_none(),
        "geometrically adjacent water must stay hidden until traversal is authorized"
    );
    add_traversable_edge_for_test(&mut state, player_position, VoxelCoord::new(1, 0, 0));
    assert_eq!(
        available_local_drink_sources(&registries, &state)
            .map(|source| (
                source.store(),
                source.fluid(),
                source.available_volume(),
                source.temperature(),
            ))
            .collect::<Vec<_>>(),
        vec![(
            local,
            FLUID_WATER,
            serving,
            Temperature::from_millikelvin(293_150),
        )]
    );
    assert_eq!(
        available_adjacent_drink_sources(&registries, &state)
            .map(|source| (source.store(), source.fluid(), source.position()))
            .collect::<Vec<_>>(),
        vec![(remote, FLUID_WATER, VoxelCoord::new(1, 0, 0))],
        "adjacent observation should expose only drinkable face-neighbor sources without leaking exact volume or temperature"
    );
}

#[test]
fn adjacent_drink_source_observation_preserves_store_id_order_across_neighbor_indexes() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player = VoxelCoord::new(0, 0, 0);
    let east = VoxelCoord::new(1, 0, 0);
    let west = VoxelCoord::new(-1, 0, 0);
    let far = VoxelCoord::new(100, 0, 100);
    let serving = minimum_drink_volume(&registries);
    let place = |state: &mut AppState, position| {
        let store = add_fluid_store_with_contents_for_fixture(
            &registries,
            state,
            serving,
            FLUID_WATER,
            serving,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("adjacent water fixture failed: {error}"));
        validate_place_fluid_store(state, store, position)
            .unwrap_or_else(|error| panic!("adjacent water placement failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("adjacent water placement commit failed: {error}"));
        store
    };
    let first = place(&mut state, east);
    let remote = place(&mut state, far);
    let second = place(&mut state, west);
    let third = place(&mut state, east);
    validate_initialize_player_logistics(&state, player, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("adjacent water player setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("adjacent water player commit failed: {error}"));
    add_traversable_edge_for_test(&mut state, player, west);
    add_traversable_edge_for_test(&mut state, player, east);
    let observed = available_adjacent_drink_sources(&registries, &state)
        .map(|source| (source.store(), source.position()))
        .collect::<Vec<_>>();
    assert_eq!(observed, vec![(first, east), (second, west), (third, east)]);
    assert!(!observed.iter().any(|(id, _)| *id == remote));

    let _ = place(&mut state, far);
    assert_eq!(
        available_adjacent_drink_sources(&registries, &state)
            .map(|source| (source.store(), source.position()))
            .collect::<Vec<_>>(),
        observed,
        "remote liquid custody cannot change adjacent choices or their ID order"
    );
}

#[test]
fn final_subminimum_sip_is_drinkable_and_survives_save_and_continuation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, player_position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = advance_tick(&registries, &mut state).unwrap();
    let minimum = minimum_drink_volume(&registries);
    let remainder = minimum.checked_sub(Volume::from_microliters(1)).unwrap();
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        minimum,
        FLUID_WATER,
        remainder,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_place_fluid_store(&state, store, player_position)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    assert_eq!(
        available_local_drink_sources(&registries, &state)
            .map(|source| (source.store(), source.available_volume()))
            .collect::<Vec<_>>(),
        vec![(store, remainder)],
        "observation must preserve the exact last drops in their source"
    );
    let hydration_before = state.survival().player().unwrap().hydration();
    let reachable_target = hydration_before
        .checked_add(Volume::from_microliters(1))
        .unwrap();
    assert!(
        validate_drink_store_to_hydration_target(&registries, &state, store, reachable_target)
            .unwrap()
            .is_some(),
        "an attainable exact hydration target must accept the same final remnant"
    );
    let admission = validate_drink_store_to_full(&registries, &state, store)
        .unwrap()
        .expect("the default use action must consume the last drinkable remnant");
    let outcome = admission.commit(&mut state).unwrap();
    assert_eq!(outcome.volume(), remainder);
    assert_eq!(
        state.fluid().get_store(store).unwrap().contents(),
        None,
        "a final sip withdraws the entire physical remainder"
    );
    assert_eq!(
        state.survival().consumed_fluid_volume(FLUID_WATER),
        AggregateVolume::from_volume(remainder),
        "the conserved volume must move into the consumption ledger"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    while state.tick() < outcome.completes_at() {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut replay).unwrap();
        assert_eq!(state, replay);
    }
    assert!(state.survival().player().unwrap().hydration() > hydration_before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn arbitrary_subminimum_sips_from_a_larger_water_source_remain_rejected() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let minimum = minimum_drink_volume(&registries);
    let tiny_portion = minimum.checked_sub(Volume::from_microliters(1)).unwrap();
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        minimum,
        FLUID_WATER,
        minimum,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_place_fluid_store(&state, store, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        validate_drink(&registries, &state, store, tiny_portion).err(),
        Some(DrinkError::DrinkVolumeBelowIntakeMinimum {
            volume: tiny_portion,
            minimum,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn drinking_rejects_known_remote_fluid_store() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote drinking logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote drinking logistics commit failed: {error}"));
    let volume = minimum_drink_volume(&registries);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        Temperature::from_millikelvin(340_000),
    )
    .unwrap_or_else(|error| panic!("remote drinking water fixture failed: {error}"));
    let store_position = VoxelCoord::new(1, 0, 0);
    validate_place_fluid_store(&state, water, store_position)
        .unwrap_or_else(|error| panic!("remote drinking placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote drinking placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_drink(&registries, &state, water, volume).err(),
        Some(DrinkError::Access(
            PlayerFluidStoreAccessError::RemoteKnownFluidStore {
                store: water,
                store_position,
                player_position,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn drinking_token_rejects_fluid_location_added_after_validation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let volume = minimum_drink_volume(&registries);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-location drinking water fixture failed: {error}"));
    let validated = validate_drink(&registries, &state, water, volume)
        .unwrap_or_else(|error| panic!("stale-location drinking validation failed: {error}"));
    let expected = state.logistics().revision();
    validate_place_fluid_store(&state, water, VoxelCoord::new(5, 0, 0))
        .unwrap_or_else(|error| panic!("stale-location drinking placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale-location drinking placement commit failed: {error}"));
    let actual = state.logistics().revision();
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(DrinkCommitError::StaleLogisticsRevision { expected, actual })
    );
    assert_eq!(state, before);
}

#[test]
fn eating_rejects_known_remote_ground_food_source() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote eating logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote eating logistics commit failed: {error}"));
    let meal_mass = minimum_meal_mass(&registries);
    let source = add_solid_stockpile_for_test(&mut state, meal_mass)
        .unwrap_or_else(|error| panic!("remote eating source failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("remote eating food failed: {error}"));
    let source_position = VoxelCoord::new(1, 0, 0);
    validate_place_ground_stockpile(&state, source, source_position)
        .unwrap_or_else(|error| panic!("remote eating placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote eating placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_eat(
            &registries,
            &state,
            source,
            &[MaterialLotSelection::new(food, meal_mass)],
        )
        .err(),
        Some(EatError::Access(
            PlayerStockpileAccessError::RemoteKnownStockpile {
                stockpile: source,
                stockpile_position: source_position,
                player_position,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn eating_token_rejects_ground_location_added_after_validation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let meal_mass = minimum_meal_mass(&registries);
    let source = add_solid_stockpile_for_test(&mut state, meal_mass)
        .unwrap_or_else(|error| panic!("stale-location eating source failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-location eating food failed: {error}"));
    let validated = validate_eat(
        &registries,
        &state,
        source,
        &[MaterialLotSelection::new(food, meal_mass)],
    )
    .unwrap_or_else(|error| panic!("stale-location eating validation failed: {error}"));
    validate_place_ground_stockpile(&state, source, VoxelCoord::new(5, 0, 0))
        .unwrap_or_else(|error| panic!("stale-location eating placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale-location eating placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(EatCommitError::StaleLogisticsRevision {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before);
}

fn minimum_drink_volume(registries: &Registries) -> Volume {
    registries
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume()
}

fn minimum_meal_mass(registries: &Registries) -> Mass {
    registries
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_meal_mass()
}

fn load_with_owner_revisions(
    registries: &Registries,
    state: &AppState,
    player_work_revision: Option<u64>,
    survival_revision: Option<u64>,
) -> AppState {
    let mut encoded = serde_json::to_value(SaveEnvelope::new(registries, state))
        .unwrap_or_else(|error| panic!("owner-revision fixture serialization failed: {error}"));
    if let Some(revision) = player_work_revision {
        encoded["state"]["systems"]["player_work"]["revision"] = serde_json::json!(revision);
    }
    if let Some(revision) = survival_revision {
        encoded["state"]["systems"]["survival"]["revision"] = serde_json::json!(revision);
    }
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("owner-revision fixture decode failed: {error}"));
    decoded
        .into_state(registries)
        .unwrap_or_else(|error| panic!("owner-revision fixture should load: {error}"))
}

fn direct_consumption_fixture(
    registries: &Registries,
) -> (
    AppState,
    crate::inventory::StockpileId,
    crate::inventory::MaterialLotId,
    crate::fluid::FluidStoreId,
) {
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    admit_and_spend_reserves(registries, &mut state, position);
    let meal_mass = minimum_meal_mass(registries);
    let stockpile = add_solid_stockpile_for_test(&mut state, meal_mass)
        .unwrap_or_else(|error| panic!("direct-consumption revision stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("direct-consumption revision food failed: {error}"));
    let water = add_fluid_store_with_contents_for_fixture(
        registries,
        &mut state,
        minimum_drink_volume(registries),
        FLUID_WATER,
        minimum_drink_volume(registries),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("direct-consumption revision water failed: {error}"));
    validate_place_ground_stockpile(&state, stockpile, position)
        .unwrap_or_else(|error| panic!("direct-consumption stockpile placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("direct-consumption stockpile placement commit failed: {error}")
        });
    validate_place_fluid_store(&state, water, position)
        .unwrap_or_else(|error| panic!("direct-consumption fluid placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("direct-consumption fluid placement commit failed: {error}")
        });
    (state, stockpile, food, water)
}

#[path = "consumption_tests/admission.rs"]
mod admission;

fn finish_direct_consumption(registries: &Registries, state: &mut AppState) -> u64 {
    let active = state
        .player_work()
        .active()
        .unwrap_or_else(|| panic!("direct-consumption test has no active player work"));
    let completes_at = match active {
        PlayerWork::Eating { work } => work.completes_at(),
        PlayerWork::Drinking { work } => work.completes_at(),
        other @ (PlayerWork::SurfaceGathering { .. }
        | PlayerWork::TreeFelling { .. }
        | PlayerWork::BlockExcavation { .. }
        | PlayerWork::ManualProduction { .. }
        | PlayerWork::Mining { .. }
        | PlayerWork::ManualPower { .. }
        | PlayerWork::Prospecting { .. }
        | PlayerWork::EquipmentMaintenance { .. }
        | PlayerWork::StorageEnclosureDismantling { .. }
        | PlayerWork::StructuralConstruction { .. }
        | PlayerWork::StructuralDismantling { .. }) => {
            panic!("direct-consumption test has wrong active work: {other:?}")
        }
    };
    let started = state.tick().value();
    while state.tick() < completes_at {
        let _ = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("direct-consumption completion tick failed: {error}"));
    }
    assert_eq!(state.player_work().active(), None);
    completes_at.value() - started
}

#[path = "consumption_tests/death_attention.rs"]
mod death_attention;

#[path = "consumption_tests/drink_projection.rs"]
mod drink_projection;

#[path = "consumption_tests/meal_projection.rs"]
mod meal_projection;

fn start_attention_owning_craft(registries: &Registries, state: &mut AppState) -> PlayerWork {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("attention craft source fixture failed: {error}"));
    let destination = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("attention craft destination fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        registries,
        state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("attention craft material fixture failed: {error}"));
    validate_start_manual_craft(
        registries,
        state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("attention craft validation failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("attention craft commit failed: {error}"));
    state
        .player_work()
        .active()
        .unwrap_or_else(|| panic!("attention craft did not claim player work"))
}

#[path = "consumption_tests/commit_races.rs"]
mod commit_races;

#[path = "consumption_tests/persistence_absorption.rs"]
mod persistence_absorption;

#[path = "consumption_tests/persistence_validation.rs"]
mod persistence_validation;

#[path = "consumption_tests/custody_freshness.rs"]
mod custody_freshness;
