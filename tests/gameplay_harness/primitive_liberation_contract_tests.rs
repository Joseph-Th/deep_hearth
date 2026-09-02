//! Primitive ore-liberation content contracts.

use deep_hearth::content::gameplay_fixture::{seed_composed_lot, seed_lot};
use deep_hearth::content::{
    ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
    EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN, EQUIPMENT_STONE_CRUSHER,
    EQUIPMENT_STONE_ROTARY_QUERN, EQUIPMENT_STONE_SEPARATOR, EQUIPMENT_TIMBER_TREADLE_DRIVE,
    FORM_BOARD, FORM_REINFORCEMENT, FORM_SCRAP, FORM_SCREEN_PLATE, MANUAL_POWER_FOOT_TREADLE,
    MATERIAL_COPPER, MATERIAL_WOOD, PROCESS_CONCENTRATE_COPPER, PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
    PROCESS_GRIND_CRUSHED_ORE, PROCESS_PIERCE_COPPER_SCREEN_PLATE, PROCESS_SCREEN_CRUSHED_ORE,
    build_registries,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use deep_hearth::energy::{EnergyStoreId, validate_assemble_energy_store};
use deep_hearth::equipment::{EquipmentId, validate_assemble_equipment};
use deep_hearth::inventory::MaterialLotSelection;
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::ore_processing::{
    ComminutionRequest, ConstituentSeparationProcessDefinition, ConstituentSeparationRequest,
    ScreeningProcessDefinition, ScreeningRequest, resolve_comminution_process,
    resolve_constituent_separation_process, resolve_screening_process,
};
use deep_hearth::production::{
    ProcessOutputRoute, validate_start_process, validate_start_process_routed,
};
use deep_hearth::survival::initialize_player_survival;

use super::catalog::{ProcessResolverKind, process_catalog_entries};
use super::environment::ROOM_TEMPERATURE;
use super::inventory_support::add_solid_stockpile;
use super::manual_power_timing::finish_manual_power_work;
use super::ore_fixture::copper_ore_composition;
use super::production_timing::finish_uninterrupted_production_job;

fn assemble_equipment_from_authored_parts(
    registries: &deep_hearth::registry::Registries,
    state: &mut AppState,
    definition: deep_hearth::equipment::EquipmentDefinitionId,
) -> EquipmentId {
    let (mass, inputs) = registries
        .equipment()
        .get_equipment(definition)
        .and_then(|equipment| equipment.assembly_profile())
        .map(|profile| (profile.input_mass(), profile.inputs().to_vec()))
        .unwrap_or_else(|| panic!("primitive liberation equipment lost authored assembly"));
    let source = add_solid_stockpile(state, mass);
    for input in inputs {
        seed_lot(
            registries,
            state,
            source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    validate_assemble_equipment(registries, state, definition, source)
        .unwrap_or_else(|error| panic!("primitive liberation equipment assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive liberation equipment commit failed: {error}"))
}

fn assemble_energy_store_from_authored_parts(
    registries: &deep_hearth::registry::Registries,
    state: &mut AppState,
    definition: deep_hearth::energy::EnergyStoreDefinitionId,
) -> EnergyStoreId {
    let (mass, inputs) = registries
        .energy()
        .get_store(definition)
        .and_then(|store| store.assembly_profile())
        .map(|profile| (profile.input_mass(), profile.inputs().to_vec()))
        .unwrap_or_else(|| panic!("primitive liberation drive lost authored assembly"));
    let source = add_solid_stockpile(state, mass);
    for input in inputs {
        seed_lot(
            registries,
            state,
            source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    validate_assemble_energy_store(registries, state, definition, source)
        .unwrap_or_else(|error| panic!("primitive liberation drive assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive liberation drive commit failed: {error}"))
}

fn full_stockpile_selection(
    state: &AppState,
    stockpile: deep_hearth::inventory::StockpileId,
) -> Vec<MaterialLotSelection> {
    state
        .inventory()
        .lot_ids(stockpile)
        .map(|lot| {
            let mass = state
                .inventory()
                .get_lot(lot)
                .unwrap_or_else(|| panic!("full-selection lot disappeared"))
                .mass();
            MaterialLotSelection::new(lot, mass)
        })
        .collect()
}

#[test]
fn primitive_liberation_content_executes_the_existing_ore_chain() {
    let registries = build_registries();
    let batch_mass = Mass::from_milligrams(100_000);
    let mut state = AppState::new(WorldSeed::new(0x51A2_1B3A_7100_0001));
    let ore = add_solid_stockpile(&mut state, batch_mass);
    let crushed = add_solid_stockpile(&mut state, batch_mass);
    let ground = add_solid_stockpile(&mut state, batch_mass);
    let undersize = add_solid_stockpile(&mut state, batch_mass);
    let oversize = add_solid_stockpile(&mut state, batch_mass);
    let concentrate = add_solid_stockpile(&mut state, batch_mass);
    let tailings = add_solid_stockpile(&mut state, batch_mass);
    let ore_lot = seed_composed_lot(
        &registries,
        &mut state,
        ore,
        CommodityKey::new(MATERIAL_COPPER, deep_hearth::content::FORM_ORE),
        batch_mass,
        ROOM_TEMPERATURE,
        copper_ore_composition(400_000, 300_000),
    );
    let crusher =
        assemble_equipment_from_authored_parts(&registries, &mut state, EQUIPMENT_STONE_CRUSHER);
    let quern = assemble_equipment_from_authored_parts(
        &registries,
        &mut state,
        EQUIPMENT_STONE_ROTARY_QUERN,
    );
    let screen_assembly = add_solid_stockpile(&mut state, Mass::from_milligrams(1_620_000));
    seed_lot(
        &registries,
        &mut state,
        screen_assembly,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        Mass::from_milligrams(1_600_000),
        ROOM_TEMPERATURE,
    );
    let screen_plate_source = add_solid_stockpile(&mut state, Mass::from_milligrams(20_000));
    let screen_plate_input = seed_lot(
        &registries,
        &mut state,
        screen_plate_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(20_000),
        ROOM_TEMPERATURE,
    );
    let separator =
        assemble_equipment_from_authored_parts(&registries, &mut state, EQUIPMENT_STONE_SEPARATOR);
    let treadle = assemble_equipment_from_authored_parts(
        &registries,
        &mut state,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
    );
    let drive = assemble_energy_store_from_authored_parts(
        &registries,
        &mut state,
        ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
    );
    let drive_capacity = registries
        .energy()
        .get_store(ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("paired primitive drive disappeared"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("primitive liberation matter setup failed: {error}"))
        .total();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("primitive liberation survival setup failed: {error}"));
    let plate_job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_PIERCE_COPPER_SCREEN_PLATE,
            screen_plate_source,
            MaterialLotSelection::new(screen_plate_input, Mass::from_milligrams(20_000)),
            screen_assembly,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive sizing-plate craft failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive sizing-plate craft commit failed: {error}"));
    let plate_duration = state
        .production()
        .get_job(plate_job)
        .map(|job| job.active_duration())
        .unwrap_or_else(|| panic!("primitive sizing-plate craft disappeared after start"));
    finish_uninterrupted_production_job(
        &registries,
        &mut state,
        plate_job,
        plate_duration,
        "primitive copper sizing plate",
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(screen_assembly)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP))
            }),
        Some(Mass::from_milligrams(2_000)),
        "piercing the sizing plate must retain its copper offcut as reworkable scrap"
    );
    let screen = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
        screen_assembly,
    )
    .unwrap_or_else(|error| panic!("primitive sizing-screen assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive sizing-screen assembly commit failed: {error}"));

    let charge = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_FOOT_TREADLE, treadle, drive, drive_capacity),
    )
    .unwrap_or_else(|error| panic!("primitive liberation treadle charge failed: {error}"));
    let charge_work = charge.work();
    charge.commit(&mut state).unwrap_or_else(|error| {
        panic!("primitive liberation treadle charge commit failed: {error}")
    });
    finish_manual_power_work(
        &registries,
        &mut state,
        charge_work,
        "primitive liberation treadle charge",
    );
    assert!(
        state
            .energy()
            .get_store(drive)
            .is_some_and(|record| record.stored() >= Energy::from_nanojoules(700_000_000_000)),
        "ordinary primitive charging must fund the bounded liberation batch"
    );
    let crush = resolve_comminution_process(
        &registries,
        &state,
        ComminutionRequest::new(
            deep_hearth::content::PROCESS_CRUSH_ORE,
            ore,
            &[MaterialLotSelection::new(ore_lot, batch_mass)],
            crusher,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive liberation crushing failed: {error}"));
    let crush_job = validate_start_process(
        &registries,
        &state,
        crush.process_resolution(),
        ore,
        crushed,
    )
    .unwrap_or_else(|error| panic!("primitive liberation crushing start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive liberation crushing commit failed: {error}"));
    finish_uninterrupted_production_job(
        &registries,
        &mut state,
        crush_job,
        crush.process_resolution().duration(),
        "primitive liberation crushing",
    );

    let ground_feed = full_stockpile_selection(&state, crushed);
    let grind = resolve_comminution_process(
        &registries,
        &state,
        ComminutionRequest::new(
            PROCESS_GRIND_CRUSHED_ORE,
            crushed,
            &ground_feed,
            quern,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive rotary-quern grinding failed: {error}"));
    let grind_job = validate_start_process(
        &registries,
        &state,
        grind.process_resolution(),
        crushed,
        ground,
    )
    .unwrap_or_else(|error| panic!("primitive rotary-quern start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive rotary-quern commit failed: {error}"));
    finish_uninterrupted_production_job(
        &registries,
        &mut state,
        grind_job,
        grind.process_resolution().duration(),
        "primitive rotary-quern grinding",
    );

    let screen_feed = full_stockpile_selection(&state, ground);
    let screened = resolve_screening_process(
        &registries,
        &state,
        ScreeningRequest::new(
            PROCESS_SCREEN_CRUSHED_ORE,
            ground,
            &screen_feed,
            screen,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive copper sizing screen failed: {error}"));
    let screen_job = validate_start_process_routed(
        &registries,
        &state,
        screened.process_resolution(),
        ground,
        &[
            ProcessOutputRoute::new(ScreeningProcessDefinition::UNDERSIZE_STREAM, undersize),
            ProcessOutputRoute::new(ScreeningProcessDefinition::OVERSIZE_STREAM, oversize),
        ],
    )
    .unwrap_or_else(|error| panic!("primitive copper sizing-screen start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive copper sizing-screen commit failed: {error}"));
    finish_uninterrupted_production_job(
        &registries,
        &mut state,
        screen_job,
        screened.process_resolution().duration(),
        "primitive copper sizing screen",
    );

    let oversize_mass = state
        .inventory()
        .get_stockpile(oversize)
        .map(|record| record.stored_mass())
        .unwrap_or_else(|| panic!("primitive oversize stockpile disappeared"));
    assert!(!oversize_mass.is_zero());
    let regrind_feed = full_stockpile_selection(&state, oversize);
    let regrind = resolve_comminution_process(
        &registries,
        &state,
        ComminutionRequest::new(
            PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
            oversize,
            &regrind_feed,
            quern,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive rotary-quern regrinding failed: {error}"));
    let regrind_job = validate_start_process(
        &registries,
        &state,
        regrind.process_resolution(),
        oversize,
        undersize,
    )
    .unwrap_or_else(|error| panic!("primitive rotary-quern regrind start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive rotary-quern regrind commit failed: {error}"));
    finish_uninterrupted_production_job(
        &registries,
        &mut state,
        regrind_job,
        regrind.process_resolution().duration(),
        "primitive rotary-quern regrinding",
    );

    let concentration_feed = full_stockpile_selection(&state, undersize);
    let separated = resolve_constituent_separation_process(
        &registries,
        &state,
        ConstituentSeparationRequest::new(
            PROCESS_CONCENTRATE_COPPER,
            undersize,
            &concentration_feed,
            separator,
            drive,
        ),
    )
    .unwrap_or_else(|error| panic!("primitive concentration failed: {error}"));
    let separation_job = validate_start_process_routed(
        &registries,
        &state,
        separated.process_resolution(),
        undersize,
        &[
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::TARGET_STREAM,
                concentrate,
            ),
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                tailings,
            ),
        ],
    )
    .unwrap_or_else(|error| panic!("primitive concentration start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("primitive concentration commit failed: {error}"));
    finish_uninterrupted_production_job(
        &registries,
        &mut state,
        separation_job,
        separated.process_resolution().duration(),
        "primitive concentration",
    );

    let concentrate_record = state
        .inventory()
        .lot_ids(concentrate)
        .next()
        .and_then(|lot| state.inventory().get_lot(lot))
        .unwrap_or_else(|| panic!("primitive concentration produced no concentrate"));
    assert!(
        concentrate_record
            .composition()
            .parts_per_million(MATERIAL_COPPER)
            > 400_000
    );
    assert!(
        state
            .inventory()
            .get_stockpile(tailings)
            .is_some_and(|record| !record.stored_mass().is_zero())
    );
    assert!(
        state
            .energy()
            .get_store(drive)
            .is_some_and(|record| record.stored() < drive_capacity)
    );
    for equipment in [crusher, quern, screen, separator] {
        assert!(
            state
                .equipment()
                .get_equipment(equipment)
                .is_some_and(|record| record.condition() < Condition::PRISTINE),
            "every primitive liberation machine must incur real operation wear"
        );
    }
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("primitive liberation matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("primitive liberation trusted-state audit failed: {error}"));
}

#[test]
fn primitive_liberation_content_closes_the_pre_smelting_processing_gap() {
    let registries = build_registries();
    let plate = registries
        .crafting()
        .get_manual(PROCESS_PIERCE_COPPER_SCREEN_PLATE)
        .unwrap_or_else(|| panic!("copper sizing-plate craft disappeared"));
    assert_eq!(
        plate.input(),
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)
    );
    assert_eq!(plate.input_mass().milligrams(), 20_000);
    assert_eq!(
        plate
            .outputs()
            .iter()
            .map(|output| output.mass())
            .try_fold(deep_hearth::core::quantity::Mass::ZERO, |total, mass| total
                .checked_add(mass)),
        Some(plate.input_mass()),
        "sizing-plate piercing must conserve copper between the plate and offcut scrap"
    );
    assert!(plate.outputs().iter().any(|output| {
        output.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE)
            && output.mass().milligrams() == 18_000
    }));
    assert!(plate.outputs().iter().any(|output| {
        output.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP)
            && output.mass().milligrams() == 2_000
    }));

    let screen = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_PLATE_SIZING_SCREEN)
        .unwrap_or_else(|| panic!("primitive sizing screen disappeared"));
    assert!(screen.assembly_profile().is_some_and(|assembly| {
        assembly.inputs().iter().any(|input| {
            input.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE)
                && input.mass().milligrams() == 18_000
        })
    }));
    let quern = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_ROTARY_QUERN)
        .unwrap_or_else(|| panic!("stone rotary quern disappeared"));
    let reinforced = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN)
        .unwrap_or_else(|| panic!("reinforced stone rotary quern disappeared"));
    assert!(quern.assembly_profile().is_some());
    assert_eq!(
        reinforced.upgrade_profile().map(|upgrade| upgrade.from()),
        Some(EQUIPMENT_STONE_ROTARY_QUERN)
    );

    let catalog = process_catalog_entries(&registries);
    for process in [
        PROCESS_GRIND_CRUSHED_ORE,
        PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
        PROCESS_SCREEN_CRUSHED_ORE,
        PROCESS_CONCENTRATE_COPPER,
    ] {
        let entry = catalog
            .iter()
            .find(|entry| entry.process == process)
            .unwrap_or_else(|| panic!("primitive liberation process disappeared from catalog"));
        assert!(
            entry.nominal_provider_count >= 2,
            "process {} must have both primitive and later machinery available through canonical capability discovery",
            process.value()
        );
        assert!(entry.compatible_energy_store_count > 0);
        assert!(!matches!(
            entry.resolver,
            ProcessResolverKind::ManualCraft
                | ProcessResolverKind::ManualComminution
                | ProcessResolverKind::ManualSeparation
        ));
    }
}
