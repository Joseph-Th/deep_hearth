//! Scarce junction nuts make adjacent wild berries a new physical scout target.
use super::*;

use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use deep_hearth::surface::available_adjacent_surface_resources;

pub(super) fn exercise_kobold_side_food_forage(print: bool) {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WILDERNESS_WORLD_SEED;
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .expect("side forage opening must generate")
        .commit(&mut state)
        .expect("side forage generation commit");
    let carried = validate_admit_player(&registries, &state, spawn)
        .expect("side forage player admission")
        .commit(&mut state)
        .expect("side forage player commit")
        .carried_stockpile();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let berry = adjacent_surface_resource(&state, berries).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "discover second natural food source",
    );
    let berry_before = state
        .available_surface_resources()
        .find(|resource| resource.commodity() == berries)
        .expect("locally observed berries")
        .remaining_mass();
    let junction = adjacent_surface_resource(&state, nuts).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "discover junction nuts",
    );
    let nuts_source = local_surface_resource(&state, nuts);
    let nuts_mass = state
        .available_surface_resources()
        .find(|resource| resource.commodity() == nuts)
        .expect("observed junction nuts")
        .remaining_mass();
    gather_in_batches(
        &registries,
        &mut state,
        nuts_source,
        nuts_mass,
        "secure all junction nuts before scout arrives",
    );
    assert!(
        !state
            .available_surface_resources()
            .any(|resource| resource.commodity() == nuts),
        "the actor actually exhausted junction wild food",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "carry junction food toward camp",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        spawn,
        "compare exposed neighboring berry patch",
    );
    assert!(state.tick().value() < 180);
    let mut guarded = state.clone();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut guarded,
        berry,
        "guard the neighboring wild berries",
    );
    advance_to(
        &registries,
        &mut guarded,
        SimulationTick::new(195),
        TickEventAllowance::default(),
        "guard neighboring berries while nuts depleted",
    );
    let guarded_berries = guarded
        .available_surface_resources()
        .find(|resource| resource.commodity() == berries)
        .expect("guarded berries remain locally available")
        .remaining_mass();
    assert_eq!(guarded_berries, berry_before);

    let step_x = spawn.x() - berry.x();
    let step_z = spawn.z() - berry.z();
    let outside = VoxelCoord::new(spawn.x() + step_x, spawn.y(), spawn.z() + step_z);
    let farther = VoxelCoord::new(outside.x() + step_x, spawn.y(), outside.z() + step_z);
    walk_body_to_adjacent(
        &registries,
        &mut state,
        outside,
        "leave second food source alone",
    );
    walk_body_to_adjacent(
        &registries,
        &mut state,
        farther,
        "avoid deterring side-branch scavenger",
    );
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut state)
            .expect("side forage withdrawal settlement");
    }
    assert!(state.tick().value() < 180);
    advance_to(
        &registries,
        &mut state,
        SimulationTick::new(189),
        TickEventAllowance::default(),
        "scout reaches unguarded berry source",
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .expect("side forage outbound save");
    let loaded: LoadedSaveEnvelope =
        serde_json::from_slice(&saved).expect("side forage outbound decode");
    let mut restored = loaded
        .into_state(&registries)
        .expect("side forage detour must pass trusted load");
    for _ in 0..6 {
        assert_eq!(
            advance_tick(&registries, &mut state).expect("side forage tick"),
            advance_tick(&registries, &mut restored).expect("side forage replay tick"),
        );
    }
    assert_eq!(state, restored);
    assert_eq!(state.tick().value(), 195);
    walk_body_to_adjacent(
        &registries,
        &mut state,
        outside,
        "return to camp after berry contest",
    );
    walk_body_to_adjacent(
        &registries,
        &mut state,
        spawn,
        "follow original forage trail",
    );
    while !state.control_tick().is_simulation_boundary() {
        let _ =
            advance_control_tick(&registries, &mut state).expect("side forage return settlement");
    }
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "inspect contested berries locally",
    );
    let berry_after = state
        .available_surface_resources()
        .find(|resource| resource.commodity() == berries)
        .expect("berry opportunity must remain finite")
        .remaining_mass();
    let lost = berry_before
        .checked_sub(berry_after)
        .expect("unattended berries must not increase");
    assert_eq!(lost, Mass::from_milligrams(100_000));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        nuts_mass,
        "foraging did not silently add a remote ration to the player",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "follow side forage toward scout den",
    );
    let den = available_adjacent_surface_resources(&state)
        .find(|resource| resource.commodity() == berries && resource.position() != berry)
        .expect("side forage must become a recoverable den drop")
        .position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        den,
        "recover finite berry catch at den",
    );
    for _ in 0..24 {
        if state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass()
            == nuts_mass.checked_add(lost).unwrap()
        {
            break;
        }
        let _ = advance_control_tick(&registries, &mut state)
            .expect("berry den pickup must execute through ordinary control time");
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        nuts_mass.checked_add(lost).unwrap(),
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));

    if print {
        std::println!(
            "PLAYER JOURNEY seed=0x{:016X} episode=coverage/rested/scavenger-side-food phase=alternate-wild-food choice=[secure-junction-nuts guard-berries-or-withdraw] observation=[berries-before:{}mg guarded:{}mg unattended:{}mg] consequence=[side-branch-catch:{}mg] recovery=[den-berries:{}mg] matter-conserved=true",
            seed.value(),
            berry_before.milligrams(),
            guarded_berries.milligrams(),
            berry_after.milligrams(),
            lost.milligrams(),
            lost.milligrams(),
        );
        std::println!(
            "KOBOLD SIDE FOOD EXPERIENCE seed=0x{:016X} episode=coverage/rested/scavenger-side-food source-before:{}mg guarded:{}mg source-after:{}mg foraged:{}mg recovered:{}mg depleted-junction:true physical-return:true save-replay:true matter-conserved:true",
            seed.value(),
            berry_before.milligrams(),
            guarded_berries.milligrams(),
            berry_after.milligrams(),
            lost.milligrams(),
            lost.milligrams(),
        );
    }
}
