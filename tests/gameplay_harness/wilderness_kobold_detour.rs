//! A lone scout physically changes its route to investigate a side-branch parcel.
use super::*;

use deep_hearth::kobolds::available_nearby_kobold_scouts;
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use deep_hearth::surface::{available_player_world_drops, validate_drop_player_material};

pub(super) fn exercise_kobold_side_branch_scavenging(print: bool) {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WILDERNESS_WORLD_SEED;
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("side cache generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("side cache world failed: {error}"));
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap_or_else(|error| panic!("side cache admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("side cache player failed: {error}"))
        .carried_stockpile();
    let conserved = calculate_matter_accounting(&state).unwrap().total();

    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let berry = adjacent_surface_resource(&state, berries).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "side cache discover berries",
    );
    let nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let junction = adjacent_surface_resource(&state, nuts).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "side cache inspect fork",
    );
    let source = local_surface_resource(&state, nuts);
    gather(
        &registries,
        &mut state,
        source,
        Mass::from_milligrams(250_000),
        "side cache gather finite ration",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "place cache off the patrol route",
    );
    let drop = validate_drop_player_material(&registries, &state, nuts)
        .unwrap_or_else(|error| panic!("side cache ordinary drop rejected: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("side cache ordinary drop failed: {error}"));
    assert_eq!(drop.position(), berry);
    assert_eq!(drop.dropped_mass(), Mass::from_milligrams(250_000));
    assert_eq!(drop.resources().len(), 1);
    let parcel = drop.resources()[0];
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        spawn,
        "withdraw from exposed side cache",
    );
    // A deliberate ordinary body-control step off the trail, away from the
    // witnessed cache. This is not a test relocation or enemy-state edit.
    let safe = VoxelCoord::new(
        spawn.x() + spawn.x() - berry.x(),
        spawn.y(),
        spawn.z() + spawn.z() - berry.z(),
    );
    walk_body_to_adjacent(
        &registries,
        &mut state,
        safe,
        "withdraw far enough to avoid startling the scout",
    );
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("side cache settling off trail failed: {error}"));
    }
    advance_to(
        &registries,
        &mut state,
        SimulationTick::new(179),
        TickEventAllowance::default(),
        "side cache scout approaches fork",
    );
    let first = advance_tick(&registries, &mut state).unwrap();
    assert!(
        first.kobold_scout().is_none(),
        "side cache outside the local scout view at the fork"
    );
    advance_to(
        &registries,
        &mut state,
        SimulationTick::new(184),
        TickEventAllowance::default(),
        "side cache scout considers alternate route",
    );
    let detour = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("side cache detour step failed: {error}"))
        .kobold_scout()
        .expect("the scout becomes visible while examining the player's side cache");
    assert_eq!(detour.position(), berry);
    assert!(detour.discovered_player_drop());
    assert_eq!(detour.relocated_drop(), None);
    assert!(available_nearby_kobold_scouts(&state).next().is_some());

    let save =
        serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).expect("detour should save");
    let envelope: LoadedSaveEnvelope =
        serde_json::from_slice(&save).expect("detour should deserialize");
    let mut resumed = envelope
        .into_state(&registries)
        .expect("detour route should be trusted-load valid");
    for _ in 0..10 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut resumed).unwrap(),
        );
    }
    assert_eq!(state, resumed);
    assert_eq!(state.tick().value(), 195);
    assert!(available_nearby_kobold_scouts(&state).next().is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        Mass::ZERO,
    );

    walk_body_to_adjacent(&registries, &mut state, spawn, "return to known trail");
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("side cache return settle failed: {error}"));
    }
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "side cache aftermath",
    );
    assert!(
        available_player_world_drops(&state).next().is_none(),
        "the parcel should not remain where it was left"
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "follow scout to patrol fork",
    );
    let den = available_adjacent_surface_resources(&state)
        .find(|item| item.resource() == parcel)
        .expect("displaced parcel must become locally discoverable beside the fork")
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        den,
        "reclaim same side-branch parcel at den",
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        Mass::from_milligrams(250_000),
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    if print {
        std::println!(
            "PLAYER JOURNEY seed=0x{:016X} episode=coverage/rested/scavenger-detour phase=side-branch-cache choice=[drop-ration-on-branch withdraw-off-trail] observation=[scout-diverts-local:true remote-return-hidden:true] consequence=[cache-moved-to-den:250000mg] recovery=[same-parcel:250000mg] matter-conserved=true world=ordinary-generated-opening",
            seed.value(),
        );
        std::println!(
            "KOBOLD DETOUR EXPERIENCE seed=0x{:016X} episode=coverage/rested/scavenger-detour cache-exposed:250000mg detoured:true carried:250000mg recovered:250000mg remote-omniscience:false matter-conserved:true",
            seed.value(),
        );
    }
}
