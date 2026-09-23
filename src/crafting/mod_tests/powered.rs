//! End-to-end contracts for settlement machines that trade stored work for player attention.

use super::*;
use crate::content::{
    ENERGY_MECHANICAL_SMALL_DRIVE, EQUIPMENT_TIMBER_HELVE_HAMMER, EQUIPMENT_TIMBER_SASH_SAWMILL,
    FORM_BOARD, FORM_CHIP, FORM_HANDLE, FORM_LOG, FORM_NATIVE_METAL, FORM_REINFORCEMENT,
    FORM_SAW_BLADE, FORM_SCRAP, FORM_TOOL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT, PROCESS_POWER_HAMMER_COPPER_SAW_BLADE,
    PROCESS_POWER_HAMMER_COPPER_SCRAP_REINFORCEMENT, PROCESS_POWER_SAW_WOOD_BOARDS,
    build_registries,
};
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::energy::add_energy_store_with_initial_for_fixture;
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
