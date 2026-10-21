//! A second resident makes real, ration-funded trips rather than appearing at a level threshold.

use crate::content::{FORM_FOOD, MATERIAL_WILD_NUTS};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::validate_loaded_state;
use crate::core::time::SimulationTick;
use crate::energy::EnergyCarrier;
use crate::kobolds::{KoboldRole, available_nearby_kobold_scouts};
use crate::logistics::relocate_player_for_test;
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

use super::{advance_to, scenario};

fn ration(
    registries: &crate::Registries,
    state: &mut crate::core::state::AppState,
    den: VoxelCoord,
) -> crate::surface::SurfaceResourceId {
    ration_mass(registries, state, den, Mass::from_milligrams(100_000))
}

fn ration_mass(
    registries: &crate::Registries,
    state: &mut crate::core::state::AppState,
    den: VoxelCoord,
    amount: Mass,
) -> crate::surface::SurfaceResourceId {
    let commodity = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let plan = validate_create_surface_resources(
        registries,
        state,
        [SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            den,
            commodity,
            amount,
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_WILD_NUTS),
        )
        .unwrap()],
    )
    .unwrap();
    let id = plan.first_resource_id();
    plan.commit(state).unwrap();
    id
}

#[test]
fn bright_den_holds_a_calm_guard_but_does_not_trap_one_already_in_bright_light() {
    use crate::content::{
        BLOCK_STATE_STONE, CAMPFIRE_CHIP_FUEL_MASS, CAMPFIRE_FUEL_MASS, ENERGY_ELECTRICAL_BUFFER,
        ENERGY_STONE_RING_CAMPFIRE, FORM_CHIP, FORM_LOG, FORM_LUMP, MATERIAL_STONE, MATERIAL_WOOD,
        build_registries,
    };
    use crate::core::quantity::Energy;
    use crate::energy::{
        add_energy_store_with_initial_for_fixture, assess_local_illumination,
        validate_assemble_energy_store, validate_combust_selected_fuel,
    };
    use crate::inventory::deposit_bulk_for_test;
    use crate::logistics::{add_traversable_edge_for_test, place_energy_store_for_fixture};
    use crate::surface::SurfaceResourceId;
    use crate::world::{
        BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut starting = crate::core::state::AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(0, 0, 1);
    let withdraw_to = VoxelCoord::new(4, 0, 0);
    for (from, to) in [(den, junction), (junction, tip)] {
        add_traversable_edge_for_test(&mut starting, from, to);
    }
    starting
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    for position in [den, junction] {
        validate_generate_chunk(
            &registries,
            &starting,
            WorldGenerationSeed::new(7),
            ChunkCoord::containing(position),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut starting)
        .unwrap();
    }
    for position in [den, junction, tip, withdraw_to] {
        set_generated_block_for_test(
            registries.world(),
            &mut starting,
            VoxelCoord::new(position.x(), -1, position.z()),
            BLOCK_STATE_STONE,
        );
        for y in 0..=1 {
            set_generated_block_for_test(
                registries.world(),
                &mut starting,
                VoxelCoord::new(position.x(), y, position.z()),
                BlockStateId::AIR,
            );
        }
    }
    let machine = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut starting,
        ENERGY_ELECTRICAL_BUFFER,
        Energy::ZERO,
    )
    .unwrap();
    place_energy_store_for_fixture(&mut starting, machine, junction);
    advance_to(&registries, &mut starting, 195);
    let food: SurfaceResourceId = ration(&registries, &mut starting, den);
    advance_to(&registries, &mut starting, 200);
    assert_eq!(
        starting
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        junction,
        "a locally reported machine and a real 20 g ration fund the guard"
    );
    let ignite = |state: &mut crate::core::state::AppState,
                  at: VoxelCoord,
                  fuel_form,
                  fuel_mass| {
        validate_admit_player(&registries, state, at)
            .unwrap()
            .commit(state)
            .unwrap();
        let carried = state.logistics().player().unwrap().carried_stockpile();
        for (commodity, mass) in [
            (
                CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
                Mass::from_milligrams(1_000_000),
            ),
            (CommodityKey::new(MATERIAL_WOOD, fuel_form), fuel_mass),
        ] {
            deposit_bulk_for_test(&registries, state, carried, commodity, mass).unwrap();
        }
        let fire =
            validate_assemble_energy_store(&registries, state, ENERGY_STONE_RING_CAMPFIRE, carried)
                .unwrap()
                .commit(state)
                .unwrap();
        validate_combust_selected_fuel(
            &registries,
            state,
            fire,
            carried,
            CommodityKey::new(MATERIAL_WOOD, fuel_form),
        )
        .unwrap()
        .commit(state)
        .unwrap();
        relocate_player_for_test(state, withdraw_to);
    };
    let mut fleeing = starting.clone();
    ignite(&mut fleeing, junction, FORM_LOG, CAMPFIRE_FUEL_MASS);
    let mut waiting = starting.clone();
    ignite(&mut waiting, den, FORM_CHIP, CAMPFIRE_CHIP_FUEL_MASS);
    let fleeing_total = calculate_matter_accounting(&fleeing).unwrap().total();
    let waiting_total = calculate_matter_accounting(&waiting).unwrap().total();
    let mut occupied = fleeing.clone();
    relocate_player_for_test(&mut occupied, den);
    assert!(
        assess_local_illumination(&registries, &fleeing, junction).level() >= 3
            && assess_local_illumination(&registries, &fleeing, den).level() >= 3
    );
    assert!(
        assess_local_illumination(&registries, &waiting, junction).level() < 3
            && assess_local_illumination(&registries, &waiting, den).level() >= 3
    );

    advance_to(&registries, &mut fleeing, 205);
    assert_eq!(
        fleeing
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        den,
        "a threatened guard must be able to flee through a lit home edge"
    );
    assert_eq!(
        fleeing
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_tracks
            .last()
            .unwrap()
            .observed_activity(),
        crate::kobolds::KoboldTrailKind::Fleeing
    );
    advance_to(&registries, &mut occupied, 205);
    let blocked_guard = occupied.kobold_state().colony.as_ref().unwrap();
    assert_eq!(
        blocked_guard.watch_position(),
        junction,
        "emergency retreat must not override physical player occupancy"
    );
    assert!(blocked_guard.watch_deployed_at.is_some());
    assert!(blocked_guard.watch_alarm_pending);
    advance_to(&registries, &mut waiting, 220);
    assert_eq!(
        waiting
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        junction,
        "a calm sentry must still refuse a destination guarded by bright light"
    );
    assert_eq!(
        waiting.surface().get(food).unwrap().remaining_mass(),
        Mass::from_milligrams(80_000),
        "blocked return and urgent flight both spend no new ration"
    );

    for (state, conserved) in [
        (&mut fleeing, fleeing_total),
        (&mut waiting, waiting_total),
        (&mut occupied, fleeing_total),
    ] {
        let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, state)).unwrap();
        let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        for _ in 0..10 {
            assert_eq!(
                crate::simulation::advance_control_tick(&registries, state).unwrap(),
                crate::simulation::advance_control_tick(&registries, &mut replay).unwrap()
            );
        }
        assert_eq!(*state, replay);
        assert_eq!(validate_loaded_state(&registries, state), Ok(()));
        assert_eq!(
            calculate_matter_accounting(state).unwrap().total(),
            conserved,
            "energy and food spent are represented rather than silently lost"
        );
    }
}

#[test]
fn guarded_home_approach_delays_watchman_warning_until_real_safe_return() {
    let (registries, mut held, route) =
        scenario(EnergyCarrier::Electrical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut held, 195);
    let ration = ration(&registries, &mut held, route[0]);
    let conserved = calculate_matter_accounting(&held).unwrap().total();
    advance_to(&registries, &mut held, 200);
    assert_eq!(
        held.kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[1]
    );
    let mut unguarded = held.clone();
    // The player stands two cells behind the active junction watchman: close
    // enough to see it and guard the den, but not close enough to frighten
    // the guard at its current position.
    let observer = VoxelCoord::new(route[0].x() - 1, route[0].y(), route[0].z());
    validate_admit_player(&registries, &held, observer)
        .unwrap()
        .commit(&mut held)
        .unwrap();
    assert!(!crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &held,
        route[1]
    ));
    assert!(crate::kobolds::patrol::scout_sees_guarded_cell(
        &registries,
        &held,
        route[1],
        route[0]
    ));
    advance_to(&registries, &mut held, 220);
    advance_to(&registries, &mut unguarded, 220);
    let waiting = held.kobold_state().colony.as_ref().unwrap();
    assert_eq!(waiting.watch_position(), route[1]);
    assert!(waiting.watch_deployed_at.is_some());
    assert!(waiting.watch_alarm_pending);
    assert!(!waiting.watch_counterpatrol_pending);
    assert_eq!(
        unguarded
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[0],
        "the identical provisioned patrol returns on time if the den is unguarded"
    );
    assert_eq!(
        held.surface().get(ration).unwrap().remaining_mass(),
        Mass::from_milligrams(80_000),
        "waiting does not consume additional watch rations"
    );

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &held)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for state in [&mut held, &mut replay] {
        relocate_player_for_test(state, VoxelCoord::new(30, 0, 30));
    }
    advance_to(&registries, &mut held, 225);
    advance_to(&registries, &mut replay, 225);
    assert_eq!(held, replay);
    let returned = held.kobold_state().colony.as_ref().unwrap();
    assert_eq!(returned.watch_position(), route[0]);
    assert!(returned.watch_deployed_at.is_none());
    assert!(!returned.watch_alarm_pending);
    assert!(returned.watch_counterpatrol_pending);
    assert!(
        returned.wary_until.is_some(),
        "the warning reaches the scout only after the same watchman actually gets home"
    );
    assert_eq!(
        held.surface().get(ration).unwrap().remaining_mass(),
        Mass::from_milligrams(80_000)
    );
    assert_eq!(
        calculate_matter_accounting(&held).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &held), Ok(()));
}

#[test]
fn directly_frightened_watchman_can_still_flee_home_on_a_real_route_edge() {
    let (registries, mut threatened, route) =
        scenario(EnergyCarrier::Electrical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut threatened, 195);
    let food = ration(&registries, &mut threatened, route[0]);
    let conserved = calculate_matter_accounting(&threatened).unwrap().total();
    advance_to(&registries, &mut threatened, 200);
    validate_admit_player(&registries, &threatened, route[2])
        .unwrap()
        .commit(&mut threatened)
        .unwrap();
    assert!(crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &threatened,
        route[1]
    ));
    assert!(crate::kobolds::patrol::scout_sees_guarded_cell(
        &registries,
        &threatened,
        route[1],
        route[0]
    ));
    advance_to(&registries, &mut threatened, 205);
    let colony = threatened.kobold_state().colony.as_ref().unwrap();
    assert!(colony.watch_deployed_at.is_none());
    assert_eq!(colony.watch_position(), route[0]);
    assert_eq!(
        colony.watch_tracks.last().unwrap().observed_activity(),
        crate::kobolds::KoboldTrailKind::Fleeing
    );
    assert!(colony.watch_counterpatrol_pending);
    assert_eq!(
        threatened.surface().get(food).unwrap().remaining_mass(),
        Mass::from_milligrams(80_000),
        "fleeing consumes no additional food"
    );
    assert_eq!(
        calculate_matter_accounting(&threatened).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &threatened), Ok(()));
}

#[test]
fn watchman_is_an_opening_den_resident_but_only_organizes_after_report_and_a_full_meal() {
    let (registries, mut alerted, route) =
        scenario(EnergyCarrier::Electrical, Some(VoxelCoord::new(0, 0, 0)));
    let (_, mut ordinary, _) = scenario(EnergyCarrier::Electrical, None);
    advance_to(&registries, &mut alerted, 195);
    advance_to(&registries, &mut ordinary, 195);
    let food = ration(&registries, &mut alerted, route[0]);
    let ordinary_food = ration(&registries, &mut ordinary, route[0]);
    let conserved = calculate_matter_accounting(&alerted).unwrap().total();
    advance_to(&registries, &mut alerted, 200);
    advance_to(&registries, &mut ordinary, 200);
    assert_eq!(
        alerted
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[1]
    );
    assert_eq!(
        ordinary
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[0]
    );
    assert_eq!(
        alerted.surface().get(food).unwrap().remaining_mass(),
        Mass::from_milligrams(80_000)
    );
    assert_eq!(
        ordinary
            .surface()
            .get(ordinary_food)
            .unwrap()
            .remaining_mass(),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(
        alerted
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_ready_at,
        SimulationTick::new(300)
    );
    let mut observer = alerted.clone();
    validate_admit_player(&registries, &observer, route[2])
        .unwrap()
        .commit(&mut observer)
        .unwrap();
    assert!(
        available_nearby_kobold_scouts(&registries, &observer)
            .any(|resident| resident.role() == KoboldRole::Watchman
                && resident.position() == route[1])
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &alerted)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut replay = decoded.into_state(&registries).unwrap();
    for _ in 0..20 {
        assert_eq!(
            crate::simulation::advance_tick(&registries, &mut alerted).unwrap(),
            crate::simulation::advance_tick(&registries, &mut replay).unwrap(),
        );
    }
    assert_eq!(alerted, replay);
    assert_eq!(
        alerted
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[0]
    );
    assert_eq!(
        alerted.surface().get(food).unwrap().remaining_mass(),
        Mass::from_milligrams(80_000),
        "one ration buys one finite watch, not a meal every scout tick"
    );
    validate_admit_player(&registries, &alerted, route[1])
        .unwrap()
        .commit(&mut alerted)
        .unwrap();
    let sightings = available_nearby_kobold_scouts(&registries, &alerted).collect::<Vec<_>>();
    assert!(
        sightings
            .iter()
            .any(|s| s.role() == KoboldRole::Watchman && s.position() == route[0])
    );
    assert_eq!(
        calculate_matter_accounting(&alerted).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&ordinary).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &alerted), Ok(()));
}

#[test]
fn watchman_requires_unblocked_safe_route_and_whole_food_ration() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Electrical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut state, 195);
    let food = ration(&registries, &mut state, route[0]);
    let mut nearby = state.clone();
    validate_admit_player(&registries, &nearby, route[2])
        .unwrap()
        .commit(&mut nearby)
        .unwrap();
    advance_to(&registries, &mut nearby, 200);
    assert!(
        nearby
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_deployed_at
            .is_none(),
        "a watchman cannot ignore a living player adjacent to its planned post"
    );
    let mut blocked = state.clone();
    validate_admit_player(&registries, &blocked, route[1])
        .unwrap()
        .commit(&mut blocked)
        .unwrap();
    advance_to(&registries, &mut blocked, 200);
    assert!(
        blocked
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_deployed_at
            .is_none()
    );
    assert_eq!(
        blocked
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[0]
    );
    advance_to(&registries, &mut state, 200);
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_deployed_at
            .is_some()
    );
    assert_eq!(
        state.surface().get(food).unwrap().remaining_mass(),
        Mass::from_milligrams(80_000)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &blocked), Ok(()));
}

#[test]
fn short_rations_cannot_deploy_watchman_and_a_player_can_trap_a_returning_guard() {
    let (registries, mut hungry, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut hungry, 195);
    let small = ration_mass(
        &registries,
        &mut hungry,
        route[0],
        Mass::from_milligrams(15_000),
    );
    advance_to(&registries, &mut hungry, 200);
    assert!(
        hungry
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_deployed_at
            .is_none()
    );
    assert_eq!(
        hungry.surface().get(small).unwrap().remaining_mass(),
        Mass::from_milligrams(15_000)
    );
    let mut provisioned = hungry.clone();
    ration(&registries, &mut provisioned, route[0]);
    advance_to(&registries, &mut provisioned, 205);
    assert_eq!(
        provisioned
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[0],
        "the watchman waits while the scout passes the junction"
    );
    advance_to(&registries, &mut provisioned, 210);
    assert_eq!(
        provisioned
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[1]
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &provisioned)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut replay = loaded.into_state(&registries).unwrap();
    validate_admit_player(&registries, &provisioned, route[0])
        .unwrap()
        .commit(&mut provisioned)
        .unwrap();
    validate_admit_player(&registries, &replay, route[0])
        .unwrap()
        .commit(&mut replay)
        .unwrap();
    advance_to(&registries, &mut provisioned, 235);
    advance_to(&registries, &mut replay, 235);
    assert_eq!(provisioned, replay);
    assert_eq!(
        provisioned
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[1],
        "the guard cannot vanish through a player-occupied den entrance"
    );
    assert_eq!(validate_loaded_state(&registries, &provisioned), Ok(()));
}

#[test]
fn watchman_spots_player_at_outer_visible_range_and_reports_only_after_return() {
    let (registries, mut observed, route) =
        scenario(EnergyCarrier::Electrical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut observed, 195);
    ration(&registries, &mut observed, route[0]);
    advance_to(&registries, &mut observed, 200);
    assert_eq!(
        observed
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[1]
    );
    let mut unobserved = observed.clone();
    // At two voxels the visible person is not adjacent enough to force a
    // retreat. A sentry nevertheless has real reason to report the intrusion.
    validate_admit_player(&registries, &observed, VoxelCoord::new(0, 0, 2))
        .unwrap()
        .commit(&mut observed)
        .unwrap();
    assert!(!crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &observed,
        route[1]
    ));
    let initial_matter = calculate_matter_accounting(&observed).unwrap().total();
    advance_to(&registries, &mut observed, 205);
    advance_to(&registries, &mut unobserved, 205);
    let colony = observed.kobold_state().colony.as_ref().unwrap();
    assert!(colony.watch_alarm_pending);
    assert!(colony.watch_deployed_at.is_some());
    assert!(colony.wary_until.is_none());
    assert!(
        !unobserved
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_alarm_pending
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &observed)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 205..=220 {
        let event = crate::simulation::advance_tick(&registries, &mut observed).unwrap();
        let replay_event = crate::simulation::advance_tick(&registries, &mut replay).unwrap();
        assert_eq!(event, replay_event);
    }
    assert_eq!(observed, replay);
    let colony = observed.kobold_state().colony.as_ref().unwrap();
    assert_eq!(colony.watch_position(), route[0]);
    assert!(!colony.watch_alarm_pending);
    assert!(colony.watch_counterpatrol_pending);
    assert!(colony.wary_until.is_some());
    assert_eq!(
        calculate_matter_accounting(&observed).unwrap().total(),
        initial_matter
    );
    assert_eq!(validate_loaded_state(&registries, &observed), Ok(()));
}

#[test]
fn trusted_load_rejects_unreported_or_out_of_schedule_watch_deployments() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut state, 195);
    ration(&registries, &mut state, route[0]);
    advance_to(&registries, &mut state, 200);
    let valid = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    for (deployed, ready) in [(199_u64, 299_u64), (201, 301), (205, 305), (200, 250)] {
        let mut invalid = valid.clone();
        invalid["state"]["systems"]["kobolds"]["colony"]["watch_deployed_at"] =
            serde_json::json!(deployed);
        invalid["state"]["systems"]["kobolds"]["colony"]["watch_ready_at"] =
            serde_json::json!(ready);
        let loaded: LoadedSaveEnvelope = serde_json::from_value(invalid).unwrap();
        assert!(loaded.into_state(&registries).is_err());
    }
    let mut missing = valid;
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("watch_deployed_at");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());
}

#[test]
fn watchman_locally_warns_scout_only_after_its_actual_return_to_the_den() {
    let (registries, mut warned, route) =
        scenario(EnergyCarrier::Electrical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut warned, 195);
    let food = ration(&registries, &mut warned, route[0]);
    let conserved = calculate_matter_accounting(&warned).unwrap().total();
    advance_to(&registries, &mut warned, 200);
    assert_eq!(
        warned
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[1]
    );
    assert_eq!(
        warned
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[0]
    );
    let mut peaceful = warned.clone();
    validate_admit_player(&registries, &warned, route[2])
        .unwrap()
        .commit(&mut warned)
        .unwrap();
    advance_to(&registries, &mut warned, 205);
    advance_to(&registries, &mut peaceful, 205);
    let colony = warned.kobold_state().colony.as_ref().unwrap();
    assert_eq!(colony.watch_position(), route[0]);
    assert_eq!(colony.scout_position(), route[0]);
    assert!(
        !colony.watch_alarm_pending,
        "the returned watchman delivered its warning"
    );
    assert_eq!(colony.wary_until, Some(SimulationTick::new(245)));
    assert_eq!(
        peaceful
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[1]
    );
    assert!(
        peaceful
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .wary_until
            .is_none()
    );
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &warned)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&checkpoint).unwrap();
    let mut replay = loaded.into_state(&registries).unwrap();
    advance_to(&registries, &mut warned, 220);
    advance_to(&registries, &mut replay, 220);
    advance_to(&registries, &mut peaceful, 220);
    assert_eq!(warned, replay);
    assert_eq!(
        warned
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[0]
    );
    assert_eq!(
        peaceful
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[1],
        "without warning the scout resumes its ordinary patrol"
    );
    assert_eq!(
        warned.surface().get(food).unwrap().remaining_mass(),
        Mass::from_milligrams(80_000)
    );
    assert_eq!(
        calculate_matter_accounting(&warned).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &warned), Ok(()));
}

#[test]
fn blocked_guard_retreat_keeps_locally_seen_warning_pending_not_teleported() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut state, 195);
    ration(&registries, &mut state, route[0]);
    advance_to(&registries, &mut state, 200);
    validate_admit_player(&registries, &state, route[0])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to(&registries, &mut state, 205);
    let colony = state.kobold_state().colony.as_ref().unwrap();
    assert_eq!(colony.watch_position(), route[1]);
    assert!(colony.watch_alarm_pending);
    assert_eq!(colony.watch_deployed_at, Some(SimulationTick::new(200)));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let restored = loaded.into_state(&registries).unwrap();
    assert_eq!(state, restored);
    let mut forged = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    forged["state"]["systems"]["kobolds"]["colony"]["watch_deployed_at"] = serde_json::Value::Null;
    let loaded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
    assert!(
        loaded.into_state(&registries).is_err(),
        "a sheltered pair cannot retain an undelivered alarm"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn physically_reported_intrusion_funds_one_two_edge_outer_picket() {
    let (registries, mut alerted, route) =
        scenario(EnergyCarrier::Electrical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut alerted, 195);
    let food = ration(&registries, &mut alerted, route[0]);
    let original = calculate_matter_accounting(&alerted).unwrap().total();
    advance_to(&registries, &mut alerted, 200);
    validate_admit_player(&registries, &alerted, route[2])
        .unwrap()
        .commit(&mut alerted)
        .unwrap();
    advance_to(&registries, &mut alerted, 205);
    assert!(
        alerted
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_counterpatrol_pending
    );
    // Focused owner setup: the player withdraws after the physically delivered
    // alarm; the test exercises kobold behavior, not player locomotion.
    relocate_player_for_test(&mut alerted, VoxelCoord::new(40, 0, 0));
    let mut reached_tip = None;
    for tick in (210..=365).step_by(5) {
        advance_to(&registries, &mut alerted, tick);
        if alerted.kobold_state().colony.as_ref().unwrap().watch_leg == 1 {
            reached_tip = Some(tick);
            break;
        }
    }
    let tip_tick = reached_tip.expect("a fed watchman must eventually extend its picket");
    let watcher = alerted.kobold_state().colony.as_ref().unwrap();
    assert_eq!(watcher.watch_position(), route[2]);
    assert!(!watcher.watch_counterpatrol_pending);
    assert_ne!(watcher.scout_position(), watcher.watch_position());
    assert_eq!(
        alerted.surface().get(food).unwrap().remaining_mass(),
        Mass::from_milligrams(60_000),
        "one regular watch and one actual outer picket consume exactly two full rations"
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &alerted)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&bytes).unwrap();
    let mut replay = decoded.into_state(&registries).unwrap();
    for _ in 0..30 {
        assert_eq!(
            crate::simulation::advance_control_tick(&registries, &mut alerted).unwrap(),
            crate::simulation::advance_control_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(alerted, replay);
    advance_to(&registries, &mut alerted, tip_tick + 25);
    advance_to(&registries, &mut replay, tip_tick + 25);
    assert_eq!(alerted, replay);
    let watcher = alerted.kobold_state().colony.as_ref().unwrap();
    assert_eq!(watcher.watch_position(), route[0]);
    assert_eq!(watcher.watch_leg, 0);
    assert!(!watcher.watch_counterpatrol_pending);
    assert_eq!(
        alerted.surface().get(food).unwrap().remaining_mass(),
        Mass::from_milligrams(60_000)
    );
    assert_eq!(
        calculate_matter_accounting(&alerted).unwrap().total(),
        original
    );
    assert_eq!(validate_loaded_state(&registries, &alerted), Ok(()));
}

#[test]
fn a_single_ration_cannot_fund_the_counterpicket_even_after_real_warning() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut state, 195);
    let ration_id = ration_mass(
        &registries,
        &mut state,
        route[0],
        Mass::from_milligrams(20_000),
    );
    advance_to(&registries, &mut state, 200);
    validate_admit_player(&registries, &state, route[2])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to(&registries, &mut state, 205);
    relocate_player_for_test(&mut state, VoxelCoord::new(40, 0, 0));
    advance_to(&registries, &mut state, 350);
    let watcher = state.kobold_state().colony.as_ref().unwrap();
    assert!(watcher.watch_counterpatrol_pending);
    assert_eq!(watcher.watch_position(), route[0]);
    assert_eq!(watcher.watch_leg, 0);
    assert_eq!(
        state.surface().get(ration_id).unwrap().remaining_mass(),
        Mass::ZERO
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn player_can_turn_back_an_outer_picket_before_the_guard_enters_its_tip() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Electrical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut state, 195);
    let food = ration(&registries, &mut state, route[0]);
    let total = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 200);
    validate_admit_player(&registries, &state, route[2])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to(&registries, &mut state, 205);
    relocate_player_for_test(&mut state, VoxelCoord::new(40, 0, 0));
    let mut at_junction = None;
    for tick in (210..=365).step_by(5) {
        advance_to(&registries, &mut state, tick);
        let colony = state.kobold_state().colony.as_ref().unwrap();
        if colony.watch_deployed_at.is_some()
            && colony.watch_ready_at.value() >= 400
            && colony.watch_position() == route[1]
        {
            at_junction = Some(tick);
            break;
        }
    }
    let junction_tick = at_junction.expect("a second provisioned watch must depart");
    relocate_player_for_test(&mut state, route[2]);
    advance_to(&registries, &mut state, junction_tick + 5);
    let watcher = state.kobold_state().colony.as_ref().unwrap();
    assert_eq!(watcher.watch_position(), route[0]);
    assert_eq!(watcher.watch_leg, 0);
    assert!(watcher.watch_counterpatrol_pending);
    assert_eq!(
        state.surface().get(food).unwrap().remaining_mass(),
        Mass::from_milligrams(60_000)
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn outer_picket_responds_to_visible_flank_but_not_an_unseen_opaque_corner() {
    use crate::content::{BLOCK_STATE_STONE, ENERGY_MECHANICAL_SMALL_DRIVE, build_registries};
    use crate::core::quantity::Energy;
    use crate::energy::add_energy_store_with_initial_for_fixture;
    use crate::logistics::{add_traversable_edge_for_test, place_energy_store_for_fixture};
    use crate::world::{
        BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut state = crate::core::state::AppState::new();
    let route = [
        VoxelCoord::new(-1, 0, 0),
        VoxelCoord::new(0, 0, 0),
        VoxelCoord::new(0, 0, 1),
    ];
    for (from, to) in [(route[0], route[1]), (route[1], route[2])] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    state.kobold_state_mut().apply_opening_colony(route);
    for coord in [
        ChunkCoord::containing(route[0]),
        ChunkCoord::containing(route[1]),
    ] {
        validate_generate_chunk(
            &registries,
            &state,
            WorldGenerationSeed::new(7),
            coord,
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
    }
    let corner = VoxelCoord::new(1, 0, 0);
    let observer = VoxelCoord::new(1, 0, 1);
    let camp = VoxelCoord::new(4, 0, 0);
    for cell in [route[0], route[1], route[2], corner, observer, camp] {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(cell.x(), -1, cell.z()),
            BLOCK_STATE_STONE,
        );
        for y in 0..=1 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(cell.x(), y, cell.z()),
                BlockStateId::AIR,
            );
        }
    }
    // Witnessed industry and finite den food are the only way this resident
    // can organize its second, wider patrol.
    let store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        Energy::ZERO,
    )
    .unwrap();
    place_energy_store_for_fixture(&mut state, store, route[1]);
    advance_to(&registries, &mut state, 195);
    ration(&registries, &mut state, route[0]);
    advance_to(&registries, &mut state, 200);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[1]
    );
    validate_admit_player(&registries, &state, route[2])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to(&registries, &mut state, 205);
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_counterpatrol_pending
    );
    relocate_player_for_test(&mut state, camp);

    let mut second_watch = None;
    for tick in (210..=365).step_by(5) {
        advance_to(&registries, &mut state, tick);
        let watcher = state.kobold_state().colony.as_ref().unwrap();
        if watcher.watch_deployed_at.is_some()
            && watcher.watch_ready_at.value() >= 400
            && watcher.watch_position() == route[1]
        {
            second_watch = Some(tick);
            break;
        }
    }
    let at_junction = second_watch.expect("a second provisioned watchman must really deploy");
    let mut hidden = state.clone();
    set_generated_block_for_test(registries.world(), &mut hidden, corner, BLOCK_STATE_STONE);
    for state in [&mut hidden, &mut state] {
        relocate_player_for_test(state, observer);
    }
    assert!(!crate::kobolds::visibility::scout_visible_from(
        &registries,
        &hidden,
        route[1],
        observer
    ));
    assert!(crate::kobolds::visibility::scout_visible_from(
        &registries,
        &state,
        route[1],
        observer
    ));
    assert!(crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &hidden,
        route[2]
    ));
    let conserved = calculate_matter_accounting(&hidden).unwrap().total();
    advance_to(&registries, &mut hidden, at_junction + 5);
    advance_to(&registries, &mut state, at_junction + 5);
    assert_eq!(
        hidden
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[2],
        "the sentry must not anticipate a player concealed at its next post"
    );
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[0],
        "a genuinely visible intruder sends the same sentry home"
    );
    advance_to(&registries, &mut hidden, at_junction + 10);
    assert!(
        hidden
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_alarm_pending
    );
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &hidden)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(hidden, restored);
    assert_eq!(
        calculate_matter_accounting(&hidden).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &hidden), Ok(()));
}

#[test]
fn forged_outer_picket_legs_and_unearned_alarm_orders_fail_save_admission() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut state, 195);
    ration(&registries, &mut state, route[0]);
    advance_to(&registries, &mut state, 200);
    let snapshot = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    for (leg, pending) in [(1, false), (2, false), (3, false), (0, true)] {
        let mut tampered = snapshot.clone();
        let colony = &mut tampered["state"]["systems"]["kobolds"]["colony"];
        colony["watch_leg"] = serde_json::json!(leg);
        colony["watch_counterpatrol_pending"] = serde_json::json!(pending);
        let loaded: LoadedSaveEnvelope = serde_json::from_value(tampered).unwrap();
        assert!(loaded.into_state(&registries).is_err());
    }
    let mut missing = snapshot;
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("watch_leg");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());
}

#[path = "watch_track_tests.rs"]
mod watch_track_tests;
