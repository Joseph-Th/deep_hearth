//! Commodity and equipment appearance bindings.

use crate::material::CommodityKey;
use crate::texture::{CommodityAppearanceBinding, EquipmentAppearanceBinding};

use crate::content::equipment::{
    EQUIPMENT_CASTING_MOLD, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
    EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    EQUIPMENT_DRY_SCREEN, EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_GRAVITY_SEPARATOR,
    EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER, EQUIPMENT_STONE_CRUSHER,
    EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK, EQUIPMENT_STONE_QUARRY_PICK,
    EQUIPMENT_STONE_SEPARATOR, EQUIPMENT_TIMBER_TREADLE_DRIVE,
};
use crate::content::materials::{
    FORM_BOARD, FORM_BULK_CRATE_BODY, FORM_CHEST_BODY, FORM_CHIP, FORM_CONCENTRATE, FORM_CRUSHED,
    FORM_DOUBLE_WALL_CHEST_BODY, FORM_FLYWHEEL, FORM_HANDLE, FORM_INGOT,
    FORM_INSULATED_PANTRY_BODY, FORM_LOG, FORM_LUMP, FORM_MOLTEN, FORM_NATIVE_METAL, FORM_ORE,
    FORM_REINFORCEMENT, FORM_ROUGH_BOX_BODY, FORM_SCRAP, FORM_STONE_CROCK_BODY, FORM_TAILINGS,
    FORM_TOOL, MATERIAL_CHARCOAL, MATERIAL_CLAY, MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_STONE,
    MATERIAL_WOOD,
};

use super::{
    BLOCK_CHARCOAL, BLOCK_COPPER, BLOCK_COPPER_ORE, BLOCK_SLAG, BLOCK_TIMBER,
    OBJECT_BULK_TIMBER_CRATE_BODY, OBJECT_CASTING_MOLD, OBJECT_CHARCOAL, OBJECT_COPPER_INGOT,
    OBJECT_COPPER_ORE, OBJECT_COPPER_REINFORCED_HAND_CRANK, OBJECT_COPPER_REINFORCED_PICK,
    OBJECT_COPPER_REINFORCED_STONE_CRUSHER, OBJECT_COPPER_REINFORCED_STONE_QUARRY_PICK,
    OBJECT_COPPER_REINFORCED_STONE_SEPARATOR, OBJECT_COPPER_REINFORCEMENT, OBJECT_COPPER_SCRAP,
    OBJECT_CRUSHED_ORE, OBJECT_DOUBLE_WALL_TIMBER_CHEST_BODY, OBJECT_DRY_SCREEN,
    OBJECT_ELECTRIC_FURNACE, OBJECT_GRAVITY_SEPARATOR, OBJECT_GRINDING_MILL,
    OBJECT_INSULATED_TIMBER_PANTRY_BODY, OBJECT_JAW_CRUSHER, OBJECT_LOG, OBJECT_MOLTEN_COPPER,
    OBJECT_NATIVE_COPPER, OBJECT_ROUGH_TIMBER_FIELD_BOX_BODY, OBJECT_SLAG, OBJECT_STONE_CHIP,
    OBJECT_STONE_CRUSHER, OBJECT_STONE_FLYWHEEL, OBJECT_STONE_HAND_CRANK, OBJECT_STONE_LUMP,
    OBJECT_STONE_PICK, OBJECT_STONE_PROVISIONS_CROCK_BODY, OBJECT_STONE_QUARRY_PICK,
    OBJECT_STONE_SEPARATOR, OBJECT_STONE_TOOL, OBJECT_TAILINGS, OBJECT_TIMBER_CHEST_BODY,
    OBJECT_TIMBER_TREADLE_DRIVE, OBJECT_WOOD_BOARD, OBJECT_WOOD_CHIP, OBJECT_WOOD_HANDLE,
};

pub(super) fn build_commodity_bindings() -> Vec<CommodityAppearanceBinding> {
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
            CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
            Some(BLOCK_CHARCOAL),
            Some(OBJECT_CHARCOAL),
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
            CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
            None,
            Some(OBJECT_MOLTEN_COPPER),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_SLAG, FORM_LUMP),
            Some(BLOCK_SLAG),
            Some(OBJECT_SLAG),
        ),
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_SLAG, FORM_CRUSHED),
            None,
            Some(OBJECT_SLAG),
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
    ]
}

pub(super) fn build_equipment_bindings() -> Vec<EquipmentAppearanceBinding> {
    vec![
        EquipmentAppearanceBinding::new(EQUIPMENT_JAW_CRUSHER, OBJECT_JAW_CRUSHER),
        EquipmentAppearanceBinding::new(EQUIPMENT_ELECTRIC_FURNACE, OBJECT_ELECTRIC_FURNACE),
        EquipmentAppearanceBinding::new(EQUIPMENT_CASTING_MOLD, OBJECT_CASTING_MOLD),
        EquipmentAppearanceBinding::new(EQUIPMENT_DRY_SCREEN, OBJECT_DRY_SCREEN),
        EquipmentAppearanceBinding::new(EQUIPMENT_GRINDING_MILL, OBJECT_GRINDING_MILL),
        EquipmentAppearanceBinding::new(EQUIPMENT_STONE_PICK, OBJECT_STONE_PICK),
        EquipmentAppearanceBinding::new(EQUIPMENT_STONE_HAND_CRANK, OBJECT_STONE_HAND_CRANK),
        EquipmentAppearanceBinding::new(EQUIPMENT_STONE_QUARRY_PICK, OBJECT_STONE_QUARRY_PICK),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_TREADLE_DRIVE,
            OBJECT_TIMBER_TREADLE_DRIVE,
        ),
        EquipmentAppearanceBinding::new(EQUIPMENT_STONE_CRUSHER, OBJECT_STONE_CRUSHER),
        EquipmentAppearanceBinding::new(EQUIPMENT_STONE_SEPARATOR, OBJECT_STONE_SEPARATOR),
        EquipmentAppearanceBinding::new(EQUIPMENT_GRAVITY_SEPARATOR, OBJECT_GRAVITY_SEPARATOR),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_COPPER_REINFORCED_PICK,
            OBJECT_COPPER_REINFORCED_PICK,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            OBJECT_COPPER_REINFORCED_HAND_CRANK,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
            OBJECT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            OBJECT_COPPER_REINFORCED_STONE_CRUSHER,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            OBJECT_COPPER_REINFORCED_STONE_SEPARATOR,
        ),
    ]
}
