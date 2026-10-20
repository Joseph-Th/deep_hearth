//! One ordinary wilderness actor faces a real scavenger instead of a timed flavor sighting.
use super::*;

use deep_hearth::kobolds::{
    KoboldTrailKind, available_nearby_kobold_scouts, available_nearby_kobold_tracks,
};
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};
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
    // At one-voxel distance the living player can frighten this opportunistic scout.
    // Wait one further leg away to give the unsecured alternative a real chance to steal.
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        spawn,
        "scout observation from safe distance",
    );
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
        available_nearby_kobold_scouts(&registries, &state)
            .next()
            .is_some_and(|sighting| sighting.has_noticed_player_drop())
    );

    // Three genuine player choices begin from one saved state: ignore the scout,
    // recover the ration immediately, or approach just closely enough to scare it off.
    let mut exposed = state.clone();
    let mut warded = state.clone();
    let mut defended = state;
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut defended,
        forage,
        "scout defensive return",
    );
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
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut warded,
        forage,
        "scout nonviolent deterrence",
    );
    assert_eq!(
        warded
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        Mass::ZERO,
        "warding must defend exposed supplies without fetching them"
    );
    advance_to(
        &registries,
        &mut warded,
        SimulationTick::new(184),
        TickEventAllowance::default(),
        "scout warded wait",
    );
    advance_to(
        &registries,
        &mut defended,
        SimulationTick::new(184),
        TickEventAllowance::default(),
        "scout defended wait",
    );
    advance_to(
        &registries,
        &mut exposed,
        SimulationTick::new(184),
        TickEventAllowance::default(),
        "scout unsecured wait",
    );
    assert!(
        advance_tick(&registries, &mut exposed)
            .unwrap_or_else(|error| panic!("scout exposed retreat failed: {error}"))
            .kobold_scout()
            .is_none(),
        "the retreat beyond local observation must not reveal the scout's secret position"
    );
    let defended_step = advance_tick(&registries, &mut defended)
        .unwrap_or_else(|error| panic!("scout defended continuation failed: {error}"))
        .kobold_scout()
        .unwrap_or_else(|| panic!("defended scout movement not visible"));
    let warded_step = advance_tick(&registries, &mut warded)
        .unwrap_or_else(|error| panic!("scout warded continuation failed: {error}"))
        .kobold_scout()
        .unwrap_or_else(|| panic!("warded scout retreat not visible"));
    assert_eq!(defended_step.relocated_drop(), None);
    assert!(defended_step.fled_player());
    assert_eq!(warded_step.relocated_drop(), None);
    assert!(warded_step.fled_player());
    assert_eq!(warded_step.abandoned_drop(), None);
    assert_eq!(warded_step.position(), defended_step.position());
    let retreat_trace = available_nearby_kobold_tracks(&registries, &warded)
        .find(|track| track.position() == junction && track.toward() == warded_step.position())
        .expect("player near the fork should see the scout's real retreat track");
    assert_eq!(retreat_trace.observed_activity(), KoboldTrailKind::Fleeing);
    assert_eq!(retreat_trace.left_at(), SimulationTick::new(185));
    assert!(
        available_nearby_kobold_scouts(&registries, &warded)
            .next()
            .is_some_and(|sighting| sighting.scavenged_material().is_none() && sighting.is_wary())
    );
    assert_eq!(
        available_adjacent_surface_resources(&warded)
            .find(|item| item.resource() == drop)
            .expect("warded player should still see the original exposed drop")
            .position(),
        junction
    );
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
    assert_eq!(
        calculate_matter_accounting(&warded).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &exposed), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &defended), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &warded), Ok(()));

    // One frightened scout, two follow-up decisions. The player does not
    // inspect the remote den: each branch returns through ordinary body
    // movement to establish whether its exact parcel survived.
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &warded))
        .expect("frightened scout should save");
    let envelope: LoadedSaveEnvelope =
        serde_json::from_slice(&encoded).expect("frightened scout should decode");
    let mut early_return = envelope
        .into_state(&registries)
        .expect("scout fear must persist after trusted save load");
    assert_eq!(early_return, warded);
    assert_eq!(
        available_nearby_kobold_tracks(&registries, &early_return).collect::<Vec<_>>(),
        available_nearby_kobold_tracks(&registries, &warded).collect::<Vec<_>>(),
        "trusted load must preserve player-local physical retreat evidence"
    );
    let mut late_return = early_return.clone();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut early_return,
        spawn,
        "leave scout alone during caution",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut late_return,
        spawn,
        "leave scout alone past caution",
    );
    advance_to(
        &registries,
        &mut early_return,
        SimulationTick::new(215),
        TickEventAllowance::default(),
        "caution protects neglected ration",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut early_return,
        forage,
        "check still-safe cache before expiration",
    );
    assert_eq!(
        available_adjacent_surface_resources(&early_return)
            .find(|item| item.resource() == drop)
            .expect("cautious scout must not take the unguarded ration yet")
            .position(),
        junction
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut early_return,
        junction,
        "recover cache after temporary deterrence",
    );
    assert_eq!(
        early_return
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        ration
    );
    assert!(early_return.tick().value() < 225);

    advance_to(
        &registries,
        &mut late_return,
        SimulationTick::new(235),
        TickEventAllowance::default(),
        "caution expires before unattended camp recovery",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut late_return,
        forage,
        "inspect ration after caution window",
    );
    assert!(
        available_adjacent_surface_resources(&late_return).all(|item| item.resource() != drop),
        "old forage junction parcel must have been taken once caution expires"
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut late_return,
        junction,
        "follow scout to den after renewed theft",
    );
    let late_den = available_adjacent_surface_resources(&late_return)
        .find(|item| item.resource() == drop)
        .expect("the old ration should become locally visible at the den")
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut late_return,
        late_den,
        "reclaim ration after caution ends",
    );
    assert_eq!(
        late_return
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        ration
    );
    assert_eq!(
        calculate_matter_accounting(&early_return).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&late_return).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &early_return), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &late_return), Ok(()));

    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut warded,
        junction,
        "recover ration after scaring away scout",
    );
    assert_eq!(
        warded
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        ration,
        "deterring a scout must preserve ordinary recovery of the original parcel"
    );

    // The unattended branch receives no omniscient scout result. Return through the
    // world and discover the new location from a local sighting and adjacent matter.
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut exposed,
        forage,
        "scout aftermath reconnaissance",
    );
    let den = available_nearby_kobold_scouts(&registries, &exposed)
        .next()
        .expect("retreating scout must become visible again on approach")
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut exposed,
        junction,
        "scout loot recovery approach",
    );
    assert_eq!(
        available_adjacent_surface_resources(&exposed)
            .find(|item| item.resource() == drop)
            .expect("the same parcel should be visible near the scout")
            .position(),
        den
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
            "PLAYER JOURNEY seed=0x{:016X} episode=coverage/rested/scavenger-patrol phase=exposed-ration observed=[drop:{}mg left-at-junction:{}t scout-noticed-at:180] choice=[fetch:return-and-pickup deter:approach-without-pickup neglect:wait] consequence=[fetch:retained-{}mg scout-flees deter:scout-flees-parcel-stays neglect:scout-diverts-home-with-{}mg] recovery=[follow-scout-to-den:reclaimed same-parcel:true] matter-conserved=true mode=matched-generated-world",
            seed.value(),
            ration.milligrams(),
            dropped_at.value(),
            ration.milligrams(),
            ration.milligrams(),
        );
        std::println!(
            "KOBOLD EXPERIENCE seed=0x{:016X} episode=coverage/rested/scavenger-patrol evidence=ordinary-generated-world detection=180t theft=185t unsecured-moved:{}mg guarded-retained:{}mg recovered:{}mg deterrence=presence-routes-retreat drop-protected:{}mg cautious-retained:{}mg stolen-after-caution:{}mg wary-window:40t route-changed:true matter-conserved:true",
            seed.value(),
            ration.milligrams(),
            ration.milligrams(),
            ration.milligrams(),
            ration.milligrams(),
            ration.milligrams(),
            ration.milligrams(),
        );
    }
}
