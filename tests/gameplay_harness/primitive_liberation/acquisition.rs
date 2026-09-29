//! Executed raw-material-to-kit fabrication witness for the reusable primitive liberation kit.
//!
//! Raw stone and logs are disclosed fixture state because ordinary world gathering and haulage have
//! no production owner. After admission they cross ordinary same-voxel pickup into finite carried
//! custody; all subsequent work uses canonical runtime boundaries.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use deep_hearth::content::gameplay_fixture::seed_lot;
use deep_hearth::content::{
    ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_ROTARY_QUERN,
    EQUIPMENT_STONE_SEPARATOR, EQUIPMENT_STONE_WOODWORKING_ADZE,
    EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN, EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
    EQUIPMENT_TIMBER_TREADLE_DRIVE, FORM_BOARD, FORM_FLYWHEEL, FORM_HANDLE, FORM_LOG, FORM_LUMP,
    FORM_TIMBER_RIDDLE_PANEL, MATERIAL_STONE, MATERIAL_WOOD,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::crafting::{project_manual_craft_equipment, project_manual_craft_hand_work};
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId, validate_assemble_equipment};
use deep_hearth::inventory::{MaterialLotSelection, StockpileId};
use deep_hearth::logistics::{
    assess_player_carrying, validate_allocate_ground_stockpile,
    validate_initialize_player_logistics, validate_pickup_from_ground,
};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::{CommodityKey, MaterialAssemblyProfile};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::survival::{assess_survival, initialize_player_survival};

use super::super::capital_investment_policy::{CapitalInvestmentPolicy, clears_attention_return};
use super::super::environment::ROOM_TEMPERATURE;
use super::super::manual_craft_batches::execute_manual_craft_batches;
use super::super::manual_craft_equipment_planning::{
    manual_craft_plan_with_equipment, manual_craft_topology_plan_with_equipment,
};
use super::super::manual_craft_execution::execute_manual_craft;
use super::super::manual_craft_planning::manual_craft_plan_for_available_output;
use super::super::manual_craft_selection::select_manual_craft_request;
use super::super::manual_craft_topology_planning::manual_craft_topology_plan_for_output_from_inputs;
use super::super::seed::mix64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RawKitAcquisitionReview {
    pub(super) attention_ticks: u64,
    pub(super) metabolic_cost_nj: u128,
    pub(super) hydration_cost_ul: u64,
}

fn nonzero_batches(batches: u64, context: &'static str) -> NonZeroU64 {
    NonZeroU64::new(batches)
        .unwrap_or_else(|| panic!("liberation kit {context} requires a nonzero batch count"))
}

fn add_attention(total: &mut u64, ticks: u64, context: &'static str) {
    *total = total
        .checked_add(ticks)
        .unwrap_or_else(|| panic!("liberation kit {context} attention overflowed"));
}

/// Projects the complete raw-source-to-kit fabrication attention from authored mechanics.
///
/// This is a pre-action planning surface: it reads immutable recipes/equipment physics only and
/// carries the future adze condition through each assisted craft. Runtime acquisition asserts the
/// projection against executed attention so the decision model cannot silently drift.
#[derive(Clone, Copy)]
struct RawKitAttentionProjection {
    total_ticks: u64,
    lathe_setup_ticks: u64,
}

fn project_raw_kit_attention_route(
    registries: &Registries,
    stage_with_lathe: bool,
) -> RawKitAttentionProjection {
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
    let board = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let (panel_definition, panel_batches) = manual_craft_topology_plan_with_equipment(
        registries,
        panel,
        required_panel,
        &[board],
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        Condition::PRISTINE,
        "liberation kit riddle-panel projection",
    );
    let panel_board_mass = Mass::from_milligrams(
        panel_definition
            .input_mass()
            .milligrams()
            .checked_mul(panel_batches)
            .unwrap_or_else(|| panic!("liberation kit riddle-panel board demand overflowed")),
    );

    let mut attention = 0_u64;
    for input in adze_profile.inputs() {
        let (definition, batches) = raw_component_plan(
            registries,
            input.commodity(),
            input.mass(),
            "liberation kit adze component projection",
        );
        let projection = project_manual_craft_hand_work(
            registries,
            definition.process(),
            nonzero_batches(batches, "adze component projection"),
        )
        .unwrap_or_else(|error| panic!("liberation kit adze projection failed: {error}"));
        add_attention(
            &mut attention,
            projection.duration().value(),
            "adze component projection",
        );
    }

    let mut adze_condition = Condition::PRISTINE;
    let mut lathe_condition = Condition::PRISTINE;
    let attention_before_lathe = attention;
    if stage_with_lathe {
        let lathe_profile = equipment_profile(registries, EQUIPMENT_TIMBER_SPRING_POLE_LATHE);
        for input in lathe_profile.inputs() {
            if input.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD) {
                let (definition, batches) = manual_craft_topology_plan_with_equipment(
                    registries,
                    input.commodity(),
                    input.mass(),
                    &disclosed_raw_inputs(),
                    EQUIPMENT_STONE_WOODWORKING_ADZE,
                    adze_condition,
                    "liberation kit spring-pole board projection",
                );
                let projection = project_manual_craft_equipment(
                    registries,
                    definition.process(),
                    nonzero_batches(batches, "spring-pole board projection"),
                    EQUIPMENT_STONE_WOODWORKING_ADZE,
                    adze_condition,
                )
                .unwrap_or_else(|error| {
                    panic!("liberation kit spring-pole board projection failed: {error}")
                });
                add_attention(
                    &mut attention,
                    projection.duration().value(),
                    "spring-pole board projection",
                );
                adze_condition = projection.condition_after();
            } else {
                let (definition, batches) = raw_component_plan(
                    registries,
                    input.commodity(),
                    input.mass(),
                    "liberation kit spring-pole component projection",
                );
                let projection = project_manual_craft_hand_work(
                    registries,
                    definition.process(),
                    nonzero_batches(batches, "spring-pole component projection"),
                )
                .unwrap_or_else(|error| {
                    panic!("liberation kit spring-pole component projection failed: {error}")
                });
                add_attention(
                    &mut attention,
                    projection.duration().value(),
                    "spring-pole component projection",
                );
            }
        }
    }
    let lathe_setup_ticks = attention - attention_before_lathe;
    for (commodity, required) in final_requirements {
        if commodity == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD) {
            let (definition, batches) = manual_craft_topology_plan_with_equipment(
                registries,
                commodity,
                required,
                &disclosed_raw_inputs(),
                EQUIPMENT_STONE_WOODWORKING_ADZE,
                adze_condition,
                "liberation kit adze-assisted component projection",
            );
            let projection = project_manual_craft_equipment(
                registries,
                definition.process(),
                nonzero_batches(batches, "adze-assisted component projection"),
                EQUIPMENT_STONE_WOODWORKING_ADZE,
                adze_condition,
            )
            .unwrap_or_else(|error| {
                panic!("liberation kit assisted component projection failed: {error}")
            });
            add_attention(
                &mut attention,
                projection.duration().value(),
                "adze-assisted component projection",
            );
            adze_condition = projection.condition_after();
        } else if stage_with_lathe
            && commodity.material() == MATERIAL_WOOD
            && matches!(commodity.form(), FORM_HANDLE | FORM_FLYWHEEL)
        {
            let (definition, batches) = manual_craft_topology_plan_with_equipment(
                registries,
                commodity,
                required,
                &disclosed_raw_inputs(),
                EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
                lathe_condition,
                "liberation kit spring-pole-assisted component projection",
            );
            let projection = project_manual_craft_equipment(
                registries,
                definition.process(),
                nonzero_batches(batches, "spring-pole-assisted component projection"),
                EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
                lathe_condition,
            )
            .unwrap_or_else(|error| {
                panic!("liberation kit spring-pole-assisted projection failed: {error}")
            });
            add_attention(
                &mut attention,
                projection.duration().value(),
                "spring-pole-assisted component projection",
            );
            lathe_condition = projection.condition_after();
        } else {
            let (definition, batches) = raw_component_plan(
                registries,
                commodity,
                required,
                "liberation kit component projection",
            );
            let projection = project_manual_craft_hand_work(
                registries,
                definition.process(),
                nonzero_batches(batches, "component projection"),
            )
            .unwrap_or_else(|error| panic!("liberation kit component projection failed: {error}"));
            add_attention(
                &mut attention,
                projection.duration().value(),
                "component projection",
            );
        }
    }

    let (board_definition, board_batches) = manual_craft_topology_plan_with_equipment(
        registries,
        board,
        panel_board_mass,
        &disclosed_raw_inputs(),
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        adze_condition,
        "liberation kit riddle-board projection",
    );
    let board_projection = project_manual_craft_equipment(
        registries,
        board_definition.process(),
        nonzero_batches(board_batches, "riddle-board projection"),
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        adze_condition,
    )
    .unwrap_or_else(|error| panic!("liberation kit riddle-board projection failed: {error}"));
    add_attention(
        &mut attention,
        board_projection.duration().value(),
        "riddle-board projection",
    );
    adze_condition = board_projection.condition_after();
    let (worn_panel_definition, worn_panel_batches) = manual_craft_topology_plan_with_equipment(
        registries,
        panel,
        required_panel,
        &[board],
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        adze_condition,
        "liberation kit worn riddle-panel projection",
    );
    assert_eq!(worn_panel_definition.process(), panel_definition.process());
    assert_eq!(worn_panel_batches, panel_batches);
    let panel_projection = project_manual_craft_equipment(
        registries,
        worn_panel_definition.process(),
        nonzero_batches(worn_panel_batches, "riddle-panel projection"),
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        adze_condition,
    )
    .unwrap_or_else(|error| panic!("liberation kit riddle-panel projection failed: {error}"));
    add_attention(
        &mut attention,
        panel_projection.duration().value(),
        "riddle-panel projection",
    );
    RawKitAttentionProjection {
        total_ticks: attention,
        lathe_setup_ticks,
    }
}

fn project_staged_raw_kit_attention(registries: &Registries) -> RawKitAttentionProjection {
    project_raw_kit_attention_route(registries, true)
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

fn disclosed_raw_inputs() -> [CommodityKey; 2] {
    [
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
    ]
}

fn raw_component_plan<'a>(
    registries: &'a Registries,
    commodity: CommodityKey,
    required: Mass,
    context: &'static str,
) -> (&'a deep_hearth::crafting::ManualCraftDefinition, u64) {
    manual_craft_topology_plan_for_output_from_inputs(
        registries,
        commodity,
        required,
        &disclosed_raw_inputs(),
        context,
    )
}

fn add_raw_cost(
    registries: &Registries,
    raw: &mut BTreeMap<CommodityKey, Mass>,
    commodity: CommodityKey,
    required: Mass,
) {
    let (definition, batches) = raw_component_plan(
        registries,
        commodity,
        required,
        "liberation kit raw-material costing",
    );
    let input = Mass::from_milligrams(
        definition
            .input_mass()
            .milligrams()
            .checked_mul(batches)
            .unwrap_or_else(|| panic!("liberation kit raw-cost mass overflowed")),
    );
    add_requirement(raw, definition.input(), input);
}

fn raw_kit_requirements(
    registries: &Registries,
    adze_profile: &MaterialAssemblyProfile,
    final_requirements: &BTreeMap<CommodityKey, Mass>,
    panel_board_mass: Mass,
    include_fabrication_lathe: bool,
) -> BTreeMap<CommodityKey, Mass> {
    let mut raw = BTreeMap::new();
    for input in adze_profile.inputs() {
        add_raw_cost(registries, &mut raw, input.commodity(), input.mass());
    }
    if include_fabrication_lathe {
        for input in equipment_profile(registries, EQUIPMENT_TIMBER_SPRING_POLE_LATHE).inputs() {
            add_raw_cost(registries, &mut raw, input.commodity(), input.mass());
        }
    }
    for (commodity, required) in final_requirements {
        add_raw_cost(registries, &mut raw, *commodity, *required);
    }
    add_raw_cost(
        registries,
        &mut raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        panel_board_mass,
    );
    raw
}

fn raw_requirement_mass(requirements: &BTreeMap<CommodityKey, Mass>) -> Mass {
    requirements
        .values()
        .copied()
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("liberation kit raw mass overflowed"))
}

fn stockpile_funds_requirements(
    state: &AppState,
    stockpile: StockpileId,
    requirements: &BTreeMap<CommodityKey, Mass>,
) -> bool {
    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .unwrap_or_else(|| panic!("liberation carried raw stockpile disappeared"));
    requirements
        .iter()
        .all(|(commodity, required)| record.get_mass(*commodity) >= *required)
}

fn staging_material_available(seed: u64) -> bool {
    !mix64(seed ^ 0x4C49_4245_5354_4147).is_multiple_of(2)
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
    if let Some(equipment) = plan.equipment {
        let (definition, batches, source) = manual_craft_plan_with_equipment(
            registries,
            state,
            &[plan.raw],
            plan.commodity,
            plan.required,
            equipment,
            plan.context,
        );
        let request = select_manual_craft_request(
            registries,
            state,
            definition.process(),
            source,
            batches,
            plan.context,
        )
        .with_equipment(equipment);
        return execute_manual_craft(registries, state, request, plan.destination, plan.context)
            .value();
    }
    let (definition, batches, source) = manual_craft_plan_for_available_output(
        registries,
        state,
        &[plan.raw],
        plan.commodity,
        plan.required,
        plan.context,
    );
    execute_manual_craft_batches(
        registries,
        state,
        definition.process(),
        source,
        plan.destination,
        batches,
        plan.context,
    )
    .value()
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
    let panel = CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL);
    let board = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let adze_record = state
        .equipment()
        .get_equipment(adze)
        .unwrap_or_else(|| panic!("liberation kit adze disappeared before panel planning"));
    let (definition, panel_batches) = manual_craft_topology_plan_with_equipment(
        registries,
        panel,
        required_panel,
        &[board],
        adze_record.definition(),
        adze_record.condition(),
        "liberation kit riddle-panel feed planning",
    );
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
            commodity: board,
            required: board_mass,
            equipment: Some(adze),
            context: "liberation kit riddle boards",
        },
    );
    let (live_panel, live_panel_batches, panel_source) = manual_craft_plan_with_equipment(
        registries,
        state,
        &[panel_feed],
        panel,
        required_panel,
        adze,
        "liberation kit riddle panel",
    );
    assert_eq!(live_panel.process(), definition.process());
    assert_eq!(live_panel_batches, panel_batches);
    let panel_request = select_manual_craft_request(
        registries,
        state,
        live_panel.process(),
        panel_source,
        live_panel_batches,
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

    let board = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let (panel_definition, panel_batches) = manual_craft_topology_plan_with_equipment(
        registries,
        panel,
        required_panel,
        &[board],
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        Condition::PRISTINE,
        "liberation kit riddle-panel raw costing",
    );
    let panel_board_mass = Mass::from_milligrams(
        panel_definition
            .input_mass()
            .milligrams()
            .checked_mul(panel_batches)
            .unwrap_or_else(|| panic!("liberation kit riddle-panel board demand overflowed")),
    );

    let direct_raw_requirements = raw_kit_requirements(
        registries,
        adze_profile,
        &final_requirements,
        panel_board_mass,
        false,
    );
    let staged_raw_requirements = raw_kit_requirements(
        registries,
        adze_profile,
        &final_requirements,
        panel_board_mass,
        true,
    );
    for (commodity, direct) in &direct_raw_requirements {
        assert!(
            staged_raw_requirements
                .get(commodity)
                .copied()
                .unwrap_or(Mass::ZERO)
                >= *direct,
            "liberation staging route must not create raw material from the direct route"
        );
    }
    let direct_raw_mass = raw_requirement_mass(&direct_raw_requirements);
    let staged_raw_mass = raw_requirement_mass(&staged_raw_requirements);
    let staging_extra_raw = staged_raw_mass
        .checked_sub(direct_raw_mass)
        .unwrap_or_else(|| unreachable!("staged liberation raw requirement includes direct kit"));
    // The controlled raw opportunity belongs to world setup, not actor policy. Every world funds
    // the direct kit; only a replayable subset also has enough local raw material to stage the
    // spring-pole lathe. This makes material scarcity a real input to the fabrication decision.
    let staging_material_available = staging_material_available(seed);
    let raw_requirements = if staging_material_available {
        staged_raw_requirements.clone()
    } else {
        direct_raw_requirements.clone()
    };
    let raw_mass = raw_requirement_mass(&raw_requirements);
    let stone_raw = raw_requirements
        .get(&CommodityKey::new(MATERIAL_STONE, FORM_LUMP))
        .copied()
        .unwrap_or(Mass::ZERO);
    let wood_raw = raw_requirements
        .get(&CommodityKey::new(MATERIAL_WOOD, FORM_LOG))
        .copied()
        .unwrap_or(Mass::ZERO);

    let mut state = AppState::new();
    let player_position = super::PRIMITIVE_LIBERATION_ORIGIN;
    let ground_raw = validate_allocate_ground_stockpile(&state, player_position, raw_mass)
        .unwrap_or_else(|error| panic!("liberation ground raw allocation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("liberation ground raw allocation commit failed: {error}"));
    let mut pickup = Vec::new();
    for (commodity, mass) in raw_requirements {
        let lot = seed_lot(
            registries,
            &mut state,
            ground_raw,
            commodity,
            mass,
            ROOM_TEMPERATURE,
        );
        pickup.push(MaterialLotSelection::new(lot, mass));
    }
    let parts = validate_allocate_ground_stockpile(&state, player_position, raw_mass)
        .unwrap_or_else(|error| panic!("liberation parts allocation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("liberation parts allocation commit failed: {error}"));
    let panel_feed = validate_allocate_ground_stockpile(&state, player_position, raw_mass)
        .unwrap_or_else(|error| panic!("liberation panel-feed allocation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("liberation panel-feed allocation commit failed: {error}"));
    let bootstrap = bootstrap_before_admission(&mut state);
    let raw = validate_initialize_player_logistics(&state, player_position, raw_mass)
        .unwrap_or_else(|error| panic!("liberation carried-custody setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("liberation carried-custody commit failed: {error}"))
        .carried_stockpile();
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("liberation kit survival setup failed: {error}"));
    let survival_before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("liberation kit player survival disappeared"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("liberation kit matter setup failed: {error}"))
        .total();
    let started_at = state.tick().value();

    validate_pickup_from_ground(registries, &state, ground_raw, &pickup)
        .unwrap_or_else(|error| panic!("liberation same-voxel raw pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("liberation same-voxel raw pickup commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ground_raw)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "liberation ground bootstrap must be emptied through canonical pickup",
    );

    assert!(
        stockpile_funds_requirements(&state, raw, &direct_raw_requirements),
        "every liberation world must fund the disclosed direct primitive kit"
    );
    let staging_funded = stockpile_funds_requirements(&state, raw, &staged_raw_requirements);
    assert_eq!(
        staging_funded, staging_material_available,
        "liberation staging availability must come from disclosed raw opportunity"
    );
    let direct_projection = project_raw_kit_attention_route(registries, false);
    let staged_projection = project_staged_raw_kit_attention(registries);
    let investment_policy = CapitalInvestmentPolicy::baseline();
    let minimum_lathe_return =
        investment_policy.minimum_attention_return(0, staged_projection.lathe_setup_ticks);
    let stage_with_lathe = staging_funded
        && clears_attention_return(
            direct_projection.total_ticks,
            staged_projection.total_ticks,
            minimum_lathe_return,
        );
    let selected_projection = if stage_with_lathe {
        staged_projection
    } else {
        direct_projection
    };

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
    let adze_ready_at = state.tick().value();

    let fabrication_lathe = if stage_with_lathe {
        let lathe_profile = equipment_profile(registries, EQUIPMENT_TIMBER_SPRING_POLE_LATHE);
        for input in lathe_profile.inputs() {
            let equipment =
                (input.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)).then_some(adze);
            craft_component(
                registries,
                &mut state,
                ComponentCraftPlan {
                    raw,
                    destination: parts,
                    commodity: input.commodity(),
                    required: input.mass(),
                    equipment,
                    context: "liberation kit spring-pole component",
                },
            );
        }
        Some(
            validate_assemble_equipment(
                registries,
                &state,
                EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
                parts,
            )
            .unwrap_or_else(|error| panic!("liberation kit spring-pole assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("liberation kit spring-pole commit failed: {error}")),
        )
    } else {
        None
    };
    let fabrication_lathe_ready_at = state.tick().value();

    for (commodity, required) in final_requirements {
        let equipment = match (commodity.material(), commodity.form()) {
            (MATERIAL_WOOD, FORM_BOARD) => Some(adze),
            (MATERIAL_WOOD, FORM_HANDLE | FORM_FLYWHEEL) => fabrication_lathe,
            _ => None,
        };
        craft_component(
            registries,
            &mut state,
            ComponentCraftPlan {
                raw,
                destination: parts,
                commodity,
                required,
                equipment,
                context: "liberation kit component",
            },
        );
    }
    let machine_components_ready_at = state.tick().value();
    craft_riddle_panel(
        registries,
        &mut state,
        raw,
        panel_feed,
        parts,
        adze,
        required_panel,
    );
    let riddle_ready_at = state.tick().value();
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
    let carrying = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("liberation carried-custody assessment disappeared"));
    assert_eq!(carrying.position(), player_position);
    assert_eq!(carrying.stockpile(), raw);
    assert_eq!(carrying.capacity(), raw_mass);
    assert_eq!(carrying.stored(), Mass::ZERO);
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
    let adze_attention = adze_ready_at - started_at;
    let fabrication_lathe_attention = fabrication_lathe_ready_at - adze_ready_at;
    let machine_component_attention = machine_components_ready_at - fabrication_lathe_ready_at;
    let riddle_attention = riddle_ready_at - machine_components_ready_at;
    assert_eq!(
        adze_attention
            + fabrication_lathe_attention
            + machine_component_attention
            + riddle_attention,
        attention,
        "liberation kit phase attention must account for the full fabrication wall"
    );
    assert_eq!(
        attention, selected_projection.total_ticks,
        "liberation kit executed fabrication attention diverged from the pre-action projection"
    );
    let metabolic = survival_before
        .metabolic_energy()
        .checked_sub(survival_after.metabolic_energy())
        .unwrap_or_else(|| panic!("liberation kit metabolic reserve increased"));
    let hydration = survival_before
        .hydration()
        .checked_sub(survival_after.hydration())
        .unwrap_or_else(|| panic!("liberation kit hydration reserve increased"));
    let attention_saved = direct_projection.total_ticks.saturating_sub(attention);
    reviewln!(
        "LIBERATION KIT ACQUISITION seed=0x{seed:016X} scope=raw-stone+logs->adze+reusable-base-processing-kit raw-origin=pre-admission-fixture pickup=same-voxel-runtime carried-custody=finite@voxel world-gathering-proved=false disclosed-campaign={}batches workload-known-before-build=true fabrication-choice=[spring-pole:{} staging-funded:{} policy-min-return:{}ppm threshold:{}t direct:{}t selected:{}t saved:{}t extra-raw:{}mg] raw=[stone:{}mg wood:{}mg total:{}mg] built=[adze:true spring-pole:{} crusher:true quern:true timber-riddle:true separator:true treadle:true paired-flywheel:true] attention:{}t fabrication=[adze:{}t spring-pole:{}t machine-components:{}t riddle-panel:{}t] body={}nJ/{}uL copper-screen-upgrade=proved-by-progression-continuation matter=conserved",
        planned_batches,
        stage_with_lathe,
        staging_funded,
        investment_policy.minimum_return_ppm(),
        minimum_lathe_return,
        direct_projection.total_ticks,
        attention,
        attention_saved,
        staging_extra_raw.milligrams(),
        stone_raw.milligrams(),
        wood_raw.milligrams(),
        raw_mass.milligrams(),
        stage_with_lathe,
        attention,
        adze_attention,
        fabrication_lathe_attention,
        machine_component_attention,
        riddle_attention,
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
