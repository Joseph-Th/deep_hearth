//! End-to-end resident water visits, depleted sources, local cues and trusted replay.

use super::*;

use crate::content::build_registries;
use crate::core::state::validate_loaded_state;
use crate::fluid::calculate_fluid_volume_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;
use crate::world::WorldGenerationSeed;
use crate::world_generation::validate_generate_initial_wilderness;

fn opening_water() -> (Registries, AppState, FluidStoreId, VoxelCoord) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_initial_wilderness(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4B4F_424F_4C44),
        VoxelCoord::new(8, 2, 8),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let (id, position) = state.logistics().fluid_store_locations().next().unwrap();
    (registries, state, id, position)
}

#[test]
fn witnessed_outer_patrol_drink_survives_unseen_return_without_disclosing_destination() {
    use crate::core::quantity::Temperature;
    use crate::fluid::{GeneratedFluidStoreSpec, validate_generate_fluid_store};
    use crate::logistics::{add_traversable_edge_for_test, validate_place_fluid_store};
    use crate::player::validate_admit_player;

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(1, 0, 0);
    add_traversable_edge_for_test(&mut state, den, junction);
    add_traversable_edge_for_test(&mut state, junction, tip);
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    while state.tick().value() < 989 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        tip
    );
    let water = validate_generate_fluid_store(
        &registries,
        &state,
        GeneratedFluidStoreSpec::new(
            SCOUT_DRINK_VOLUME,
            FLUID_WATER,
            SCOUT_DRINK_VOLUME,
            Temperature::from_millikelvin(293_150),
        ),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    validate_place_fluid_store(&state, water, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    // The player sees the outer water visit from two cells away, but
    // the scout's next return cell is outside local sight. The den guard
    // is also too distant to witness the player or warn the scout.
    let observer = VoxelCoord::new(3, 0, 0);
    validate_admit_player(&registries, &state, observer)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(crate::world::player_can_see_local_cell(
        &registries,
        &state,
        observer,
        tip
    ));
    assert!(!crate::world::player_can_see_local_cell(
        &registries,
        &state,
        observer,
        junction
    ));
    assert!(!crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &state,
        tip
    ));
    let fluid_before = calculate_fluid_volume_accounting(&state).unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let outcome = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(outcome, advance_tick(&registries, &mut replay).unwrap());
    assert_eq!(state, replay);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        junction
    );
    let event = outcome
        .kobold_scout()
        .expect("a witnessed water visit remains observable when the scout leaves sight");
    assert_eq!(
        event.position(),
        tip,
        "unseen return destination must stay concealed"
    );
    assert_eq!(event.drank_water(), Some((water, SCOUT_DRINK_VOLUME)));
    assert_eq!(event.relocated_drop(), None);
    assert_eq!(
        state.fluid().get_store(water).unwrap().stored_volume(),
        Volume::ZERO
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        fluid_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn thirsty_scout_can_drink_at_reached_water_without_abandoning_carried_cargo() {
    use crate::content::{FORM_FOOD, MATERIAL_BERRIES};
    use crate::core::quantity::{Mass, Temperature};
    use crate::fluid::{GeneratedFluidStoreSpec, validate_generate_fluid_store};
    use crate::kobolds::SCOUT_STEP_TICKS;
    use crate::logistics::{add_traversable_edge_for_test, validate_place_fluid_store};
    use crate::material::{CommodityKey, MaterialComposition};
    use crate::surface::{
        SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let water_position = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(1, 0, 0);
    add_traversable_edge_for_test(&mut state, den, water_position);
    add_traversable_edge_for_test(&mut state, water_position, tip);
    state
        .kobold_state_mut()
        .apply_opening_colony([den, water_position, tip]);

    let drink_due = state.kobold_state().colony.as_ref().unwrap().next_drink_at;
    while state.tick().value() + SCOUT_STEP_TICKS < drink_due.value() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    // Create loot on the actual outer patrol stop, not in the scout's hand.
    // The resident must retrieve it and carry it back through the water cell.
    let cargo = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        tip,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_BERRIES),
    )
    .unwrap();
    let planned = validate_create_surface_resources(&registries, &state, [cargo]).unwrap();
    let cargo_id = planned.first_resource_id();
    planned.commit(&mut state).unwrap();
    while state.tick().value() < drink_due.value() + SCOUT_STEP_TICKS * 2 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let scout = state.kobold_state().colony.as_ref().unwrap();
    assert_eq!(scout.scout_position(), water_position);
    assert_eq!(scout.scavenged_drop, Some(cargo_id));
    assert!(scout.returning);

    // Water appears locally while the scout is returning with a real parcel.
    let water = validate_generate_fluid_store(
        &registries,
        &state,
        GeneratedFluidStoreSpec::new(
            SCOUT_DRINK_VOLUME,
            FLUID_WATER,
            SCOUT_DRINK_VOLUME,
            Temperature::from_millikelvin(293_150),
        ),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    validate_place_fluid_store(&state, water, water_position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let water_before = calculate_fluid_volume_accounting(&state).unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    while state.tick().value() < drink_due.value() + SCOUT_STEP_TICKS * 3 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert_eq!(
        state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied(),
        Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME)),
        "a recoverable parcel must not prohibit a local water visit"
    );
    assert_eq!(
        state.fluid().get_store(water).unwrap().stored_volume(),
        Volume::ZERO
    );
    assert_eq!(
        state.surface().get(cargo_id).unwrap().position(),
        den,
        "the same real parcel must return to the den after the water visit"
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        water_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn scout_uses_authored_drink_temperature_and_preserves_unusable_water() {
    use crate::core::quantity::Temperature;
    use crate::fluid::{GeneratedFluidStoreSpec, validate_generate_fluid_store};
    use crate::logistics::{add_traversable_edge_for_test, validate_place_fluid_store};

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(1, 0, 0);
    let water = VoxelCoord::new(0, 0, 1);
    for (a, b) in [(den, junction), (junction, tip), (junction, water)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    let place = |state: &mut AppState, temperature| {
        let store = validate_generate_fluid_store(
            &registries,
            state,
            GeneratedFluidStoreSpec::new(
                SCOUT_DRINK_VOLUME,
                FLUID_WATER,
                SCOUT_DRINK_VOLUME,
                temperature,
            ),
        )
        .unwrap()
        .commit(state)
        .unwrap();
        validate_place_fluid_store(state, store, water)
            .unwrap()
            .commit(state)
            .unwrap();
        store
    };
    // The fluid owner rejects a liquid store below water's melting point.
    // This leaves no frozen-liquid custody for the scout to consume.
    let before_rejected_generation = state.clone();
    assert!(
        validate_generate_fluid_store(
            &registries,
            &state,
            GeneratedFluidStoreSpec::new(
                SCOUT_DRINK_VOLUME,
                FLUID_WATER,
                SCOUT_DRINK_VOLUME,
                Temperature::from_millikelvin(270_150),
            ),
        )
        .is_err()
    );
    assert_eq!(state, before_rejected_generation);
    let scalding = place(&mut state, Temperature::from_millikelvin(345_150));
    let observer_route = |state: &AppState| {
        reachable_water_from_junction(
            &registries,
            state,
            state.kobold_state().colony.as_ref().unwrap(),
            None,
        )
    };
    assert_eq!(usable_water_at(&registries, &state, water), None);
    assert_eq!(observer_route(&state), None);
    let safe = place(&mut state, Temperature::from_millikelvin(293_150));
    assert_eq!(
        usable_water_at(&registries, &state, water),
        Some((safe, SCOUT_DRINK_VOLUME))
    );
    assert_eq!(observer_route(&state), Some((water, None)));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let initial = calculate_fluid_volume_accounting(&state).unwrap();
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..1_400 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut replay).unwrap();
        if state
            .kobold_state()
            .consumed_fluids
            .contains_key(&FLUID_WATER)
        {
            break;
        }
    }
    assert_eq!(state, replay);
    assert_eq!(
        state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied(),
        Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME))
    );
    assert_eq!(
        state.fluid().get_store(safe).unwrap().stored_volume(),
        Volume::ZERO
    );
    assert_eq!(
        state.fluid().get_store(scalding).unwrap().stored_volume(),
        SCOUT_DRINK_VOLUME,
        "undrinkable water is never silently withdrawn"
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), initial);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn one_unwithdrawable_local_vessel_cannot_hide_another_real_drink() {
    use crate::content::{FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION};
    use crate::core::quantity::{Area, Length, Temperature};
    use crate::fluid::{
        GeneratedFluidStoreSpec, validate_fluid_egress, validate_generate_fluid_store,
        validate_mount_fluid_store,
    };
    use crate::logistics::{add_traversable_edge_for_test, validate_place_fluid_store};
    use crate::spatial::VoxelBounds;
    use crate::structural::{
        add_structural_element, make_test_structural_geometry,
        materialize_structural_element_for_test, validate_activate_structural_element,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    add_traversable_edge_for_test(&mut state, den, VoxelCoord::new(0, 0, 0));
    add_traversable_edge_for_test(
        &mut state,
        VoxelCoord::new(0, 0, 0),
        VoxelCoord::new(1, 0, 0),
    );
    state.kobold_state_mut().apply_opening_colony([
        den,
        VoxelCoord::new(0, 0, 0),
        VoxelCoord::new(1, 0, 0),
    ]);
    let frame = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        make_test_structural_geometry(
            VoxelBounds::new(den, VoxelCoord::new(0, 1, 1)).unwrap(),
            Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, frame, FORM_LOG);
    let _ = validate_activate_structural_element(&registries, &state, frame)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let add_water = |state: &mut AppState| {
        let store = validate_generate_fluid_store(
            &registries,
            state,
            GeneratedFluidStoreSpec::new(
                SCOUT_DRINK_VOLUME,
                FLUID_WATER,
                SCOUT_DRINK_VOLUME,
                Temperature::from_millikelvin(293_150),
            ),
        )
        .unwrap()
        .commit(state)
        .unwrap();
        validate_place_fluid_store(state, store, den)
            .unwrap()
            .commit(state)
            .unwrap();
        store
    };
    let blocked = add_water(&mut state);
    let available = add_water(&mut state);
    let _ = validate_mount_fluid_store(&registries, &state, blocked, frame)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX);
    let mut state = serde_json::from_value::<LoadedSaveEnvelope>(encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert!(validate_fluid_egress(&registries, &state, blocked, SCOUT_DRINK_VOLUME).is_err());
    assert!(validate_fluid_egress(&registries, &state, available, SCOUT_DRINK_VOLUME).is_ok());
    let before = calculate_fluid_volume_accounting(&state).unwrap();
    for _ in 0..1_550 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        if state
            .kobold_state()
            .consumed_fluids
            .contains_key(&FLUID_WATER)
        {
            break;
        }
    }
    assert_eq!(
        state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied(),
        Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME)),
        "a failed withdrawal at the first vessel must not conceal the usable neighbor"
    );
    assert_eq!(
        state.fluid().get_store(blocked).unwrap().stored_volume(),
        SCOUT_DRINK_VOLUME
    );
    assert_eq!(
        state.fluid().get_store(available).unwrap().stored_volume(),
        Volume::ZERO
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..100 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut replay).unwrap();
    }
    assert_eq!(state, replay);
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), before);
}

#[test]
fn junction_water_search_prefers_a_real_nearby_source_over_far_detours() {
    use crate::content::{FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION};
    use crate::core::quantity::{Area, Length, Temperature};
    use crate::fluid::{
        GeneratedFluidStoreSpec, validate_fluid_egress, validate_generate_fluid_store,
        validate_mount_fluid_store,
    };
    use crate::logistics::{add_traversable_edge_for_test, validate_place_fluid_store};
    use crate::spatial::VoxelBounds;
    use crate::structural::{
        add_structural_element, make_test_structural_geometry,
        materialize_structural_element_for_test, validate_activate_structural_element,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(1, 0, 0);
    let near = VoxelCoord::new(0, 0, 1);
    let via = VoxelCoord::new(0, 0, -1);
    let far = VoxelCoord::new(0, 0, -2);
    for (from, to) in [
        (den, junction),
        (junction, tip),
        (junction, near),
        (junction, via),
        (via, far),
        (near, VoxelCoord::new(0, 0, 2)),
    ] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    let place = |state: &mut AppState, position| {
        let store = validate_generate_fluid_store(
            &registries,
            state,
            GeneratedFluidStoreSpec::new(
                SCOUT_DRINK_VOLUME,
                FLUID_WATER,
                SCOUT_DRINK_VOLUME,
                Temperature::from_millikelvin(293_150),
            ),
        )
        .unwrap()
        .commit(state)
        .unwrap();
        validate_place_fluid_store(state, store, position)
            .unwrap()
            .commit(state)
            .unwrap();
        store
    };
    let observed = |state: &AppState| {
        reachable_water_from_junction(
            &registries,
            state,
            state.kobold_state().colony.as_ref().unwrap(),
            None,
        )
    };
    assert_eq!(observed(&state), None, "empty passages are not water");
    let far_store = place(&mut state, far);
    assert_eq!(observed(&state), Some((via, Some(far))));
    let near_store = place(&mut state, near);
    assert_eq!(observed(&state), Some((near, None)));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);
    assert_eq!(observed(&replay), Some((near, None)));

    // A nearby source with an unavailable structural-load revision cannot
    // satisfy a real drink. The scout must choose the still-reachable farther
    // water rather than repeatedly visiting the unusable nearer vessel.
    let support = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        make_test_structural_geometry(
            VoxelBounds::new(near, VoxelCoord::new(near.x() + 1, 1, near.z() + 1)).unwrap(),
            Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, support, FORM_LOG);
    let _ = validate_activate_structural_element(&registries, &state, support)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = validate_mount_fluid_store(&registries, &state, near_store, support)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX);
    let mut blocked = serde_json::from_value::<LoadedSaveEnvelope>(encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &blocked), Ok(()));
    assert!(validate_fluid_egress(&registries, &blocked, near_store, SCOUT_DRINK_VOLUME).is_err());
    assert_eq!(
        observed(&blocked),
        Some((via, Some(far))),
        "the water-search decision must consider actual withdrawal feasibility"
    );
    let water_before = calculate_fluid_volume_accounting(&blocked).unwrap();
    for _ in 0..1_550 {
        let _ = advance_tick(&registries, &mut blocked).unwrap();
        if blocked
            .kobold_state()
            .consumed_fluids
            .contains_key(&FLUID_WATER)
        {
            break;
        }
    }
    assert_eq!(
        blocked
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied(),
        Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME)),
        "the patrol must reach and actually drink from its farther viable goal"
    );
    assert_eq!(
        blocked
            .fluid()
            .get_store(near_store)
            .unwrap()
            .stored_volume(),
        SCOUT_DRINK_VOLUME
    );
    assert_eq!(
        blocked
            .fluid()
            .get_store(far_store)
            .unwrap()
            .stored_volume(),
        Volume::ZERO
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&blocked).unwrap(),
        water_before
    );
    assert_eq!(validate_loaded_state(&registries, &blocked), Ok(()));
}

#[test]
fn thirsty_scout_seeks_real_water_and_its_drink_survives_replay() {
    let (registries, mut state, source, position) = opening_water();
    let original = state.fluid().get_store(source).unwrap().stored_volume();
    let total = calculate_fluid_volume_accounting(&state).unwrap();
    let mut drank = false;
    for _ in 0..1_550 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        if state
            .kobold_state()
            .consumed_fluids
            .contains_key(&FLUID_WATER)
        {
            drank = true;
            break;
        }
    }
    assert!(
        drank,
        "the unwatched, thirsty resident must reach its existing opening water branch"
    );
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .tracks
            .iter()
            .any(|track| {
                track.position() == position
                    && track.observed_activity() == super::super::KoboldTrailKind::Drinking
            }),
        "a real drink must leave a distinguishable physical departure trace"
    );
    assert_eq!(
        state.fluid().get_store(source).unwrap().stored_volume(),
        original.checked_sub(SCOUT_DRINK_VOLUME).unwrap()
    );
    assert_eq!(
        state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied(),
        Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME))
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        state.logistics().fluid_store_position(source),
        Some(position)
    );
    let json = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut restored = serde_json::from_slice::<LoadedSaveEnvelope>(&json)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..120 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut restored).unwrap();
    }
    assert_eq!(state, restored);
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), total);
}

#[test]
fn no_water_is_minted_if_scout_cannot_reach_the_source() {
    let (registries, mut state, source, water) = opening_water();
    let start_total = calculate_fluid_volume_accounting(&state).unwrap();
    let original = state.fluid().get_store(source).unwrap().stored_volume();
    crate::player::validate_admit_player(&registries, &state, water)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..1_250 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.kobold_state().consumed_fluids.len(), 0);
    assert_eq!(
        state.fluid().get_store(source).unwrap().stored_volume(),
        original
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        start_total
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn a_drained_pocket_does_not_regenerate_water_or_satisfy_another_thirst() {
    use crate::core::quantity::Temperature;
    use crate::fluid::{GeneratedFluidStoreSpec, validate_generate_fluid_store};
    use crate::logistics::{add_traversable_edge_for_test, validate_place_fluid_store};

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(0, 0, 1);
    let water = VoxelCoord::new(1, 0, 0);
    for (a, b) in [(den, junction), (junction, tip), (junction, water)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    let generated = validate_generate_fluid_store(
        &registries,
        &state,
        GeneratedFluidStoreSpec::new(
            SCOUT_DRINK_VOLUME,
            FLUID_WATER,
            SCOUT_DRINK_VOLUME,
            Temperature::from_millikelvin(293_150),
        ),
    )
    .unwrap();
    let store = generated.commit(&mut state).unwrap();
    validate_place_fluid_store(&state, store, water)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let original = calculate_fluid_volume_accounting(&state).unwrap();
    for _ in 0..1_300 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.fluid().get_store(store).unwrap().stored_volume(),
        Volume::ZERO
    );
    assert_eq!(
        state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied(),
        Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME))
    );
    for _ in 0..1_000 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(
        state.kobold_state().colony.as_ref().unwrap().next_drink_at <= state.tick(),
        "a missed drink remains a genuine unresolved need"
    );
    assert_eq!(
        state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied(),
        Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME))
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), original);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn nearby_observer_hears_one_real_drink_without_guarding_the_source() {
    use crate::kobolds::{KoboldFootfallKind, available_nearby_kobold_footfalls};
    let (registries, mut state, source, water) = opening_water();
    let original = state.fluid().get_store(source).unwrap().stored_volume();
    let junction = state.kobold_state().colony.as_ref().unwrap().route[1];
    let approach = state
        .logistics()
        .traversable_destinations(water)
        .find(|position| state.logistics().has_traversal_edge(*position, junction))
        .unwrap();
    let observer = VoxelCoord::new(
        water.x() + 2 * (water.x() - approach.x()),
        water.y(),
        water.z() + 2 * (water.z() - approach.z()),
    );
    crate::player::validate_admit_player(&registries, &state, observer)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let mut heard = false;
    for _ in 0..1_500 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        if state
            .kobold_state()
            .consumed_fluids
            .contains_key(&FLUID_WATER)
        {
            heard = available_nearby_kobold_footfalls(&registries, &state)
                .iter()
                .any(|event| event.kind() == KoboldFootfallKind::Splashing);
            break;
        }
    }
    assert!(
        heard,
        "a real nearby water visit produces an audible splash cue"
    );
    assert_eq!(
        state.fluid().get_store(source).unwrap().stored_volume(),
        original.checked_sub(SCOUT_DRINK_VOLUME).unwrap()
    );
    for _ in 0..3 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(
        available_nearby_kobold_footfalls(&registries, &state)
            .iter()
            .all(|cue| cue.kind() != KoboldFootfallKind::Splashing),
        "splashing must fade rather than become a permanent noise source"
    );
}

#[test]
fn trusted_load_rejects_forged_thirst_deadlines_and_drink_ledger() {
    let (registries, mut state, _, _) = opening_water();
    let initial = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut forged = initial.clone();
    forged["state"]["systems"]["kobolds"]["colony"]["next_drink_at"] =
        serde_json::to_value(SimulationTick::new(985)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
    assert!(decoded.into_state(&registries).is_err());

    // A drinking track is evidence of an actual resident at a water source;
    // a fabricated label cannot establish a nonexistent consumption ledger.
    for _ in 0..185 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let mut false_tracks = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let trail = false_tracks["state"]["systems"]["kobolds"]["colony"]["tracks"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap();
    trail["kind"] = serde_json::json!("Drinking");
    let decoded: LoadedSaveEnvelope = serde_json::from_value(false_tracks).unwrap();
    assert!(decoded.into_state(&registries).is_err());

    for _ in 0..1_250 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let mut fake_volume = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let ledger = fake_volume["state"]["systems"]["kobolds"]["consumed_fluids"]
        .as_object_mut()
        .unwrap();
    assert_eq!(ledger.len(), 1, "resident must have consumed real water");
    *ledger.values_mut().next().unwrap() = serde_json::json!(61_000);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(fake_volume).unwrap();
    assert!(decoded.into_state(&registries).is_err());
}

/// A real generated clearing and a physical water source on an adjacent
/// traversable edge. The test does not synthesize climatic or fluid values.
fn generated_outdoor_source() -> (Registries, AppState, FluidStoreId, VoxelCoord) {
    use crate::content::BLOCK_STATE_STONE;
    use crate::core::quantity::Temperature;
    use crate::fluid::{GeneratedFluidStoreSpec, validate_generate_fluid_store};
    use crate::logistics::{add_traversable_edge_for_test, validate_place_fluid_store};
    use crate::world::{ChunkCoord, validate_generate_chunk};

    let registries = build_registries();
    for seed_number in 0..32 {
        let seed = WorldGenerationSeed::new(seed_number);
        let mut state = AppState::new();
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        for x in 3..11 {
            for z in 3..11 {
                for y in -4..=5 {
                    let den = VoxelCoord::new(x, y, z);
                    let junction = VoxelCoord::new(x + 1, y, z);
                    let tip = VoxelCoord::new(x + 2, y, z);
                    let water = VoxelCoord::new(x + 1, y, z + 1);
                    if ![den, junction, tip, water].iter().all(|&position| {
                        state.world().block_at(VoxelCoord::new(
                            position.x(),
                            position.y() - 1,
                            position.z(),
                        )) == Some(BLOCK_STATE_STONE)
                            && state
                                .world()
                                .has_standing_clearance(registries.world(), position)
                            && crate::world::exposed_air_at(
                                &registries,
                                &state,
                                position,
                                SimulationTick::new(1_000),
                            )
                            .is_some()
                    }) {
                        continue;
                    }
                    for (a, b) in [(den, junction), (junction, tip), (junction, water)] {
                        add_traversable_edge_for_test(&mut state, a, b);
                    }
                    state
                        .kobold_state_mut()
                        .apply_opening_colony([den, junction, tip]);
                    let generated = validate_generate_fluid_store(
                        &registries,
                        &state,
                        GeneratedFluidStoreSpec::new(
                            Volume::from_microliters(300_000),
                            FLUID_WATER,
                            Volume::from_microliters(300_000),
                            Temperature::from_millikelvin(293_150),
                        ),
                    )
                    .unwrap();
                    let store = generated.commit(&mut state).unwrap();
                    validate_place_fluid_store(&state, store, water)
                        .unwrap()
                        .commit(&mut state)
                        .unwrap();
                    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
                    return (registries, state, store, water);
                }
            }
        }
    }
    panic!("no generated clearing with physical scout access to water");
}

#[test]
fn local_weather_modulates_thirst_and_a_roof_removes_outdoor_exposure() {
    use crate::content::BLOCK_STATE_STONE;
    let (registries, mut state, _, water) = generated_outdoor_source();
    let calendar = registries.core().calendar();
    let season = u64::from(calendar.days_per_month())
        * u64::from(calendar.months_per_year() / 4)
        * calendar.ticks_per_day();
    let mut hot = None;
    let mut cold = None;
    let mut windy = None;
    for window in 0..30 {
        let summer = SimulationTick::new(season + calendar.ticks_per_day() / 4 + window * 200);
        let winter = SimulationTick::new(season * 3 + window * 200);
        let heat = crate::world::exposed_air_at(&registries, &state, water, summer).unwrap();
        if heat.ambient_temperature().millikelvin() >= 298_150 {
            hot = Some(summer);
        } else if heat.ambient_temperature().millikelvin() >= 293_150
            && heat.wind() == crate::world::WindCondition::Gusting
        {
            windy = Some(summer);
        }
        let cold_air = crate::world::exposed_air_at(&registries, &state, water, winter).unwrap();
        if cold_air.ambient_temperature().millikelvin() < 278_150 {
            cold = Some(winter);
        }
    }
    let hot = hot.expect("summer should have a hot weather front");
    let cold = cold.expect("winter should have a cold weather front");
    assert_eq!(
        drink_interval_at(&registries, &state, water, hot),
        SCOUT_DRINK_INTERVAL_TICKS / 2,
    );
    assert_eq!(
        drink_interval_at(&registries, &state, water, cold),
        SCOUT_DRINK_INTERVAL_TICKS * 3 / 2,
    );
    if let Some(windy) = windy {
        assert_eq!(
            drink_interval_at(&registries, &state, water, windy),
            SCOUT_DRINK_INTERVAL_TICKS * 3 / 4,
        );
    }
    let roof = VoxelCoord::new(water.x(), water.y() + 3, water.z());
    crate::world::set_generated_block_for_test(
        registries.world(),
        &mut state,
        roof,
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        drink_interval_at(&registries, &state, water, hot),
        SCOUT_DRINK_INTERVAL_TICKS,
    );
    assert_eq!(
        drink_interval_at(&registries, &state, water, cold),
        SCOUT_DRINK_INTERVAL_TICKS,
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn generated_world_scout_drinks_real_water_and_schedules_from_climate() {
    let (registries, mut state, store, water) = generated_outdoor_source();
    let baseline = calculate_fluid_volume_accounting(&state).unwrap();
    let mut drank_at = None;
    for _ in 0..1_350 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        if state
            .kobold_state()
            .consumed_fluids
            .contains_key(&FLUID_WATER)
        {
            drank_at = Some(state.tick());
            break;
        }
    }
    let at = drank_at.expect("the actual scout should seek and reach exposed water");
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().next_drink_at,
        at.checked_add_span(TickSpan::new(drink_interval_at(
            &registries,
            &state,
            water,
            at,
        )))
        .unwrap(),
    );
    assert_eq!(
        state.fluid().get_store(store).unwrap().stored_volume(),
        Volume::from_microliters(240_000),
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), baseline);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..150 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut replay).unwrap();
    }
    assert_eq!(state, replay);
}

#[test]
fn trusted_load_requires_a_real_last_drink_clock_and_allowed_climate_interval() {
    let (registries, mut state, _, _) = opening_water();
    let fresh = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut invented = fresh.clone();
    invented["state"]["systems"]["kobolds"]["colony"]["last_drink_at"] = serde_json::json!(180);
    invented["state"]["systems"]["kobolds"]["colony"]["last_drink_interval_ticks"] =
        serde_json::json!(800);
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(invented)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
    let mut missing = fresh;
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("last_drink_at");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());

    for _ in 0..1_400 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        if state
            .kobold_state()
            .consumed_fluids
            .contains_key(&FLUID_WATER)
        {
            break;
        }
    }
    let colony = state.kobold_state().colony.as_ref().unwrap();
    assert!(colony.last_drink_at.is_some());
    assert_eq!(
        colony.next_drink_at,
        colony
            .last_drink_at
            .unwrap()
            .checked_add_span(TickSpan::new(colony.last_drink_interval_ticks.unwrap()))
            .unwrap(),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let trusted = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut forged_due = trusted.clone();
    forged_due["state"]["systems"]["kobolds"]["colony"]["next_drink_at"] =
        serde_json::to_value(SimulationTick::new(colony.next_drink_at.value() + 5)).unwrap();
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(forged_due)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
    let mut forged_interval = trusted.clone();
    forged_interval["state"]["systems"]["kobolds"]["colony"]["last_drink_interval_ticks"] =
        serde_json::json!(700);
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(forged_interval)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
    let mut forged_future = trusted;
    forged_future["state"]["systems"]["kobolds"]["colony"]["last_drink_at"] =
        serde_json::to_value(SimulationTick::new(state.tick().value() + 5)).unwrap();
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(forged_future)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
}

#[test]
fn prolonged_water_denial_stops_nonessential_salvage_until_scout_drinks_again() {
    use crate::content::{FORM_LUMP, MATERIAL_STONE};
    use crate::core::quantity::{Mass, Temperature};
    use crate::fluid::{GeneratedFluidStoreSpec, validate_generate_fluid_store};
    use crate::logistics::{add_traversable_edge_for_test, validate_place_fluid_store};
    use crate::material::{CommodityKey, MaterialComposition};
    use crate::surface::{
        SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(0, 0, 1);
    let water = VoxelCoord::new(1, 0, 0);
    for (a, b) in [(den, junction), (junction, tip), (junction, water)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    let supply = |state: &mut AppState, microliters: u64| {
        let volume = Volume::from_microliters(microliters);
        let source = validate_generate_fluid_store(
            &registries,
            state,
            GeneratedFluidStoreSpec::new(
                volume,
                FLUID_WATER,
                volume,
                Temperature::from_millikelvin(293_150),
            ),
        )
        .unwrap()
        .commit(state)
        .unwrap();
        validate_place_fluid_store(state, source, water)
            .unwrap()
            .commit(state)
            .unwrap();
        source
    };
    let original = supply(&mut state, SCOUT_DRINK_VOLUME.microliters());
    for _ in 0..2_700 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.fluid().get_store(original).unwrap().stored_volume(),
        Volume::ZERO,
        "the first actual ration must have completely exhausted the source",
    );
    assert!(prolonged_water_deprivation(
        state.kobold_state().colony.as_ref().unwrap(),
        state.tick(),
    ));

    let cargo = validate_create_surface_resources(
        &registries,
        &state,
        [SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            tip,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(100_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()],
    )
    .unwrap();
    let cargo_id = cargo.first_resource_id();
    cargo.commit(&mut state).unwrap();
    for _ in 0..100 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.surface().get(cargo_id).unwrap().position(),
        tip,
        "a water-deprived resident must not start a new nonfood raid",
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let water_baseline = calculate_fluid_volume_accounting(&state).unwrap();
    let restored_water = supply(&mut state, 120_000);
    let with_supply = calculate_fluid_volume_accounting(&state).unwrap();
    assert_eq!(
        with_supply.total().microliters() - water_baseline.total().microliters(),
        120_000,
        "the recovered source is actual newly created fixture water",
    );
    let mut second_drink = false;
    let mut salvaged = false;
    for _ in 0..260 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        second_drink |= state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .is_some_and(|volume| volume.microliters() >= 120_000);
        if state.surface().get(cargo_id).unwrap().position() != tip {
            salvaged = true;
            break;
        }
    }
    assert!(
        second_drink,
        "recovery must require reaching an actual new water supply"
    );
    assert!(
        salvaged,
        "rehydrated resident should recover the previously ignored cargo"
    );
    assert!(!prolonged_water_deprivation(
        state.kobold_state().colony.as_ref().unwrap(),
        state.tick(),
    ));
    assert_eq!(
        state
            .fluid()
            .get_store(restored_water)
            .unwrap()
            .stored_volume(),
        Volume::from_microliters(60_000),
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        with_supply
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
