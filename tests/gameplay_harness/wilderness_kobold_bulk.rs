//! A real survivor learns that bulk loose piles are not secure storage.
use super::*;

use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use deep_hearth::surface::{available_adjacent_surface_resources, validate_drop_player_material};

pub(super) fn exercise_kobold_bulk_scavenging(print: bool) {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WILDERNESS_WORLD_SEED;
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("bulk scavenging generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("bulk world commit failed: {error}"));
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap_or_else(|error| panic!("bulk scavenging admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("bulk player commit failed: {error}"))
        .carried_stockpile();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let berry = adjacent_surface_resource(&state, berries).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "bulk scout inspect trail",
    );
    let nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let junction = adjacent_surface_resource(&state, nuts).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "bulk scout inspect material junction",
    );
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let stone_opportunity = adjacent_surface_resource(&state, stone);
    let stone_leaf = stone_opportunity.position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        stone_leaf,
        "bulk scout gather real stone",
    );
    let portion = Mass::from_milligrams(2_000_000);
    let stone_source = local_surface_resource(&state, stone);
    gather(
        &registries,
        &mut state,
        stone_source,
        portion,
        "bulk scout accumulate exposed materials",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "bulk scout return with stone",
    );
    let dropped = validate_drop_player_material(&registries, &state, stone)
        .unwrap_or_else(|error| panic!("bulk loose stone drop rejected: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("bulk loose stone drop failed: {error}"));
    assert_eq!(dropped.position(), junction);
    assert_eq!(dropped.dropped_mass(), portion);
    assert_eq!(dropped.resources().len(), 1);
    let original = dropped.resources()[0];
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        berry,
        "bulk scout leave large pile",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        spawn,
        "bulk scout leave pile unattended",
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        Mass::ZERO
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );

    // The same known world offers two ordinary responses: return and pick up the
    // whole pile before the scout, or trust exposed bulk storage.
    let mut defended = state.clone();
    let mut exposed = state;
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut defended,
        berry,
        "bulk scout protect cache approach",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut defended,
        junction,
        "bulk scout protect cache by pickup",
    );
    assert_eq!(
        defended
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        portion
    );
    advance_to(
        &registries,
        &mut defended,
        SimulationTick::new(195),
        TickEventAllowance::default(),
        "guarded bulk loose cache",
    );
    assert_eq!(
        defended
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        portion
    );

    advance_to(
        &registries,
        &mut exposed,
        SimulationTick::new(179),
        TickEventAllowance::default(),
        "bulk pile left exposed near scout route",
    );
    let discovery = advance_tick(&registries, &mut exposed)
        .unwrap()
        .kobold_scout()
        .expect("the approaching scout must locally notice a loose bulk pile");
    assert_eq!(discovery.position(), junction);
    assert!(discovery.discovered_player_drop());
    assert_eq!(discovery.split_drop(), None);

    advance_to(
        &registries,
        &mut exposed,
        SimulationTick::new(185),
        TickEventAllowance::default(),
        "bulky pile first finite split",
    );
    // The player is still at spawn; exact remote pile mass is not projected.
    // Save and replay the actual unattended branch before inspecting it locally.
    let envelope = serde_json::to_vec(&SaveEnvelope::new(&registries, &exposed)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&envelope).unwrap();
    let mut restored = loaded
        .into_state(&registries)
        .expect("split ordinary materials must survive save/load");
    for _ in 0..10 {
        assert_eq!(
            advance_tick(&registries, &mut exposed).unwrap(),
            advance_tick(&registries, &mut restored).unwrap()
        );
    }
    assert_eq!(exposed, restored);
    assert_eq!(exposed.tick().value(), 195);
    assert_eq!(
        exposed
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        Mass::ZERO,
    );
    assert_eq!(
        calculate_matter_accounting(&exposed).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &exposed), Ok(()));

    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut exposed,
        berry,
        "bulk scout inspect disturbed cache",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut exposed,
        junction,
        "bulk scout reclaim original remnant",
    );
    let remaining = exposed
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .stored_mass();
    assert_eq!(remaining, Mass::from_milligrams(1_500_000));
    assert!(
        !exposed
            .available_surface_resources()
            .any(|resource| resource.id() == original),
        "the player must recover the old bulk parcel, not a replacement",
    );
    let den = available_adjacent_surface_resources(&exposed)
        .find(|record| {
            record.commodity() == stone && record.resource() != stone_opportunity.resource()
        })
        .expect("split stone must be visible at a neighboring den")
        .position();
    assert_ne!(den, stone_leaf);
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut exposed,
        den,
        "bulk scout recover split den parcels",
    );
    assert_eq!(
        exposed
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        portion
    );
    assert_eq!(
        calculate_matter_accounting(&exposed).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&defended).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &exposed), Ok(()));

    if print {
        std::println!(
            "PLAYER JOURNEY seed=0x{:016X} episode=coverage/rested/scavenger-bulk phase=exposed-bulk-cache choice=[secure-2kg:pickup expose-2kg:loose-drop] observation=[scout-detects-bulk:true] consequence=[split-from-bulk:500000mg remnant:1500000mg] recovery=[remnant-and-den:2000000mg] matter-conserved=true world=ordinary-generated-opening",
            seed.value(),
        );
        std::println!(
            "KOBOLD BULK EXPERIENCE seed=0x{:016X} episode=coverage/rested/scavenger-bulk original:2000000mg split:500000mg remaining:1500000mg recovered:2000000mg guarded:2000000mg finite-den=true matter-conserved=true",
            seed.value(),
        );
    }
}
