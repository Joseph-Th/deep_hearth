//! End-to-end contracts for settlement machines that trade stored work for player attention.

use super::*;
use crate::content::{
    ENERGY_MECHANICAL_SMALL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE,
    EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH, EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
    EQUIPMENT_TIMBER_HELVE_HAMMER, EQUIPMENT_TIMBER_SASH_SAWMILL, EQUIPMENT_TIMBER_SPINDLE_DRILL,
    FORM_BOARD, FORM_CHIP, FORM_DRILL_BIT, FORM_FLYWHEEL, FORM_GRINDSTONE_WHEEL, FORM_HANDLE,
    FORM_LOG, FORM_NATIVE_METAL, FORM_REINFORCEMENT, FORM_SAW_BLADE, FORM_SCRAP, FORM_SCREEN_PLATE,
    FORM_TOOL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE, PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
    PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT, PROCESS_POWER_HAMMER_COPPER_SAW_BLADE,
    PROCESS_POWER_HAMMER_COPPER_SCRAP_REINFORCEMENT, PROCESS_POWER_SAW_WOOD_BOARDS,
    PROCESS_POWER_TURN_TIMBER_FLYWHEEL, build_registries,
};
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::energy::{
    EnergySupplyError, add_energy_store, add_energy_store_with_initial_for_fixture,
};
use crate::equipment::validate_assemble_equipment;
use crate::inventory::{
    MaterialLotSelection, StockpileId, add_solid_stockpile_for_test, deposit_lot_for_test,
};
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;

const ROOM_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn stockpile(state: &mut AppState, capacity_mg: u64) -> StockpileId {
    add_solid_stockpile_for_test(state, Mass::from_milligrams(capacity_mg))
        .unwrap_or_else(|error| panic!("powered craft stockpile fixture failed: {error}"))
}

#[test]
fn spindle_drill_preserves_screen_plate_yield_while_spending_stored_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    let assembly = stockpile(&mut state, 4_000_000);
    for (commodity, mass) in [
        (CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL), 900_000),
        (CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT), 100_000),
        (CommodityKey::new(MATERIAL_WOOD, FORM_BOARD), 1_600_000),
        (CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE), 800_000),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            20_000,
        ),
    ] {
        deposit(&registries, &mut state, assembly, commodity, mass);
    }
    let drill = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_SPINDLE_DRILL,
        assembly,
    )
    .unwrap_or_else(|error| panic!("spindle drill assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("spindle drill assembly commit failed: {error}"));

    let source = stockpile(&mut state, 20_000);
    let reinforcement = deposit(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        20_000,
    );
    let destination = stockpile(&mut state, 20_000);
    let drive = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        Energy::from_nanojoules(100_000_000_000),
    )
    .unwrap_or_else(|error| panic!("spindle drill drive fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("spindle drill matter setup failed: {error}"))
        .total();

    let job = validate_start_powered_craft(
        &registries,
        &state,
        PoweredCraftRequest::single(
            PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
            source,
            MaterialLotSelection::new(reinforcement, Mass::from_milligrams(20_000)),
            drill,
            drive,
        ),
        destination,
    )
    .unwrap_or_else(|error| panic!("powered screen-plate drilling failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("powered screen-plate drilling commit failed: {error}"));

    let record = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("powered screen-plate drilling job disappeared"));
    assert_eq!(record.active_duration().value(), 4);
    assert_eq!(
        record.consumed_energy().map(|trace| trace.energy()),
        Some(Energy::from_nanojoules(50_000_000_000))
    );
    assert_eq!(state.player_work().active(), None);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("in-flight spindle drill state failed replay: {error}"));

    finish_job(&registries, &mut state, job);
    let output = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("powered screen-plate output disappeared"));
    assert_eq!(
        output.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE)),
        Mass::from_milligrams(18_000)
    );
    assert_eq!(
        output.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP)),
        Mass::from_milligrams(2_000)
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(drill)
            .map(|record| record.condition()),
        Some(Condition::new(999_000).unwrap_or_else(|error| panic!("condition failed: {error}")))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("spindle drill matter audit failed: {error}"))
            .total(),
        matter_before
    );
}

#[test]
fn helve_hammer_executes_scrap_rework_and_saw_blade_routes() {
    let registries = build_registries();
    let mut state = AppState::new();
    let assembly = stockpile(&mut state, 7_000_000);
    for (commodity, mass) in [
        (CommodityKey::new(MATERIAL_WOOD, FORM_BOARD), 4_000_000),
        (CommodityKey::new(MATERIAL_STONE, FORM_TOOL), 800_000),
        (CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE), 800_000),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            20_000,
        ),
    ] {
        deposit(&registries, &mut state, assembly, commodity, mass);
    }
    let hammer =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_TIMBER_HELVE_HAMMER, assembly)
            .unwrap_or_else(|error| panic!("helve hammer assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("helve hammer assembly commit failed: {error}"));
    let drive = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        Energy::from_nanojoules(1_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("helve hammer drive fixture failed: {error}"));
    let scrap_source = stockpile(&mut state, 20_000);
    let scrap = deposit(
        &registries,
        &mut state,
        scrap_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
        20_000,
    );
    let reinforcement_output = stockpile(&mut state, 20_000);
    let blade_source = stockpile(&mut state, 60_000);
    let native_copper = deposit(
        &registries,
        &mut state,
        blade_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        60_000,
    );
    let blade_output = stockpile(&mut state, 60_000);
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("helve hammer matter setup failed: {error}"))
        .total();

    let scrap_job = validate_start_powered_craft(
        &registries,
        &state,
        PoweredCraftRequest::single(
            PROCESS_POWER_HAMMER_COPPER_SCRAP_REINFORCEMENT,
            scrap_source,
            MaterialLotSelection::new(scrap, Mass::from_milligrams(20_000)),
            hammer,
            drive,
        ),
        reinforcement_output,
    )
    .unwrap_or_else(|error| panic!("powered scrap reinforcement start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("powered scrap reinforcement commit failed: {error}"));
    assert_eq!(
        state
            .production()
            .get_job(scrap_job)
            .and_then(|record| record.consumed_energy())
            .map(|trace| trace.energy()),
        Some(Energy::from_nanojoules(100_000_000_000))
    );
    finish_job(&registries, &mut state, scrap_job);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(reinforcement_output)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))
            }),
        Some(Mass::from_milligrams(20_000))
    );

    let blade_job = validate_start_powered_craft(
        &registries,
        &state,
        PoweredCraftRequest::single(
            PROCESS_POWER_HAMMER_COPPER_SAW_BLADE,
            blade_source,
            MaterialLotSelection::new(native_copper, Mass::from_milligrams(60_000)),
            hammer,
            drive,
        ),
        blade_output,
    )
    .unwrap_or_else(|error| panic!("powered saw-blade start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("powered saw-blade commit failed: {error}"));
    assert_eq!(
        state
            .production()
            .get_job(blade_job)
            .and_then(|record| record.consumed_energy())
            .map(|trace| trace.energy()),
        Some(Energy::from_nanojoules(300_000_000_000))
    );
    finish_job(&registries, &mut state, blade_job);
    let output = state
        .inventory()
        .get_stockpile(blade_output)
        .unwrap_or_else(|| panic!("powered saw-blade output disappeared"));
    assert_eq!(
        output.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE)),
        Mass::from_milligrams(54_000)
    );
    assert_eq!(
        output.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP)),
        Mass::from_milligrams(6_000)
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("helve hammer matter audit failed: {error}"))
            .total(),
        matter_before
    );
}

fn deposit(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass_mg: u64,
) -> crate::inventory::MaterialLotId {
    deposit_lot_for_test(
        registries,
        state,
        stockpile,
        commodity,
        Mass::from_milligrams(mass_mg),
        ROOM_TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("powered craft material fixture failed: {error}"))
}

fn finish_job(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    job: crate::production::ProductionJobId,
) {
    while state.production().get_job(job).is_some() {
        let _ = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("powered craft completion tick failed: {error}"));
        assert_eq!(
            state.player_work().active(),
            None,
            "unattended settlement machinery must not capture player attention"
        );
    }
}

#[test]
fn sash_sawmill_preserves_frame_saw_yield_while_spending_stored_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    let assembly = stockpile(&mut state, 6_000_000);
    deposit(
        &registries,
        &mut state,
        assembly,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        4_000_000,
    );
    deposit(
        &registries,
        &mut state,
        assembly,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        800_000,
    );
    deposit(
        &registries,
        &mut state,
        assembly,
        CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
        54_000,
    );
    deposit(
        &registries,
        &mut state,
        assembly,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        20_000,
    );
    let sawmill =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_TIMBER_SASH_SAWMILL, assembly)
            .unwrap_or_else(|error| panic!("sash sawmill assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("sash sawmill assembly commit failed: {error}"));

    let source = stockpile(&mut state, 1_000_000);
    let log = deposit(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        1_000_000,
    );
    let destination = stockpile(&mut state, 1_000_000);
    let empty_drive = add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_SMALL_DRIVE)
        .unwrap_or_else(|error| panic!("empty sash sawmill drive fixture failed: {error}"));
    assert_eq!(
        project_powered_craft_work(
            &registries,
            &state,
            PROCESS_POWER_SAW_WOOD_BOARDS,
            Mass::from_milligrams(801_000_000),
            sawmill,
            empty_drive,
        ),
        Err(PoweredCraftError::EnergyCapacityExceeded {
            store: empty_drive,
            capacity: Energy::from_nanojoules(200_000_000_000_000),
            requested: Energy::from_nanojoules(200_250_000_000_000),
        })
    );
    let projection = project_powered_craft_work(
        &registries,
        &state,
        PROCESS_POWER_SAW_WOOD_BOARDS,
        Mass::from_milligrams(1_000_000),
        sawmill,
        empty_drive,
    )
    .unwrap_or_else(|error| {
        panic!("replenishable empty drive should remain projectable within capacity: {error}")
    });
    assert_eq!(
        projection.required_energy(),
        Energy::from_nanojoules(250_000_000_000)
    );
    assert_eq!(projection.duration().value(), 3);
    assert_eq!(
        projection.condition_after(),
        Condition::new(997_600).unwrap_or_else(|error| panic!("condition failed: {error}"))
    );
    assert!(matches!(
        resolve_powered_craft(
            &registries,
            &state,
            &PoweredCraftRequest::single(
                PROCESS_POWER_SAW_WOOD_BOARDS,
                source,
                MaterialLotSelection::new(log, Mass::from_milligrams(1_000_000)),
                sawmill,
                empty_drive,
            ),
        ),
        Err(PoweredCraftError::Energy(
            EnergySupplyError::InsufficientEnergy {
                store,
                available: Energy::ZERO,
                requested,
            }
        )) if store == empty_drive && requested == projection.required_energy()
    ));

    let initial_energy = Energy::from_nanojoules(1_000_000_000_000);
    let drive = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        initial_energy,
    )
    .unwrap_or_else(|error| panic!("sash sawmill drive fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("sash sawmill matter setup failed: {error}"))
        .total();

    let job = validate_start_powered_craft(
        &registries,
        &state,
        PoweredCraftRequest::single(
            PROCESS_POWER_SAW_WOOD_BOARDS,
            source,
            MaterialLotSelection::new(log, Mass::from_milligrams(1_000_000)),
            sawmill,
            drive,
        ),
        destination,
    )
    .unwrap_or_else(|error| panic!("sash sawmill start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("sash sawmill start commit failed: {error}"));

    let record = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("sash sawmill job disappeared at admission"));
    assert_eq!(record.active_duration().value(), 3);
    assert_eq!(
        record.consumed_energy().map(|trace| trace.energy()),
        Some(Energy::from_nanojoules(250_000_000_000))
    );
    assert_eq!(
        state.energy().get_store(drive).map(|store| store.stored()),
        Some(Energy::from_nanojoules(750_000_000_000))
    );
    assert_eq!(state.player_work().active(), None);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("in-flight sash sawmill state failed replay: {error}"));

    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("sash sawmill energy tamper serialization failed: {error}"));
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]["consumed_energy"]
        ["energy"] = serde_json::json!(1_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("sash sawmill energy tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::CraftingJob(
            CraftingJobValidationError::EnergyAmountMismatch {
                job,
                stored: Energy::from_nanojoules(1),
                required: Energy::from_nanojoules(250_000_000_000),
            }
        )))
    );

    finish_job(&registries, &mut state, job);
    let output = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("sash sawmill output stockpile disappeared"));
    assert_eq!(
        output.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)),
        Mass::from_milligrams(900_000)
    );
    assert_eq!(
        output.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(sawmill)
            .map(|record| record.condition()),
        Some(Condition::new(997_600).unwrap_or_else(|error| panic!("condition failed: {error}")))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("sash sawmill matter audit failed: {error}"))
            .total(),
        matter_before
    );
}

#[test]
fn flywheel_lathe_turns_a_full_timber_rotor_from_one_primitive_work_charge() {
    let registries = build_registries();
    let mut state = AppState::new();
    let assembly = stockpile(&mut state, 6_000_000);
    for (commodity, mass) in [
        (CommodityKey::new(MATERIAL_WOOD, FORM_BOARD), 3_200_000),
        (CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE), 800_000),
        (CommodityKey::new(MATERIAL_STONE, FORM_TOOL), 800_000),
        (CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL), 900_000),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            20_000,
        ),
    ] {
        deposit(&registries, &mut state, assembly, commodity, mass);
    }
    let lathe = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
        assembly,
    )
    .unwrap_or_else(|error| panic!("flywheel lathe assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("flywheel lathe assembly commit failed: {error}"));

    let source = stockpile(&mut state, 2_400_000);
    let log = deposit(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        2_400_000,
    );
    let destination = stockpile(&mut state, 2_400_000);
    let drive = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        Energy::from_nanojoules(500_000_000_000),
    )
    .unwrap_or_else(|error| panic!("lathe drive fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("lathe matter setup failed: {error}"))
        .total();

    let projection = project_powered_craft_work(
        &registries,
        &state,
        PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
        Mass::from_milligrams(2_400_000),
        lathe,
        drive,
    )
    .unwrap_or_else(|error| panic!("flywheel turning projection failed: {error}"));
    assert_eq!(
        projection.required_energy(),
        Energy::from_nanojoules(480_000_000_000)
    );
    assert_eq!(projection.duration().value(), 7);
    let first_flywheel_capacity = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("stone flywheel store definition disappeared"))
        .capacity();
    assert_eq!(
        first_flywheel_capacity,
        Energy::from_nanojoules(500_000_000_000)
    );
    assert!(projection.required_energy() <= first_flywheel_capacity);

    let job = validate_start_powered_craft(
        &registries,
        &state,
        PoweredCraftRequest::single(
            PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
            source,
            MaterialLotSelection::new(log, Mass::from_milligrams(2_400_000)),
            lathe,
            drive,
        ),
        destination,
    )
    .unwrap_or_else(|error| panic!("powered flywheel turning failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("powered flywheel turning commit failed: {error}"));

    let record = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("powered flywheel turning job disappeared"));
    assert_eq!(record.active_duration().value(), 7);
    assert_eq!(
        record.consumed_energy().map(|trace| trace.energy()),
        Some(Energy::from_nanojoules(480_000_000_000))
    );
    assert_eq!(state.player_work().active(), None);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("in-flight flywheel turning failed replay: {error}"));

    finish_job(&registries, &mut state, job);
    let output = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("flywheel lathe output disappeared"));
    assert_eq!(
        output.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_FLYWHEEL)),
        Mass::from_milligrams(2_000_000)
    );
    assert_eq!(
        output.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)),
        Mass::from_milligrams(400_000)
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("flywheel lathe matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("flywheel lathe final replay audit failed: {error}"));
}

#[test]
fn flywheel_toolroom_grindstone_recovers_service_stock_from_finite_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    let assembly = stockpile(&mut state, 6_400_000);
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_GRINDSTONE_WHEEL),
            1_400_000,
        ),
        (CommodityKey::new(MATERIAL_WOOD, FORM_BOARD), 3_200_000),
        (CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE), 800_000),
        (CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL), 900_000),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            20_000,
        ),
    ] {
        deposit(&registries, &mut state, assembly, commodity, mass);
    }
    let grindstone = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
        assembly,
    )
    .unwrap_or_else(|error| panic!("flywheel grindstone assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("flywheel grindstone assembly commit failed: {error}"));

    let source = stockpile(&mut state, 900_000);
    let scrap = deposit(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
        900_000,
    );
    let destination = stockpile(&mut state, 900_000);
    let drive = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        Energy::from_nanojoules(500_000_000_000),
    )
    .unwrap_or_else(|error| panic!("toolroom grindstone drive fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("toolroom grindstone matter setup failed: {error}"))
        .total();

    let projection = project_powered_craft_work(
        &registries,
        &state,
        PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
        Mass::from_milligrams(900_000),
        grindstone,
        drive,
    )
    .unwrap_or_else(|error| panic!("powered service-stock grinding projection failed: {error}"));
    assert_eq!(
        projection.required_energy(),
        Energy::from_nanojoules(270_000_000_000)
    );
    assert_eq!(projection.duration().value(), 7);

    let job = validate_start_powered_craft(
        &registries,
        &state,
        PoweredCraftRequest::single(
            PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
            source,
            MaterialLotSelection::new(scrap, Mass::from_milligrams(900_000)),
            grindstone,
            drive,
        ),
        destination,
    )
    .unwrap_or_else(|error| panic!("powered service-stock grinding failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("powered service-stock grinding commit failed: {error}"));
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.consumed_energy())
            .map(|trace| trace.energy()),
        Some(Energy::from_nanojoules(270_000_000_000))
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("in-flight toolroom grinding failed replay: {error}"));

    finish_job(&registries, &mut state, job);
    let output = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("toolroom grindstone output disappeared"));
    assert_eq!(
        output.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL)),
        Mass::from_milligrams(800_000)
    );
    assert_eq!(
        output.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CHIP)),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("toolroom grindstone matter audit failed: {error}"))
            .total(),
        matter_before
    );
}

#[test]
fn helve_hammer_turns_native_copper_into_reinforcement_without_player_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    let assembly = stockpile(&mut state, 7_000_000);
    for (commodity, mass) in [
        (CommodityKey::new(MATERIAL_WOOD, FORM_BOARD), 4_000_000),
        (CommodityKey::new(MATERIAL_STONE, FORM_TOOL), 800_000),
        (CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE), 800_000),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            20_000,
        ),
    ] {
        deposit(&registries, &mut state, assembly, commodity, mass);
    }
    let hammer =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_TIMBER_HELVE_HAMMER, assembly)
            .unwrap_or_else(|error| panic!("helve hammer assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("helve hammer assembly commit failed: {error}"));
    let source = stockpile(&mut state, 20_000);
    let copper = deposit(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        20_000,
    );
    let destination = stockpile(&mut state, 20_000);
    let drive = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        Energy::from_nanojoules(500_000_000_000),
    )
    .unwrap_or_else(|error| panic!("helve hammer drive fixture failed: {error}"));

    let job = validate_start_powered_craft(
        &registries,
        &state,
        PoweredCraftRequest::single(
            PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
            source,
            MaterialLotSelection::new(copper, Mass::from_milligrams(20_000)),
            hammer,
            drive,
        ),
        destination,
    )
    .unwrap_or_else(|error| panic!("helve hammer start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("helve hammer start commit failed: {error}"));
    let record = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("helve hammer job disappeared at admission"));
    assert_eq!(record.active_duration().value(), 4);
    assert_eq!(
        record.consumed_energy().map(|trace| trace.energy()),
        Some(Energy::from_nanojoules(100_000_000_000))
    );
    assert_eq!(state.player_work().active(), None);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("in-flight helve hammer state failed replay: {error}"));

    finish_job(&registries, &mut state, job);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))
            }),
        Some(Mass::from_milligrams(20_000))
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(hammer)
            .map(|record| record.condition()),
        Some(Condition::new(999_200).unwrap_or_else(|error| panic!("condition failed: {error}")))
    );
}
