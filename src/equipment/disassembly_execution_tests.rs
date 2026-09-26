//! Contract tests for equipment disassembly and recovery.

use super::*;
use crate::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_COPPER_REINFORCED_PICK,
    EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
    EQUIPMENT_STONE_SEPARATOR, EQUIPMENT_TIMBER_FRAME_SAW_BENCH, FORM_BOARD, FORM_FLYWHEEL,
    FORM_HANDLE, FORM_REINFORCEMENT, FORM_SAW_BLADE, FORM_SCRAP, FORM_TOOL,
    MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::{Energy, Temperature};
use crate::core::state::validate_loaded_state;
use crate::energy::{calculate_explicit_energy_accounting, validate_assemble_energy_store};
use crate::equipment::{
    EquipmentDefinitionId, degrade_equipment_condition_for_test, validate_assemble_equipment,
    validate_upgrade_equipment,
};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::{ManualPowerRequest, validate_start_manual_power};
use crate::logistics::validate_initialize_player_logistics;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
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

#[test]
fn disassembly_removes_equipment_location() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(2, 0, -1);
    let equipment = assembled_pick(&registries, &mut state);
    let destination =
        validate_initialize_player_logistics(&state, position, Mass::from_milligrams(2_000_000))
            .unwrap_or_else(|error| panic!("located disassembly logistics setup failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("located disassembly logistics commit failed: {error}"))
            .carried_stockpile();
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        revision,
        revision + 1,
        equipment,
        position,
    );
    assert_eq!(
        state.logistics().equipment_position(equipment),
        Some(position)
    );

    let _ = validate_disassemble_equipment(&registries, &state, equipment, destination)
        .unwrap_or_else(|error| panic!("located equipment disassembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("located equipment disassembly commit failed: {error}"));

    assert_eq!(state.logistics().equipment_position(equipment), None);
    assert!(state.equipment().get_equipment(equipment).is_none());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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
fn worn_pick_disassembly_reuses_intact_reinforcement_without_resetting_worn_head() {
    let registries = build_registries();
    let mut state = AppState::new();
    let first = assembled_pick(&registries, &mut state);
    let second = assembled_pick(&registries, &mut state);
    upgrade_pick(&registries, &mut state, first);
    degrade_equipment_condition_for_test(&mut state, first, 1);
    let recovery = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_020_000))
        .unwrap_or_else(|error| panic!("scrap-loop recovery stockpile failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("scrap-loop matter-before audit failed: {error}"))
        .total();

    let _ = validate_disassemble_equipment(&registries, &state, first, recovery)
        .unwrap_or_else(|error| panic!("scrap-loop disassembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("scrap-loop disassembly commit failed: {error}"));
    assert_eq!(
        state.inventory().get_stockpile(recovery).map(|stockpile| {
            stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))
        }),
        Some(Mass::from_milligrams(20_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(recovery)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)) }),
        Some(Mass::from_milligrams(800_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(recovery)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))),
        Some(Mass::ZERO),
        "the worn working component must not return as reusable tool stock"
    );

    validate_upgrade_equipment(
        &registries,
        &state,
        second,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        recovery,
    )
    .unwrap_or_else(|error| {
        panic!("intact-reinforcement second upgrade validation failed: {error}")
    })
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("intact-reinforcement second upgrade commit failed: {error}"));
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
fn worn_reinforced_processing_machines_preserve_unworn_components() {
    let registries = build_registries();
    for (base, upgraded, total_mass, stone_mass, wood_mass) in [
        (
            EQUIPMENT_STONE_CRUSHER,
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            Mass::from_milligrams(2_020_000),
            Mass::from_milligrams(1_600_000),
            Mass::from_milligrams(400_000),
        ),
        (
            EQUIPMENT_STONE_SEPARATOR,
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            Mass::from_milligrams(1_220_000),
            Mass::from_milligrams(800_000),
            Mass::from_milligrams(400_000),
        ),
    ] {
        let mut state = AppState::new();
        let equipment = assembled_authored_equipment(&registries, &mut state, base);
        upgrade_with_reinforcement(&registries, &mut state, equipment, upgraded);
        degrade_equipment_condition_for_test(&mut state, equipment, 1);
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
                (CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE), wood_mass),
                (
                    CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
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
fn worn_upgraded_equipment_preserves_unworn_components_and_upgrade_material() {
    let registries = build_registries();
    let mut state = AppState::new();
    let pick = assembled_pick(&registries, &mut state);
    upgrade_pick(&registries, &mut state, pick);
    degrade_equipment_condition_for_test(&mut state, pick, 1);
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
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            (
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
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
    let mut state = AppState::new();
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
fn worn_component_equipment_recovers_only_wear_component_as_spent_material() {
    let registries = build_registries();
    let mut state = AppState::new();
    let pick = assembled_pick(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("worn disassembly destination failed: {error}"));
    degrade_equipment_condition_for_test(&mut state, pick, 1);
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
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
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
fn worn_saw_disassembly_preserves_frame_and_spends_only_blade() {
    let registries = build_registries();
    let mut state = AppState::new();
    let saw =
        assembled_authored_equipment(&registries, &mut state, EQUIPMENT_TIMBER_FRAME_SAW_BENCH);
    degrade_equipment_condition_for_test(&mut state, saw, 1);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_854_000))
        .unwrap_or_else(|error| panic!("saw disassembly destination failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("saw disassembly matter-before failed: {error}"))
        .total();

    let outcome = validate_disassemble_equipment(&registries, &state, saw, destination)
        .unwrap_or_else(|error| panic!("saw disassembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("saw disassembly commit failed: {error}"));

    let recovered = outcome
        .recovered_lots()
        .iter()
        .map(|lot| {
            let lot = state
                .inventory()
                .get_lot(*lot)
                .unwrap_or_else(|| panic!("saw recovery lot disappeared"));
            (lot.commodity(), lot.mass())
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        recovered,
        std::collections::BTreeMap::from([
            (
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(1_600_000),
            ),
            (
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            (
                CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
                Mass::from_milligrams(54_000),
            ),
        ])
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE))),
        Some(Mass::ZERO),
        "worn blade must not return as reusable blade stock"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("saw disassembly matter-after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn manual_power_start_invalidates_prior_pristine_disassembly_without_equipment_revision_change() {
    let registries = build_registries();
    let mut state = AppState::new();
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
