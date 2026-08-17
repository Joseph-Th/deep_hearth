//! Built-in pre-machine hand-processing definitions.

use crate::core::quantity::Mass;
use crate::core::time::TickSpan;
use crate::crafting::{CraftingRegistry, ManualCraftDefinition, ManualCraftOutput};
use crate::material::CommodityKey;

use super::{
    FORM_CHIP, FORM_LUMP, FORM_TOOL, FORM_UNFIRED_POTTERY, MATERIAL_CLAY, MATERIAL_STONE,
    PROCESS_FORM_CLAY_VESSEL, PROCESS_KNAP_STONE_TOOL,
};

pub(crate) fn build_crafting_registry() -> CraftingRegistry {
    CraftingRegistry::new([
        ManualCraftDefinition::new(
            PROCESS_KNAP_STONE_TOOL,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000),
            TickSpan::new(40),
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
            vec![ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_CLAY, FORM_UNFIRED_POTTERY),
                Mass::from_milligrams(1_000),
            )],
        ),
    ])
}
