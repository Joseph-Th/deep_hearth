//! One uninterrupted generated opening where enemy foraging competes with player forage.
use super::*;

use deep_hearth::kobolds::available_nearby_kobold_scouts;

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

    if print {
        std::println!(
            "PLAYER JOURNEY seed=0x{:016X} episode=coverage/rested/scavenger-forage phase=wild-food-contest observation=[visited-source:{}mg] choice=[guard-source:stay-near neglect:leave] consequence=[guarded:{}mg unattended:{}mg] recovery=[follow-local-scout:{}mg] world=ordinary-generated-opening matter-conserved=true",
            seed.value(),
            initial.milligrams(),
            protected.milligrams(),
            remaining.milligrams(),
            lost.milligrams(),
        );
        std::println!(
            "KOBOLD FORAGE EXPERIENCE seed=0x{:016X} episode=coverage/rested/scavenger-forage source-before:{}mg source-after:{}mg protected:{}mg contested:{}mg recovered:{}mg no-player-drop=true matter-conserved=true",
            seed.value(),
            initial.milligrams(),
            remaining.milligrams(),
            protected.milligrams(),
            lost.milligrams(),
            lost.milligrams(),
        );
    }
}
