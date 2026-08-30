//! Built-in pre-machine hand-processing definitions.

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::crafting::{CraftingRegistry, ManualCraftDefinition, ManualCraftOutput};
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

use super::{
    FORM_BOARD, FORM_CHEST_BODY, FORM_CHIP, FORM_DOUBLE_WALL_CHEST_BODY, FORM_FLYWHEEL,
    FORM_HANDLE, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, FORM_REINFORCEMENT, FORM_SCRAP, FORM_TOOL,
    MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
    PROCESS_ASSEMBLE_TIMBER_CHEST, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT, PROCESS_KNAP_STONE_TOOL,
    PROCESS_REKNAP_STONE_SCRAP_TOOL, PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY,
    PROCESS_SALVAGE_TIMBER_CHEST_BODY, PROCESS_SHAPE_STONE_FLYWHEEL, PROCESS_SHAPE_WOOD_BOARDS,
    PROCESS_SHAPE_WOOD_HANDLE,
};

pub(crate) fn build_crafting_registry() -> CraftingRegistry {
    CraftingRegistry::new([
        ManualCraftDefinition::new(
            PROCESS_KNAP_STONE_TOOL,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000_000),
            TickSpan::new(40),
            SurvivalExertion::new(
                Energy::from_nanojoules(1_000_000_000_000),
                Volume::from_microliters(250),
            ),
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
        ),
        ManualCraftDefinition::new(
            PROCESS_REKNAP_STONE_SCRAP_TOOL,
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            Mass::from_milligrams(1_000_000),
            TickSpan::new(60),
            SurvivalExertion::new(
                Energy::from_nanojoules(1_000_000_000_000),
                Volume::from_microliters(250),
            ),
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
        ),
        ManualCraftDefinition::new(
            PROCESS_ASSEMBLE_TIMBER_CHEST,
            CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
            Mass::from_milligrams(2_400_000),
            TickSpan::new(80),
            SurvivalExertion::new(
                Energy::from_nanojoules(750_000_000_000),
                Volume::from_microliters(200),
            ),
            vec![ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
                Mass::from_milligrams(2_400_000),
            )],
        ),
        ManualCraftDefinition::new(
            PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
            CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
            Mass::from_milligrams(4_000_000),
            TickSpan::new(120),
            SurvivalExertion::new(
                Energy::from_nanojoules(750_000_000_000),
                Volume::from_microliters(200),
            ),
            vec![ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
                Mass::from_milligrams(4_000_000),
            )],
        ),
        ManualCraftDefinition::new(
            PROCESS_SALVAGE_TIMBER_CHEST_BODY,
            CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
            Mass::from_milligrams(2_400_000),
            TickSpan::new(70),
            SurvivalExertion::new(
                Energy::from_nanojoules(750_000_000_000),
                Volume::from_microliters(200),
            ),
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
        ),
        ManualCraftDefinition::new(
            PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY,
            CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
            Mass::from_milligrams(4_000_000),
            TickSpan::new(100),
            SurvivalExertion::new(
                Energy::from_nanojoules(750_000_000_000),
                Volume::from_microliters(200),
            ),
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
        ),
        ManualCraftDefinition::new(
            PROCESS_SHAPE_WOOD_BOARDS,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1_000_000),
            TickSpan::new(50),
            SurvivalExertion::new(
                Energy::from_nanojoules(750_000_000_000),
                Volume::from_microliters(200),
            ),
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
        ),
        ManualCraftDefinition::new(
            PROCESS_SHAPE_WOOD_HANDLE,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1_000_000),
            TickSpan::new(40),
            SurvivalExertion::new(
                Energy::from_nanojoules(750_000_000_000),
                Volume::from_microliters(200),
            ),
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
        ),
        ManualCraftDefinition::new(
            PROCESS_SHAPE_STONE_FLYWHEEL,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000_000),
            TickSpan::new(60),
            SurvivalExertion::new(
                Energy::from_nanojoules(1_000_000_000_000),
                Volume::from_microliters(250),
            ),
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
        ),
        ManualCraftDefinition::new(
            PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            Mass::from_milligrams(20_000),
            TickSpan::new(40),
            SurvivalExertion::new(
                Energy::from_nanojoules(1_000_000_000_000),
                Volume::from_microliters(250),
            ),
            vec![ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(20_000),
            )],
        ),
        ManualCraftDefinition::new(
            PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
            CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
            Mass::from_milligrams(20_000),
            TickSpan::new(50),
            SurvivalExertion::new(
                Energy::from_nanojoules(1_000_000_000_000),
                Volume::from_microliters(250),
            ),
            vec![ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(20_000),
            )],
        ),
    ])
}
