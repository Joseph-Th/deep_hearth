//! Commodity-to-appearance bindings for visible material forms.

use crate::content::materials::{
    FORM_BOARD, FORM_BULK_CRATE_BODY, FORM_CHEST_BODY, FORM_CHIP, FORM_CONCENTRATE, FORM_CRUSHED,
    FORM_DOUBLE_WALL_CHEST_BODY, FORM_DRILL_BIT, FORM_EXHAUSTED_TAILINGS, FORM_FLYWHEEL,
    FORM_GRINDSTONE_WHEEL, FORM_HANDLE, FORM_INGOT, FORM_INSULATED_PANTRY_BODY, FORM_LOG,
    FORM_LUMP, FORM_MOLTEN, FORM_NATIVE_METAL, FORM_ORE, FORM_REINFORCEMENT, FORM_ROUGH_BOX_BODY,
    FORM_SAW_BLADE, FORM_SCRAP, FORM_SCREEN_PLATE, FORM_STONE_CROCK_BODY, FORM_TAILINGS,
    FORM_TIMBER_RIDDLE_PANEL, FORM_TOOL, MATERIAL_CLAY, MATERIAL_COPPER, MATERIAL_SLAG,
    MATERIAL_STONE, MATERIAL_WOOD,
};
use crate::material::CommodityKey;
use crate::texture::CommodityAppearanceBinding;

use super::super::{
    BLOCK_COPPER, BLOCK_COPPER_ORE, BLOCK_TIMBER, OBJECT_BULK_TIMBER_CRATE_BODY,
    OBJECT_COPPER_INGOT, OBJECT_COPPER_ORE, OBJECT_COPPER_REINFORCEMENT, OBJECT_COPPER_SAW_BLADE,
    OBJECT_COPPER_SCRAP, OBJECT_COPPER_SCREEN_PLATE, OBJECT_CRUSHED_ORE,
    OBJECT_DOUBLE_WALL_TIMBER_CHEST_BODY, OBJECT_INSULATED_TIMBER_PANTRY_BODY, OBJECT_LOG,
    OBJECT_MOLTEN_COPPER, OBJECT_NATIVE_COPPER, OBJECT_ROUGH_TIMBER_FIELD_BOX_BODY,
    OBJECT_STONE_CHIP, OBJECT_STONE_DRILL_BIT, OBJECT_STONE_FLYWHEEL,
    OBJECT_STONE_GRINDSTONE_WHEEL, OBJECT_STONE_LUMP, OBJECT_STONE_PROVISIONS_CROCK_BODY,
    OBJECT_STONE_TOOL, OBJECT_TAILINGS, OBJECT_TIMBER_CHEST_BODY, OBJECT_TIMBER_FLYWHEEL,
    OBJECT_TIMBER_RIDDLE_PANEL, OBJECT_WOOD_BOARD, OBJECT_WOOD_CHIP, OBJECT_WOOD_HANDLE,
};

pub(in crate::content::textures) fn build_commodity_bindings() -> Vec<CommodityAppearanceBinding> {
    vec![
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Some(BLOCK_TIMBER),
            Some(OBJECT_LOG),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
            None,
            Some(OBJECT_WOOD_BOARD),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
            None,
            Some(OBJECT_TIMBER_CHEST_BODY),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
            None,
            Some(OBJECT_DOUBLE_WALL_TIMBER_CHEST_BODY),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_BULK_CRATE_BODY),
            None,
            Some(OBJECT_BULK_TIMBER_CRATE_BODY),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_INSULATED_PANTRY_BODY),
            None,
            Some(OBJECT_INSULATED_TIMBER_PANTRY_BODY),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
            None,
            Some(OBJECT_ROUGH_TIMBER_FIELD_BOX_BODY),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
            None,
            Some(OBJECT_STONE_PROVISIONS_CROCK_BODY),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Some(BLOCK_COPPER_ORE),
            Some(OBJECT_COPPER_ORE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_CONCENTRATE),
            None,
            Some(OBJECT_CRUSHED_ORE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
            None,
            Some(OBJECT_CRUSHED_ORE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
            Some(BLOCK_COPPER),
            Some(OBJECT_COPPER_INGOT),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            None,
            Some(OBJECT_COPPER_REINFORCEMENT),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE),
            None,
            Some(OBJECT_COPPER_SCREEN_PLATE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
            None,
            Some(OBJECT_COPPER_SAW_BLADE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            None,
            Some(OBJECT_NATIVE_COPPER),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
            None,
            Some(OBJECT_COPPER_SCRAP),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_CHIP),
            None,
            Some(OBJECT_COPPER_SCRAP),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            None,
            Some(OBJECT_MOLTEN_COPPER),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_SLAG, FORM_CRUSHED),
            None,
            Some(OBJECT_TAILINGS),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_CRUSHED),
            None,
            Some(OBJECT_TAILINGS),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_CLAY, FORM_CRUSHED),
            None,
            Some(OBJECT_TAILINGS),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_SLAG, FORM_TAILINGS),
            None,
            Some(OBJECT_TAILINGS),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_TAILINGS),
            None,
            Some(OBJECT_TAILINGS),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_CLAY, FORM_TAILINGS),
            None,
            Some(OBJECT_TAILINGS),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_SLAG, FORM_EXHAUSTED_TAILINGS),
            None,
            Some(OBJECT_TAILINGS),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_EXHAUSTED_TAILINGS),
            None,
            Some(OBJECT_TAILINGS),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_CLAY, FORM_EXHAUSTED_TAILINGS),
            None,
            Some(OBJECT_TAILINGS),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            None,
            Some(OBJECT_STONE_LUMP),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            None,
            Some(OBJECT_STONE_TOOL),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT),
            None,
            Some(OBJECT_STONE_DRILL_BIT),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_GRINDSTONE_WHEEL),
            None,
            Some(OBJECT_STONE_GRINDSTONE_WHEEL),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            None,
            Some(OBJECT_STONE_CHIP),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            None,
            Some(OBJECT_STONE_CHIP),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            None,
            Some(OBJECT_STONE_FLYWHEEL),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            None,
            Some(OBJECT_WOOD_HANDLE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
            None,
            Some(OBJECT_WOOD_CHIP),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP),
            None,
            Some(OBJECT_WOOD_CHIP),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL),
            None,
            Some(OBJECT_TIMBER_RIDDLE_PANEL),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_FLYWHEEL),
            None,
            Some(OBJECT_TIMBER_FLYWHEEL),
        ),
    ]
}
