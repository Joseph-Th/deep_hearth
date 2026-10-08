//! Pre-action raw-material and attention planning for the incremental primitive liberation kit.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use deep_hearth::content::{
    EQUIPMENT_STONE_ROTARY_QUERN, EQUIPMENT_STONE_WOODWORKING_ADZE,
    EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN, FORM_BOARD, FORM_LOG, FORM_LUMP,
    FORM_TIMBER_RIDDLE_PANEL, MATERIAL_STONE, MATERIAL_WOOD, SURFACE_GATHERING_HAND_COLLECT_STONE,
    SURFACE_GATHERING_HAND_COLLECT_TIMBER,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::crafting::{project_manual_craft_equipment, project_manual_craft_hand_work};
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::labor::SurfaceGatheringMethodId;
use deep_hearth::maintenance::Condition;
use deep_hearth::material::{CommodityKey, MaterialAssemblyProfile};
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;

use super::manual_craft_equipment_topology_planning::manual_craft_topology_plan_with_equipment;
use super::manual_craft_topology_planning::manual_craft_topology_plan_for_output_from_inputs;

#[derive(Clone)]
pub(super) struct RawKitMaterialPlan {
    pub(super) adze_requirements: Vec<(CommodityKey, Mass)>,
    pub(super) final_requirements: BTreeMap<CommodityKey, Mass>,
    pub(super) required_panel: Mass,
    pub(super) raw_requirements: BTreeMap<CommodityKey, Mass>,
    pub(super) raw_mass: Mass,
    panel_process: ProcessId,
    panel_batches: u64,
    panel_board_mass: Mass,
}

#[derive(Clone, Copy)]
pub(super) struct RawKitAttentionProjection {
    pub(super) gathering_ticks: u64,
    pub(super) fabrication_ticks: u64,
    pub(super) total_ticks: u64,
}

pub(super) fn gathering_method_for(commodity: CommodityKey) -> SurfaceGatheringMethodId {
    if commodity == CommodityKey::new(MATERIAL_STONE, FORM_LUMP) {
        SURFACE_GATHERING_HAND_COLLECT_STONE
    } else if commodity == CommodityKey::new(MATERIAL_WOOD, FORM_LOG) {
        SURFACE_GATHERING_HAND_COLLECT_TIMBER
    } else {
        panic!("liberation kit has no authored surface gathering method for {commodity:?}")
    }
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

pub(super) fn add_profile_requirements(
    requirements: &mut BTreeMap<CommodityKey, Mass>,
    profile: &MaterialAssemblyProfile,
) {
    for input in profile.inputs() {
        add_requirement(requirements, input.commodity(), input.mass());
    }
}

pub(super) fn equipment_profile(
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
) -> BTreeMap<CommodityKey, Mass> {
    let mut raw = BTreeMap::new();
    for input in adze_profile.inputs() {
        add_raw_cost(registries, &mut raw, input.commodity(), input.mass());
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

pub(super) fn raw_requirement_mass(requirements: &BTreeMap<CommodityKey, Mass>) -> Mass {
    requirements
        .values()
        .copied()
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("liberation kit raw mass overflowed"))
}

pub(super) fn raw_kit_material_plan(registries: &Registries) -> RawKitMaterialPlan {
    let adze_profile = equipment_profile(registries, EQUIPMENT_STONE_WOODWORKING_ADZE);
    let adze_requirements = adze_profile
        .inputs()
        .iter()
        .map(|input| (input.commodity(), input.mass()))
        .collect::<Vec<_>>();
    let mut final_requirements = BTreeMap::new();
    for definition in [
        EQUIPMENT_STONE_ROTARY_QUERN,
        EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
    ] {
        add_profile_requirements(
            &mut final_requirements,
            equipment_profile(registries, definition),
        );
    }
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
    let raw_requirements = raw_kit_requirements(
        registries,
        adze_profile,
        &final_requirements,
        panel_board_mass,
    );
    let raw_mass = raw_requirement_mass(&raw_requirements);
    RawKitMaterialPlan {
        adze_requirements,
        final_requirements,
        required_panel,
        raw_requirements,
        raw_mass,
        panel_process: panel_definition.process(),
        panel_batches,
        panel_board_mass,
    }
}

/// Projects the complete raw-source-to-kit acquisition attention from authored mechanics.
///
/// This is a pre-action planning surface: it reads immutable gathering, recipe, and equipment
/// physics only and carries the future adze condition through each assisted craft. Runtime
/// acquisition asserts both gathering and fabrication against this projection.
pub(super) fn project_incremental_kit_acquisition_attention(
    registries: &Registries,
) -> RawKitAttentionProjection {
    let plan = raw_kit_material_plan(registries);
    let mut gathering_ticks = 0_u64;
    for (commodity, mass) in &plan.raw_requirements {
        let method_id = gathering_method_for(*commodity);
        let gathering = registries
            .labor()
            .get_surface_gathering(method_id)
            .copied()
            .unwrap_or_else(|| panic!("liberation authored gathering method disappeared"));
        let mut remaining = *mass;
        while !remaining.is_zero() {
            let batch = remaining.min(gathering.maximum_batch_mass());
            let duration = gathering
                .duration_for_mass(batch)
                .unwrap_or_else(|| unreachable!("bounded gathering batch has a duration"));
            add_attention(
                &mut gathering_ticks,
                duration.value(),
                "surface gathering projection",
            );
            remaining = remaining.checked_sub(batch).unwrap_or_else(|| {
                unreachable!("projected gathering batch is bounded by remaining demand")
            });
        }
    }

    let mut fabrication_ticks = 0_u64;
    for (commodity, required) in &plan.adze_requirements {
        let (definition, batches) = raw_component_plan(
            registries,
            *commodity,
            *required,
            "liberation kit adze component projection",
        );
        let projection = project_manual_craft_hand_work(
            registries,
            definition.process(),
            nonzero_batches(batches, "adze component projection"),
        )
        .unwrap_or_else(|error| panic!("liberation kit adze projection failed: {error}"));
        add_attention(
            &mut fabrication_ticks,
            projection.duration().value(),
            "adze component projection",
        );
    }

    let mut adze_condition = Condition::PRISTINE;
    for (commodity, required) in plan.final_requirements {
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
                &mut fabrication_ticks,
                projection.duration().value(),
                "adze-assisted component projection",
            );
            adze_condition = projection.condition_after();
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
                &mut fabrication_ticks,
                projection.duration().value(),
                "component projection",
            );
        }
    }

    let board = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let (board_definition, board_batches) = manual_craft_topology_plan_with_equipment(
        registries,
        board,
        plan.panel_board_mass,
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
        &mut fabrication_ticks,
        board_projection.duration().value(),
        "riddle-board projection",
    );
    adze_condition = board_projection.condition_after();
    let panel = CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL);
    let (worn_panel_definition, worn_panel_batches) = manual_craft_topology_plan_with_equipment(
        registries,
        panel,
        plan.required_panel,
        &[board],
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        adze_condition,
        "liberation kit worn riddle-panel projection",
    );
    assert_eq!(worn_panel_definition.process(), plan.panel_process);
    assert_eq!(worn_panel_batches, plan.panel_batches);
    let panel_projection = project_manual_craft_equipment(
        registries,
        worn_panel_definition.process(),
        nonzero_batches(worn_panel_batches, "riddle-panel projection"),
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        adze_condition,
    )
    .unwrap_or_else(|error| panic!("liberation kit riddle-panel projection failed: {error}"));
    add_attention(
        &mut fabrication_ticks,
        panel_projection.duration().value(),
        "riddle-panel projection",
    );
    RawKitAttentionProjection {
        gathering_ticks,
        fabrication_ticks,
        total_ticks: gathering_ticks
            .checked_add(fabrication_ticks)
            .unwrap_or_else(|| panic!("liberation acquisition projection overflowed")),
    }
}
