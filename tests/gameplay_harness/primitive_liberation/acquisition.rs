//! Executed raw-material acquisition witness for the reusable primitive liberation kit.

use std::collections::BTreeMap;

use deep_hearth::content::gameplay_fixture::seed_lot;
use deep_hearth::content::{
    ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_ROTARY_QUERN,
    EQUIPMENT_STONE_SEPARATOR, EQUIPMENT_STONE_WOODWORKING_ADZE,
    EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN, EQUIPMENT_TIMBER_TREADLE_DRIVE, FORM_BOARD,
    FORM_FLYWHEEL, FORM_HANDLE, FORM_LOG, FORM_LUMP, FORM_TIMBER_RIDDLE_PANEL, FORM_TOOL,
    MATERIAL_STONE, MATERIAL_WOOD, PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_STONE_FLYWHEEL,
    PROCESS_SHAPE_TIMBER_FLYWHEEL, PROCESS_SHAPE_TIMBER_RIDDLE_PANEL, PROCESS_SHAPE_WOOD_BOARDS,
    PROCESS_SHAPE_WOOD_HANDLE,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId, validate_assemble_equipment};
use deep_hearth::material::{CommodityKey, MaterialAssemblyProfile};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;
use deep_hearth::survival::{assess_survival, initialize_player_survival};

use super::super::environment::ROOM_TEMPERATURE;
use super::super::inventory_support::add_solid_stockpile;
use super::super::manual_craft_execution::{execute_manual_craft, execute_manual_craft_batches};
use super::super::manual_craft_selection::select_manual_craft_request;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RawKitAcquisitionReview {
    pub(super) attention_ticks: u64,
    pub(super) metabolic_cost_nj: u128,
    pub(super) hydration_cost_ul: u64,
}

pub(super) struct AcquiredPrimitiveKit {
    pub(super) state: AppState,
    pub(super) crusher: EquipmentId,
    pub(super) quern: EquipmentId,
    pub(super) screen: EquipmentId,
    pub(super) separator: EquipmentId,
    pub(super) treadle: EquipmentId,
    pub(super) drive: deep_hearth::energy::EnergyStoreId,
    pub(super) review: RawKitAcquisitionReview,
}

fn add_requirement(
    requirements: &mut BTreeMap<CommodityKey, Mass>,
    commodity: CommodityKey,
    mass: Mass,
) {
    let entry = requirements.entry(commodity).or_insert(Mass::ZERO);
    *entry = entry
        .checked_add(mass)
        .unwrap_or_else(|| panic!("liberation kit component requirement overflowed"));
}

fn add_profile_requirements(
    requirements: &mut BTreeMap<CommodityKey, Mass>,
    profile: &MaterialAssemblyProfile,
) {
    for input in profile.inputs() {
        add_requirement(requirements, input.commodity(), input.mass());
    }
}

fn equipment_profile(
    registries: &Registries,
    definition: EquipmentDefinitionId,
) -> &MaterialAssemblyProfile {
    registries
        .equipment()
        .get_equipment(definition)
        .and_then(|equipment| equipment.assembly_profile())
        .unwrap_or_else(|| panic!("liberation kit equipment lost authored assembly"))
}

fn fresh_component_process(commodity: CommodityKey) -> ProcessId {
    match (commodity.material(), commodity.form()) {
        (MATERIAL_STONE, FORM_TOOL) => PROCESS_KNAP_STONE_TOOL,
        (MATERIAL_STONE, FORM_FLYWHEEL) => PROCESS_SHAPE_STONE_FLYWHEEL,
        (MATERIAL_WOOD, FORM_HANDLE) => PROCESS_SHAPE_WOOD_HANDLE,
        (MATERIAL_WOOD, FORM_BOARD) => PROCESS_SHAPE_WOOD_BOARDS,
        (MATERIAL_WOOD, FORM_FLYWHEEL) => PROCESS_SHAPE_TIMBER_FLYWHEEL,
        _ => panic!(
            "liberation kit has no fresh primitive route for component {}",
            commodity.value()
        ),
    }
}

fn batches_for_output(
    registries: &Registries,
    process: ProcessId,
    commodity: CommodityKey,
    required: Mass,
) -> u64 {
    let definition = registries
        .crafting()
        .get_manual(process)
        .unwrap_or_else(|| panic!("liberation kit manual craft disappeared"));
    let output = definition
        .outputs()
        .iter()
        .find(|output| output.commodity() == commodity)
        .unwrap_or_else(|| panic!("liberation kit craft lost requested component output"));
    required.milligrams().div_ceil(output.mass().milligrams())
}

fn add_raw_cost(
    registries: &Registries,
    raw: &mut BTreeMap<CommodityKey, Mass>,
    process: ProcessId,
    commodity: CommodityKey,
    required: Mass,
) {
    let definition = registries
        .crafting()
        .get_manual(process)
        .unwrap_or_else(|| panic!("liberation kit raw-cost craft disappeared"));
    let batches = batches_for_output(registries, process, commodity, required);
    let input = Mass::from_milligrams(
        definition
            .input_mass()
            .milligrams()
            .checked_mul(batches)
            .unwrap_or_else(|| panic!("liberation kit raw-cost mass overflowed")),
    );
    add_requirement(raw, definition.input(), input);
}

struct ComponentCraftPlan {
    raw: deep_hearth::inventory::StockpileId,
    destination: deep_hearth::inventory::StockpileId,
    commodity: CommodityKey,
    required: Mass,
    equipment: Option<EquipmentId>,
    context: &'static str,
}

fn craft_component(registries: &Registries, state: &mut AppState, plan: ComponentCraftPlan) -> u64 {
    let process = fresh_component_process(plan.commodity);
    let definition = registries
        .crafting()
        .get_manual(process)
        .unwrap_or_else(|| panic!("liberation kit component craft disappeared"));
    let batches = batches_for_output(registries, process, plan.commodity, plan.required);
    if plan.equipment.is_none() {
        return execute_manual_craft_batches(
            registries,
            state,
            process,
            plan.raw,
            plan.destination,
            batches,
            plan.context,
        )
        .value();
    }
    let request =
        select_manual_craft_request(registries, state, process, plan.raw, batches, plan.context);
    let request = match (definition.equipment_profile(), plan.equipment) {
        (Some(_), Some(equipment)) => request.with_equipment(equipment),
        _ => request,
    };
    execute_manual_craft(registries, state, request, plan.destination, plan.context).value()
}

fn craft_riddle_panel(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    panel_feed: deep_hearth::inventory::StockpileId,
    parts: deep_hearth::inventory::StockpileId,
    adze: EquipmentId,
    required_panel: Mass,
) -> u64 {
    let definition = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_TIMBER_RIDDLE_PANEL)
        .unwrap_or_else(|| panic!("liberation kit riddle-panel craft disappeared"));
    let output = definition
        .outputs()
        .iter()
        .find(|output| {
            output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL)
        })
        .unwrap_or_else(|| panic!("liberation kit riddle-panel output disappeared"));
    let panel_batches = required_panel
        .milligrams()
        .div_ceil(output.mass().milligrams());
    let board_mass = Mass::from_milligrams(
        definition
            .input_mass()
            .milligrams()
            .checked_mul(panel_batches)
            .unwrap_or_else(|| panic!("liberation kit riddle-panel feed overflowed")),
    );
    let mut ticks = craft_component(
        registries,
        state,
        ComponentCraftPlan {
            raw,
            destination: panel_feed,
            commodity: CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
            required: board_mass,
            equipment: Some(adze),
            context: "liberation kit riddle boards",
        },
    );
    let panel_request = select_manual_craft_request(
        registries,
        state,
        PROCESS_SHAPE_TIMBER_RIDDLE_PANEL,
        panel_feed,
        panel_batches,
        "liberation kit riddle panel",
    )
    .with_equipment(adze);
    ticks = ticks
        .checked_add(
            execute_manual_craft(
                registries,
                state,
                panel_request,
                parts,
                "liberation kit riddle panel",
            )
            .value(),
        )
        .unwrap_or_else(|| panic!("liberation kit riddle attention overflowed"));
    ticks
}

pub(super) fn acquire_raw_kit<T>(
    registries: &Registries,
    seed: u64,
    planned_batches: u64,
    bootstrap_before_admission: impl FnOnce(&mut AppState) -> T,
) -> (AcquiredPrimitiveKit, T) {
    assert!(
        planned_batches > 0,
        "liberation kit acquisition requires a disclosed nonzero campaign horizon"
    );
    let adze_profile = equipment_profile(registries, EQUIPMENT_STONE_WOODWORKING_ADZE);
    let equipment = [
        EQUIPMENT_STONE_CRUSHER,
        EQUIPMENT_STONE_ROTARY_QUERN,
        EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
        EQUIPMENT_STONE_SEPARATOR,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
    ];
    let mut final_requirements = BTreeMap::new();
    for definition in equipment {
        add_profile_requirements(
            &mut final_requirements,
            equipment_profile(registries, definition),
        );
    }
    let drive_profile = registries
        .energy()
        .get_store(ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE)
        .and_then(|store| store.assembly_profile())
        .unwrap_or_else(|| panic!("liberation kit drive lost authored assembly"));
    add_profile_requirements(&mut final_requirements, drive_profile);
    let panel = CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL);
    let required_panel = final_requirements
        .remove(&panel)
        .unwrap_or_else(|| panic!("liberation kit screen lost its riddle-panel input"));

    let panel_definition = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_TIMBER_RIDDLE_PANEL)
        .unwrap_or_else(|| panic!("liberation kit riddle-panel craft disappeared"));
    let panel_output = panel_definition
        .outputs()
        .iter()
        .find(|output| output.commodity() == panel)
        .unwrap_or_else(|| panic!("liberation kit riddle-panel output disappeared"));
    let panel_batches = required_panel
        .milligrams()
        .div_ceil(panel_output.mass().milligrams());
    let panel_board_mass = Mass::from_milligrams(
        panel_definition
            .input_mass()
            .milligrams()
            .checked_mul(panel_batches)
            .unwrap_or_else(|| panic!("liberation kit riddle-panel board demand overflowed")),
    );

    let mut raw_requirements = BTreeMap::new();
    for input in adze_profile.inputs() {
        add_raw_cost(
            registries,
            &mut raw_requirements,
            fresh_component_process(input.commodity()),
            input.commodity(),
            input.mass(),
        );
    }
    for (commodity, required) in &final_requirements {
        add_raw_cost(
            registries,
            &mut raw_requirements,
            fresh_component_process(*commodity),
            *commodity,
            *required,
        );
    }
    add_raw_cost(
        registries,
        &mut raw_requirements,
        PROCESS_SHAPE_WOOD_BOARDS,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        panel_board_mass,
    );
    let raw_mass = raw_requirements
        .values()
        .copied()
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("liberation kit raw mass overflowed"));
    let stone_raw = raw_requirements
        .get(&CommodityKey::new(MATERIAL_STONE, FORM_LUMP))
        .copied()
        .unwrap_or(Mass::ZERO);
    let wood_raw = raw_requirements
        .get(&CommodityKey::new(MATERIAL_WOOD, FORM_LOG))
        .copied()
        .unwrap_or(Mass::ZERO);

    let mut state = AppState::new();
    let raw = add_solid_stockpile(&mut state, raw_mass);
    for (commodity, mass) in raw_requirements {
        seed_lot(
            registries,
            &mut state,
            raw,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
    }
    let parts = add_solid_stockpile(&mut state, raw_mass);
    let panel_feed = add_solid_stockpile(&mut state, raw_mass);
    let bootstrap = bootstrap_before_admission(&mut state);
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("liberation kit survival setup failed: {error}"));
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("liberation kit player survival disappeared"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("liberation kit matter setup failed: {error}"))
        .total();
    let started_at = state.tick().value();

    for input in adze_profile.inputs() {
        craft_component(
            registries,
            &mut state,
            ComponentCraftPlan {
                raw,
                destination: parts,
                commodity: input.commodity(),
                required: input.mass(),
                equipment: None,
                context: "liberation kit adze component",
            },
        );
    }
    let adze =
        validate_assemble_equipment(registries, &state, EQUIPMENT_STONE_WOODWORKING_ADZE, parts)
            .unwrap_or_else(|error| panic!("liberation kit adze assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("liberation kit adze commit failed: {error}"));

    for (commodity, required) in final_requirements {
        craft_component(
            registries,
            &mut state,
            ComponentCraftPlan {
                raw,
                destination: parts,
                commodity,
                required,
                equipment: Some(adze),
                context: "liberation kit component",
            },
        );
    }
    craft_riddle_panel(
        registries,
        &mut state,
        raw,
        panel_feed,
        parts,
        adze,
        required_panel,
    );
    let assemble = |state: &mut AppState, definition| {
        validate_assemble_equipment(registries, state, definition, parts)
            .unwrap_or_else(|error| panic!("liberation kit equipment assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("liberation kit equipment commit failed: {error}"))
    };
    let crusher = assemble(&mut state, EQUIPMENT_STONE_CRUSHER);
    let quern = assemble(&mut state, EQUIPMENT_STONE_ROTARY_QUERN);
    let screen = assemble(&mut state, EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN);
    let separator = assemble(&mut state, EQUIPMENT_STONE_SEPARATOR);
    let treadle = assemble(&mut state, EQUIPMENT_TIMBER_TREADLE_DRIVE);
    let drive = validate_assemble_energy_store(
        registries,
        &state,
        ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
        parts,
    )
    .unwrap_or_else(|error| panic!("liberation kit drive assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("liberation kit drive commit failed: {error}"));

    assert_eq!(
        state
            .inventory()
            .get_stockpile(raw)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "liberation kit raw witness must consume its exact disclosed stone/log opportunity",
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("liberation kit final matter audit failed: {error}"))
            .total(),
        matter_before,
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("liberation kit final state invalid: {error}"));
    let survival_after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("liberation kit final survival disappeared"));
    let attention = state.tick().value() - started_at;
    let metabolic = survival_before
        .metabolic_energy()
        .checked_sub(survival_after.metabolic_energy())
        .unwrap_or_else(|| panic!("liberation kit metabolic reserve increased"));
    let hydration = survival_before
        .hydration()
        .checked_sub(survival_after.hydration())
        .unwrap_or_else(|| panic!("liberation kit hydration reserve increased"));
    reviewln!(
        "LIBERATION KIT ACQUISITION seed=0x{seed:016X} scope=raw-stone+logs->adze+reusable-base-processing-kit disclosed-campaign={}batches workload-known-before-build=true raw=[stone:{}mg wood:{}mg total:{}mg] built=[adze:true crusher:true quern:true timber-riddle:true separator:true treadle:true paired-flywheel:true] attention:{}t body={}nJ/{}uL copper-screen-upgrade=proved-by-progression-continuation matter=conserved",
        planned_batches,
        stone_raw.milligrams(),
        wood_raw.milligrams(),
        raw_mass.milligrams(),
        attention,
        metabolic.nanojoules(),
        hydration.microliters(),
    );
    let kit = AcquiredPrimitiveKit {
        state,
        crusher,
        quern,
        screen,
        separator,
        treadle,
        drive,
        review: RawKitAcquisitionReview {
            attention_ticks: attention,
            metabolic_cost_nj: metabolic.nanojoules(),
            hydration_cost_ul: hydration.microliters(),
        },
    };
    (kit, bootstrap)
}
