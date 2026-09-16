//! Timber enclosure-body joinery and loss-bearing salvage definitions.

use crate::content::materials::{
    FORM_BOARD, FORM_BULK_CRATE_BODY, FORM_CHEST_BODY, FORM_CHIP, FORM_DOUBLE_WALL_CHEST_BODY,
    FORM_INSULATED_PANTRY_BODY, FORM_ROUGH_BOX_BODY, MATERIAL_WOOD,
};
use crate::content::processes::{
    PROCESS_ASSEMBLE_BULK_TIMBER_CRATE, PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
    PROCESS_ASSEMBLE_INSULATED_TIMBER_PANTRY, PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
    PROCESS_ASSEMBLE_TIMBER_CHEST, PROCESS_SALVAGE_BULK_TIMBER_CRATE_BODY,
    PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY, PROCESS_SALVAGE_INSULATED_TIMBER_PANTRY_BODY,
    PROCESS_SALVAGE_ROUGH_TIMBER_FIELD_BOX_BODY, PROCESS_SALVAGE_TIMBER_CHEST_BODY,
};
use crate::core::quantity::Mass;
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftOutput};
use crate::material::CommodityKey;

use super::wood_exertion;

pub(super) fn definitions() -> [ManualCraftDefinition; 10] {
    [
        assemble_rough_timber_field_box(),
        assemble_timber_chest(),
        assemble_double_wall_timber_chest(),
        assemble_bulk_timber_crate(),
        assemble_insulated_timber_pantry(),
        salvage_rough_timber_field_box_body(),
        salvage_timber_chest_body(),
        salvage_double_wall_timber_chest_body(),
        salvage_bulk_timber_crate_body(),
        salvage_insulated_timber_pantry_body(),
    ]
}

fn assemble_rough_timber_field_box() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        Mass::from_milligrams(1_600_000),
        TickSpan::new(50),
        wood_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
            Mass::from_milligrams(1_600_000),
        )],
    )
}

fn assemble_bulk_timber_crate() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_ASSEMBLE_BULK_TIMBER_CRATE,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        Mass::from_milligrams(3_200_000),
        TickSpan::new(90),
        wood_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_BULK_CRATE_BODY),
            Mass::from_milligrams(3_200_000),
        )],
    )
}

fn assemble_insulated_timber_pantry() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_ASSEMBLE_INSULATED_TIMBER_PANTRY,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        Mass::from_milligrams(4_800_000),
        TickSpan::new(140),
        wood_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_INSULATED_PANTRY_BODY),
            Mass::from_milligrams(4_800_000),
        )],
    )
}

fn assemble_timber_chest() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_ASSEMBLE_TIMBER_CHEST,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        Mass::from_milligrams(2_400_000),
        TickSpan::new(80),
        wood_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
            Mass::from_milligrams(2_400_000),
        )],
    )
}

fn assemble_double_wall_timber_chest() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        Mass::from_milligrams(4_000_000),
        TickSpan::new(120),
        wood_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
            Mass::from_milligrams(4_000_000),
        )],
    )
}

fn salvage_rough_timber_field_box_body() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SALVAGE_ROUGH_TIMBER_FIELD_BOX_BODY,
        CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
        Mass::from_milligrams(1_600_000),
        TickSpan::new(50),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(800_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(800_000),
            ),
        ],
    )
}

fn salvage_bulk_timber_crate_body() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SALVAGE_BULK_TIMBER_CRATE_BODY,
        CommodityKey::new(MATERIAL_WOOD, FORM_BULK_CRATE_BODY),
        Mass::from_milligrams(3_200_000),
        TickSpan::new(80),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(2_400_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(800_000),
            ),
        ],
    )
}

fn salvage_insulated_timber_pantry_body() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SALVAGE_INSULATED_TIMBER_PANTRY_BODY,
        CommodityKey::new(MATERIAL_WOOD, FORM_INSULATED_PANTRY_BODY),
        Mass::from_milligrams(4_800_000),
        TickSpan::new(120),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(4_000_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(800_000),
            ),
        ],
    )
}

fn salvage_timber_chest_body() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SALVAGE_TIMBER_CHEST_BODY,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
        Mass::from_milligrams(2_400_000),
        TickSpan::new(70),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(1_600_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(800_000),
            ),
        ],
    )
}

fn salvage_double_wall_timber_chest_body() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY,
        CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
        Mass::from_milligrams(4_000_000),
        TickSpan::new(100),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(3_200_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(800_000),
            ),
        ],
    )
}
