//! Built-in workshop material transformations with physical resolver ownership.

use crate::capability::{
    CapabilityComparison, CapabilityId, CapabilityRequirement, CapabilityValue,
};
use crate::core::quantity::{Mass, MassFlow, Power, Temperature};
use crate::production::{ProcessId, ProductionRegistry};

use super::capabilities::{CAPABILITY_THERMAL_BATCH, CAPABILITY_THERMAL_MAX_TEMPERATURE};

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
pub const PROCESS_SHAPE_TIMBER_RIDDLE_PANEL: ProcessId = ProcessId::new(34);
pub const PROCESS_SHAPE_TIMBER_FLYWHEEL: ProcessId = ProcessId::new(35);
pub const PROCESS_REWORK_WOOD_SCRAP_HANDLE: ProcessId = ProcessId::new(36);
pub const PROCESS_RECOVER_WOOD_SCRAP_BOARDS: ProcessId = ProcessId::new(37);
pub const PROCESS_REGRIND_COPPER_TAILINGS: ProcessId = ProcessId::new(38);
pub const PROCESS_SCAVENGE_COPPER_TAILINGS: ProcessId = ProcessId::new(39);
pub const PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE: ProcessId = ProcessId::new(40);
pub const PROCESS_POWER_SAW_WOOD_BOARDS: ProcessId = ProcessId::new(41);
pub const PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT: ProcessId = ProcessId::new(42);
pub const PROCESS_POWER_HAMMER_COPPER_SCRAP_REINFORCEMENT: ProcessId = ProcessId::new(43);
pub const PROCESS_POWER_HAMMER_COPPER_SAW_BLADE: ProcessId = ProcessId::new(44);
pub const PROCESS_KNAP_STONE_DRILL_BIT: ProcessId = ProcessId::new(45);
pub const PROCESS_DRESS_STONE_CHIP_DRILL_BIT: ProcessId = ProcessId::new(46);
pub const PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE: ProcessId = ProcessId::new(47);
pub const PROCESS_POWER_TURN_WOOD_HANDLE: ProcessId = ProcessId::new(48);
pub const PROCESS_POWER_TURN_TIMBER_FLYWHEEL: ProcessId = ProcessId::new(49);
pub const PROCESS_SHAPE_STONE_GRINDSTONE_WHEEL: ProcessId = ProcessId::new(50);
pub const PROCESS_GRIND_STONE_SCRAP_TOOL: ProcessId = ProcessId::new(51);
pub const PROCESS_GRIND_STONE_SCRAP_DRILL_BIT: ProcessId = ProcessId::new(52);
pub const PROCESS_POWER_GRIND_STONE_SCRAP_TOOL: ProcessId = ProcessId::new(53);
pub const PROCESS_POWER_GRIND_STONE_SCRAP_DRILL_BIT: ProcessId = ProcessId::new(54);
pub const PROCESS_COLD_WORK_COPPER_INGOT_REINFORCEMENT: ProcessId = ProcessId::new(55);

mod fabrication;
mod ore;
mod powered;
mod storage;
mod thermal;

fn single_mass_flow_requirement(capability: CapabilityId) -> Vec<CapabilityRequirement> {
    vec![CapabilityRequirement::new(
        capability,
        CapabilityComparison::AtLeast,
        CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
    )]
}

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
    for process in ore::definitions()
        .into_iter()
        .chain(thermal::definitions())
        .chain(storage::definitions())
        .chain(fabrication::definitions())
        .chain(powered::definitions())
    {
        registry.register_process(process);
    }
    registry
}
