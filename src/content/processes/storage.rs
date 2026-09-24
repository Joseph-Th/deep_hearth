//! Storage-body construction and salvage process definitions.

use crate::production::ProcessDefinition;

use super::{
    PROCESS_ASSEMBLE_BULK_TIMBER_CRATE, PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
    PROCESS_ASSEMBLE_INSULATED_TIMBER_PANTRY, PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
    PROCESS_ASSEMBLE_TIMBER_CHEST, PROCESS_SALVAGE_BULK_TIMBER_CRATE_BODY,
    PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY, PROCESS_SALVAGE_INSULATED_TIMBER_PANTRY_BODY,
    PROCESS_SALVAGE_ROUGH_TIMBER_FIELD_BOX_BODY, PROCESS_SALVAGE_STONE_PROVISIONS_CROCK_BODY,
    PROCESS_SALVAGE_TIMBER_CHEST_BODY, PROCESS_SHAPE_STONE_PROVISIONS_CROCK,
};

pub(super) fn definitions() -> [ProcessDefinition; 12] {
    [
        ProcessDefinition::new(
            PROCESS_ASSEMBLE_TIMBER_CHEST,
            "assemble timber chest body",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
            "assemble double-wall timber chest body",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_ASSEMBLE_BULK_TIMBER_CRATE,
            "assemble bulk timber crate body",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_ASSEMBLE_INSULATED_TIMBER_PANTRY,
            "assemble insulated timber pantry body",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
            "assemble rough timber field box body",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_SALVAGE_TIMBER_CHEST_BODY,
            "salvage timber chest body",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY,
            "salvage double-wall timber chest body",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_SALVAGE_BULK_TIMBER_CRATE_BODY,
            "salvage bulk timber crate body",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_SALVAGE_INSULATED_TIMBER_PANTRY_BODY,
            "salvage insulated timber pantry body",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_SALVAGE_ROUGH_TIMBER_FIELD_BOX_BODY,
            "salvage rough timber field box body",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_SHAPE_STONE_PROVISIONS_CROCK,
            "shape carved stone provisions crock body",
            Vec::new(),
        ),
        ProcessDefinition::new(
            PROCESS_SALVAGE_STONE_PROVISIONS_CROCK_BODY,
            "salvage carved stone provisions crock body",
            Vec::new(),
        ),
    ]
}
