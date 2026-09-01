//! Timber shaping, enclosure-body assembly, and salvage definitions.

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftDefinition, ManualCraftOutput};
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

use crate::content::materials::{
    FORM_BOARD, FORM_CHEST_BODY, FORM_CHIP, FORM_DOUBLE_WALL_CHEST_BODY, FORM_HANDLE, FORM_LOG,
    MATERIAL_WOOD,
};
use crate::content::processes::{
    PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST, PROCESS_ASSEMBLE_TIMBER_CHEST,
    PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY, PROCESS_SALVAGE_TIMBER_CHEST_BODY,
    PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE,
};

pub(super) fn definitions() -> [ManualCraftDefinition; 6] {
    [
        assemble_timber_chest(),
        assemble_double_wall_timber_chest(),
        salvage_timber_chest_body(),
        salvage_double_wall_timber_chest_body(),
        shape_wood_boards(),
        shape_wood_handle(),
    ]
}

fn wood_exertion() -> SurvivalExertion {
    SurvivalExertion::new(
        Energy::from_nanojoules(750_000_000_000),
        Volume::from_microliters(200),
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

fn shape_wood_boards() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_WOOD_BOARDS,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(50),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(800_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(200_000),
            ),
        ],
    )
}

fn shape_wood_handle() -> ManualCraftDefinition {
    ManualCraftDefinition::new(
        PROCESS_SHAPE_WOOD_HANDLE,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        TickSpan::new(40),
        wood_exertion(),
        vec![
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(800_000),
            ),
        ],
    )
}
