//! Empty-hand direct-use contracts: drinking, gathering, context choices and capacity.

use super::*;

#[test]
fn empty_hand_secondary_use_drinks_from_one_finite_local_source() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let _carried = admit_player(&registries, &mut state, position);
    let source_volume = Volume::from_microliters(1_000_000);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        source_volume,
        FLUID_WATER,
        source_volume,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("direct-drink water fixture failed: {error}"));
    validate_place_fluid_store(&state, water, position)
        .unwrap_or_else(|error| panic!("direct-drink water placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("direct-drink water placement commit failed: {error}"));
    assert_eq!(state.logistics().active_selection(), None);

    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(positive_x_view()));
    let queued = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("direct-drink queue control failed: {error}"));
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    assert!(state.logistics().pending_secondary_use().is_some());
    assert_eq!(
        crate::logistics::validate_loaded_logistics(&registries, &state),
        Ok(())
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("queued direct-drink serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("queued direct-drink decode failed: {error}"));
    state = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("queued direct-drink load failed: {error}"));

    let started = loop {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("direct-drink neutral continuation failed: {error}"));
        match control.secondary_action() {
            Some(SecondaryActionOutcome::DrinkingStarted(outcome)) => break *outcome,
            None => {}
            other => panic!("direct-drink returned unexpected outcome: {other:?}"),
        }
    };
    assert!(!started.volume().is_zero());
    assert_eq!(started.store(), water);
    assert_eq!(
        state
            .fluid()
            .get_store(water)
            .and_then(|store| store.contents())
            .map(|contents| contents.volume()),
        source_volume.checked_sub(started.volume())
    );
    assert!(matches!(
        state.player_work().active(),
        Some(crate::labor::PlayerWork::Drinking { work }) if work.volume() == started.volume()
    ));
}

#[test]
fn empty_hand_secondary_use_refuses_ambiguous_local_drink_sources() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let _carried = admit_player(&registries, &mut state, position);
    let source_volume = Volume::from_microliters(1_000_000);
    for _ in 0..2 {
        let store = add_fluid_store_with_contents_for_fixture(
            &registries,
            &mut state,
            source_volume,
            FLUID_WATER,
            source_volume,
            TEMPERATURE,
        )
        .unwrap_or_else(|error| panic!("ambiguous direct-drink water fixture failed: {error}"));
        validate_place_fluid_store(&state, store, position)
            .unwrap_or_else(|error| {
                panic!("ambiguous direct-drink water placement failed: {error}")
            })
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("ambiguous direct-drink water placement commit failed: {error}")
            });
    }
    let before = state.clone();
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(positive_x_view()));

    let control = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("ambiguous direct-drink control failed: {error}"));

    assert_eq!(
        control.secondary_action(),
        Some(&SecondaryActionOutcome::AmbiguousLocalDrinkSources { count: 2 })
    );
    assert_eq!(state.fluid(), before.fluid());
    assert_eq!(state.player_work().active(), None);
}

#[test]
fn empty_hand_secondary_use_gathers_one_local_natural_opportunity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let source_mass = Mass::from_milligrams(6_000_000);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        source_mass,
    );
    let carried = admit_player(&registries, &mut state, position);
    let direct_batch = registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_STONE)
        .map(|method| method.direct_batch_mass())
        .unwrap_or_else(|| panic!("direct-gather stone method disappeared"));
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(positive_x_view()));

    let queued = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("direct-gather queue control failed: {error}"));
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    let work = loop {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("direct-gather neutral continuation failed: {error}"));
        match control.secondary_action() {
            Some(SecondaryActionOutcome::SurfaceGatheringStarted(work)) => break *work,
            None => {}
            other => panic!("direct-gather returned unexpected outcome: {other:?}"),
        }
    };

    assert_eq!(work.resource(), stone);
    assert_eq!(work.method(), SURFACE_GATHERING_HAND_COLLECT_STONE);
    assert_eq!(work.destination(), carried);
    assert_eq!(work.gathered_mass(), direct_batch);
    assert_eq!(
        state
            .surface()
            .get(stone)
            .map(|resource| resource.remaining_mass()),
        Some(source_mass),
        "starting direct gathering must not remove surface matter before completion"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(direct_batch)
    );
}

#[test]
fn empty_hand_secondary_use_clamps_surface_gathering_to_carried_capacity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(5_000_000),
    );
    let carried = admit_player(&registries, &mut state, position);
    let remaining_capacity = Mass::from_milligrams(250_000);
    let capacity = state
        .inventory()
        .get_stockpile(carried)
        .map(|stockpile| stockpile.capacity())
        .unwrap_or_else(|| panic!("capacity-clamped direct-gather carried stockpile disappeared"));
    let filler = capacity.checked_sub(remaining_capacity).unwrap_or_else(|| {
        panic!("direct-gather fixture capacity is smaller than desired remainder")
    });
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        filler,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("capacity-clamped direct-gather filler failed: {error}"));
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(positive_x_view()));

    let queued = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("capacity-clamped direct-gather queue failed: {error}"));
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    let work = loop {
        let control = advance_control_tick(&registries, &mut state).unwrap_or_else(|error| {
            panic!("capacity-clamped direct-gather neutral continuation failed: {error}")
        });
        match control.secondary_action() {
            Some(SecondaryActionOutcome::SurfaceGatheringStarted(work)) => break *work,
            None => {}
            other => {
                panic!("capacity-clamped direct-gather returned unexpected outcome: {other:?}")
            }
        }
    };

    assert_eq!(work.resource(), stone);
    assert_eq!(work.gathered_mass(), remaining_capacity);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.available_capacity()),
        Some(Mass::ZERO),
        "direct gathering should reserve exactly the remaining carrying envelope"
    );
}

#[test]
fn empty_hand_secondary_use_refuses_water_and_surface_context_ambiguity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let stone_mass = Mass::from_milligrams(1_000_000);
    let stone = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        stone_mass,
    );
    let _carried = admit_player(&registries, &mut state, position);
    let water_volume = Volume::from_microliters(1_000_000);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        water_volume,
        FLUID_WATER,
        water_volume,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("ambiguous-context water fixture failed: {error}"));
    validate_place_fluid_store(&state, water, position)
        .unwrap_or_else(|error| panic!("ambiguous-context water placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("ambiguous-context water placement commit failed: {error}"));
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(positive_x_view()));

    let control = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("ambiguous-context direct use failed: {error}"));

    assert_eq!(
        control.secondary_action(),
        Some(&SecondaryActionOutcome::AmbiguousEmptyHandContext {
            drink_sources: 1,
            surface_resources: 1,
        })
    );
    assert_eq!(
        state
            .fluid()
            .get_store(water)
            .map(|store| store.stored_volume()),
        Some(water_volume)
    );
    assert_eq!(
        state
            .surface()
            .get(stone)
            .map(|resource| resource.remaining_mass()),
        Some(stone_mass)
    );
    assert_eq!(state.player_work().active(), None);

    // A second gathering opportunity must not erase the water choice from the ambiguity.
    let timber_mass = Mass::from_milligrams(2_000_000);
    let timber = seed_surface(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        timber_mass,
    );
    let before = state.clone();
    let control = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("multi-resource drinking choice failed: {error}"));
    assert_eq!(
        control.secondary_action(),
        Some(&SecondaryActionOutcome::AmbiguousEmptyHandContext {
            drink_sources: 1,
            surface_resources: 2,
        })
    );
    assert_eq!(state.fluid(), before.fluid());
    assert_eq!(state.surface(), before.surface());
    assert_eq!(
        state
            .surface()
            .get(timber)
            .map(|resource| resource.remaining_mass()),
        Some(timber_mass)
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn empty_hand_secondary_use_reports_all_co_located_gathering_choices_without_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let _carried = admit_player(&registries, &mut state, position);
    for _ in 0..128 {
        seed_surface(
            &registries,
            &mut state,
            position,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(100_000),
        );
    }
    let before = state.clone();
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(positive_x_view()));
    let outcome = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("many-source contextual use failed: {error}"));
    assert_eq!(
        outcome.secondary_action(),
        Some(&SecondaryActionOutcome::AmbiguousLocalSurfaceResources { count: 128 })
    );
    assert_eq!(state.surface(), before.surface());
    assert_eq!(state.inventory(), before.inventory());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
