//! Built-in workshop material transformations with physical resolver ownership.

use crate::capability::{
    CapabilityComparison, CapabilityId, CapabilityRequirement, CapabilityValue,
};
use crate::core::quantity::{Mass, MassFlow, Power, Temperature};
use crate::production::{ProcessDefinition, ProcessId, ProductionRegistry};

use super::capabilities::{
    CAPABILITY_COOLING_POWER, CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW,
    CAPABILITY_GRINDER_BATCH, CAPABILITY_GRINDER_FLOW, CAPABILITY_HEATING_POWER,
    CAPABILITY_SCREEN_BATCH, CAPABILITY_SCREEN_FLOW, CAPABILITY_SEPARATOR_BATCH,
    CAPABILITY_SEPARATOR_FLOW, CAPABILITY_THERMAL_BATCH, CAPABILITY_THERMAL_MAX_TEMPERATURE,
};

pub const PROCESS_CRUSH_ORE: ProcessId = ProcessId::new(1);
pub const PROCESS_MELT_PURE_COPPER: ProcessId = ProcessId::new(2);
pub const PROCESS_CAST_PURE_COPPER: ProcessId = ProcessId::new(3);
pub const PROCESS_SCREEN_CRUSHED_ORE: ProcessId = ProcessId::new(4);
pub const PROCESS_GRIND_CRUSHED_ORE: ProcessId = ProcessId::new(5);
pub const PROCESS_FINE_GRIND_SCREEN_OVERSIZE: ProcessId = ProcessId::new(6);
pub const PROCESS_KNAP_STONE_TOOL: ProcessId = ProcessId::new(7);
pub const PROCESS_SHAPE_WOOD_HANDLE: ProcessId = ProcessId::new(8);
pub const PROCESS_SHAPE_STONE_FLYWHEEL: ProcessId = ProcessId::new(9);
pub const PROCESS_COLD_WORK_COPPER_REINFORCEMENT: ProcessId = ProcessId::new(10);
pub const PROCESS_SEPARATE_NATIVE_COPPER: ProcessId = ProcessId::new(11);
pub const PROCESS_CONCENTRATE_COPPER: ProcessId = ProcessId::new(12);
pub const PROCESS_HAND_SORT_NATIVE_COPPER: ProcessId = ProcessId::new(13);
pub const PROCESS_SHAPE_WOOD_BOARDS: ProcessId = ProcessId::new(14);
pub const PROCESS_HAND_BREAK_ORE: ProcessId = ProcessId::new(15);
pub const PROCESS_ASSEMBLE_TIMBER_CHEST: ProcessId = ProcessId::new(16);
pub const PROCESS_HEAT_MATERIAL_BATCH: ProcessId = ProcessId::new(17);
pub const PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT: ProcessId = ProcessId::new(18);
pub const PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST: ProcessId = ProcessId::new(19);
pub const PROCESS_SALVAGE_TIMBER_CHEST_BODY: ProcessId = ProcessId::new(20);
pub const PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY: ProcessId = ProcessId::new(21);
pub const PROCESS_REKNAP_STONE_SCRAP_TOOL: ProcessId = ProcessId::new(22);
pub const PROCESS_ASSEMBLE_BULK_TIMBER_CRATE: ProcessId = ProcessId::new(23);
pub const PROCESS_SALVAGE_BULK_TIMBER_CRATE_BODY: ProcessId = ProcessId::new(24);
pub const PROCESS_ASSEMBLE_INSULATED_TIMBER_PANTRY: ProcessId = ProcessId::new(25);
pub const PROCESS_SALVAGE_INSULATED_TIMBER_PANTRY_BODY: ProcessId = ProcessId::new(26);
pub const PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX: ProcessId = ProcessId::new(27);
pub const PROCESS_SALVAGE_ROUGH_TIMBER_FIELD_BOX_BODY: ProcessId = ProcessId::new(28);
pub const PROCESS_SHAPE_STONE_PROVISIONS_CROCK: ProcessId = ProcessId::new(29);
pub const PROCESS_SALVAGE_STONE_PROVISIONS_CROCK_BODY: ProcessId = ProcessId::new(30);
pub const PROCESS_PIERCE_COPPER_SCREEN_PLATE: ProcessId = ProcessId::new(31);
pub const PROCESS_COLD_WORK_COPPER_SAW_BLADE: ProcessId = ProcessId::new(32);
pub const PROCESS_SAW_WOOD_BOARDS: ProcessId = ProcessId::new(33);

fn mass_flow_resolver_requirements(
    flow_capability: CapabilityId,
    batch_capability: CapabilityId,
) -> Vec<CapabilityRequirement> {
    vec![
        CapabilityRequirement::new(
            flow_capability,
            CapabilityComparison::AtLeast,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
        ),
        CapabilityRequirement::new(
            batch_capability,
            CapabilityComparison::AtLeast,
            CapabilityValue::Mass(Mass::from_milligrams(1)),
        ),
    ]
}

fn thermal_resolver_requirements(
    transfer_power_capability: CapabilityId,
) -> Vec<CapabilityRequirement> {
    // Generic provider discovery only requires the resolver-owned capability dimensions to be
    // productive. The thermal resolver owns actual target/input temperature, batch, power-duration,
    // finite-energy, and condition-adjusted admission for each concrete operation.
    vec![
        CapabilityRequirement::new(
            transfer_power_capability,
            CapabilityComparison::AtLeast,
            CapabilityValue::Power(Power::from_picowatts(1)),
        ),
        CapabilityRequirement::new(
            CAPABILITY_THERMAL_MAX_TEMPERATURE,
            CapabilityComparison::AtLeast,
            CapabilityValue::Temperature(Temperature::from_millikelvin(1)),
        ),
        CapabilityRequirement::new(
            CAPABILITY_THERMAL_BATCH,
            CapabilityComparison::AtLeast,
            CapabilityValue::Mass(Mass::from_milligrams(1)),
        ),
    ]
}

pub(crate) fn build_production_registry() -> ProductionRegistry {
    let mut registry = ProductionRegistry::new();
    for process in [
        ProcessDefinition::new_selected_batch(
            PROCESS_CRUSH_ORE,
            "crush ore",
            mass_flow_resolver_requirements(CAPABILITY_CRUSHER_FLOW, CAPABILITY_CRUSHER_BATCH),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_MELT_PURE_COPPER,
            "melt pure copper",
            thermal_resolver_requirements(CAPABILITY_HEATING_POWER),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_HEAT_MATERIAL_BATCH,
            "sensible heat material batch",
            thermal_resolver_requirements(CAPABILITY_HEATING_POWER),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_CAST_PURE_COPPER,
            "cast pure copper",
            thermal_resolver_requirements(CAPABILITY_COOLING_POWER),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_SCREEN_CRUSHED_ORE,
            "screen crushed ore",
            mass_flow_resolver_requirements(CAPABILITY_SCREEN_FLOW, CAPABILITY_SCREEN_BATCH),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_GRIND_CRUSHED_ORE,
            "grind crushed ore",
            mass_flow_resolver_requirements(CAPABILITY_GRINDER_FLOW, CAPABILITY_GRINDER_BATCH),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
            "fine grind screen oversize",
            mass_flow_resolver_requirements(CAPABILITY_GRINDER_FLOW, CAPABILITY_GRINDER_BATCH),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_HAND_SORT_NATIVE_COPPER,
            "hand sort native copper from crushed ore",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_REKNAP_STONE_SCRAP_TOOL,
            "reknap stone scrap tool",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_ASSEMBLE_TIMBER_CHEST,
            "assemble timber chest body",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
            "assemble double-wall timber chest body",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_ASSEMBLE_BULK_TIMBER_CRATE,
            "assemble bulk timber crate body",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_ASSEMBLE_INSULATED_TIMBER_PANTRY,
            "assemble insulated timber pantry body",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
            "assemble rough timber field box body",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_SALVAGE_TIMBER_CHEST_BODY,
            "salvage timber chest body",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY,
            "salvage double-wall timber chest body",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_SALVAGE_BULK_TIMBER_CRATE_BODY,
            "salvage bulk timber crate body",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_SALVAGE_INSULATED_TIMBER_PANTRY_BODY,
            "salvage insulated timber pantry body",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_SALVAGE_ROUGH_TIMBER_FIELD_BOX_BODY,
            "salvage rough timber field box body",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_SHAPE_STONE_PROVISIONS_CROCK,
            "shape carved stone provisions crock body",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_SALVAGE_STONE_PROVISIONS_CROCK_BODY,
            "salvage carved stone provisions crock body",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(PROCESS_HAND_BREAK_ORE, "hand break ore", Vec::new()),
        ProcessDefinition::new_selected_batch(
            PROCESS_SHAPE_WOOD_BOARDS,
            "shape timber boards",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_SEPARATE_NATIVE_COPPER,
            "separate native copper from crushed ore",
            mass_flow_resolver_requirements(CAPABILITY_SEPARATOR_FLOW, CAPABILITY_SEPARATOR_BATCH),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_CONCENTRATE_COPPER,
            "concentrate copper from liberated ore",
            mass_flow_resolver_requirements(CAPABILITY_SEPARATOR_FLOW, CAPABILITY_SEPARATOR_BATCH),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_KNAP_STONE_TOOL,
            "knap stone tool",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_SHAPE_WOOD_HANDLE,
            "shape wood handle",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_SHAPE_STONE_FLYWHEEL,
            "shape stone flywheel",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
            "cold-work native copper reinforcement",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
            "rework copper scrap reinforcement",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_PIERCE_COPPER_SCREEN_PLATE,
            "pierce copper sizing screen plate",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_COLD_WORK_COPPER_SAW_BLADE,
            "cold-work copper frame-saw blade",
            Vec::new(),
        ),
        ProcessDefinition::new_selected_batch(
            PROCESS_SAW_WOOD_BOARDS,
            "rip timber boards on frame saw",
            Vec::new(),
        ),
    ] {
        registry.register_process(process);
    }
    registry
}
