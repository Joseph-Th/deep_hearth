//! Direct-labor fabrication, shaping, and material-recovery process definitions.

use crate::production::ProcessDefinition;

use super::{
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_COLD_WORK_COPPER_SAW_BLADE,
    PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT, PROCESS_DRESS_STONE_CHIP_DRILL_BIT,
    PROCESS_GRIND_STONE_SCRAP_DRILL_BIT, PROCESS_GRIND_STONE_SCRAP_TOOL,
    PROCESS_KNAP_STONE_DRILL_BIT, PROCESS_KNAP_STONE_TOOL, PROCESS_PIERCE_COPPER_SCREEN_PLATE,
    PROCESS_RECOVER_WOOD_SCRAP_BOARDS, PROCESS_REKNAP_STONE_SCRAP_TOOL,
    PROCESS_REWORK_WOOD_SCRAP_HANDLE, PROCESS_SAW_WOOD_BOARDS, PROCESS_SHAPE_STONE_FLYWHEEL,
    PROCESS_SHAPE_STONE_GRINDSTONE_WHEEL, PROCESS_SHAPE_TIMBER_FLYWHEEL,
    PROCESS_SHAPE_TIMBER_RIDDLE_PANEL, PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE,
};

pub(super) fn definitions() -> [ProcessDefinition; 19] {
    [
        ProcessDefinition::new(
            PROCESS_REKNAP_STONE_SCRAP_TOOL,
            "reknap stone scrap tool",
            Vec::new(),
        ),
        ProcessDefinition::new(PROCESS_KNAP_STONE_TOOL, "knap stone tool", Vec::new()),
        ProcessDefinition::new(
            PROCESS_KNAP_STONE_DRILL_BIT,
            "knap stone rotary drill bit",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_DRESS_STONE_CHIP_DRILL_BIT,
            "dress stone chip into rotary drill bit",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_SHAPE_STONE_GRINDSTONE_WHEEL,
            "shape abrasive grindstone wheel",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_GRIND_STONE_SCRAP_TOOL,
            "grind stone scrap into service tool stock",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_GRIND_STONE_SCRAP_DRILL_BIT,
            "grind stone scrap into rotary drill bit",
            Vec::new(),
        ),
        ProcessDefinition::new(PROCESS_SHAPE_WOOD_HANDLE, "shape wood handle", Vec::new()),
        ProcessDefinition::new(
            PROCESS_SHAPE_STONE_FLYWHEEL,
            "shape stone flywheel",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
            "cold-work native copper reinforcement",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
            "rework copper scrap reinforcement",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_PIERCE_COPPER_SCREEN_PLATE,
            "pierce copper sizing screen plate",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_COLD_WORK_COPPER_SAW_BLADE,
            "cold-work copper frame-saw blade",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_SAW_WOOD_BOARDS,
            "rip timber boards on frame saw",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_SHAPE_TIMBER_RIDDLE_PANEL,
            "shape timber riddle panel",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_SHAPE_TIMBER_FLYWHEEL,
            "shape timber flywheel",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_REWORK_WOOD_SCRAP_HANDLE,
            "rework wood scrap into handle stock",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_RECOVER_WOOD_SCRAP_BOARDS,
            "recover board stock from wood scrap",
            Vec::new(),
        ),
        ProcessDefinition::new(PROCESS_SHAPE_WOOD_BOARDS, "shape timber boards", Vec::new()),
    ]
}
