//! Contract tests for equipment disassembly and recovery.

use super::*;
use crate::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_PICK,
    EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
    EQUIPMENT_STONE_SEPARATOR, FORM_FLYWHEEL, FORM_HANDLE, FORM_REINFORCEMENT, FORM_SCRAP,
    FORM_TOOL, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT, build_registries,
};
use crate::core::quantity::{Energy, Temperature};
use crate::core::state::validate_loaded_state;
use crate::core::time::WorldSeed;
use crate::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use crate::energy::{calculate_explicit_energy_accounting, validate_assemble_energy_store};
use crate::equipment::{
    EquipmentDefinitionId, apply_equipment_condition_plan, decide_equipment_wear,
    validate_assemble_equipment, validate_upgrade_equipment,
};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::{ManualPowerRequest, validate_start_manual_power};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::simulation::advance_tick;
use crate::survival::initialize_player_survival;

fn assembled_pick(registries: &Registries, state: &mut AppState) -> EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("disassembly pick source failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            registries,
            state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("disassembly pick material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, EQUIPMENT_STONE_PICK, source)
        .unwrap_or_else(|error| panic!("disassembly pick assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("disassembly pick assembly commit failed: {error}"))
}

fn assembled_authored_equipment(
    registries: &Registries,
    state: &mut AppState,
    definition: EquipmentDefinitionId,
) -> EquipmentId {
    let assembly = registries
        .equipment()
        .get_equipment(definition)
        .and_then(|record| record.assembly_profile())
        .unwrap_or_else(|| panic!("disassembly fixture equipment lost its assembly profile"));
    let mass = assembly
        .inputs()
        .iter()
        .try_fold(Mass::ZERO, |total, input| total.checked_add(input.mass()))
        .unwrap_or_else(|| panic!("disassembly fixture assembly mass overflowed"));
    let source = add_solid_stockpile_for_test(state, mass)
        .unwrap_or_else(|error| panic!("disassembly fixture assembly source failed: {error}"));
    for input in assembly.inputs() {
        deposit_lot_for_test(
            registries,
            state,
            source,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("disassembly fixture assembly material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, definition, source)
        .unwrap_or_else(|error| panic!("disassembly fixture assembly validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("disassembly fixture assembly commit failed: {error}"))
}

fn upgrade_with_reinforcement(
    registries: &Registries,
    state: &mut AppState,
    equipment: EquipmentId,
    upgraded: EquipmentDefinitionId,
) {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("disassembly reinforcement source failed: {error}"));
    deposit_lot_for_test(
        registries,
        state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("disassembly reinforcement material failed: {error}"));
    validate_upgrade_equipment(registries, state, equipment, upgraded, source)
        .unwrap_or_else(|error| panic!("disassembly reinforcement validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("disassembly reinforcement commit failed: {error}"));
}

#[test]
fn worn_pick_copper_scrap_can_be_reworked_into_a_second_pick_upgrade() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xD15A_0005));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("scrap-loop survival setup failed: {error}"));
    let first = assembled_pick(&registries, &mut state);
    let second = assembled_pick(&registries, &mut state);
    upgrade_pick(&registries, &mut state, first);
    let wear = decide_equipment_wear(&state, first, 1)
        .unwrap_or_else(|error| panic!("scrap-loop wear decision failed: {error}"));
    apply_equipment_condition_plan(&mut state, wear)
        .unwrap_or_else(|error| panic!("scrap-loop wear commit failed: {error}"));
    let recovery = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_020_000))
        .unwrap_or_else(|error| panic!("scrap-loop recovery stockpile failed: {error}"));
    let reinforcement = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("scrap-loop reinforcement stockpile failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("scrap-loop matter-before audit failed: {error}"))
        .total();

    let _ = validate_disassemble_equipment(&registries, &state, first, recovery)
        .unwrap_or_else(|error| panic!("scrap-loop disassembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("scrap-loop disassembly commit failed: {error}"));
    assert_eq!(
        state.inventory().get_stockpile(recovery).map(|stockpile| {
            stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP))
        }),
        Some(Mass::from_milligrams(20_000))
    );

    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
            recovery,
            reinforcement,
        ),
    )
    .unwrap_or_else(|error| panic!("scrap-loop rework validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("scrap-loop rework commit failed: {error}"));
    while state.production().get_job(job).is_some() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("scrap-loop rework tick failed: {error}"));
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(reinforcement)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))
            }),
        Some(Mass::from_milligrams(20_000))
    );

    validate_upgrade_equipment(
        &registries,
        &state,
        second,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        reinforcement,
    )
    .unwrap_or_else(|error| panic!("scrap-loop second upgrade validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("scrap-loop second upgrade commit failed: {error}"));
    assert_eq!(
        state
            .equipment()
            .get_equipment(second)
            .map(|record| record.definition()),
        Some(EQUIPMENT_COPPER_REINFORCED_PICK)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("scrap-loop matter-after audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn worn_reinforced_processing_machines_return_copper_to_the_scrap_recovery_loop() {
    let registries = build_registries();
    for (seed, base, upgraded, total_mass, stone_mass, wood_mass) in [
        (
            0xD15A_1001,
            EQUIPMENT_STONE_CRUSHER,
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            Mass::from_milligrams(2_020_000),
            Mass::from_milligrams(1_600_000),
            Mass::from_milligrams(400_000),
        ),
        (
            0xD15A_1002,
            EQUIPMENT_STONE_SEPARATOR,
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            Mass::from_milligrams(1_220_000),
            Mass::from_milligrams(800_000),
            Mass::from_milligrams(400_000),
        ),
    ] {
        let mut state = AppState::new(WorldSeed::new(seed));
        let equipment = assembled_authored_equipment(&registries, &mut state, base);
        upgrade_with_reinforcement(&registries, &mut state, equipment, upgraded);
        let wear = decide_equipment_wear(&state, equipment, 1)
            .unwrap_or_else(|error| panic!("processing disassembly wear decision failed: {error}"));
        apply_equipment_condition_plan(&mut state, wear)
            .unwrap_or_else(|error| panic!("processing disassembly wear commit failed: {error}"));
        let destination = add_solid_stockpile_for_test(&mut state, total_mass)
            .unwrap_or_else(|error| panic!("processing disassembly destination failed: {error}"));
        let matter_before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("processing disassembly matter-before failed: {error}"))
            .total();

        let outcome = validate_disassemble_equipment(&registries, &state, equipment, destination)
            .unwrap_or_else(|error| panic!("processing disassembly validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("processing disassembly commit failed: {error}"));

        assert!(state.equipment().get_equipment(equipment).is_none());
        assert_eq!(outcome.recovered_lots().len(), 3);
        let recovered = outcome
            .recovered_lots()
            .iter()
            .map(|lot| {
                let lot = state
                    .inventory()
                    .get_lot(*lot)
                    .unwrap_or_else(|| panic!("processing recovery lot disappeared"));
                (lot.commodity(), lot.mass())
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            recovered,
            std::collections::BTreeMap::from([
                (CommodityKey::new(MATERIAL_STONE, FORM_SCRAP), stone_mass),
                (CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP), wood_mass),
                (
                    CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
                    Mass::from_milligrams(20_000),
                ),
            ])
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!(
                    "processing disassembly matter-after failed: {error}"
                ))
                .total(),
            matter_before
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
}

fn upgrade_pick(registries: &Registries, state: &mut AppState, pick: EquipmentId) {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("disassembly upgrade source failed: {error}"));
    deposit_lot_for_test(
        registries,
        state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("disassembly upgrade reinforcement failed: {error}"));
    validate_upgrade_equipment(
        registries,
        state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        source,
    )
    .unwrap_or_else(|error| panic!("disassembly upgrade validation failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("disassembly upgrade commit failed: {error}"));
}

fn explicit_energy(registries: &Registries, state: &AppState) -> crate::energy::PreciseEnergy {
    calculate_explicit_energy_accounting(registries, state)
        .unwrap_or_else(|error| panic!("disassembly explicit energy accounting failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("disassembly explicit energy total overflowed"))
}

#[test]
fn worn_upgraded_equipment_recovers_every_embodied_material_as_scrap() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xD15A_0004));
    let pick = assembled_pick(&registries, &mut state);
    upgrade_pick(&registries, &mut state, pick);
    let wear = decide_equipment_wear(&state, pick, 1)
        .unwrap_or_else(|error| panic!("upgraded disassembly wear decision failed: {error}"));
    apply_equipment_condition_plan(&mut state, wear)
        .unwrap_or_else(|error| panic!("upgraded disassembly wear commit failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_020_000))
        .unwrap_or_else(|error| panic!("upgraded disassembly destination failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("upgraded disassembly matter before failed: {error}"))
        .total();
    let energy_before = explicit_energy(&registries, &state);

    let outcome = validate_disassemble_equipment(&registries, &state, pick, destination)
        .unwrap_or_else(|error| panic!("upgraded disassembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("upgraded disassembly commit failed: {error}"));

    assert!(state.equipment().get_equipment(pick).is_none());
    assert_eq!(outcome.recovered_lots().len(), 3);
    let recovered = outcome
        .recovered_lots()
        .iter()
        .map(|lot| {
            let lot = state
                .inventory()
                .get_lot(*lot)
                .unwrap_or_else(|| panic!("upgraded recovery lot disappeared"));
            (lot.commodity(), lot.mass())
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        recovered,
        std::collections::BTreeMap::from([
            (
                CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
                Mass::from_milligrams(800_000),
            ),
            (
                CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP),
                Mass::from_milligrams(200_000),
            ),
            (
                CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
                Mass::from_milligrams(20_000),
            ),
        ])
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(1_020_000))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("upgraded disassembly matter after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(explicit_energy(&registries, &state), energy_before);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("upgraded disassembly state audit failed: {error}"));
}

fn assembled_crank(registries: &Registries, state: &mut AppState) -> EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("disassembly crank source failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            registries,
            state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("disassembly crank material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, EQUIPMENT_STONE_HAND_CRANK, source)
        .unwrap_or_else(|error| panic!("disassembly crank assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("disassembly crank assembly commit failed: {error}"))
}

fn assembled_store(registries: &Registries, state: &mut AppState) -> crate::energy::EnergyStoreId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("disassembly store source failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            registries,
            state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("disassembly store material failed: {error}"));
    }
    validate_assemble_energy_store(registries, state, ENERGY_STONE_FLYWHEEL_DRIVE, source)
        .unwrap_or_else(|error| panic!("disassembly store assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("disassembly store assembly commit failed: {error}"))
}

#[test]
fn pristine_disassembly_recovers_exact_matter_without_reusing_identity() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xD15A_0001));
    let pick = assembled_pick(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("disassembly destination failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("disassembly matter before failed: {error}"))
        .total();
    let energy_before = explicit_energy(&registries, &state);

    let outcome = validate_disassemble_equipment(&registries, &state, pick, destination)
        .unwrap_or_else(|error| panic!("disassembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("disassembly commit failed: {error}"));
    assert_eq!(outcome.recovered_lots().len(), 2);
    assert!(state.equipment().get_equipment(pick).is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(1_000_000))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("disassembly matter after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(explicit_energy(&registries, &state), energy_before);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("disassembly state audit failed: {error}"));

    let replacement = assembled_pick(&registries, &mut state);
    assert!(
        replacement > pick,
        "equipment IDs must remain monotonic after disassembly"
    );
}

#[test]
fn worn_equipment_recovers_as_same_material_scrap_without_resetting_components() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xD15A_0002));
    let pick = assembled_pick(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("worn disassembly destination failed: {error}"));
    let wear = decide_equipment_wear(&state, pick, 1)
        .unwrap_or_else(|error| panic!("worn disassembly wear decision failed: {error}"));
    apply_equipment_condition_plan(&mut state, wear)
        .unwrap_or_else(|error| panic!("worn disassembly wear commit failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("worn disassembly matter before failed: {error}"))
        .total();
    let energy_before = explicit_energy(&registries, &state);

    let outcome = validate_disassemble_equipment(&registries, &state, pick, destination)
        .unwrap_or_else(|error| panic!("worn disassembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("worn disassembly commit failed: {error}"));
    assert!(state.equipment().get_equipment(pick).is_none());
    let recovered = outcome
        .recovered_lots()
        .iter()
        .map(|lot| {
            state
                .inventory()
                .get_lot(*lot)
                .unwrap_or_else(|| panic!("worn recovery lot disappeared"))
                .commodity()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        recovered,
        std::collections::BTreeSet::from([
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP),
        ])
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("worn disassembly matter after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(explicit_energy(&registries, &state), energy_before);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("worn disassembly state audit failed: {error}"));
}

#[test]
fn manual_power_start_invalidates_prior_pristine_disassembly_without_equipment_revision_change() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xD15A_0003));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("disassembly race survival setup failed: {error}"));
    let crank = assembled_crank(&registries, &mut state);
    let store = assembled_store(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("disassembly race destination failed: {error}"));
    let token = validate_disassemble_equipment(&registries, &state, crank, destination)
        .unwrap_or_else(|error| panic!("disassembly race validation failed: {error}"));
    let equipment_revision = state.equipment().revision();
    validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            store,
            Energy::from_nanojoules(1_000_000_000),
        ),
    )
    .unwrap_or_else(|error| panic!("disassembly race manual-power validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("disassembly race manual-power commit failed: {error}"));
    assert_eq!(
        state.equipment().revision(),
        equipment_revision,
        "manual-power admission should reserve the crank without front-loading wear"
    );

    assert_eq!(
        token.commit(&mut state),
        Err(EquipmentDisassemblyCommitError::EquipmentBusyManualPower { equipment: crank })
    );
    assert!(state.equipment().get_equipment(crank).is_some());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
}
