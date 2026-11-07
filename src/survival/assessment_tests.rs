//! Warning-horizon arithmetic for player-facing resource forecasts.

use super::{warning_runway, warning_runway_with_pending_exertion};
use crate::core::time::TickSpan;

#[test]
fn resting_warning_timing_honors_strict_threshold_and_partial_last_tick() {
    assert_eq!(warning_runway(100, 50, 10), Some(TickSpan::new(6)));
    assert_eq!(warning_runway(50, 50, 10), Some(TickSpan::new(1)));
    assert_eq!(warning_runway(49, 50, 10), Some(TickSpan::ZERO));
    assert_eq!(warning_runway(51, 50, 3), Some(TickSpan::new(1)));
    assert_eq!(warning_runway(52, 50, 3), Some(TickSpan::new(1)));
    assert_eq!(warning_runway(53, 50, 3), Some(TickSpan::new(2)));
    assert_eq!(warning_runway(0, 0, 1), Some(TickSpan::ZERO));
    assert_eq!(warning_runway(3, 0, 1), Some(TickSpan::new(3)));
    assert_eq!(warning_runway(100, 50, 0), None);
    assert_eq!(warning_runway(49, 50, 0), Some(TickSpan::ZERO));
    assert_eq!(warning_runway(u128::MAX, 1, 1), None);
}

#[test]
fn unsettled_exertion_warning_horizons_include_first_tick_and_zero_rate_cases() {
    assert_eq!(
        warning_runway_with_pending_exertion(100, 50, 10, 20),
        Some(TickSpan::new(4)),
    );
    assert_eq!(
        warning_runway_with_pending_exertion(60, 50, 10, 1),
        Some(TickSpan::new(1)),
    );
    assert_eq!(
        warning_runway_with_pending_exertion(49, 50, 10, 20),
        Some(TickSpan::ZERO),
    );
    assert_eq!(
        warning_runway_with_pending_exertion(60, 50, 0, 11),
        Some(TickSpan::new(1)),
    );
    assert_eq!(warning_runway_with_pending_exertion(60, 50, 0, 10), None);
    assert_eq!(warning_runway_with_pending_exertion(60, 50, 0, 0), None);
    assert_eq!(
        warning_runway_with_pending_exertion(u128::MAX, 1, 1, 1),
        None,
    );
}

#[test]
fn active_gathering_does_not_advertise_an_idle_resting_runway() {
    use crate::content::{
        FORM_CHIP, MATERIAL_STONE, SURFACE_GATHERING_HAND_COLLECT_STONE_CHIPS, build_registries,
    };
    use crate::core::quantity::{Mass, Temperature};
    use crate::core::state::{AppState, validate_loaded_state};
    use crate::material::{CommodityKey, MaterialComposition};
    use crate::player::validate_admit_player;
    use crate::simulation::advance_tick;
    use crate::spatial::VoxelCoord;
    use crate::surface::{
        SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources,
        validate_start_player_surface_gathering,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(super::assess_player_resting_provision_runway(&registries, &state).is_some());
    let mass = Mass::from_milligrams(50_000);
    let spec = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let creation = validate_create_surface_resources(&registries, &state, [spec]).unwrap();
    let resource = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    validate_start_player_surface_gathering(
        &registries,
        &state,
        SURFACE_GATHERING_HAND_COLLECT_STONE_CHIPS,
        resource,
        mass,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let in_progress = state.clone();
    assert_eq!(
        super::assess_player_resting_provision_runway(&registries, &state),
        None
    );
    assert_eq!(state, in_progress);
    while state.player_work().active().is_some() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(super::assess_player_resting_provision_runway(&registries, &state).is_some());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn resting_forecast_charges_unsettled_control_exertion_at_the_first_boundary() {
    use crate::content::{BLOCK_STATE_STONE, build_registries};
    use crate::core::state::{AppState, validate_loaded_state};
    use crate::logistics::{HorizontalControlInput, PlayerControlInput};
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::player::validate_admit_player;
    use crate::simulation::{advance_control_tick, advance_control_tick_with_input};
    use crate::spatial::VoxelCoord;
    use crate::survival::{HungerState, assess_survival, player_record_with_thermal_comfort};
    use crate::world::{
        ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(41),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let floor = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(8, y, 8)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let position = VoxelCoord::new(8, floor + 1, 8);
    assert!(
        state
            .world()
            .has_standing_clearance(registries.world(), position)
    );
    validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    // A real partial movement frame incurs exertion now, but physiology only
    // charges it at the next slow boundary. Standing still afterward does not
    // forgive the already incurred energy debt.
    let walk = PlayerControlInput::new(HorizontalControlInput::new(1_000_000, 0).unwrap());
    let _ = advance_control_tick_with_input(&registries, &mut state, walk).unwrap();
    let player = state.logistics().player().unwrap();
    assert!(
        !player
            .locomotion_exertion()
            .pending()
            .energy_cost_per_tick()
            .is_zero()
    );
    let physiology = registries.survival().physiology();
    let before = *state.survival().player().unwrap();
    let energy = physiology
        .hungry_below()
        .checked_add(physiology.basal_energy_cost_per_tick())
        .unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            energy,
            before.hydration(),
            before.vitality(),
            before.nutrition(),
            before.vitality_recovery_remainder(),
            before.thermal_comfort_ppm(),
        ),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        warning_runway(
            energy.nanojoules(),
            physiology.hungry_below().nanojoules(),
            physiology.basal_energy_cost_per_tick().nanojoules(),
        ),
        Some(TickSpan::new(2)),
    );
    assert_eq!(
        super::assess_player_resting_provision_runway(&registries, &state)
            .unwrap()
            .hungry_in(),
        Some(TickSpan::new(1)),
        "a pending locomotion charge must advance the next hunger warning",
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);
    while !state.control_tick().is_simulation_boundary() {
        assert_eq!(
            advance_control_tick(&registries, &mut state).unwrap(),
            advance_control_tick(&registries, &mut replay).unwrap(),
        );
    }
    assert_eq!(state, replay);
    assert_eq!(
        assess_survival(&registries, &state).unwrap().hunger(),
        HungerState::Hungry
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn dead_players_have_no_actionable_survival_runway_before_respawn() {
    use crate::content::build_registries;
    use crate::core::state::{AppState, validate_loaded_state};
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::player::{validate_admit_player, validate_respawn_player};
    use crate::spatial::VoxelCoord;
    use crate::survival::{Vitality, player_record_with_thermal_comfort};

    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(super::assess_player_resting_provision_runway(&registries, &state).is_some());
    assert!(super::assess_player_unheated_comfort_runway(&registries, &state).is_some());
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
    let at_death = state.clone();
    assert_eq!(
        super::assess_player_resting_provision_runway(&registries, &state),
        None
    );
    assert_eq!(
        super::assess_player_unheated_comfort_runway(&registries, &state),
        None
    );
    assert_eq!(
        state, at_death,
        "forecast queries cannot mutate a deceased actor"
    );

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(
        super::assess_player_resting_provision_runway(&registries, &replay),
        None
    );
    assert_eq!(
        super::assess_player_unheated_comfort_runway(&registries, &replay),
        None
    );
    let _ = validate_respawn_player(&registries, &state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = validate_respawn_player(&registries, &replay)
        .unwrap()
        .commit(&mut replay)
        .unwrap();
    assert_eq!(state, replay);
    assert!(super::assess_player_resting_provision_runway(&registries, &state).is_some());
    assert!(super::assess_player_unheated_comfort_runway(&registries, &state).is_some());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn admitted_drink_invalidates_no_intake_runway_until_absorption_finishes() {
    use crate::content::{FLUID_WATER, build_registries};
    use crate::core::quantity::Temperature;
    use crate::core::state::{AppState, validate_loaded_state};
    use crate::fluid::add_fluid_store_with_contents_for_fixture;
    use crate::logistics::validate_place_fluid_store;
    use crate::player::validate_admit_player;
    use crate::simulation::advance_tick;
    use crate::spatial::VoxelCoord;
    use crate::survival::validate_drink;

    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let volume = registries
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_place_fluid_store(&state, store, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = advance_tick(&registries, &mut state).unwrap();
    assert!(super::assess_player_resting_provision_runway(&registries, &state).is_some());
    let completed = validate_drink(&registries, &state, store, volume)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .completes_at();
    let before = state.clone();
    assert_eq!(
        super::assess_player_resting_provision_runway(&registries, &state),
        None
    );
    assert!(super::assess_player_unheated_comfort_runway(&registries, &state).is_some());
    assert_eq!(state, before);
    while state.tick() < completed {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(state.survival().pending_direct_consumption().is_none());
    assert!(super::assess_player_resting_provision_runway(&registries, &state).is_some());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn admitted_meal_invalidates_no_intake_runway_and_replays_absorption() {
    use crate::content::{FORM_FOOD, MATERIAL_BERRIES, build_registries};
    use crate::core::quantity::Temperature;
    use crate::core::state::{AppState, validate_loaded_state};
    use crate::inventory::{MaterialLotSelection, deposit_lot_for_test};
    use crate::material::CommodityKey;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::player::validate_admit_player;
    use crate::simulation::advance_tick;
    use crate::spatial::VoxelCoord;
    use crate::survival::validate_eat;

    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = advance_tick(&registries, &mut state).unwrap();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let mass = registries
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_meal_mass();
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let completes_at = validate_eat(
        &registries,
        &state,
        carried,
        &[MaterialLotSelection::new(lot, mass)],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap()
    .completes_at();
    assert_eq!(
        super::assess_player_resting_provision_runway(&registries, &state),
        None
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    while state.tick() < completes_at {
        let outcome = advance_tick(&registries, &mut state).unwrap();
        let resumed = advance_tick(&registries, &mut replay).unwrap();
        assert_eq!(outcome, resumed);
        assert_eq!(state, replay);
    }
    assert!(state.survival().pending_direct_consumption().is_none());
    assert!(super::assess_player_resting_provision_runway(&registries, &state).is_some());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
