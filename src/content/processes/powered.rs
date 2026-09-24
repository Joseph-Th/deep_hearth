//! Mechanized fabrication process definitions.

use crate::production::ProcessDefinition;

use super::super::capabilities::{
    CAPABILITY_POWERED_COPPER_HAMMERING_FLOW, CAPABILITY_POWERED_COPPER_PIERCING_FLOW,
    CAPABILITY_POWERED_SAWING_FLOW, CAPABILITY_POWERED_STONE_GRINDING_FLOW,
    CAPABILITY_POWERED_WOOD_TURNING_FLOW,
};

use super::{
    PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE, PROCESS_POWER_GRIND_STONE_SCRAP_DRILL_BIT,
    PROCESS_POWER_GRIND_STONE_SCRAP_TOOL, PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
    PROCESS_POWER_HAMMER_COPPER_SAW_BLADE, PROCESS_POWER_HAMMER_COPPER_SCRAP_REINFORCEMENT,
    PROCESS_POWER_SAW_WOOD_BOARDS, PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
    PROCESS_POWER_TURN_WOOD_HANDLE, single_mass_flow_requirement,
};

pub(super) fn definitions() -> [ProcessDefinition; 9] {
    [
        ProcessDefinition::new(
            PROCESS_POWER_SAW_WOOD_BOARDS,
            "power-saw timber boards",
            single_mass_flow_requirement(CAPABILITY_POWERED_SAWING_FLOW),
        ),
        ProcessDefinition::new(
            PROCESS_POWER_HAMMER_COPPER_REINFORCEMENT,
            "power-hammer native copper reinforcement",
            single_mass_flow_requirement(CAPABILITY_POWERED_COPPER_HAMMERING_FLOW),
        ),
        ProcessDefinition::new(
            PROCESS_POWER_HAMMER_COPPER_SCRAP_REINFORCEMENT,
            "power-hammer copper scrap reinforcement",
            single_mass_flow_requirement(CAPABILITY_POWERED_COPPER_HAMMERING_FLOW),
        ),
        ProcessDefinition::new(
            PROCESS_POWER_HAMMER_COPPER_SAW_BLADE,
            "power-hammer copper saw blade",
            single_mass_flow_requirement(CAPABILITY_POWERED_COPPER_HAMMERING_FLOW),
        ),
        ProcessDefinition::new(
            PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
            "power-drill copper sizing screen plate",
            single_mass_flow_requirement(CAPABILITY_POWERED_COPPER_PIERCING_FLOW),
        ),
        ProcessDefinition::new(
            PROCESS_POWER_TURN_WOOD_HANDLE,
            "power-turn wood handle stock",
            single_mass_flow_requirement(CAPABILITY_POWERED_WOOD_TURNING_FLOW),
        ),
        ProcessDefinition::new(
            PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
            "power-turn timber flywheel",
            single_mass_flow_requirement(CAPABILITY_POWERED_WOOD_TURNING_FLOW),
        ),
        ProcessDefinition::new(
            PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
            "power-grind stone scrap into service tool stock",
            single_mass_flow_requirement(CAPABILITY_POWERED_STONE_GRINDING_FLOW),
        ),
        ProcessDefinition::new(
            PROCESS_POWER_GRIND_STONE_SCRAP_DRILL_BIT,
            "power-grind stone scrap into rotary drill bit",
            single_mass_flow_requirement(CAPABILITY_POWERED_STONE_GRINDING_FLOW),
        ),
    ]
}
