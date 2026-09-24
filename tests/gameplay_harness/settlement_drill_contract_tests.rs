//! Ordinary pump-drill to spindle-drill investment contract.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_TIMBER_SPINDLE_DRILL, FORM_DRILL_BIT, FORM_FLYWHEEL, FORM_HANDLE, FORM_LOG,
    FORM_REINFORCEMENT, FORM_SCRAP, FORM_SCREEN_PLATE, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_PIERCE_COPPER_SCREEN_PLATE, PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
    PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE, build_registries,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{
    PoweredCraftRequest, resolve_manual_craft, validate_start_powered_craft,
};
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::equipment::{validate_assemble_equipment, validate_upgrade_equipment};
use deep_hearth::inventory::StockpileStorageProfile;
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::survival::initialize_player_survival;

use super::environment::ROOM_TEMPERATURE;
use super::manual_craft_execution::execute_manual_craft;
use super::manual_craft_selection::select_manual_craft_request;
use super::manual_power_timing::finish_manual_power_work;
use super::material_selection::select_stockpile_mass;
use super::production_timing::finish_uninterrupted_production_job;

const SHORT_PLATE_ORDER: u64 = 8;
const PROJECT_PLATE_ORDER: u64 = 12;
const PLATE_INPUT_MASS: Mass = Mass::from_milligrams(20_000);
const SPINDLE_WORK_PER_PLATE: Energy = Energy::from_nanojoules(50_000_000_000);

fn seed_material(
    registries: &deep_hearth::registry::Registries,
    state: &mut AppState,
    stockpile: deep_hearth::inventory::StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) {
    let _ = seed_lot(
        registries,
        state,
        stockpile,
        commodity,
        mass,
        ROOM_TEMPERATURE,
    );
}

pub(super) fn assert_spindle_drill_investment_contract() {
    let registries = build_registries();
    let mut state = AppState::new();

    // This stock is the already-shaped primitive workshop package. The later upgrade additions are
    // deliberately made through ordinary manual recipes below so the investment cost remains
    // player attention rather than fixture-only finished parts.
    let bootstrap = seed_stockpile(
        &mut state,
        Mass::from_milligrams(3_600_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(2_700_000),
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT),
            Mass::from_milligrams(100_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(800_000),
        ),
    ] {
        seed_material(&registries, &mut state, bootstrap, commodity, mass);
    }
    let pump = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("pump drill assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("pump drill assembly commit failed: {error}"));
    let crank =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_HAND_CRANK, bootstrap)
            .unwrap_or_else(|error| panic!("spindle investment crank assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("spindle investment crank commit failed: {error}"));
    let drive =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, bootstrap)
            .unwrap_or_else(|error| panic!("spindle investment flywheel assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("spindle investment flywheel commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bootstrap)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );

    // Wear the portable drill once before the decision so the upgrade has to preserve real use,
    // not merely a pristine definition identity.
    let calibration_source = seed_stockpile(
        &mut state,
        PLATE_INPUT_MASS,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_material(
        &registries,
        &mut state,
        calibration_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        PLATE_INPUT_MASS,
    );
    let calibration_output = seed_stockpile(
        &mut state,
        PLATE_INPUT_MASS,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let upgrade_raw = seed_stockpile(
        &mut state,
        Mass::from_milligrams(4_020_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_material(
        &registries,
        &mut state,
        upgrade_raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(4_000_000),
    );
    seed_material(
        &registries,
        &mut state,
        upgrade_raw,
        CommodityKey::new(MATERIAL_COPPER, deep_hearth::content::FORM_NATIVE_METAL),
        Mass::from_milligrams(20_000),
    );
    let upgrade_parts = seed_stockpile(
        &mut state,
        Mass::from_milligrams(4_020_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let work_mass = Mass::from_milligrams(PROJECT_PLATE_ORDER * PLATE_INPUT_MASS.milligrams());
    let work_source = seed_stockpile(
        &mut state,
        work_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_material(
        &registries,
        &mut state,
        work_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        work_mass,
    );
    let baseline_output = seed_stockpile(
        &mut state,
        work_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let powered_output = seed_stockpile(
        &mut state,
        work_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );

    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("spindle investment survival setup failed: {error}"));
    let calibration = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_PIERCE_COPPER_SCREEN_PLATE,
        calibration_source,
        1,
        "spindle investment pump-drill calibration",
    )
    .with_equipment(pump);
    let _ = execute_manual_craft(
        &registries,
        &mut state,
        calibration,
        calibration_output,
        "spindle investment pump-drill calibration",
    );
    let pump_condition = state
        .equipment()
        .get_equipment(pump)
        .unwrap_or_else(|| panic!("worn pump drill disappeared"))
        .condition();
    assert!(pump_condition < deep_hearth::maintenance::Condition::PRISTINE);

    let short_baseline = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_PIERCE_COPPER_SCREEN_PLATE,
            work_source,
            SHORT_PLATE_ORDER,
            "spindle short-order pump baseline",
        )
        .with_equipment(pump),
    )
    .unwrap_or_else(|error| panic!("spindle short baseline projection failed: {error}"));
    let project_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_PIERCE_COPPER_SCREEN_PLATE,
        work_source,
        PROJECT_PLATE_ORDER,
        "spindle project pump baseline",
    )
    .with_equipment(pump);
    let project_baseline = resolve_manual_craft(&registries, &state, &project_request)
        .unwrap_or_else(|error| panic!("spindle project baseline projection failed: {error}"));

    let board_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        upgrade_raw,
        2,
        "spindle frame boards",
    );
    let handle_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_HANDLE,
        upgrade_raw,
        2,
        "spindle frame handles",
    );
    let copper_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        upgrade_raw,
        1,
        "spindle bearing reinforcement",
    );
    let setup_attention = resolve_manual_craft(&registries, &state, &board_request)
        .unwrap_or_else(|error| panic!("spindle board projection failed: {error}"))
        .duration()
        .value()
        + resolve_manual_craft(&registries, &state, &handle_request)
            .unwrap_or_else(|error| panic!("spindle handle projection failed: {error}"))
            .duration()
            .value()
        + resolve_manual_craft(&registries, &state, &copper_request)
            .unwrap_or_else(|error| panic!("spindle copper projection failed: {error}"))
            .duration()
            .value();
    let charge = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            drive,
            SPINDLE_WORK_PER_PLATE,
        ),
    )
    .unwrap_or_else(|error| panic!("spindle charge projection failed: {error}"));
    let charge_ticks = charge.work().completes_at().value() - state.tick().value();
    assert!(
        setup_attention + charge_ticks * SHORT_PLATE_ORDER >= short_baseline.duration().value(),
        "a short screen-plate order should keep using the already-owned pump drill"
    );
    let project_machine_attention = setup_attention + charge_ticks * PROJECT_PLATE_ORDER;
    assert!(
        project_machine_attention < project_baseline.duration().value(),
        "repeated screen-plate work should eventually repay the spindle frame in player attention"
    );

    let decision_state = state.clone();
    let matter_before = calculate_matter_accounting(&decision_state)
        .unwrap_or_else(|error| panic!("spindle investment matter setup failed: {error}"))
        .total();
    let mut baseline = decision_state.clone();
    let baseline_ticks = execute_manual_craft(
        &registries,
        &mut baseline,
        project_request,
        baseline_output,
        "spindle project pump baseline",
    );

    let mut powered = decision_state;
    let mut executed_setup = 0_u64;
    for (process, batches, context) in [
        (PROCESS_SHAPE_WOOD_BOARDS, 2, "spindle frame boards"),
        (PROCESS_SHAPE_WOOD_HANDLE, 2, "spindle frame handles"),
        (
            PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
            1,
            "spindle bearing reinforcement",
        ),
    ] {
        let request = select_manual_craft_request(
            &registries,
            &powered,
            process,
            upgrade_raw,
            batches,
            context,
        );
        executed_setup +=
            execute_manual_craft(&registries, &mut powered, request, upgrade_parts, context)
                .value();
    }
    assert_eq!(executed_setup, setup_attention);
    let spindle = validate_upgrade_equipment(
        &registries,
        &powered,
        pump,
        EQUIPMENT_TIMBER_SPINDLE_DRILL,
        upgrade_parts,
    )
    .unwrap_or_else(|error| panic!("pump-to-spindle upgrade failed: {error}"))
    .commit(&mut powered)
    .unwrap_or_else(|error| panic!("pump-to-spindle upgrade commit failed: {error}"));
    assert_eq!(spindle, pump);
    assert_eq!(
        powered
            .equipment()
            .get_equipment(spindle)
            .map(|record| record.condition()),
        Some(pump_condition),
        "spindle conversion must retain the worn pump drill's condition"
    );

    let mut charge_attention = 0_u64;
    for _ in 0..PROJECT_PLATE_ORDER {
        let charge = validate_start_manual_power(
            &registries,
            &powered,
            ManualPowerRequest::new(
                MANUAL_POWER_HAND_CRANK,
                crank,
                drive,
                SPINDLE_WORK_PER_PLATE,
            ),
        )
        .unwrap_or_else(|error| panic!("spindle project charge failed: {error}"))
        .commit(&mut powered)
        .unwrap_or_else(|error| panic!("spindle project charge commit failed: {error}"));
        charge_attention +=
            finish_manual_power_work(&registries, &mut powered, charge, "spindle project charge");
        let selections = select_stockpile_mass(
            &powered,
            work_source,
            PLATE_INPUT_MASS,
            "spindle powered plate input",
        );
        let job = validate_start_powered_craft(
            &registries,
            &powered,
            PoweredCraftRequest::new(
                PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
                work_source,
                selections,
                spindle,
                drive,
            ),
            powered_output,
        )
        .unwrap_or_else(|error| panic!("spindle project start failed: {error}"))
        .commit(&mut powered)
        .unwrap_or_else(|error| panic!("spindle project commit failed: {error}"));
        assert_eq!(powered.player_work().active(), None);
        finish_uninterrupted_production_job(
            &registries,
            &mut powered,
            job,
            "spindle project unattended drilling",
        );
    }
    assert_eq!(charge_attention, charge_ticks * PROJECT_PLATE_ORDER);
    assert_eq!(executed_setup + charge_attention, project_machine_attention);
    assert!(project_machine_attention < baseline_ticks.value());
    for commodity in [
        CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE),
        CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
    ] {
        assert_eq!(
            powered
                .inventory()
                .get_stockpile(powered_output)
                .map(|stockpile| stockpile.get_mass(commodity)),
            baseline
                .inventory()
                .get_stockpile(baseline_output)
                .map(|stockpile| stockpile.get_mass(commodity)),
            "powered spindle drilling must preserve the manual pump drill's exact material transform"
        );
    }
    assert_eq!(
        calculate_matter_accounting(&baseline)
            .unwrap_or_else(|error| panic!("spindle baseline matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_matter_accounting(&powered)
            .unwrap_or_else(|error| panic!("spindle investment matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &powered)
        .unwrap_or_else(|error| panic!("spindle investment final state invalid: {error}"));
}
