//! One uninterrupted generated opening where enemy foraging competes with player forage.
use super::*;

use deep_hearth::kobolds::available_nearby_kobold_scouts;
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};

pub(super) fn exercise_kobold_natural_foraging(print: bool) {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WILDERNESS_WORLD_SEED;
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("natural forage world generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("natural forage opening commit failed: {error}"));
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap_or_else(|error| panic!("forage encounter player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("forage encounter player commit failed: {error}"))
        .carried_stockpile();
    let conserved = calculate_matter_accounting(&state).unwrap().total();

    // A real player learns the source by walking to it, not by inspecting hidden
    // opening roles or reading the scout route from persistence.
    let berry = adjacent_surface_resource(&state, CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD))
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "natural forage reconnaissance",
    );
    let nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let fork = adjacent_surface_resource(&state, nuts).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        fork,
        "natural forage inspection",
    );
    let initial = state
        .available_surface_resources()
        .find(|resource| resource.commodity() == nuts)
        .expect("actor must reach actual natural wild nuts")
        .remaining_mass();
    // An alternative physical defense uses the same player-built, gathered-stone
    // and finite-timber campfire as the survival continuation. Light is not
    // spawned by the fixture and remains dependent on stored combustion energy.
    let mut fire_guarded = state.clone();
    let fire =
        super::mobile_continuation::build_scout_warding_fire(&registries, &mut fire_guarded, fork);
    let fire_lit_at = fire_guarded.tick().value();
    assert!(
        fire_lit_at < 180,
        "defensive camp must be available before the scout arrives"
    );
    let initial_brightness = assess_local_illumination(&registries, &fire_guarded, fork).level();
    assert!(initial_brightness >= 3);
    // A player can save at the lit camp and later load without resetting the fire
    // or the enemy's schedule. Continue this same genuine gameplay branch from
    // the trusted persistence envelope rather than silently ignoring save state.
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &fire_guarded))
        .expect("lit field camp should encode");
    let envelope: LoadedSaveEnvelope =
        serde_json::from_slice(&save).expect("lit field camp should decode");
    let loaded = envelope
        .into_state(&registries)
        .expect("lit field camp must survive trusted load admission");
    assert_eq!(fire_guarded, loaded);
    fire_guarded = loaded;
    assert_eq!(
        assess_local_illumination(&registries, &fire_guarded, fork).level(),
        initial_brightness
    );
    let remaining_fuel = fire_guarded.energy().get_store(fire).unwrap().stored();
    let until_dark = passive_dissipation_ticks_until_empty(
        &registries,
        ENERGY_STONE_RING_CAMPFIRE,
        remaining_fuel,
    )
    .unwrap()
    .expect("campfire must consume its finite heat charge")
    .value();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut fire_guarded,
        berry,
        "leave defensive campfire and forage intact",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut fire_guarded,
        spawn,
        "watch campfire protect unattended food from afar",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "natural forage observation withdrawal",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        spawn,
        "natural forage leave unattended",
    );

    // Both policies branch from the same witnessed world, with no created loot and
    // no hidden scout coordinates: the player either guards the known source or
    // continues other work, then returns to inspect the change.
    let mut unattended = state.clone();
    let mut guarded = state;
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut guarded,
        berry,
        "natural forage defensive approach",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut guarded,
        fork,
        "natural forage defend in person",
    );
    advance_to(
        &registries,
        &mut guarded,
        SimulationTick::new(195),
        TickEventAllowance::default(),
        "guard finite natural food",
    );
    advance_to(
        &registries,
        &mut unattended,
        SimulationTick::new(195),
        TickEventAllowance::default(),
        "leave finite natural food exposed",
    );
    advance_to(
        &registries,
        &mut fire_guarded,
        SimulationTick::new(195),
        TickEventAllowance::default(),
        "lit food source left unguarded",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut fire_guarded,
        berry,
        "inspect protected firelit source approach",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut fire_guarded,
        fork,
        "inspect protected firelit source locally",
    );
    let fire_protected = fire_guarded
        .available_surface_resources()
        .find(|record| record.commodity() == nuts)
        .expect("source cannot vanish in the defended camp")
        .remaining_mass();
    assert_eq!(
        fire_protected, initial,
        "maintained firelight must prevent foraging"
    );
    assert!(assess_local_illumination(&registries, &fire_guarded, fork).level() >= 3);
    assert_eq!(
        fire_guarded
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        Mass::ZERO,
        "fire protection requires no direct player pickup"
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut fire_guarded,
        berry,
        "leave protected light active",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut fire_guarded,
        spawn,
        "remain distant while fire burns",
    );
    let protected = guarded
        .available_surface_resources()
        .find(|resource| resource.commodity() == nuts)
        .expect("guarded source should remain")
        .remaining_mass();
    assert_eq!(protected, initial);

    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut unattended,
        berry,
        "investigate missing forage",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut unattended,
        fork,
        "inspect contested forage source",
    );
    let remaining = unattended
        .available_surface_resources()
        .find(|resource| resource.commodity() == nuts)
        .expect("foraged source should be depleted but not gone")
        .remaining_mass();
    let lost = initial
        .checked_sub(remaining)
        .expect("natural forage cannot increase while unguarded");
    assert_eq!(lost, Mass::from_milligrams(100_000));
    let parcel = available_adjacent_surface_resources(&unattended)
        .find(|resource| resource.commodity() == nuts)
        .expect("stolen food must be physically present on an adjacent den");
    assert_ne!(parcel.position(), fork);
    let den = parcel.position();
    let sighting = available_nearby_kobold_scouts(&unattended)
        .next()
        .expect("scout becomes visible during local recovery");
    assert_eq!(sighting.position(), den);
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut unattended,
        den,
        "reclaim wild nuts from den",
    );
    assert_eq!(
        unattended
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        lost,
        "the extracted natural ration must remain ordinary recoverable world matter"
    );
    assert_eq!(
        calculate_matter_accounting(&unattended).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &unattended), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));
    assert_eq!(
        calculate_matter_accounting(&fire_guarded).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &fire_guarded), Ok(()));

    // Fire protection is finite. Once the last authored heat dissipates, the
    // same unattended resource should again be accessible to an opportunistic scout.
    let burnout = SimulationTick::new(fire_lit_at + until_dark + 1);
    advance_to(
        &registries,
        &mut fire_guarded,
        burnout,
        TickEventAllowance::default(),
        "defensive campfire burns to embers",
    );
    assert_eq!(
        assess_local_illumination(&registries, &fire_guarded, fork).level(),
        0
    );
    // Protection ends as brightness drops below the scout's threshold,
    // not artificially at the last joule of heat. The later inspection is a
    // genuine reminder to replenish the camp, not a free permanent ward.
    advance_to(
        &registries,
        &mut fire_guarded,
        SimulationTick::new(burnout.value() + 20),
        TickEventAllowance::default(),
        "scout resumes foraging when firelight is gone",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut fire_guarded,
        berry,
        "investigate dimmed defensive camp",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut fire_guarded,
        fork,
        "inspect food after firelight fades",
    );
    let after_dark_patrol = fire_guarded
        .available_surface_resources()
        .find(|record| record.commodity() == nuts)
        .unwrap()
        .remaining_mass();
    let after_dark_loss = initial.checked_sub(after_dark_patrol).unwrap();
    assert!(
        after_dark_loss >= Mass::from_milligrams(100_000),
        "once the defensive light dims scouts must resume finite foraging"
    );
    assert_eq!(
        calculate_matter_accounting(&fire_guarded).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &fire_guarded), Ok(()));

    if print {
        std::println!(
            "PLAYER JOURNEY seed=0x{:016X} episode=coverage/rested/scavenger-forage phase=finite-fire-ward choice=[gather-stone-and-timber:build-fire] observation=[initial-light:{} fuel-runway:{}t] consequence=[source-at-195t:{}mg dark-after:{}t source-after-dark:{}mg] tradeoff=spend-finite-fuel-to-protect-food matter-conserved=true",
            seed.value(),
            initial_brightness,
            until_dark,
            fire_protected.milligrams(),
            burnout.value(),
            after_dark_patrol.milligrams(),
        );
        std::println!(
            "PLAYER JOURNEY seed=0x{:016X} episode=coverage/rested/scavenger-forage phase=wild-food-contest observation=[visited-source:{}mg] choice=[guard-source:stay-near neglect:leave] consequence=[guarded:{}mg unattended:{}mg] recovery=[follow-local-scout:{}mg] world=ordinary-generated-opening matter-conserved=true",
            seed.value(),
            initial.milligrams(),
            protected.milligrams(),
            remaining.milligrams(),
            lost.milligrams(),
        );
        std::println!(
            "KOBOLD FORAGE EXPERIENCE seed=0x{:016X} episode=coverage/rested/scavenger-forage source-before:{}mg source-after:{}mg protected:{}mg contested:{}mg recovered:{}mg fire-protected:{}mg resumed-after-extinction:{}mg no-player-drop=true matter-conserved=true",
            seed.value(),
            initial.milligrams(),
            remaining.milligrams(),
            protected.milligrams(),
            lost.milligrams(),
            lost.milligrams(),
            fire_protected.milligrams(),
            after_dark_loss.milligrams(),
        );
    }
}
