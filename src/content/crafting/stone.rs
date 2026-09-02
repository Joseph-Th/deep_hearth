//! Stone hand-processing definitions.

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftOutput};
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

use crate::content::materials::{
    FORM_CHIP, FORM_FLYWHEEL, FORM_LUMP, FORM_SCRAP, FORM_STONE_CROCK_BODY, FORM_TOOL,
    MATERIAL_STONE,
};
use crate::content::processes::{
    PROCESS_KNAP_STONE_TOOL, PROCESS_REKNAP_STONE_SCRAP_TOOL,
    PROCESS_SALVAGE_STONE_PROVISIONS_CROCK_BODY, PROCESS_SHAPE_STONE_FLYWHEEL,
    PROCESS_SHAPE_STONE_PROVISIONS_CROCK,
};

pub(super) fn definitions() -> [ManualCraftDefinition; 5] {
    [
        knap_stone_tool(),
        reknap_stone_scrap_tool(),
        shape_stone_flywheel(),
        shape_stone_provisions_crock(),
        salvage_stone_provisions_crock_body(),
    ]
}

fn shape_stone_provisions_crock() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_STONE_PROVISIONS_CROCK,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(3_000_000),
        TickSpan::new(180),
        stone_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
                Mass::from_milligrams(2_400_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(600_000),
            ),
        ],
    )
}

fn salvage_stone_provisions_crock_body() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SALVAGE_STONE_PROVISIONS_CROCK_BODY,
        CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
        Mass::from_milligrams(2_400_000),
        TickSpan::new(70),
        stone_exertion(),
        vec![ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            Mass::from_milligrams(2_400_000),
        )],
    )
}

fn stone_exertion() -> SurvivalExertion {
    SurvivalExertion::new(
        Energy::from_nanojoules(1_000_000_000_000),
        Volume::from_microliters(250),
    )
}

fn knap_stone_tool() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_KNAP_STONE_TOOL,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(40),
        stone_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(200_000),
            ),
        ],
    )
}

fn reknap_stone_scrap_tool() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_REKNAP_STONE_SCRAP_TOOL,
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(60),
        stone_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(200_000),
            ),
        ],
    )
}

fn shape_stone_flywheel() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_STONE_FLYWHEEL,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(60),
        stone_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(900_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(100_000),
            ),
        ],
    )
}
