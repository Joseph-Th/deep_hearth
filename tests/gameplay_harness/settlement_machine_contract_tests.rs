//! Ordinary settlement-machine investment contracts.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
    EQUIPMENT_TIMBER_SASH_SAWMILL, FORM_BOARD, FORM_CHIP, FORM_FLYWHEEL, FORM_HANDLE, FORM_LOG,
    FORM_NATIVE_METAL, FORM_SAW_BLADE, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_STONE,
    MATERIAL_WOOD, PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_POWER_SAW_WOOD_BOARDS,
    PROCESS_SAW_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE, build_registries,
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

const SHORT_LUMBER_ORDER: u64 = 20;
const PROJECT_LUMBER_ORDER: u64 = 40;
const SAWMILL_WORK_PER_LOG: Energy = Energy::from_nanojoules(250_000_000_000);

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
fn sash_sawmill_upgrades_existing_workshop_only_when_disclosed_lumber_demand_repays_attention() {
    let registries = build_registries();
    let mut state = AppState::new();

    // Disclosed bootstrap: the settlement already owns the earlier frame-saw and mechanical-work
    // tier. The decision under test is whether to keep using that durable infrastructure or spend
    // current raw material and attention converting it into unattended sawing capacity.
    let bootstrap = seed_stockpile(
        &mut state,
        Mass::from_milligrams(4_054_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
            Mass::from_milligrams(1_600_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(600_000),
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
            Mass::from_milligrams(54_000),
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
        Mass::from_milligrams(6_020_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_material(
        &registries,
        &mut state,
        upgrade_raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(6_000_000),
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
        Mass::from_milligrams(7_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let work_source = seed_stockpile(
        &mut state,
        Mass::from_milligrams(PROJECT_LUMBER_ORDER * 1_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let work_lot = seed_material(
        &registries,
        &mut state,
        work_source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(PROJECT_LUMBER_ORDER * 1_000_000),
    );
    let baseline_output = seed_stockpile(
        &mut state,
        Mass::from_milligrams(PROJECT_LUMBER_ORDER * 1_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let powered_output = seed_stockpile(
        &mut state,
        Mass::from_milligrams(PROJECT_LUMBER_ORDER * 1_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );

    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("sawmill investment survival setup failed: {error}"));
    let frame_saw = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
        bootstrap,
    )
    .unwrap_or_else(|error| panic!("sawmill investment frame-saw assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("sawmill investment frame-saw commit failed: {error}"));
    let crank =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_HAND_CRANK, bootstrap)
            .unwrap_or_else(|error| panic!("sawmill investment crank assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("sawmill investment crank commit failed: {error}"));
    let drive =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, bootstrap)
            .unwrap_or_else(|error| panic!("sawmill investment flywheel assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("sawmill investment flywheel commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(bootstrap)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "disclosed prior infrastructure package must consume exactly its authored components"
    );

    let short_baseline = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_SAW_WOOD_BOARDS,
            work_source,
            SHORT_LUMBER_ORDER,
            "sawmill short-order baseline",
        )
        .with_equipment(frame_saw),
    )
    .unwrap_or_else(|error| panic!("sawmill short baseline projection failed: {error}"));
    let project_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SAW_WOOD_BOARDS,
        work_source,
        PROJECT_LUMBER_ORDER,
        "sawmill project baseline",
    )
    .with_equipment(frame_saw);
    let project_baseline = resolve_manual_craft(&registries, &state, &project_request)
        .unwrap_or_else(|error| panic!("sawmill project baseline projection failed: {error}"));

    let setup_board_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SAW_WOOD_BOARDS,
        upgrade_raw,
        3,
        "sawmill upgrade boards",
    )
    .with_equipment(frame_saw);
    let setup_board_ticks = resolve_manual_craft(&registries, &state, &setup_board_request)
        .unwrap_or_else(|error| panic!("sawmill upgrade board projection failed: {error}"))
        .duration()
        .value();
    let setup_handle_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_HANDLE,
        upgrade_raw,
        3,
        "sawmill upgrade handles",
    );
    let setup_handle_ticks = resolve_manual_craft(&registries, &state, &setup_handle_request)
        .unwrap_or_else(|error| panic!("sawmill upgrade handle projection failed: {error}"))
        .duration()
        .value();
    let setup_copper_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        upgrade_raw,
        1,
        "sawmill upgrade reinforcement",
    );
    let setup_copper_ticks = resolve_manual_craft(&registries, &state, &setup_copper_request)
        .unwrap_or_else(|error| panic!("sawmill upgrade copper projection failed: {error}"))
        .duration()
        .value();
    let setup_attention = setup_board_ticks + setup_handle_ticks + setup_copper_ticks;
    let charge = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, SAWMILL_WORK_PER_LOG),
    )
    .unwrap_or_else(|error| panic!("sawmill charge projection failed: {error}"));
    let charge_ticks = charge.work().completes_at().value() - state.tick().value();
    let short_machine_attention = setup_attention + charge_ticks * SHORT_LUMBER_ORDER;
    let project_machine_attention = setup_attention + charge_ticks * PROJECT_LUMBER_ORDER;
    assert!(
        short_machine_attention >= short_baseline.duration().value(),
        "small lumber orders must keep using the already-owned frame saw instead of forcing mechanization"
    );
    assert!(
        project_machine_attention < project_baseline.duration().value(),
        "a disclosed settlement lumber project must be large enough to repay sawmill conversion and charging attention"
    );

    let decision_state = state.clone();
    let initial_matter = calculate_matter_accounting(&decision_state)
        .unwrap_or_else(|error| panic!("sawmill investment matter setup failed: {error}"))
        .total();

    let mut baseline = decision_state.clone();
    let baseline_ticks = execute_manual_craft(
        &registries,
        &mut baseline,
        project_request,
        baseline_output,
        "sawmill project frame-saw baseline",
    );
    assert_eq!(baseline_ticks, project_baseline.duration());

    let mut powered = decision_state;
    let executed_setup = [
        execute_manual_craft(
            &registries,
            &mut powered,
            setup_board_request,
            upgrade_parts,
            "sawmill upgrade boards",
        )
        .value(),
        execute_manual_craft(
            &registries,
            &mut powered,
            setup_handle_request,
            upgrade_parts,
            "sawmill upgrade handles",
        )
        .value(),
        execute_manual_craft(
            &registries,
            &mut powered,
            setup_copper_request,
            upgrade_parts,
            "sawmill upgrade reinforcement",
        )
        .value(),
    ]
    .into_iter()
    .sum::<u64>();
    assert_eq!(executed_setup, setup_attention);
    let sawmill = validate_upgrade_equipment(
        &registries,
        &powered,
        frame_saw,
        EQUIPMENT_TIMBER_SASH_SAWMILL,
        upgrade_parts,
    )
    .unwrap_or_else(|error| panic!("frame-saw to sash-sawmill upgrade failed: {error}"))
    .commit(&mut powered)
    .unwrap_or_else(|error| panic!("frame-saw to sash-sawmill upgrade commit failed: {error}"));
    assert_eq!(
        sawmill, frame_saw,
        "mechanization must preserve equipment identity"
    );

    let mut executed_charge_attention = 0_u64;
    let mut powered_elapsed = 0_u64;
    for _ in 0..PROJECT_LUMBER_ORDER {
        let work = validate_start_manual_power(
            &registries,
            &powered,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, SAWMILL_WORK_PER_LOG),
        )
        .unwrap_or_else(|error| panic!("sawmill project charging failed: {error}"))
        .commit(&mut powered)
        .unwrap_or_else(|error| panic!("sawmill project charging commit failed: {error}"));
        let charged =
            finish_manual_power_work(&registries, &mut powered, work, "sawmill project charge");
        executed_charge_attention += charged;

        let job = validate_start_powered_craft(
            &registries,
            &powered,
            PoweredCraftRequest::single(
                PROCESS_POWER_SAW_WOOD_BOARDS,
                work_source,
                MaterialLotSelection::new(work_lot, Mass::from_milligrams(1_000_000)),
                sawmill,
                drive,
            ),
            powered_output,
        )
        .unwrap_or_else(|error| panic!("sawmill project start failed: {error}"))
        .commit(&mut powered)
        .unwrap_or_else(|error| panic!("sawmill project commit failed: {error}"));
        let duration = powered
            .production()
            .get_job(job)
            .map(|record| record.active_duration().value())
            .unwrap_or_else(|| panic!("sawmill project job disappeared after admission"));
        assert_eq!(powered.player_work().active(), None);
        finish_uninterrupted_production_job(
            &registries,
            &mut powered,
            job,
            "sawmill project unattended sawing",
        );
        powered_elapsed += duration;
    }
    assert_eq!(
        executed_charge_attention,
        charge_ticks * PROJECT_LUMBER_ORDER
    );
    assert_eq!(
        executed_setup + executed_charge_attention,
        project_machine_attention,
        "pre-action attention estimate must match the executed mechanization package"
    );
    assert!(project_machine_attention < baseline_ticks.value());
    assert!(
        powered_elapsed > 0,
        "delegated machine work must still occupy world time"
    );

    let baseline_stockpile = baseline
        .inventory()
        .get_stockpile(baseline_output)
        .unwrap_or_else(|| panic!("frame-saw baseline output disappeared"));
    let powered_stockpile = powered
        .inventory()
        .get_stockpile(powered_output)
        .unwrap_or_else(|| panic!("sawmill output disappeared"));
    for commodity in [
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
    ] {
        assert_eq!(
            powered_stockpile.get_mass(commodity),
            baseline_stockpile.get_mass(commodity),
            "mechanization must preserve the learned frame-saw material transform"
        );
    }
    assert_eq!(
        calculate_matter_accounting(&baseline)
            .unwrap_or_else(|error| panic!("frame-saw baseline matter audit failed: {error}"))
            .total(),
        initial_matter
    );
    assert_eq!(
        calculate_matter_accounting(&powered)
            .unwrap_or_else(|error| panic!("sawmill matter audit failed: {error}"))
            .total(),
        initial_matter
    );
    validate_loaded_state(&registries, &powered)
        .unwrap_or_else(|error| panic!("sawmill project final state invalid: {error}"));
}
