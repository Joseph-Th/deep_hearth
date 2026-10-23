//! Commodity-to-appearance bindings for visible material forms.

use crate::content::materials::{
    FORM_BLOCK, FORM_BOARD, FORM_BULK_CRATE_BODY, FORM_CHEST_BODY, FORM_CHIP, FORM_CONCENTRATE,
    FORM_CRUSHED, FORM_CRUSHER_BLOCK, FORM_CRUSHER_JAW_FACE, FORM_DOUBLE_WALL_CHEST_BODY,
    FORM_DRAWPLATE, FORM_DRILL_BIT, FORM_ELECTRICAL_WINDING, FORM_EXHAUSTED_TAILINGS,
    FORM_FLYWHEEL, FORM_GRINDSTONE_WHEEL, FORM_HANDLE, FORM_INGOT, FORM_INSULATED_PANTRY_BODY,
    FORM_LOG, FORM_LUMP, FORM_MATTE, FORM_MOLTEN, FORM_NATIVE_METAL, FORM_ORE,
    FORM_PACKED_CLAY_BINDER, FORM_REINFORCEMENT, FORM_ROUGH_BOX_BODY, FORM_SAW_BLADE, FORM_SCRAP,
    FORM_SCREEN_PLATE, FORM_STONE_CROCK_BODY, FORM_TAILINGS, FORM_TIMBER_RIDDLE_PANEL, FORM_TOOL,
    MATERIAL_CASSITERITE, MATERIAL_CLAY, MATERIAL_COPPER, MATERIAL_GALENA, MATERIAL_GRAPHITE,
    MATERIAL_HEMATITE, MATERIAL_IRON, MATERIAL_IRON_SULFIDE, MATERIAL_LEAD, MATERIAL_MAGNETITE,
    MATERIAL_PYRITE, MATERIAL_QUARTZ, MATERIAL_SLAG, MATERIAL_SPHALERITE, MATERIAL_STONE,
    MATERIAL_TIN, MATERIAL_WOOD, MATERIAL_ZINC,
};
use crate::material::CommodityKey;
use crate::texture::CommodityAppearanceBinding;

use super::super::{
    BLOCK_CASSITERITE_ORE, BLOCK_COPPER, BLOCK_COPPER_ORE, BLOCK_GALENA_ORE, BLOCK_GRAPHITE_ORE,
    BLOCK_HEMATITE_ORE, BLOCK_MAGNETITE_ORE, BLOCK_PYRITE_ORE, BLOCK_QUARTZ_ORE,
    BLOCK_SPHALERITE_ORE, BLOCK_STONE, BLOCK_TEMPLE_BEARING_HOUSING, BLOCK_TEMPLE_CORROSION_PANEL,
    BLOCK_TEMPLE_DATA_PLATE, BLOCK_TIMBER, OBJECT_BULK_TIMBER_CRATE_BODY, OBJECT_CASSITERITE_ORE,
    OBJECT_CLAY_EARTH_LUMP, OBJECT_COPPER_ELECTRICAL_WINDING, OBJECT_COPPER_INGOT,
    OBJECT_COPPER_ORE, OBJECT_COPPER_REINFORCEMENT, OBJECT_COPPER_SAW_BLADE, OBJECT_COPPER_SCRAP,
    OBJECT_COPPER_SCREEN_PLATE, OBJECT_CRUSHED_CASSITERITE, OBJECT_CRUSHED_GALENA,
    OBJECT_CRUSHED_GRAPHITE, OBJECT_CRUSHED_HEMATITE, OBJECT_CRUSHED_MAGNETITE, OBJECT_CRUSHED_ORE,
    OBJECT_CRUSHED_PYRITE, OBJECT_CRUSHED_QUARTZ, OBJECT_CRUSHED_SPHALERITE,
    OBJECT_DOUBLE_WALL_TIMBER_CHEST_BODY, OBJECT_GALENA_ORE, OBJECT_GRAPHITE_ORE,
    OBJECT_HEMATITE_ORE, OBJECT_INSULATED_TIMBER_PANTRY_BODY, OBJECT_IRON_PICK_HEAD,
    OBJECT_IRON_SCRAP, OBJECT_IRON_STOCK, OBJECT_IRON_SULFIDE_MATTE, OBJECT_LEAD_INGOT,
    OBJECT_LEAD_SCRAP, OBJECT_LOG, OBJECT_MAGNETITE_ORE, OBJECT_MOLTEN_COPPER, OBJECT_MOLTEN_IRON,
    OBJECT_MOLTEN_LEAD, OBJECT_MOLTEN_TIN, OBJECT_MOLTEN_ZINC, OBJECT_NATIVE_COPPER,
    OBJECT_PACKED_CLAY_BINDER, OBJECT_PYRITE_ORE, OBJECT_QUARTZ_ORE,
    OBJECT_ROUGH_TIMBER_FIELD_BOX_BODY, OBJECT_SPHALERITE_ORE, OBJECT_STONE_CHIP,
    OBJECT_STONE_DRAWPLATE, OBJECT_STONE_DRILL_BIT, OBJECT_STONE_FLYWHEEL,
    OBJECT_STONE_GRINDSTONE_WHEEL, OBJECT_STONE_LUMP, OBJECT_STONE_PROVISIONS_CROCK_BODY,
    OBJECT_STONE_TOOL, OBJECT_TAILINGS, OBJECT_TIMBER_CHEST_BODY, OBJECT_TIMBER_FLYWHEEL,
    OBJECT_TIMBER_RIDDLE_PANEL, OBJECT_TIN_SCRAP, OBJECT_TIN_STOCK, OBJECT_WOOD_BOARD,
    OBJECT_WOOD_CHIP, OBJECT_WOOD_HANDLE, OBJECT_ZINC_SCRAP, OBJECT_ZINC_STOCK,
};

pub(in crate::content::textures) fn build_commodity_bindings() -> Vec<CommodityAppearanceBinding> {
    vec![
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_LEAD, FORM_INGOT),
            None,
            Some(OBJECT_LEAD_INGOT),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_LEAD, FORM_MOLTEN),
            None,
            Some(OBJECT_MOLTEN_LEAD),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_LEAD, FORM_SCRAP),
            None,
            Some(OBJECT_LEAD_SCRAP),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_IRON_SULFIDE, FORM_MATTE),
            None,
            Some(OBJECT_IRON_SULFIDE_MATTE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_MAGNETITE, FORM_ORE),
            Some(BLOCK_MAGNETITE_ORE),
            Some(OBJECT_MAGNETITE_ORE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_MAGNETITE, FORM_CRUSHED),
            None,
            Some(OBJECT_CRUSHED_MAGNETITE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_MAGNETITE, FORM_CONCENTRATE),
            None,
            Some(OBJECT_CRUSHED_MAGNETITE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_MAGNETITE, FORM_TAILINGS),
            None,
            Some(OBJECT_TAILINGS),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_GALENA, FORM_ORE),
            Some(BLOCK_GALENA_ORE),
            Some(OBJECT_GALENA_ORE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_GALENA, FORM_CRUSHED),
            None,
            Some(OBJECT_CRUSHED_GALENA),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_PYRITE, FORM_ORE),
            Some(BLOCK_PYRITE_ORE),
            Some(OBJECT_PYRITE_ORE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_PYRITE, FORM_CRUSHED),
            None,
            Some(OBJECT_CRUSHED_PYRITE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_GRAPHITE, FORM_ORE),
            Some(BLOCK_GRAPHITE_ORE),
            Some(OBJECT_GRAPHITE_ORE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_GRAPHITE, FORM_CRUSHED),
            None,
            Some(OBJECT_CRUSHED_GRAPHITE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_QUARTZ, FORM_ORE),
            Some(BLOCK_QUARTZ_ORE),
            Some(OBJECT_QUARTZ_ORE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_QUARTZ, FORM_CRUSHED),
            None,
            Some(OBJECT_CRUSHED_QUARTZ),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_IRON, FORM_TOOL),
            None,
            Some(OBJECT_IRON_PICK_HEAD),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_HEMATITE, FORM_ORE),
            Some(BLOCK_HEMATITE_ORE),
            Some(OBJECT_HEMATITE_ORE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_HEMATITE, FORM_CRUSHED),
            None,
            Some(OBJECT_CRUSHED_HEMATITE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_CASSITERITE, FORM_ORE),
            Some(BLOCK_CASSITERITE_ORE),
            Some(OBJECT_CASSITERITE_ORE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_CASSITERITE, FORM_CRUSHED),
            None,
            Some(OBJECT_CRUSHED_CASSITERITE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_CASSITERITE, FORM_CONCENTRATE),
            None,
            Some(OBJECT_CRUSHED_CASSITERITE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_CASSITERITE, FORM_TAILINGS),
            None,
            Some(OBJECT_TAILINGS),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_SPHALERITE, FORM_ORE),
            Some(BLOCK_SPHALERITE_ORE),
            Some(OBJECT_SPHALERITE_ORE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_SPHALERITE, FORM_CRUSHED),
            None,
            Some(OBJECT_CRUSHED_SPHALERITE),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_IRON, FORM_MOLTEN),
            None,
            Some(OBJECT_MOLTEN_IRON),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_TIN, FORM_MOLTEN),
            None,
            Some(OBJECT_MOLTEN_TIN),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_ZINC, FORM_MOLTEN),
            None,
            Some(OBJECT_MOLTEN_ZINC),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_IRON, FORM_INGOT),
            Some(BLOCK_TEMPLE_BEARING_HOUSING),
            Some(OBJECT_IRON_STOCK),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_IRON, FORM_BLOCK),
            Some(BLOCK_TEMPLE_BEARING_HOUSING),
            Some(OBJECT_IRON_STOCK),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_IRON, FORM_SCRAP),
            None,
            Some(OBJECT_IRON_SCRAP),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_IRON, FORM_CHIP),
            None,
            Some(OBJECT_IRON_SCRAP),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_TIN, FORM_INGOT),
            Some(BLOCK_TEMPLE_DATA_PLATE),
            Some(OBJECT_TIN_STOCK),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_TIN, FORM_BLOCK),
            Some(BLOCK_TEMPLE_DATA_PLATE),
            Some(OBJECT_TIN_STOCK),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_TIN, FORM_SCRAP),
            None,
            Some(OBJECT_TIN_SCRAP),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_TIN, FORM_CHIP),
            None,
            Some(OBJECT_TIN_SCRAP),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_ZINC, FORM_INGOT),
            Some(BLOCK_TEMPLE_CORROSION_PANEL),
            Some(OBJECT_ZINC_STOCK),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_ZINC, FORM_BLOCK),
            Some(BLOCK_TEMPLE_CORROSION_PANEL),
            Some(OBJECT_ZINC_STOCK),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_ZINC, FORM_SCRAP),
            None,
            Some(OBJECT_ZINC_SCRAP),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_ZINC, FORM_CHIP),
            None,
            Some(OBJECT_ZINC_SCRAP),
        ),
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
            CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
            None,
            Some(OBJECT_COPPER_ELECTRICAL_WINDING),
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
            CommodityKey::new(MATERIAL_CLAY, FORM_LUMP),
            None,
            Some(OBJECT_CLAY_EARTH_LUMP),
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
            CommodityKey::new(MATERIAL_CLAY, FORM_PACKED_CLAY_BINDER),
            None,
            Some(OBJECT_PACKED_CLAY_BINDER),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_CLAY, FORM_SCRAP),
            None,
            Some(OBJECT_TAILINGS),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_CLAY, FORM_CHIP),
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
            CommodityKey::new(MATERIAL_STONE, FORM_BLOCK),
            Some(BLOCK_STONE),
            None,
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            None,
            Some(OBJECT_STONE_TOOL),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_CRUSHER_JAW_FACE),
            None,
            Some(OBJECT_STONE_TOOL),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_CRUSHER_BLOCK),
            None,
            Some(OBJECT_STONE_TOOL),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT),
            None,
            Some(OBJECT_STONE_DRILL_BIT),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_STONE, FORM_DRAWPLATE),
            None,
            Some(OBJECT_STONE_DRAWPLATE),
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
