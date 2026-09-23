//! Ordinary helve-hammer investment contract.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_HELVE_HAMMER,
    EQUIPMENT_TIMBER_TREADLE_HAMMER, FORM_BOARD, FORM_FLYWHEEL, FORM_HANDLE, FORM_LOG,
    FORM_NATIVE_METAL, FORM_REINFORCEMENT, FORM_TOOL, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT, PROCESS_SHAPE_WOOD_BOARDS,
    PROCESS_SHAPE_WOOD_HANDLE, build_registries,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{
    PoweredCraftRequest, resolve_manual_craft, validate_start_powered_craft,
};
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::equipment::{validate_assemble_equipment, validate_upgrade_equipment};
use deep_hearth::inventory::{MaterialLotSelection, StockpileStorageProfile};
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::survival::initialize_player_survival;

use super::environment::ROOM_TEMPERATURE;
use super::manual_craft_execution::execute_manual_craft;
use super::manual_craft_selection::select_manual_craft_request;
use super::manual_power_timing::finish_manual_power_work;
use super::production_timing::finish_uninterrupted_production_job;

const SHORT_COPPER_ORDER: u64 = 8;
const PROJECT_COPPER_ORDER: u64 = 14;
const HELVE_WORK_PER_REINFORCEMENT: Energy = Energy::from_nanojoules(100_000_000_000);

fn seed_material(
    registries: &deep_hearth::registry::Registries,
    state: &mut AppState,
    stockpile: deep_hearth::inventory::StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) -> deep_hearth::inventory::MaterialLotId {
    seed_lot(
        registries,
        state,
        stockpile,
        commodity,
        mass,
        ROOM_TEMPERATURE,
    )
}

#[test]
fn helve_hammer_converts_treadle_workshop_when_repeated_copper_work_repays_attention() {
    let registries = build_registries();
    let mut state = AppState::new();

    let bootstrap = seed_stockpile(
        &mut state,
        Mass::from_milligrams(6_600_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
            Mass::from_milligrams(3_200_000),
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(1_800_000),
        ),
    ] {
        seed_material(&registries, &mut state, bootstrap, commodity, mass);
    }
    let upgrade_raw = seed_stockpile(
        &mut state,
        Mass::from_milligrams(3_020_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_material(
        &registries,
        &mut state,
        upgrade_raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(3_000_000),
    );
    seed_material(
        &registries,
        &mut state,
        upgrade_raw,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        Mass::from_milligrams(20_000),
    );
    let upgrade_parts = seed_stockpile(
        &mut state,
        Mass::from_milligrams(3_020_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let order_mass = Mass::from_milligrams(PROJECT_COPPER_ORDER * 20_000);
    let work_source = seed_stockpile(
        &mut state,
        order_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let work_lot = seed_material(
        &registries,
        &mut state,
        work_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        order_mass,
    );
    let baseline_output = seed_stockpile(
        &mut state,
        order_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let powered_output = seed_stockpile(
        &mut state,
        order_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );

    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("helve investment survival setup failed: {error}"));
    let treadle_hammer = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_TREADLE_HAMMER,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("helve investment treadle-hammer assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("helve investment treadle-hammer commit failed: {error}"));
    let crank =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_HAND_CRANK, bootstrap)
            .unwrap_or_else(|error| panic!("helve investment crank assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("helve investment crank commit failed: {error}"));
    let drive =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, bootstrap)
            .unwrap_or_else(|error| panic!("helve investment flywheel assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("helve investment flywheel commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bootstrap)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );

    let short_baseline = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
            work_source,
            SHORT_COPPER_ORDER,
            "helve short-order baseline",
        )
        .with_equipment(treadle_hammer),
    )
    .unwrap_or_else(|error| panic!("helve short baseline projection failed: {error}"));
    let project_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        work_source,
        PROJECT_COPPER_ORDER,
        "helve project baseline",
    )
    .with_equipment(treadle_hammer);
    let project_baseline = resolve_manual_craft(&registries, &state, &project_request)
        .unwrap_or_else(|error| panic!("helve project baseline projection failed: {error}"));

    let setup_board_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        upgrade_raw,
        1,
        "helve upgrade board",
    );
    let setup_board_ticks = resolve_manual_craft(&registries, &state, &setup_board_request)
        .unwrap_or_else(|error| panic!("helve upgrade board projection failed: {error}"))
        .duration()
        .value();
    let setup_handle_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_HANDLE,
        upgrade_raw,
        2,
        "helve upgrade handles",
    );
    let setup_handle_ticks = resolve_manual_craft(&registries, &state, &setup_handle_request)
        .unwrap_or_else(|error| panic!("helve upgrade handle projection failed: {error}"))
        .duration()
        .value();
    let setup_copper_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        upgrade_raw,
        1,
        "helve upgrade bearing reinforcement",
    );
    let setup_copper_ticks = resolve_manual_craft(&registries, &state, &setup_copper_request)
        .unwrap_or_else(|error| panic!("helve upgrade copper projection failed: {error}"))
        .duration()
        .value();
    let setup_attention = setup_board_ticks + setup_handle_ticks + setup_copper_ticks;
    let charge = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            drive,
            HELVE_WORK_PER_REINFORCEMENT,
        ),
    )
    .unwrap_or_else(|error| panic!("helve charge projection failed: {error}"));
    let charge_ticks = charge.work().completes_at().value() - state.tick().value();
    assert!(
        setup_attention + charge_ticks * SHORT_COPPER_ORDER >= short_baseline.duration().value(),
        "a short copper run must keep using the already-owned treadle hammer"
    );
    let project_machine_attention = setup_attention + charge_ticks * PROJECT_COPPER_ORDER;
    assert!(
        project_machine_attention < project_baseline.duration().value(),
        "repeated copper forming must eventually repay the helve conversion"
    );

    let decision_state = state.clone();
    let matter_before = calculate_matter_accounting(&decision_state)
        .unwrap_or_else(|error| panic!("helve investment matter setup failed: {error}"))
        .total();
    let mut baseline = decision_state.clone();
    let baseline_ticks = execute_manual_craft(
        &registries,
        &mut baseline,
        project_request,
        baseline_output,
        "helve project treadle baseline",
    );

    let mut powered = decision_state;
    let executed_setup = [
        execute_manual_craft(
            &registries,
            &mut powered,
            setup_board_request,
            upgrade_parts,
            "helve upgrade board",
        )
        .value(),
        execute_manual_craft(
            &registries,
            &mut powered,
            setup_handle_request,
            upgrade_parts,
            "helve upgrade handles",
        )
        .value(),
        execute_manual_craft(
            &registries,
            &mut powered,
            setup_copper_request,
            upgrade_parts,
            "helve upgrade bearing reinforcement",
        )
        .value(),
    ]
    .into_iter()
    .sum::<u64>();
    assert_eq!(executed_setup, setup_attention);
    let helve = validate_upgrade_equipment(
        &registries,
        &powered,
        treadle_hammer,
        EQUIPMENT_TIMBER_HELVE_HAMMER,
        upgrade_parts,
    )
    .unwrap_or_else(|error| panic!("treadle-to-helve upgrade failed: {error}"))
    .commit(&mut powered)
    .unwrap_or_else(|error| panic!("treadle-to-helve upgrade commit failed: {error}"));
    assert_eq!(helve, treadle_hammer);

    let mut charge_attention = 0_u64;
    for _ in 0..PROJECT_COPPER_ORDER {
        let work = validate_start_manual_power(
            &registries,
            &powered,
            ManualPowerRequest::new(
                MANUAL_POWER_HAND_CRANK,
                crank,
                drive,
                HELVE_WORK_PER_REINFORCEMENT,
            ),
        )
        .unwrap_or_else(|error| panic!("helve project charge failed: {error}"))
        .commit(&mut powered)
        .unwrap_or_else(|error| panic!("helve project charge commit failed: {error}"));
        charge_attention +=
            finish_manual_power_work(&registries, &mut powered, work, "helve project charge");
        let job = validate_start_powered_craft(
            &registries,
            &powered,
            PoweredCraftRequest::single(
                PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
                work_source,
                MaterialLotSelection::new(work_lot, Mass::from_milligrams(20_000)),
                helve,
                drive,
            ),
            powered_output,
        )
        .unwrap_or_else(|error| panic!("helve project start failed: {error}"))
        .commit(&mut powered)
        .unwrap_or_else(|error| panic!("helve project commit failed: {error}"));
        assert_eq!(powered.player_work().active(), None);
        finish_uninterrupted_production_job(
            &registries,
            &mut powered,
            job,
            "helve project unattended forging",
        );
    }
    assert_eq!(charge_attention, charge_ticks * PROJECT_COPPER_ORDER);
    assert_eq!(executed_setup + charge_attention, project_machine_attention);
    assert!(project_machine_attention < baseline_ticks.value());
    assert_eq!(
        powered
            .inventory()
            .get_stockpile(powered_output)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))
            }),
        baseline
            .inventory()
            .get_stockpile(baseline_output)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))
            })
    );
    assert_eq!(
        calculate_matter_accounting(&baseline)
            .unwrap_or_else(|error| panic!("helve baseline matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_matter_accounting(&powered)
            .unwrap_or_else(|error| panic!("helve matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &powered)
        .unwrap_or_else(|error| panic!("helve project final state invalid: {error}"));
}
