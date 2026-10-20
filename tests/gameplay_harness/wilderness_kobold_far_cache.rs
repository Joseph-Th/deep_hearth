//! A camp cache two physical traversal edges beyond the scout's junction.
use super::*;

use deep_hearth::kobolds::available_nearby_kobold_scouts;
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use deep_hearth::surface::{
    available_adjacent_surface_resources, available_player_world_drops,
    validate_drop_player_material,
};

pub(super) fn exercise_kobold_far_camp_cache(print: bool) {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WILDERNESS_WORLD_SEED;
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .expect("far-cache opening must generate")
        .commit(&mut state)
        .expect("far-cache opening must commit");
    let carried = validate_admit_player(&registries, &state, spawn)
        .expect("far-cache player admitted")
        .commit(&mut state)
        .expect("far-cache player joined")
        .carried_stockpile();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let berry = adjacent_surface_resource(&state, berries).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "scout camp cache survey first trail leg",
    );
    let nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let junction = adjacent_surface_resource(&state, nuts).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "scout camp cache gather finite ration",
    );
    let food = local_surface_resource(&state, nuts);
    let cache_mass = Mass::from_milligrams(250_000);
    gather(
        &registries,
        &mut state,
        food,
        cache_mass,
        "scout camp cache gather real nuts",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "scout camp cache return toward spawn",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        spawn,
        "scout camp cache put loose food at spawn",
    );
    let dropped = validate_drop_player_material(&registries, &state, nuts)
        .expect("scout camp cache drop must be available")
        .commit(&mut state)
        .expect("scout camp cache drop must commit");
    assert_eq!(dropped.position(), spawn);
    assert_eq!(dropped.dropped_mass(), cache_mass);
    let parcel = dropped.resources()[0];
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        Mass::ZERO
    );

    // A guarded branch keeps a living player on the intervening trail,
    // without standing on or automatically picking up the loose camp cache.
    let mut guarded = state.clone();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut guarded,
        berry,
        "stand guard between scout and camp ration",
    );
    advance_to(
        &registries,
        &mut guarded,
        SimulationTick::new(205),
        TickEventAllowance::default(),
        "guard the camp approach during scout visits",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut guarded,
        spawn,
        "verify guarded camp cache is still recoverable",
    );
    assert_eq!(
        guarded
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        cache_mass
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));

    // Unattended branch: two ordinary off-trail body moves put the player
    // outside the scout's physical 1-voxel deterrence envelope at spawn.
    let step_x = spawn.x() - berry.x();
    let step_z = spawn.z() - berry.z();
    let outside = VoxelCoord::new(spawn.x() + step_x, spawn.y(), spawn.z() + step_z);
    let farther = VoxelCoord::new(outside.x() + step_x, spawn.y(), outside.z() + step_z);
    walk_body_to_adjacent(
        &registries,
        &mut state,
        outside,
        "leave loose camp cache unattended",
    );
    walk_body_to_adjacent(
        &registries,
        &mut state,
        farther,
        "withdraw beyond scout camp deterrence range",
    );
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut state)
            .expect("off-trail body settlement must work");
    }
    advance_to(
        &registries,
        &mut state,
        SimulationTick::new(184),
        TickEventAllowance::default(),
        "far scout reaches patrol junction",
    );
    let first =
        advance_tick(&registries, &mut state).expect("scout advances toward intermediary branch");
    assert!(first.kobold_scout().is_none(), "scout first step is remote");
    assert!(
        available_nearby_kobold_scouts(&registries, &state)
            .next()
            .is_none()
    );

    // The actor can reverse course after the scout has already taken its
    // first detour leg. This is a lived pickup, not a test-only parcel removal
    // or a remote query of the colony's planned destination.
    let mut intercepted = state.clone();
    walk_body_to_adjacent(
        &registries,
        &mut intercepted,
        outside,
        "intercept far scout before it reaches camp",
    );
    walk_body_to_adjacent(
        &registries,
        &mut intercepted,
        spawn,
        "collect exposed camp food while scout is en route",
    );
    assert_eq!(
        intercepted
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        cache_mass,
        "the player should recover the actual parcel before the arriving scout",
    );
    walk_body_to_adjacent(
        &registries,
        &mut intercepted,
        outside,
        "leave intercepted camp before scout next step",
    );
    walk_body_to_adjacent(
        &registries,
        &mut intercepted,
        farther,
        "let scout discover the now-empty camp",
    );
    while !intercepted.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut intercepted)
            .expect("interception return must settle");
    }
    assert!(
        intercepted.tick().value() < 190,
        "the player needs to recover the cache before the scout's next step",
    );
    advance_to(
        &registries,
        &mut intercepted,
        SimulationTick::new(205),
        TickEventAllowance::default(),
        "scout abandons empty camp detour",
    );
    assert_eq!(
        intercepted
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        cache_mass,
    );
    walk_body_to_adjacent(
        &registries,
        &mut intercepted,
        outside,
        "inspect intercepted camp after scout return",
    );
    walk_body_to_adjacent(
        &registries,
        &mut intercepted,
        spawn,
        "confirm reclaimed cache remains in carried custody",
    );
    while !intercepted.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut intercepted)
            .expect("interception verification must settle");
    }
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut intercepted,
        berry,
        "follow aborted raid back to junction",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut intercepted,
        junction,
        "verify no intercepted parcel was brought to den",
    );
    assert!(
        available_adjacent_surface_resources(&intercepted)
            .all(|record| record.resource() != parcel),
        "the intercepted parcel cannot also materialize at the den",
    );
    assert_eq!(
        calculate_matter_accounting(&intercepted).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &intercepted), Ok(()));

    advance_to(
        &registries,
        &mut state,
        SimulationTick::new(189),
        TickEventAllowance::default(),
        "scout approaches unattended camp ration",
    );
    let visible = advance_tick(&registries, &mut state)
        .expect("scout reaches camp via second authored edge")
        .kobold_scout()
        .expect("scout at camp should enter local sighting range");
    assert_eq!(visible.position(), spawn);
    assert!(visible.discovered_player_drop());
    assert_eq!(visible.relocated_drop(), None);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        Mass::ZERO
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .expect("far detour must encode");
    let loaded: LoadedSaveEnvelope =
        serde_json::from_slice(&encoded).expect("far detour must decode");
    let mut restored = loaded
        .into_state(&registries)
        .expect("far detour must survive trusted save admission");
    for _ in 0..15 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut restored).unwrap(),
        );
    }
    assert_eq!(state, restored);
    assert_eq!(state.tick().value(), 205);
    assert!(
        available_nearby_kobold_scouts(&registries, &state)
            .next()
            .is_none()
    );

    walk_body_to_adjacent(
        &registries,
        &mut state,
        outside,
        "follow stolen camp supplies toward trail",
    );
    walk_body_to_adjacent(&registries, &mut state, spawn, "inspect emptied camp cache");
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut state)
            .expect("camp return body settlement must work");
    }
    assert!(available_player_world_drops(&state).next().is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        Mass::ZERO
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "track scavenger back down trail",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "inspect den from local junction",
    );
    let den = available_adjacent_surface_resources(&state)
        .find(|resource| resource.resource() == parcel)
        .expect("same displaced parcel must be observable at neighboring den")
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        den,
        "recover original camp cache at den",
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        cache_mass
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    if print {
        std::println!(
            "PLAYER JOURNEY seed=0x{:016X} episode=coverage/rested/scavenger-camp-cache phase=two-edge-camp-theft choice=[guard-trail-or-leave-cache-or-intercept-in-time] observation=[far-scout-visible-at-camp:true remote-retreat-hidden:true] consequence=[unattended-cache-relocated:250000mg guarded-cache-safe:250000mg timely-pickup-intercepts:250000mg] recovery=[same-parcel-at-den:250000mg] matter-conserved=true",
            seed.value(),
        );
        std::println!(
            "KOBOLD FAR CACHE EXPERIENCE seed=0x{:016X} episode=coverage/rested/scavenger-camp-cache cache:250000mg stolen:250000mg recovered:250000mg guarded:250000mg intercepted:250000mg two-edges:true save-replay:true local-only:true matter-conserved:true",
            seed.value(),
        );
    }
}
