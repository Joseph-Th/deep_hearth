//! One matched primitive woodworking job with or without spending discovered copper on an adze.

use super::*;
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE, EQUIPMENT_STONE_WOODWORKING_ADZE,
    FORM_NATIVE_METAL, FORM_REINFORCEMENT, FORM_TOOL, MATERIAL_COPPER, PROCESS_SHAPE_WOOD_BOARDS,
};
use deep_hearth::equipment::validate_upgrade_equipment;

pub(super) struct WoodworkingRoute {
    pub(super) junction: VoxelCoord,
    pub(super) forage: VoxelCoord,
    pub(super) water: VoxelCoord,
    pub(super) outcrop: VoxelCoord,
    pub(super) stone: VoxelCoord,
    pub(super) timber: VoxelCoord,
    pub(super) carried: StockpileId,
}

pub(super) struct WoodworkingPayoff {
    pub(super) copper_acquisition_ticks: u64,
    pub(super) board_mass: Mass,
    pub(super) roof_panel_mass: Mass,
    pub(super) batches: u64,
    pub(super) stone_board_ticks: u64,
    pub(super) copper_board_ticks: u64,
    pub(super) stone_total_ticks: u64,
    pub(super) copper_total_ticks: u64,
    pub(super) stone_condition_ppm: u32,
    pub(super) copper_condition_ppm: u32,
}

fn shape_with_owned_adze(
    registries: &Registries,
    state: &mut AppState,
    adze: deep_hearth::equipment::EquipmentId,
    batches: NonZeroU64,
    label: &str,
) -> u64 {
    let option = player_manual_craft_options(registries, state)
        .unwrap_or_else(|error| panic!("{label} personal crafting catalog rejected: {error}"))
        .into_iter()
        .find(|option| option.process() == PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("{label} board shaping choice disappeared"));
    let started = state.tick();
    let job =
        validate_start_player_manual_craft_option(registries, state, option, batches, Some(adze))
            .unwrap_or_else(|error| panic!("{label} board craft rejected: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("{label} board craft commit failed: {error}"));
    let finish = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("{label} board craft job disappeared"))
        .completes_at();
    advance_to(
        registries,
        state,
        finish,
        TickEventAllowance {
            production_jobs: &[job],
            ..TickEventAllowance::default()
        },
        label,
    );
    finish.value() - started.value()
}

pub(super) fn exercise_reinforced_adze_investment(
    registries: &Registries,
    source: &AppState,
    route: WoodworkingRoute,
    conserved: deep_hearth::core::quantity::AggregateMass,
) -> WoodworkingPayoff {
    let mut base = source.clone();
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let log = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let board = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let reinforcement = CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT);
    // Nine log batches yield 7.2 kg of boards: enough for the authored
    // 6.5 kg overhead cover rather than arbitrary lumber stockpiling.
    let batches = NonZeroU64::new(9).unwrap();
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut base,
        route.stone,
        "adze stone blank",
    );
    let stone_source = local_surface_resource(&base, stone);
    gather_in_batches(
        registries,
        &mut base,
        stone_source,
        Mass::from_milligrams(1_000_000),
        "adze stone blank",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut base,
        route.junction,
        "adze return from stone",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut base,
        route.timber,
        "adze timber and board feed",
    );
    let timber_source = local_surface_resource(&base, log);
    gather_in_batches(
        registries,
        &mut base,
        timber_source,
        Mass::from_milligrams(10_000_000),
        "adze timber and board feed",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut base,
        route.junction,
        "adze return from timber",
    );
    craft_in_place(
        registries,
        &mut base,
        stone,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        1,
        "adze stone edge",
    );
    craft_in_place(
        registries,
        &mut base,
        log,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        1,
        "adze handle",
    );
    let option = player_equipment_assembly_options(registries, &base, route.carried)
        .unwrap_or_else(|error| panic!("adze assembly catalog failed: {error}"))
        .into_iter()
        .find(|option| option.definition() == EQUIPMENT_STONE_WOODWORKING_ADZE)
        .unwrap_or_else(|| panic!("newly crafted stone adze is not actor-visible"));
    assert!(option.is_ready());
    let adze = validate_assemble_player_equipment_option(registries, &base, &option)
        .unwrap_or_else(|error| panic!("stone adze admission failed: {error}"))
        .commit(&mut base)
        .unwrap_or_else(|error| panic!("stone adze assembly failed: {error}"));
    assert_eq!(
        base.inventory()
            .get_stockpile(route.carried)
            .unwrap()
            .get_mass(log),
        Mass::from_milligrams(9_000_000)
    );
    let mut stone_branch = base.clone();
    let mut copper_branch = base;
    let decision_at = copper_branch.tick();
    let stone_board_ticks = shape_with_owned_adze(
        registries,
        &mut stone_branch,
        adze,
        batches,
        "base adze boards",
    );
    for (to, label) in [
        (route.forage, "adze copper fork"),
        (route.water, "adze copper water"),
        (route.outcrop, "adze copper outcrop"),
    ] {
        walk_continuous_body_adjacent_and_settle(registries, &mut copper_branch, to, label);
    }
    let native_source = local_surface_resource(&copper_branch, native);
    gather_in_batches(
        registries,
        &mut copper_branch,
        native_source,
        Mass::from_milligrams(20_000),
        "adze reinforcement from finite copper float",
    );
    for (to, label) in [
        (route.water, "adze copper return water"),
        (route.forage, "adze copper return fork"),
        (route.junction, "adze copper return to worksite"),
    ] {
        walk_continuous_body_adjacent_and_settle(registries, &mut copper_branch, to, label);
    }
    craft_in_place(
        registries,
        &mut copper_branch,
        native,
        reinforcement,
        1,
        "adze copper reinforcement",
    );
    let upgraded = validate_upgrade_equipment(
        registries,
        &copper_branch,
        adze,
        EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
        route.carried,
    )
    .unwrap_or_else(|error| panic!("adze copper upgrade rejected: {error}"))
    .commit(&mut copper_branch)
    .unwrap_or_else(|error| panic!("adze copper upgrade commit failed: {error}"));
    assert_eq!(
        upgraded, adze,
        "copper reinforcement must retain the invested stone tool"
    );
    let copper_acquisition_ticks = copper_branch.tick().value() - decision_at.value();
    let copper_board_ticks = shape_with_owned_adze(
        registries,
        &mut copper_branch,
        adze,
        batches,
        "copper reinforced adze boards",
    );
    let stone_total_ticks = stone_branch.tick().value() - decision_at.value();
    let copper_total_ticks = copper_branch.tick().value() - decision_at.value();
    assert!(
        copper_total_ticks < stone_total_ticks,
        "first-metal adze must repay its entire extra copper expedition and cold-working before this roof-stock order finishes"
    );
    let board_mass = Mass::from_milligrams(7_200_000);
    let roof_panel_mass = calculate_prismatic_material_mass_ceiling(
        registries.materials(),
        MATERIAL_WOOD,
        BASIC_TIMBER_COVER_PANEL_AREA,
        BASIC_TIMBER_COVER_PANEL_THICKNESS,
    )
    .unwrap_or_else(|error| panic!("first-metal roof board demand invalid: {error}"));
    assert!(
        board_mass >= roof_panel_mass,
        "this player work order must actually prepare enough boards for an overhead cover"
    );
    for (branch, label) in [
        (&stone_branch, "stone adze"),
        (&copper_branch, "copper adze"),
    ] {
        assert_eq!(
            branch
                .inventory()
                .get_stockpile(route.carried)
                .unwrap()
                .get_mass(board),
            board_mass,
            "{label} must produce the same physical finished boards"
        );
        assert_eq!(
            branch
                .inventory()
                .get_stockpile(route.carried)
                .unwrap()
                .get_mass(log),
            Mass::ZERO,
            "{label} must consume the same nine-log production order"
        );
        assert_eq!(
            calculate_matter_accounting(branch).unwrap().total(),
            conserved
        );
        assert_eq!(validate_loaded_state(registries, branch), Ok(()));
    }
    let replay: LoadedSaveEnvelope = serde_json::from_slice(
        &serde_json::to_vec(&SaveEnvelope::new(registries, &copper_branch)).unwrap(),
    )
    .unwrap();
    assert_eq!(replay.into_state(registries), Ok(copper_branch.clone()));
    assert!(copper_board_ticks < stone_board_ticks);
    let stone_condition_ppm = stone_branch
        .equipment()
        .get_equipment(adze)
        .unwrap()
        .condition()
        .parts_per_million();
    let copper_condition_ppm = copper_branch
        .equipment()
        .get_equipment(adze)
        .unwrap()
        .condition()
        .parts_per_million();
    WoodworkingPayoff {
        copper_acquisition_ticks,
        board_mass,
        roof_panel_mass,
        batches: batches.get(),
        stone_board_ticks,
        copper_board_ticks,
        stone_total_ticks,
        copper_total_ticks,
        stone_condition_ppm,
        copper_condition_ppm,
    }
}
