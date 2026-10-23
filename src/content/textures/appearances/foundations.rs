//! Foundational resources, storage bodies, and installed industrial object appearances.

use crate::texture::ObjectAppearanceDefinition;

use super::super::{
    OBJECT_BULK_TIMBER_CRATE_BODY, OBJECT_CASSITERITE_ORE, OBJECT_CASTING_MOLD,
    OBJECT_COPPER_ELECTRICAL_WINDING, OBJECT_COPPER_INGOT, OBJECT_COPPER_ORE,
    OBJECT_CRUSHED_CASSITERITE, OBJECT_CRUSHED_GALENA, OBJECT_CRUSHED_GRAPHITE,
    OBJECT_CRUSHED_HEMATITE, OBJECT_CRUSHED_MAGNETITE, OBJECT_CRUSHED_ORE, OBJECT_CRUSHED_PYRITE,
    OBJECT_CRUSHED_QUARTZ, OBJECT_CRUSHED_SPHALERITE, OBJECT_DOUBLE_WALL_TIMBER_CHEST_BODY,
    OBJECT_DRY_SCREEN, OBJECT_ELECTRIC_FURNACE, OBJECT_GALENA_ORE, OBJECT_GRAPHITE_ORE,
    OBJECT_GRINDING_MILL, OBJECT_HEMATITE_ORE, OBJECT_INSULATED_TIMBER_PANTRY_BODY,
    OBJECT_IRON_SCRAP, OBJECT_IRON_STOCK, OBJECT_JAW_CRUSHER, OBJECT_LOG, OBJECT_MAGNETITE_ORE,
    OBJECT_MOLTEN_COPPER, OBJECT_MOLTEN_IRON, OBJECT_MOLTEN_TIN, OBJECT_MOLTEN_ZINC,
    OBJECT_PYRITE_ORE, OBJECT_QUARTZ_ORE, OBJECT_ROUGH_TIMBER_FIELD_BOX_BODY,
    OBJECT_SPHALERITE_ORE, OBJECT_STONE_CHIP, OBJECT_STONE_DRAWPLATE, OBJECT_STONE_DRILL_BIT,
    OBJECT_STONE_FLYWHEEL, OBJECT_STONE_GRINDSTONE_WHEEL, OBJECT_STONE_LUMP,
    OBJECT_STONE_PROVISIONS_CROCK_BODY, OBJECT_STONE_TOOL, OBJECT_TIMBER_CHEST_BODY,
    OBJECT_TIMBER_FLYWHEEL, OBJECT_TIN_SCRAP, OBJECT_TIN_STOCK, OBJECT_WOOD_BOARD,
    OBJECT_WOOD_CHIP, OBJECT_WOOD_HANDLE, OBJECT_ZINC_SCRAP, OBJECT_ZINC_STOCK,
    TEXTURE_CASSITERITE_ORE, TEXTURE_COPPER_HAMMERED, TEXTURE_COPPER_ORE, TEXTURE_CRUSHED_ORE,
    TEXTURE_GALENA_ORE, TEXTURE_GRAPHITE_ORE, TEXTURE_HEMATITE_ORE, TEXTURE_MACHINE_PANEL,
    TEXTURE_MAGNETITE_ORE, TEXTURE_MOLTEN_COPPER, TEXTURE_MOLTEN_IRON, TEXTURE_MOLTEN_TIN,
    TEXTURE_MOLTEN_ZINC, TEXTURE_PYRITE_ORE, TEXTURE_QUARTZ_ORE, TEXTURE_REFRACTORY,
    TEXTURE_SCREEN_MESH, TEXTURE_SPHALERITE_ORE, TEXTURE_STONE, TEXTURE_TEMPLE_BEARING_HOUSING,
    TEXTURE_TEMPLE_CORROSION_PANEL, TEXTURE_TEMPLE_DATA_PLATE, TEXTURE_WOOD_END, TEXTURE_WOOD_SIDE,
    TEXTURE_WORKING_METAL,
};
use super::object;

pub(super) fn definitions() -> impl Iterator<Item = ObjectAppearanceDefinition> {
    [
        object(
            OBJECT_MAGNETITE_ORE,
            "magnetite ore",
            &[TEXTURE_MAGNETITE_ORE],
        ),
        object(
            OBJECT_CRUSHED_MAGNETITE,
            "crushed magnetite",
            &[TEXTURE_MAGNETITE_ORE, TEXTURE_CRUSHED_ORE],
        ),
        object(OBJECT_GALENA_ORE, "galena ore", &[TEXTURE_GALENA_ORE]),
        object(
            OBJECT_CRUSHED_GALENA,
            "crushed galena",
            &[TEXTURE_GALENA_ORE, TEXTURE_CRUSHED_ORE],
        ),
        object(OBJECT_PYRITE_ORE, "pyrite ore", &[TEXTURE_PYRITE_ORE]),
        object(
            OBJECT_CRUSHED_PYRITE,
            "crushed pyrite",
            &[TEXTURE_PYRITE_ORE, TEXTURE_CRUSHED_ORE],
        ),
        object(OBJECT_GRAPHITE_ORE, "graphite ore", &[TEXTURE_GRAPHITE_ORE]),
        object(
            OBJECT_CRUSHED_GRAPHITE,
            "crushed graphite",
            &[TEXTURE_GRAPHITE_ORE, TEXTURE_CRUSHED_ORE],
        ),
        object(OBJECT_QUARTZ_ORE, "quartz ore", &[TEXTURE_QUARTZ_ORE]),
        object(
            OBJECT_CRUSHED_QUARTZ,
            "crushed quartz",
            &[TEXTURE_QUARTZ_ORE, TEXTURE_CRUSHED_ORE],
        ),
        object(OBJECT_HEMATITE_ORE, "hematite ore", &[TEXTURE_HEMATITE_ORE]),
        object(
            OBJECT_CASSITERITE_ORE,
            "cassiterite ore",
            &[TEXTURE_CASSITERITE_ORE],
        ),
        object(
            OBJECT_SPHALERITE_ORE,
            "sphalerite ore",
            &[TEXTURE_SPHALERITE_ORE],
        ),
        object(
            OBJECT_CRUSHED_HEMATITE,
            "crushed hematite",
            &[TEXTURE_HEMATITE_ORE, TEXTURE_CRUSHED_ORE],
        ),
        object(
            OBJECT_CRUSHED_CASSITERITE,
            "crushed cassiterite",
            &[TEXTURE_CASSITERITE_ORE, TEXTURE_CRUSHED_ORE],
        ),
        object(
            OBJECT_CRUSHED_SPHALERITE,
            "crushed sphalerite",
            &[TEXTURE_SPHALERITE_ORE, TEXTURE_CRUSHED_ORE],
        ),
        object(OBJECT_MOLTEN_IRON, "molten iron", &[TEXTURE_MOLTEN_IRON]),
        object(OBJECT_MOLTEN_TIN, "molten tin", &[TEXTURE_MOLTEN_TIN]),
        object(OBJECT_MOLTEN_ZINC, "molten zinc", &[TEXTURE_MOLTEN_ZINC]),
        object(
            OBJECT_IRON_STOCK,
            "iron stock",
            &[TEXTURE_TEMPLE_BEARING_HOUSING],
        ),
        object(OBJECT_TIN_STOCK, "tin stock", &[TEXTURE_TEMPLE_DATA_PLATE]),
        object(
            OBJECT_ZINC_STOCK,
            "zinc stock",
            &[TEXTURE_TEMPLE_CORROSION_PANEL],
        ),
        object(
            OBJECT_IRON_SCRAP,
            "iron scrap",
            &[TEXTURE_WORKING_METAL, TEXTURE_TEMPLE_BEARING_HOUSING],
        ),
        object(
            OBJECT_TIN_SCRAP,
            "tin scrap",
            &[TEXTURE_WORKING_METAL, TEXTURE_TEMPLE_DATA_PLATE],
        ),
        object(
            OBJECT_ZINC_SCRAP,
            "zinc scrap",
            &[TEXTURE_WORKING_METAL, TEXTURE_TEMPLE_CORROSION_PANEL],
        ),
        object(OBJECT_LOG, "log", &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END]),
        object(OBJECT_COPPER_ORE, "copper ore", &[TEXTURE_COPPER_ORE]),
        object(
            OBJECT_CRUSHED_ORE,
            "crushed copper ore",
            &[TEXTURE_CRUSHED_ORE],
        ),
        object(
            OBJECT_COPPER_INGOT,
            "copper ingot",
            &[TEXTURE_COPPER_HAMMERED],
        ),
        object(
            OBJECT_COPPER_ELECTRICAL_WINDING,
            "copper conductor winding",
            &[TEXTURE_COPPER_HAMMERED, TEXTURE_WORKING_METAL],
        ),
        object(
            OBJECT_MOLTEN_COPPER,
            "molten copper",
            &[TEXTURE_MOLTEN_COPPER],
        ),
        object(OBJECT_STONE_LUMP, "stone lump", &[TEXTURE_STONE]),
        object(
            OBJECT_WOOD_HANDLE,
            "wood handle",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
        ),
        object(
            OBJECT_JAW_CRUSHER,
            "jaw crusher",
            &[TEXTURE_MACHINE_PANEL, TEXTURE_WORKING_METAL],
        ),
        object(
            OBJECT_ELECTRIC_FURNACE,
            "electric furnace",
            &[
                TEXTURE_MACHINE_PANEL,
                TEXTURE_REFRACTORY,
                TEXTURE_MOLTEN_COPPER,
            ],
        ),
        object(
            OBJECT_ROUGH_TIMBER_FIELD_BOX_BODY,
            "assembled rough timber field box body",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
        ),
        object(
            OBJECT_STONE_PROVISIONS_CROCK_BODY,
            "carved stone provisions crock body",
            &[TEXTURE_STONE, TEXTURE_STONE],
        ),
        object(
            OBJECT_CASTING_MOLD,
            "casting mold",
            &[TEXTURE_WORKING_METAL, TEXTURE_COPPER_HAMMERED],
        ),
        object(
            OBJECT_DRY_SCREEN,
            "dry screen",
            &[TEXTURE_MACHINE_PANEL, TEXTURE_SCREEN_MESH],
        ),
        object(
            OBJECT_GRINDING_MILL,
            "grinding mill",
            &[TEXTURE_MACHINE_PANEL, TEXTURE_WORKING_METAL],
        ),
        object(OBJECT_STONE_TOOL, "worked stone tool", &[TEXTURE_STONE]),
        object(
            OBJECT_STONE_DRILL_BIT,
            "knapped rotary drill bit",
            &[TEXTURE_STONE],
        ),
        object(
            OBJECT_STONE_DRAWPLATE,
            "polished stone drawplate",
            &[TEXTURE_STONE, TEXTURE_WORKING_METAL],
        ),
        object(OBJECT_STONE_CHIP, "stone chips", &[TEXTURE_STONE]),
        object(OBJECT_WOOD_CHIP, "wood chips", &[TEXTURE_WOOD_SIDE]),
        object(
            OBJECT_WOOD_BOARD,
            "timber boards",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
        ),
        object(
            OBJECT_TIMBER_CHEST_BODY,
            "assembled timber chest body",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
        ),
        object(
            OBJECT_DOUBLE_WALL_TIMBER_CHEST_BODY,
            "assembled double-wall timber chest body",
            &[TEXTURE_WOOD_END, TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
        ),
        object(
            OBJECT_BULK_TIMBER_CRATE_BODY,
            "assembled slatted timber bulk crate body",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END, TEXTURE_WOOD_SIDE],
        ),
        object(
            OBJECT_INSULATED_TIMBER_PANTRY_BODY,
            "assembled insulated timber pantry body",
            &[
                TEXTURE_WOOD_END,
                TEXTURE_WOOD_SIDE,
                TEXTURE_WOOD_END,
                TEXTURE_WOOD_SIDE,
            ],
        ),
        object(OBJECT_STONE_FLYWHEEL, "stone flywheel", &[TEXTURE_STONE]),
        object(
            OBJECT_STONE_GRINDSTONE_WHEEL,
            "abrasive grindstone wheel",
            &[TEXTURE_STONE],
        ),
        object(
            OBJECT_TIMBER_FLYWHEEL,
            "solid timber flywheel",
            &[TEXTURE_WOOD_END, TEXTURE_WOOD_SIDE],
        ),
    ]
    .into_iter()
}
