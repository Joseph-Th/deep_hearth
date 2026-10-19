//! A cold-start path to a real material-backed settlement casting bed, without seeded stock.

use super::*;
use deep_hearth::content::{
    EQUIPMENT_CLAY_FACED_STONE_CASTING_BED, FORM_PACKED_CLAY_BINDER, MATERIAL_CLAY,
    PROCESS_PACK_CLAY_BINDER,
};
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};

pub(super) fn exercise_generated_clay_casting_bed(report: bool) {
    let registries = build_registries();
    let seed = WILDERNESS_WORLD_SEED;
    let mut state = AppState::new();
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("clay project generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("clay project generation commit failed: {error}"));
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap_or_else(|error| panic!("clay project admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("clay project admission commit failed: {error}"))
        .carried_stockpile();
    if let Some(demand) = validate_generate_player_neighborhood(&registries, &state)
        .unwrap_or_else(|error| panic!("clay project neighborhood demand failed: {error}"))
    {
        demand
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("clay project neighborhood commit failed: {error}"));
    }
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("clay project initial matter audit failed: {error}"))
        .total();
    let initial_reserves = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("clay project initial survival assessment missing"));
    let began_at = state.tick();

    let clay = CommodityKey::new(MATERIAL_CLAY, FORM_LUMP);
    let binder = CommodityKey::new(MATERIAL_CLAY, FORM_PACKED_CLAY_BINDER);
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let boards = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_CLAY_FACED_STONE_CASTING_BED)
        .unwrap_or_else(|| panic!("clay-faced casting bed definition missing"));
    let assembly = definition
        .assembly_profile()
        .unwrap_or_else(|| panic!("clay-faced casting bed lost material assembly"));
    let required = |commodity: CommodityKey| {
        assembly
            .inputs()
            .iter()
            .find(|spec| spec.commodity() == commodity)
            .unwrap_or_else(|| panic!("casting bed no longer requires commodity {commodity:?}"))
            .mass()
    };
    let binder_mass = required(binder);
    let stone_mass = required(stone);
    let boards_mass = required(boards);
    let binder_recipe = registries
        .crafting()
        .get_manual(PROCESS_PACK_CLAY_BINDER)
        .unwrap_or_else(|| panic!("clay packing recipe disappeared"));
    assert_eq!(
        binder_recipe.input_mass(),
        binder_recipe.outputs()[0].mass()
    );
    let binder_batches = binder_mass.milligrams() / binder_recipe.outputs()[0].mass().milligrams();
    assert_eq!(
        binder_mass.milligrams() % binder_recipe.outputs()[0].mass().milligrams(),
        0
    );
    let boards_recipe = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("bare-hand board recipe disappeared"));
    let board_yield = boards_recipe
        .outputs()
        .iter()
        .find(|output| output.commodity() == boards)
        .unwrap_or_else(|| panic!("bare-hand board shaping lost board output"))
        .mass()
        .milligrams();
    let board_batches = boards_mass.milligrams().div_ceil(board_yield);
    let timber_mass =
        Mass::from_milligrams(boards_recipe.input_mass().milligrams() * board_batches);

    // Clay is physically separate from the water voxel, so direct empty-hand drinking stays
    // unambiguous. The actor learns the clay spur only after reaching the water branch.
    let forage = adjacent_surface_resource(&state, CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD));
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        forage.position(),
        "clay project forage approach",
    );
    let water = adjacent_drinkable_source(&registries, &state);
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        water.position(),
        "clay project waterside approach",
    );
    assert_eq!(
        available_local_drink_sources(&registries, &state).count(),
        1
    );
    assert_eq!(state.available_surface_resources().count(), 0);
    let outcrop = adjacent_surface_resource(&state, clay);
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        outcrop.position(),
        "clay project outcrop approach",
    );
    let clay_before = state
        .available_surface_resources()
        .find(|resource| resource.id() == outcrop.resource())
        .unwrap_or_else(|| panic!("generated outcrop missing"))
        .remaining_mass();
    assert!(clay_before >= binder_mass);
    gather_in_batches(
        &registries,
        &mut state,
        outcrop.resource(),
        binder_mass,
        "casting-bed clay",
    );
    assert_eq!(
        state
            .available_surface_resources()
            .find(|resource| resource.id() == outcrop.resource())
            .map(|resource| resource.remaining_mass()),
        clay_before.checked_sub(binder_mass)
    );

    // Return to the fork and follow observed material choices, not hidden generator coordinates.
    let water_position = water.position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        water_position,
        "clay project clay return",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        forage.position(),
        "clay project fork return",
    );
    let junction = state
        .available_adjacent_traversal_destinations()
        .find(|position| *position != spawn && *position != water_position)
        .unwrap_or_else(|| panic!("clay project work junction not discoverable"));
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "clay project work junction",
    );
    let material = available_adjacent_surface_resources(&state).collect::<Vec<_>>();
    assert_eq!(
        material
            .iter()
            .filter(|resource| resource.commodity() == stone || resource.commodity() == timber)
            .count(),
        2
    );
    for (commodity, amount) in [(stone, stone_mass), (timber, timber_mass)] {
        let source = material
            .iter()
            .find(|resource| resource.commodity() == commodity)
            .unwrap_or_else(|| panic!("casting-bed raw source not actor-visible"));
        walk_continuous_body_adjacent_and_settle(
            &registries,
            &mut state,
            source.position(),
            "clay project material approach",
        );
        gather_in_batches(
            &registries,
            &mut state,
            source.resource(),
            amount,
            "casting-bed raw haul",
        );
        walk_continuous_body_adjacent_and_settle(
            &registries,
            &mut state,
            junction,
            "clay project material return",
        );
    }
    craft_in_place(
        &registries,
        &mut state,
        clay,
        binder,
        binder_batches,
        "casting-bed binder",
    );
    craft_in_place(
        &registries,
        &mut state,
        timber,
        boards,
        board_batches,
        "casting-bed boards",
    );
    let option = player_equipment_assembly_options(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("casting-bed assembly catalog failed: {error}"))
        .into_iter()
        .find(|option| option.definition() == EQUIPMENT_CLAY_FACED_STONE_CASTING_BED)
        .unwrap_or_else(|| panic!("casting-bed assembly option unavailable"));
    assert!(
        option.is_ready(),
        "local gathered matter must make casting bed assembly ready"
    );
    let built = validate_assemble_player_equipment_option(&registries, &state, &option)
        .unwrap_or_else(|error| panic!("casting-bed assembly rejected: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("casting-bed assembly commit failed: {error}"));
    assert_eq!(
        state
            .equipment()
            .get_equipment(built)
            .map(|equipment| equipment.definition()),
        Some(EQUIPMENT_CLAY_FACED_STONE_CASTING_BED)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("clay project final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("clay project final state invalid: {error}"));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("clay project save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved)
        .unwrap_or_else(|error| panic!("clay project decode failed: {error}"));
    assert_eq!(
        decoded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("clay project trusted load failed: {error}")),
        state,
        "ordinary clay gathering, finished components, and assembled bed must persist exactly"
    );
    if report {
        let final_reserves = assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("clay project final survival assessment missing"));
        let elapsed = state.tick().value() - began_at.value();
        let energy_used = initial_reserves
            .metabolic_energy()
            .checked_sub(final_reserves.metabolic_energy())
            .unwrap_or_else(|| panic!("clay project gained unexplained metabolic energy"));
        let hydration_used = initial_reserves
            .hydration()
            .checked_sub(final_reserves.hydration())
            .unwrap_or_else(|| panic!("clay project gained unexplained hydration"));
        std::println!(
            "WILDERNESS CASTING BED seed=0x{:016X} evidence=generated-exact-local path=[forage>water>clay>fork>junction>stone>junction>timber>junction] elapsed={}t/{} survival-spent=[energy:{}nJ hydration:{}uL] reserves-left=[energy:{}nJ hydration:{}uL] pressure=[hungry:{} thirsty:{}] materials=[clay:{}mg stone:{}mg timber:{}mg] craft=[binder:{}b boards:{}b] assembled=true clay-left={}mg matter=conserved",
            seed.value(),
            elapsed,
            physical_minutes_tenths(&registries, elapsed),
            energy_used.nanojoules(),
            hydration_used.microliters(),
            final_reserves.metabolic_energy().nanojoules(),
            final_reserves.hydration().microliters(),
            final_reserves.metabolic_energy() <= registries.survival().physiology().hungry_below(),
            final_reserves.hydration() <= registries.survival().physiology().thirsty_below(),
            binder_mass.milligrams(),
            stone_mass.milligrams(),
            timber_mass.milligrams(),
            binder_batches,
            board_batches,
            clay_before
                .checked_sub(binder_mass)
                .unwrap_or_else(|| unreachable!())
                .milligrams(),
        );
    }
}
