//! Built-in pre-machine hand-processing definitions.

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::time::TickSpan;
use crate::crafting::{CraftingRegistry, ManualCraftDefinition, ManualCraftOutput};
use crate::material::CommodityKey;
use crate::survival::SurvivalExertion;

use super::{
    FORM_CHIP, FORM_HANDLE, FORM_LOG, FORM_LUMP, FORM_TOOL, FORM_UNFIRED_POTTERY, MATERIAL_CLAY,
    MATERIAL_STONE, MATERIAL_WOOD, PROCESS_FORM_CLAY_VESSEL, PROCESS_KNAP_STONE_TOOL,
    PROCESS_SHAPE_WOOD_HANDLE,
};

pub(crate) fn build_crafting_registry() -> CraftingRegistry {
    CraftingRegistry::new([
        ManualCraftDefinition::new(
            PROCESS_KNAP_STONE_TOOL,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000),
            TickSpan::new(40),
            SurvivalExertion::new(
                Energy::from_nanojoules(1_000_000_000_000),
                Volume::from_microliters(250),
            ),
            vec![
                ManualCraftOutput::new(
                    CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                    Mass::from_milligrams(800),
                ),
                ManualCraftOutput::new(
                    CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                    Mass::from_milligrams(200),
                ),
            ],
        ),
        ManualCraftDefinition::new(
            PROCESS_FORM_CLAY_VESSEL,
            CommodityKey::new(MATERIAL_CLAY, FORM_LUMP),
            Mass::from_milligrams(1_000),
            TickSpan::new(80),
            SurvivalExertion::new(
                Energy::from_nanojoules(750_000_000_000),
                Volume::from_microliters(200),
            ),
            vec![ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_CLAY, FORM_UNFIRED_POTTERY),
                Mass::from_milligrams(1_000),
            )],
        ),
        ManualCraftDefinition::new(
            PROCESS_SHAPE_WOOD_HANDLE,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1_000),
            TickSpan::new(40),
            SurvivalExertion::new(
                Energy::from_nanojoules(750_000_000_000),
                Volume::from_microliters(200),
            ),
            vec![
                ManualCraftOutput::new(
                    CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                    Mass::from_milligrams(200),
                ),
                ManualCraftOutput::new(
                    CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                    Mass::from_milligrams(800),
                ),
            ],
        ),
    ])
}
