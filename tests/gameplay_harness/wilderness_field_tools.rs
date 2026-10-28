//! A generated-world player earns distinct portable cutting and stone-dressing options.

use super::*;
use deep_hearth::content::{
    EQUIPMENT_COPPER_HAND_SAW, EQUIPMENT_COPPER_STONE_CHISEL, EQUIPMENT_FLINT_WOODWORKING_KNIFE,
    FORM_NATIVE_METAL, FORM_REINFORCEMENT, FORM_SAW_BLADE, FORM_STONE_CROCK_BODY, MATERIAL_COPPER,
    MATERIAL_FLINT, PROCESS_CHISEL_STONE_PROVISIONS_CROCK, PROCESS_KNAP_FLINT_CUTTING_HEAD,
    PROCESS_SAW_WOOD_BOARDS, PROCESS_SHAPE_WOOD_BOARDS,
};
use deep_hearth::logistics::validate_pickup_equipment;

pub(super) fn generated_outcrop_supports_earned_flint_knife_copper_saw_and_chisel() {
    let registries = build_registries();
    let mut state = AppState::new();
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, WILDERNESS_WORLD_SEED);
    validate_generate_initial_wilderness(&registries, &state, WILDERNESS_WORLD_SEED, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    if let Some(demand) = validate_generate_player_neighborhood(&registries, &state).unwrap() {
        demand.commit(&mut state).unwrap();
    }
    let initial_matter = calculate_matter_accounting(&state).unwrap().total();
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let forage = adjacent_surface_resource(&state, berries).position();
    walk_continuous_body_adjacent_and_settle(&registries, &mut state, forage, "flint trail forage");
    let water = adjacent_drinkable_source(&registries, &state).position();
    walk_continuous_body_adjacent_and_settle(&registries, &mut state, water, "flint trail water");
    let flint = CommodityKey::new(MATERIAL_FLINT, FORM_LUMP);
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let outcrop = adjacent_surface_resource(&state, flint).position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        outcrop,
        "flint trail outcrop",
    );
    assert_eq!(
        local_surface_resource(&state, flint),
        state
            .available_surface_resources()
            .find(|source| source.commodity() == flint && source.position() == outcrop)
            .unwrap()
            .id()
    );
    let flint_resource = local_surface_resource(&state, flint);
    gather_in_batches(
        &registries,
        &mut state,
        flint_resource,
        Mass::from_milligrams(300_000),
        "flint knife stock",
    );
    let copper_resource = local_surface_resource(&state, native);
    gather_in_batches(
        &registries,
        &mut state,
        copper_resource,
        Mass::from_milligrams(80_000),
        "saw and chisel metal",
    );
    walk_continuous_body_adjacent_and_settle(&registries, &mut state, water, "flint return water");
    walk_continuous_body_adjacent_and_settle(&registries, &mut state, forage, "flint return fork");
    let junction = state
        .available_adjacent_traversal_destinations()
        .find(|position| *position != spawn && *position != water)
        .unwrap();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "flint return workshop",
    );
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    for (commodity, mass, label) in [
        (
            stone,
            Mass::from_milligrams(6_000_000),
            "crock shaping feed",
        ),
        (
            timber,
            Mass::from_milligrams(5_000_000),
            "tool handles and board feed",
        ),
    ] {
        let adjacent = adjacent_surface_resource(&state, commodity);
        walk_continuous_body_adjacent_and_settle(
            &registries,
            &mut state,
            adjacent.position(),
            label,
        );
        gather_in_batches(&registries, &mut state, adjacent.resource(), mass, label);
        walk_continuous_body_adjacent_and_settle(
            &registries,
            &mut state,
            junction,
            "tool stock return",
        );
    }
    let carried = state.logistics().player().unwrap().carried_stockpile();
    craft_in_place(
        &registries,
        &mut state,
        flint,
        CommodityKey::new(MATERIAL_FLINT, FORM_TOOL),
        1,
        "knap flint knife head",
    );
    craft_in_place(
        &registries,
        &mut state,
        timber,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        3,
        "shape field tool handles",
    );
    craft_in_place(
        &registries,
        &mut state,
        native,
        CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
        1,
        "cold work portable saw blade",
    );
    craft_in_place(
        &registries,
        &mut state,
        native,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        1,
        "cold work chisel edge",
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(native),
        Mass::ZERO,
    );
    let mut equipment = Vec::new();
    for definition in [
        EQUIPMENT_FLINT_WOODWORKING_KNIFE,
        EQUIPMENT_COPPER_HAND_SAW,
        EQUIPMENT_COPPER_STONE_CHISEL,
    ] {
        let option = player_equipment_assembly_options(&registries, &state, carried)
            .unwrap()
            .into_iter()
            .find(|option| option.definition() == definition)
            .unwrap();
        assert!(option.is_ready());
        let item = validate_assemble_player_equipment_option(&registries, &state, &option)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        validate_pickup_equipment(&registries, &state, item)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        equipment.push(item);
    }
    let knife = equipment[0];
    let saw = equipment[1];
    let chisel = equipment[2];
    let craft_with = |state: &mut AppState, process, provider| {
        let option = player_manual_craft_options(&registries, state)
            .unwrap()
            .into_iter()
            .find(|option| option.process() == process)
            .unwrap();
        let started = state.tick();
        let job = validate_start_player_manual_craft_option(
            &registries,
            state,
            option,
            NonZeroU64::new(1).unwrap(),
            Some(provider),
        )
        .unwrap()
        .commit(state)
        .unwrap();
        let end = state.production().get_job(job).unwrap().completes_at();
        advance_to(
            &registries,
            state,
            end,
            TickEventAllowance {
                production_jobs: &[job],
                ..TickEventAllowance::default()
            },
            "portable-specialist-work",
        );
        state.tick().value() - started.value()
    };
    let chisel_start = state.clone();
    let chisel_duration = craft_with(&mut state, PROCESS_CHISEL_STONE_PROVISIONS_CROCK, chisel);
    let mut hand_stone = chisel_start;
    let hand_start = hand_stone.tick();
    craft_in_place(
        &registries,
        &mut hand_stone,
        stone,
        CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
        1,
        "unassisted crock comparator",
    );
    assert!(chisel_duration < hand_stone.tick().value() - hand_start.value());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY)),
        Mass::from_milligrams(2_400_000),
    );
    let saw_start = state.clone();
    let saw_duration = craft_with(&mut state, PROCESS_SAW_WOOD_BOARDS, saw);
    let mut knife_branch = saw_start.clone();
    let knife_ticks = craft_with(&mut knife_branch, PROCESS_SHAPE_WOOD_BOARDS, knife);
    let mut rough = saw_start;
    let rough_start = rough.tick();
    craft_in_place(
        &registries,
        &mut rough,
        timber,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        1,
        "rough hewing comparator",
    );
    let rough_ticks = rough.tick().value() - rough_start.value();
    assert!(
        knife_ticks < rough_ticks,
        "the gathered flint head must buy meaningful woodworking time before a saw is affordable"
    );
    let boards = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let chips = CommodityKey::new(MATERIAL_WOOD, FORM_CHIP);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(boards),
        Mass::from_milligrams(900_000),
    );
    assert_eq!(
        rough
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(boards),
        Mass::from_milligrams(800_000),
    );
    assert_eq!(
        knife_branch
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(boards),
        Mass::from_milligrams(800_000),
        "flint preserves hewing recovery rather than granting saw-blade efficiency"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(chips),
        Mass::from_milligrams(2_500_000),
    );
    assert!(saw_duration > 0);
    for (branch, label) in [
        (&state, "saw-and-chisel"),
        (&rough, "rough-board-comparator"),
        (&knife_branch, "flint-knife-comparator"),
        (&hand_stone, "unassisted-stone-comparator"),
    ] {
        assert_eq!(
            calculate_matter_accounting(branch).unwrap().total(),
            initial_matter,
            "{label} must conserve actual generated material"
        );
        assert_eq!(validate_loaded_state(&registries, branch), Ok(()));
    }
    assert_eq!(
        registries
            .crafting()
            .get_manual(PROCESS_KNAP_FLINT_CUTTING_HEAD)
            .unwrap()
            .outputs()[0]
            .mass(),
        Mass::from_milligrams(220_000),
    );
    std::println!(
        "PLAYER FIELD TOOLS evidence=separate-generated-opening actual=[flint:300g copper:80g timber:5kg stone:6kg] tools=[flint-knife:true copper-hand-saw:true copper-chisel:true] knife=[work:{}t hand:{}t yield:800g] chisel=[specialist:{}t hand:{}t] boards=[saw:900g hand:800g] worn-tools=repairable conservation=true persistence=validated",
        knife_ticks,
        rough_ticks,
        chisel_duration,
        hand_stone.tick().value() - hand_start.value(),
    );
}
