//! Reclaiming a full finite kobold den opens capacity for future scavenging.
use super::*;

use deep_hearth::surface::{available_adjacent_surface_resources, available_player_world_drops};

pub(super) fn exercise_kobold_den_cache(print: bool) {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WILDERNESS_WORLD_SEED;
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("den raid generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("den raid opening commit failed: {error}"));
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap_or_else(|error| panic!("den raid admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("den raid player commit failed: {error}"))
        .carried_stockpile();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let berry = adjacent_surface_resource(&state, CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD))
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "den raid scout route reconnaissance",
    );
    let fork = adjacent_surface_resource(&state, nuts).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "den raid inspect natural food",
    );
    let initial = state
        .available_surface_resources()
        .find(|source| source.commodity() == nuts)
        .expect("den raid relies on observed wild nuts")
        .remaining_mass();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "den raid withdraw from fork",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        spawn,
        "den raid leave source unattended",
    );
    // More than five successful six-minute forage intervals pass. The scout
    // remains finite even though the observed natural source still has food.
    advance_to(
        &registries,
        &mut state,
        SimulationTick::new(815),
        TickEventAllowance::default(),
        "den cache fills during unattended patrol",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "den raid approach from known route",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "den raid return to contested food",
    );
    let remaining_when_full = state
        .available_surface_resources()
        .find(|source| source.commodity() == nuts)
        .expect("full den cannot consume the entire source")
        .remaining_mass();
    let used_food = initial
        .checked_sub(remaining_when_full)
        .expect("unattended patch must not grow");

    // Only adjacent exposed world parcels reveal the den; the fixture must not
    // read the colony's remote route. Pick the goods up via the actual control owner.
    let den = available_adjacent_surface_resources(&state)
        .find(|item| item.commodity() == nuts && item.position() != fork)
        .expect("scout must leave foraged goods visible in an adjacent den")
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        den,
        "recover full den hoard",
    );
    for _ in 0..24 {
        if available_player_world_drops(&state).next().is_none() {
            break;
        }
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("den cache ordinary pickup failed: {error}"));
    }
    assert!(available_player_world_drops(&state).next().is_none());
    let hoard = state
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .stored_mass();
    assert_eq!(
        hoard,
        Mass::from_milligrams(500_000),
        "player recovers only the physically staged den parcels"
    );
    let eaten = calculate_matter_accounting(&state).unwrap().consumed();
    assert_eq!(
        u128::from(used_food.milligrams()),
        u128::from(hoard.milligrams()) + eaten.milligrams(),
        "foraged food is either recoverable in the den or conserved as consumed matter"
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );

    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "withdraw from emptied den",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "withdraw toward original spawn",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        spawn,
        "let scout forage after raid",
    );
    let withdrawn_at = state.tick().value();
    advance_to(
        &registries,
        &mut state,
        SimulationTick::new(withdrawn_at + 30),
        TickEventAllowance::default(),
        "den reopens after player removes its cache",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "inspect forage after den raid",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "verify new forage after cache cleared",
    );
    let after_reopening = state
        .available_surface_resources()
        .find(|source| source.commodity() == nuts)
        .expect("raided source should remain locally observable")
        .remaining_mass();
    let new_foraging = remaining_when_full
        .checked_sub(after_reopening)
        .expect("source cannot gain mass from a raided den");
    assert_eq!(new_foraging, Mass::from_milligrams(100_000));
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    if print {
        std::println!(
            "PLAYER JOURNEY seed=0x{:016X} episode=coverage/rested/scavenger-den phase=raid-full-den observed=[source:{}mg reduced-to:{}mg cache:{}mg] choice=[recover-cache:ordinary-pickup] consequence=[carried:{}mg source-further-depleted:{}mg] matter-conserved=true world=ordinary-generated-opening",
            seed.value(),
            initial.milligrams(),
            remaining_when_full.milligrams(),
            hoard.milligrams(),
            hoard.milligrams(),
            new_foraging.milligrams(),
        );
        std::println!(
            "KOBOLD CACHE EXPERIENCE seed=0x{:016X} episode=coverage/rested/scavenger-den source-before:{}mg source-full:{}mg hoard:{}mg reclaimed:{}mg eaten:{}mg reopened:{}mg finite-storage=true matter-conserved=true",
            seed.value(),
            initial.milligrams(),
            remaining_when_full.milligrams(),
            hoard.milligrams(),
            hoard.milligrams(),
            eaten.milligrams(),
            new_foraging.milligrams(),
        );
    }
}
