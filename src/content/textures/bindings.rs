//! Commodity and equipment appearance bindings.

use crate::material::CommodityKey;
use crate::texture::{CommodityAppearanceBinding, EquipmentAppearanceBinding};

use crate::content::equipment::{
    EQUIPMENT_CASTING_MOLD, EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
    EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
    EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK, EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
    EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR, EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
    EQUIPMENT_DRY_SCREEN, EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_GRAVITY_SEPARATOR,
    EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER, EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
    EQUIPMENT_STONE_COBBING_HAMMER, EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL,
    EQUIPMENT_STONE_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_INGOT_MOLD,
    EQUIPMENT_STONE_PICK, EQUIPMENT_STONE_QUARRY_PICK, EQUIPMENT_STONE_ROTARY_QUERN,
    EQUIPMENT_STONE_SEPARATOR, EQUIPMENT_STONE_WOODWORKING_ADZE, EQUIPMENT_TIMBER_DRESSING_BENCH,
    EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH, EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
    EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL, EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
    EQUIPMENT_TIMBER_HELVE_HAMMER, EQUIPMENT_TIMBER_ORE_DRESSING_TABLE,
    EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN, EQUIPMENT_TIMBER_SASH_SAWMILL,
    EQUIPMENT_TIMBER_SPINDLE_DRILL, EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
    EQUIPMENT_TIMBER_TREADLE_DRIVE, EQUIPMENT_TIMBER_TREADLE_DYNAMO,
    EQUIPMENT_TIMBER_TREADLE_GRINDSTONE, EQUIPMENT_TIMBER_TREADLE_HAMMER,
    EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
};
use crate::content::materials::{
    FORM_BOARD, FORM_BULK_CRATE_BODY, FORM_CHEST_BODY, FORM_CHIP, FORM_CONCENTRATE, FORM_CRUSHED,
    FORM_DOUBLE_WALL_CHEST_BODY, FORM_DRILL_BIT, FORM_EXHAUSTED_TAILINGS, FORM_FLYWHEEL,
    FORM_GRINDSTONE_WHEEL, FORM_HANDLE, FORM_INGOT, FORM_INSULATED_PANTRY_BODY, FORM_LOG,
    FORM_LUMP, FORM_MOLTEN, FORM_NATIVE_METAL, FORM_ORE, FORM_REINFORCEMENT, FORM_ROUGH_BOX_BODY,
    FORM_SAW_BLADE, FORM_SCRAP, FORM_SCREEN_PLATE, FORM_STONE_CROCK_BODY, FORM_TAILINGS,
    FORM_TIMBER_RIDDLE_PANEL, FORM_TOOL, MATERIAL_CLAY, MATERIAL_COPPER, MATERIAL_SLAG,
    MATERIAL_STONE, MATERIAL_WOOD,
};

use super::{
    BLOCK_COPPER, BLOCK_COPPER_ORE, BLOCK_TIMBER, OBJECT_BULK_TIMBER_CRATE_BODY,
    OBJECT_CASTING_MOLD, OBJECT_COPPER_INGOT, OBJECT_COPPER_ORE, OBJECT_COPPER_PLATE_SIZING_SCREEN,
    OBJECT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, OBJECT_COPPER_REINFORCED_HAND_CRANK,
    OBJECT_COPPER_REINFORCED_PICK, OBJECT_COPPER_REINFORCED_STONE_CRUSHER,
    OBJECT_COPPER_REINFORCED_STONE_QUARRY_PICK, OBJECT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
    OBJECT_COPPER_REINFORCED_STONE_SEPARATOR, OBJECT_COPPER_REINFORCED_WOODWORKING_ADZE,
    OBJECT_COPPER_REINFORCEMENT, OBJECT_COPPER_SAW_BLADE, OBJECT_COPPER_SCRAP,
    OBJECT_COPPER_SCREEN_PLATE, OBJECT_CRUSHED_ORE, OBJECT_DOUBLE_WALL_TIMBER_CHEST_BODY,
    OBJECT_DRY_SCREEN, OBJECT_ELECTRIC_FURNACE, OBJECT_GRAVITY_SEPARATOR, OBJECT_GRINDING_MILL,
    OBJECT_INSULATED_TIMBER_PANTRY_BODY, OBJECT_JAW_CRUSHER, OBJECT_LOG, OBJECT_MOLTEN_COPPER,
    OBJECT_NATIVE_COPPER, OBJECT_ROUGH_TIMBER_FIELD_BOX_BODY, OBJECT_STONE_CHIP,
    OBJECT_STONE_COBBING_HAMMER, OBJECT_STONE_CRUSHER, OBJECT_STONE_DRILL_BIT,
    OBJECT_STONE_FLYWHEEL, OBJECT_STONE_FLYWHEEL_PUMP_DRILL, OBJECT_STONE_GEOLOGICAL_HAMMER,
    OBJECT_STONE_GRINDSTONE_WHEEL, OBJECT_STONE_HAND_CRANK, OBJECT_STONE_LUMP, OBJECT_STONE_PICK,
    OBJECT_STONE_PROVISIONS_CROCK_BODY, OBJECT_STONE_QUARRY_PICK, OBJECT_STONE_ROTARY_QUERN,
    OBJECT_STONE_SEPARATOR, OBJECT_STONE_TOOL, OBJECT_STONE_WOODWORKING_ADZE, OBJECT_TAILINGS,
    OBJECT_TIMBER_CHEST_BODY, OBJECT_TIMBER_DRESSING_BENCH, OBJECT_TIMBER_FLYWHEEL,
    OBJECT_TIMBER_FLYWHEEL_GRINDING_BENCH, OBJECT_TIMBER_FLYWHEEL_LATHE,
    OBJECT_TIMBER_FRAME_COMMINUTION_MILL, OBJECT_TIMBER_FRAME_SAW_BENCH,
    OBJECT_TIMBER_HELVE_HAMMER, OBJECT_TIMBER_ORE_DRESSING_TABLE, OBJECT_TIMBER_RIDDLE_PANEL,
    OBJECT_TIMBER_RIDDLE_SIZING_SCREEN, OBJECT_TIMBER_SASH_SAWMILL, OBJECT_TIMBER_SPINDLE_DRILL,
    OBJECT_TIMBER_SPRING_POLE_LATHE, OBJECT_TIMBER_TREADLE_DRIVE, OBJECT_TIMBER_TREADLE_GRINDSTONE,
    OBJECT_TIMBER_TREADLE_HAMMER, OBJECT_TIMBER_WALKING_WHEEL_DRIVE, OBJECT_WOOD_BOARD,
    OBJECT_WOOD_CHIP, OBJECT_WOOD_HANDLE,
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
        EquipmentAppearanceBinding::new(EQUIPMENT_STONE_ROTARY_QUERN, OBJECT_STONE_ROTARY_QUERN),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
            OBJECT_STONE_GEOLOGICAL_HAMMER,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_STONE_COBBING_HAMMER,
            OBJECT_STONE_COBBING_HAMMER,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL,
            OBJECT_STONE_FLYWHEEL_PUMP_DRILL,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_SPINDLE_DRILL,
            OBJECT_TIMBER_SPINDLE_DRILL,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
            OBJECT_TIMBER_SPRING_POLE_LATHE,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
            OBJECT_TIMBER_FLYWHEEL_LATHE,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_TREADLE_GRINDSTONE,
            OBJECT_TIMBER_TREADLE_GRINDSTONE,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
            OBJECT_TIMBER_FLYWHEEL_GRINDING_BENCH,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
            OBJECT_COPPER_PLATE_SIZING_SCREEN,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
            OBJECT_TIMBER_RIDDLE_SIZING_SCREEN,
        ),
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
        EquipmentAppearanceBinding::new(
            EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
            OBJECT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
            OBJECT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_STONE_WOODWORKING_ADZE,
            OBJECT_STONE_WOODWORKING_ADZE,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
            OBJECT_COPPER_REINFORCED_WOODWORKING_ADZE,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
            OBJECT_TIMBER_FRAME_SAW_BENCH,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_DRESSING_BENCH,
            OBJECT_TIMBER_DRESSING_BENCH,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL,
            OBJECT_TIMBER_FRAME_COMMINUTION_MILL,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_ORE_DRESSING_TABLE,
            OBJECT_TIMBER_ORE_DRESSING_TABLE,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
            OBJECT_TIMBER_WALKING_WHEEL_DRIVE,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_TREADLE_HAMMER,
            OBJECT_TIMBER_TREADLE_HAMMER,
        ),
        EquipmentAppearanceBinding::new(EQUIPMENT_TIMBER_SASH_SAWMILL, OBJECT_TIMBER_SASH_SAWMILL),
        EquipmentAppearanceBinding::new(EQUIPMENT_TIMBER_HELVE_HAMMER, OBJECT_TIMBER_HELVE_HAMMER),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_TIMBER_TREADLE_DYNAMO,
            OBJECT_TIMBER_TREADLE_DRIVE,
        ),
        EquipmentAppearanceBinding::new(
            EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
            OBJECT_ELECTRIC_FURNACE,
        ),
        EquipmentAppearanceBinding::new(EQUIPMENT_STONE_INGOT_MOLD, OBJECT_CASTING_MOLD),
    ]
}
