//! An unattended food ration is more attractive than an earlier stone decoy.
use super::*;

use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use deep_hearth::surface::{
    available_adjacent_surface_resources, available_player_world_drops,
    validate_drop_player_material,
};

pub(super) fn exercise_kobold_food_preference(print: bool) {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WILDERNESS_WORLD_SEED;
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .expect("priority world must generate")
        .commit(&mut state)
        .expect("priority world commit");
    let carried = validate_admit_player(&registries, &state, spawn)
        .expect("priority player admission")
        .commit(&mut state)
        .expect("priority player commit")
        .carried_stockpile();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let berry_key = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let stone_key = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let nuts_key = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let berry = adjacent_surface_resource(&state, berry_key).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "priority gather wild fruit first",
    );
    let berry_source = local_surface_resource(&state, berry_key);
    let portion = Mass::from_milligrams(250_000);
    gather(
        &registries,
        &mut state,
        berry_source,
        portion,
        "priority gather edible material",
    );
    let junction = adjacent_surface_resource(&state, nuts_key).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "priority find work junction",
    );
    let stone = adjacent_surface_resource(&state, stone_key).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        stone,
        "priority gather inert material",
    );
    let stone_source = local_surface_resource(&state, stone_key);
    gather(
        &registries,
        &mut state,
        stone_source,
        portion,
        "priority gather stone decoy",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "priority leave both parcels at the junction",
    );
    let stone_drop = validate_drop_player_material(&registries, &state, stone_key)
        .expect("ordinary stone drop must be available")
        .commit(&mut state)
        .expect("ordinary stone drop must commit");
    assert_eq!(stone_drop.dropped_mass(), portion);
    let stone_id = stone_drop.resources()[0];
    let fruit_drop = validate_drop_player_material(&registries, &state, berry_key)
        .expect("ordinary berry drop must be available")
        .commit(&mut state)
        .expect("ordinary berry drop must commit");
    assert_eq!(fruit_drop.dropped_mass(), portion);
    let food_id = fruit_drop.resources()[0];
    assert!(stone_id < food_id);
    assert_eq!(stone_drop.position(), junction);
    assert_eq!(fruit_drop.position(), junction);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "priority withdraw to decide guarding",
    );
    let mut guarded = state.clone();
    let mut unattended = state;
    advance_to(
        &registries,
        &mut guarded,
        SimulationTick::new(185),
        TickEventAllowance::default(),
        "guard both dropped materials from a nearby trail",
    );
    assert!(
        available_adjacent_surface_resources(&guarded)
            .any(|record| record.resource() == food_id && record.position() == junction),
        "a living nearby guard should preserve the food at the junction"
    );
    assert!(
        available_adjacent_surface_resources(&guarded)
            .any(|record| record.resource() == stone_id && record.position() == junction),
        "a guard should preserve the stone at the junction"
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut unattended,
        spawn,
        "leave both drops unguarded",
    );
    advance_to(
        &registries,
        &mut unattended,
        SimulationTick::new(185),
        TickEventAllowance::default(),
        "watch scout select its first prize remotely",
    );
    // At spawn the actor cannot inspect the remote junction or den;
    // preserve and replay the world before approaching the deposits locally.
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &unattended))
        .expect("priority world save");
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&saved).expect("priority world decode");
    let mut resumed = loaded
        .into_state(&registries)
        .expect("priority world trusted load");
    for _ in 0..2 {
        assert_eq!(
            advance_tick(&registries, &mut unattended).unwrap(),
            advance_tick(&registries, &mut resumed).unwrap(),
        );
    }
    assert_eq!(unattended, resumed);
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut unattended,
        berry,
        "inspect surviving stone from the trail",
    );
    assert!(
        available_adjacent_surface_resources(&unattended)
            .any(|resource| resource.resource() == stone_id && resource.position() == junction),
        "the older inert parcel must remain at the junction"
    );
    assert!(
        available_adjacent_surface_resources(&unattended)
            .all(|resource| resource.resource() != food_id),
        "the later food parcel has already been moved to the den"
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut unattended,
        junction,
        "reclaim stone before next scout trip",
    );
    let returned_stone = unattended
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .stored_mass();
    assert_eq!(returned_stone, portion);
    let den = available_adjacent_surface_resources(&unattended)
        .find(|resource| resource.resource() == food_id)
        .expect("the displaced food parcel must be recoverable at adjacent den")
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut unattended,
        den,
        "recover first-choice food from scout den",
    );
    assert_eq!(
        unattended
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        portion.checked_add(portion).unwrap(),
    );
    assert!(available_player_world_drops(&unattended).next().is_none());
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
            "PLAYER JOURNEY seed=0x{:016X} episode=coverage/rested/scavenger-food-choice phase=two-parcel-choice choice=[drop-stone-then-food guard-or-withdraw] observation=[stone-first-id:true food-first-theft:true] consequence=[food-moved:250000mg stone-unmoved:250000mg guarded:500000mg] recovery=[two-parcels:500000mg] matter-conserved=true",
            seed.value(),
        );
        std::println!(
            "KOBOLD FOOD PRIORITY EXPERIENCE seed=0x{:016X} episode=coverage/rested/scavenger-food-choice stone-before-food:true food-stolen:250000mg stone-left:250000mg guarded:500000mg recovered:500000mg food-preferred:true save-replay:true matter-conserved:true",
            seed.value(),
        );
    }
}
