//! Foundational resources, storage bodies, and installed industrial object appearances.

use crate::texture::ObjectAppearanceDefinition;

use super::super::{
    OBJECT_BULK_TIMBER_CRATE_BODY, OBJECT_CASTING_MOLD, OBJECT_COPPER_INGOT, OBJECT_COPPER_ORE,
    OBJECT_CRUSHED_ORE, OBJECT_DOUBLE_WALL_TIMBER_CHEST_BODY, OBJECT_DRY_SCREEN,
    OBJECT_ELECTRIC_FURNACE, OBJECT_GRINDING_MILL, OBJECT_INSULATED_TIMBER_PANTRY_BODY,
    OBJECT_JAW_CRUSHER, OBJECT_LOG, OBJECT_MOLTEN_COPPER, OBJECT_ROUGH_TIMBER_FIELD_BOX_BODY,
    OBJECT_STONE_CHIP, OBJECT_STONE_DRILL_BIT, OBJECT_STONE_FLYWHEEL,
    OBJECT_STONE_GRINDSTONE_WHEEL, OBJECT_STONE_LUMP, OBJECT_STONE_PROVISIONS_CROCK_BODY,
    OBJECT_STONE_TOOL, OBJECT_TIMBER_CHEST_BODY, OBJECT_TIMBER_FLYWHEEL, OBJECT_WOOD_BOARD,
    OBJECT_WOOD_CHIP, OBJECT_WOOD_HANDLE, TEXTURE_COPPER_HAMMERED, TEXTURE_COPPER_ORE,
    TEXTURE_CRUSHED_ORE, TEXTURE_MACHINE_PANEL, TEXTURE_MOLTEN_COPPER, TEXTURE_REFRACTORY,
    TEXTURE_SCREEN_MESH, TEXTURE_STONE, TEXTURE_WOOD_END, TEXTURE_WOOD_SIDE, TEXTURE_WORKING_METAL,
};
use super::object;

pub(super) fn definitions() -> impl Iterator<Item = ObjectAppearanceDefinition> {
    [
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
