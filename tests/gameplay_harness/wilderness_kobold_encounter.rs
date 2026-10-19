//! One ordinary wilderness actor faces a real scavenger instead of a timed flavor sighting.
use super::*;

use deep_hearth::kobolds::available_nearby_kobold_scouts;
use deep_hearth::surface::{available_player_world_drops, validate_drop_player_material};

pub(super) fn exercise_kobold_scavenging_encounter(print: bool) {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WILDERNESS_WORLD_SEED;
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("enemy wilderness generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("enemy wilderness generation commit failed: {error}"));
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap_or_else(|error| panic!("enemy player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("enemy player admission commit failed: {error}"))
        .carried_stockpile();
    let conserved = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("enemy initial matter audit failed: {error}"))
        .total();

    // Choose a real, finite ration from the local foraging economy; do not materialize loot
    // for the encounter or read the hidden den/route from the kobold owner.
    let berries = adjacent_surface_resource(&state, CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD));
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berries.position(),
        "scout forage approach",
    );
    let forage = berries.position();
    let nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let junction = adjacent_surface_resource(&state, nuts).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "scout cache approach",
    );
    let source = local_surface_resource(&state, nuts);
    let ration = Mass::from_milligrams(500_000);
    gather(
        &registries,
        &mut state,
        source,
        ration,
        "scout finite provision",
    );
    let dropped = validate_drop_player_material(&registries, &state, nuts)
        .unwrap_or_else(|error| panic!("scout physical ration drop rejected: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("scout physical ration drop commit failed: {error}"));
    assert_eq!(dropped.dropped_mass(), ration);
    assert_eq!(dropped.resources().len(), 1);
    let drop = dropped.resources()[0];
    let dropped_at = state.tick();
    assert_eq!(dropped.position(), junction);
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        forage,
        "scout observation withdrawal",
    );
    assert!(available_player_world_drops(&state).next().is_none());
    advance_to(
        &registries,
        &mut state,
        SimulationTick::new(179),
        TickEventAllowance::default(),
        "scout exposure interval",
    );
    let discovery = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("scout first sighting failed: {error}"))
        .kobold_scout()
        .unwrap_or_else(|| panic!("scout did not reach the player's visible junction"));
    assert_eq!(discovery.position(), junction);
    assert!(discovery.discovered_player_drop());
    assert_eq!(discovery.relocated_drop(), None);
    assert!(
        available_nearby_kobold_scouts(&state)
            .next()
            .is_some_and(|sighting| sighting.has_noticed_player_drop())
    );

    // The same pre-choice world diverges only because one actor returns for the ration.
    let mut exposed = state.clone();
    let mut defended = state;
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut defended,
        junction,
        "scout contested recovery",
    );
    assert_eq!(
        defended
            .inventory()
            .get_stockpile(carried)
            .map(|pile| pile.stored_mass()),
        Some(ration),
        "the player should retrieve the exposed ration through normal control-time pickup",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut defended,
        forage,
        "scout recovery withdrawal",
    );
    advance_to(
        &registries,
        &mut defended,
        SimulationTick::new(184),
        TickEventAllowance::default(),
        "scout defended wait",
    );
    let unguarded_step = {
        advance_to(
            &registries,
            &mut exposed,
            SimulationTick::new(184),
            TickEventAllowance::default(),
            "scout unsecured wait",
        );
        advance_tick(&registries, &mut exposed)
            .unwrap_or_else(|error| panic!("scout exposed retreat failed: {error}"))
            .kobold_scout()
            .unwrap_or_else(|| panic!("scout retreat was not player-visible"))
    };
    let defended_step = advance_tick(&registries, &mut defended)
        .unwrap_or_else(|error| panic!("scout defended continuation failed: {error}"))
        .kobold_scout()
        .unwrap_or_else(|| panic!("defended scout movement not visible"));
    assert_eq!(unguarded_step.relocated_drop(), Some(drop));
    assert_eq!(defended_step.relocated_drop(), None);
    assert_ne!(unguarded_step.position(), defended_step.position());
    assert_eq!(
        defended
            .inventory()
            .get_stockpile(carried)
            .map(|pile| pile.stored_mass()),
        Some(ration),
    );
    assert_eq!(
        calculate_matter_accounting(&exposed).unwrap().total(),
        conserved,
    );
    assert_eq!(
        calculate_matter_accounting(&defended).unwrap().total(),
        conserved,
    );
    assert_eq!(validate_loaded_state(&registries, &exposed), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &defended), Ok(()));

    // Scavenged matter is not despawned or transferred to inaccessible enemy inventory.
    // Follow the visible retreat destination and recover the same physical parcel.
    let den = unguarded_step.position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut exposed,
        junction,
        "scout loot recovery approach",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut exposed,
        den,
        "scout loot recovery at den",
    );
    assert_eq!(
        exposed
            .inventory()
            .get_stockpile(carried)
            .map(|pile| pile.stored_mass()),
        Some(ration),
        "following the scout should allow ordinary pickup of the same stolen ration",
    );
    assert!(available_player_world_drops(&exposed).next().is_none());
    assert_eq!(
        calculate_matter_accounting(&exposed).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &exposed), Ok(()));

    if print {
        std::println!(
            "PLAYER JOURNEY seed=0x{:016X} episode=coverage/rested/scavenger-patrol phase=exposed-ration observed=[drop:{}mg left-at-junction:{}t scout-noticed-at:180] choice=[defend:walk-back-and-pickup leave:wait] consequence=[defend:retained-{}mg scout-continues-patrol leave:scout-diverts-home-with-{}mg] recovery=[follow-scout-to-den:reclaimed same-parcel:true] matter-conserved=true mode=matched-generated-world",
            seed.value(),
            ration.milligrams(),
            dropped_at.value(),
            ration.milligrams(),
            ration.milligrams(),
        );
        std::println!(
            "KOBOLD EXPERIENCE seed=0x{:016X} episode=coverage/rested/scavenger-patrol evidence=ordinary-generated-world detection=180t theft=185t unsecured-moved:{}mg guarded-retained:{}mg recovered:{}mg route-changed:true matter-conserved:true",
            seed.value(),
            ration.milligrams(),
            ration.milligrams(),
            ration.milligrams(),
        );
    }
}
