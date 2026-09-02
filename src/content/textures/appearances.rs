//! Renderer-neutral object appearance definitions.

use crate::texture::{ObjectAppearanceDefinition, ObjectAppearanceId, TextureId};

use super::{
    OBJECT_BULK_TIMBER_CRATE_BODY, OBJECT_CASTING_MOLD, OBJECT_CHARCOAL, OBJECT_COPPER_INGOT,
    OBJECT_COPPER_ORE, OBJECT_COPPER_PLATE_SIZING_SCREEN,
    OBJECT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, OBJECT_COPPER_REINFORCED_HAND_CRANK,
    OBJECT_COPPER_REINFORCED_PICK, OBJECT_COPPER_REINFORCED_STONE_CRUSHER,
    OBJECT_COPPER_REINFORCED_STONE_QUARRY_PICK, OBJECT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
    OBJECT_COPPER_REINFORCED_STONE_SEPARATOR, OBJECT_COPPER_REINFORCED_WOODWORKING_ADZE,
    OBJECT_COPPER_REINFORCEMENT, OBJECT_COPPER_SAW_BLADE, OBJECT_COPPER_SCRAP,
    OBJECT_COPPER_SCREEN_PLATE, OBJECT_CRUSHED_ORE, OBJECT_DOUBLE_WALL_TIMBER_CHEST_BODY,
    OBJECT_DRY_SCREEN, OBJECT_ELECTRIC_FURNACE, OBJECT_GRAVITY_SEPARATOR, OBJECT_GRINDING_MILL,
    OBJECT_INSULATED_TIMBER_PANTRY_BODY, OBJECT_JAW_CRUSHER, OBJECT_LOG, OBJECT_MOLTEN_COPPER,
    OBJECT_NATIVE_COPPER, OBJECT_ROUGH_TIMBER_FIELD_BOX_BODY, OBJECT_SLAG, OBJECT_STONE_CHIP,
    OBJECT_STONE_CRUSHER, OBJECT_STONE_FLYWHEEL, OBJECT_STONE_GEOLOGICAL_HAMMER,
    OBJECT_STONE_HAND_CRANK, OBJECT_STONE_LUMP, OBJECT_STONE_PICK,
    OBJECT_STONE_PROVISIONS_CROCK_BODY, OBJECT_STONE_QUARRY_PICK, OBJECT_STONE_ROTARY_QUERN,
    OBJECT_STONE_SEPARATOR, OBJECT_STONE_TOOL, OBJECT_STONE_WOODWORKING_ADZE, OBJECT_TAILINGS,
    OBJECT_TIMBER_CHEST_BODY, OBJECT_TIMBER_FRAME_SAW_BENCH, OBJECT_TIMBER_TREADLE_DRIVE,
    OBJECT_WOOD_BOARD, OBJECT_WOOD_CHIP, OBJECT_WOOD_HANDLE, TEXTURE_COPPER_HAMMERED,
    TEXTURE_COPPER_ORE, TEXTURE_CRUSHED_ORE, TEXTURE_MACHINE_PANEL, TEXTURE_MOLTEN_COPPER,
    TEXTURE_REFRACTORY, TEXTURE_SCREEN_MESH, TEXTURE_SLAG, TEXTURE_STONE, TEXTURE_WOOD_END,
    TEXTURE_WOOD_SIDE, TEXTURE_WORKING_METAL,
};

pub(super) fn build_object_appearances() -> Vec<ObjectAppearanceDefinition> {
    vec![
        object(OBJECT_LOG, "log", &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END]),
        object(OBJECT_CHARCOAL, "charcoal lump", &[super::TEXTURE_CHARCOAL]),
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
        object(OBJECT_SLAG, "slag lump", &[TEXTURE_SLAG]),
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
            OBJECT_STONE_PICK,
            "knapped stone pick",
            &[TEXTURE_STONE, TEXTURE_WOOD_SIDE],
        ),
        object(
            OBJECT_STONE_HAND_CRANK,
            "stone hand crank",
            &[TEXTURE_STONE, TEXTURE_WOOD_SIDE],
        ),
        object(
            OBJECT_COPPER_REINFORCED_PICK,
            "copper-reinforced stone pick",
            &[TEXTURE_STONE, TEXTURE_COPPER_HAMMERED, TEXTURE_WOOD_SIDE],
        ),
        object(
            OBJECT_COPPER_REINFORCED_HAND_CRANK,
            "copper-reinforced stone hand crank",
            &[TEXTURE_STONE, TEXTURE_COPPER_HAMMERED, TEXTURE_WOOD_SIDE],
        ),
        object(
            OBJECT_STONE_QUARRY_PICK,
            "heavy stone quarry pick",
            &[TEXTURE_STONE, TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
        ),
        object(
            OBJECT_COPPER_REINFORCED_STONE_QUARRY_PICK,
            "copper-reinforced heavy quarry pick",
            &[
                TEXTURE_STONE,
                TEXTURE_COPPER_HAMMERED,
                TEXTURE_WOOD_SIDE,
                TEXTURE_WOOD_END,
            ],
        ),
        object(
            OBJECT_TIMBER_TREADLE_DRIVE,
            "timber foot-treadle drive",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END, TEXTURE_STONE],
        ),
        object(
            OBJECT_STONE_CRUSHER,
            "stone toggle crusher",
            &[TEXTURE_STONE, TEXTURE_WOOD_SIDE],
        ),
        object(
            OBJECT_STONE_SEPARATOR,
            "stone rocking separator",
            &[TEXTURE_STONE, TEXTURE_WOOD_SIDE, TEXTURE_SCREEN_MESH],
        ),
        object(
            OBJECT_COPPER_REINFORCED_STONE_CRUSHER,
            "copper-reinforced stone toggle crusher",
            &[TEXTURE_STONE, TEXTURE_WOOD_SIDE, TEXTURE_COPPER_HAMMERED],
        ),
        object(
            OBJECT_COPPER_REINFORCED_STONE_SEPARATOR,
            "copper-reinforced stone rocking separator",
            &[
                TEXTURE_STONE,
                TEXTURE_WOOD_SIDE,
                TEXTURE_SCREEN_MESH,
                TEXTURE_COPPER_HAMMERED,
            ],
        ),
        object(
            OBJECT_GRAVITY_SEPARATOR,
            "workshop gravity separator",
            &[
                TEXTURE_MACHINE_PANEL,
                TEXTURE_WORKING_METAL,
                TEXTURE_SCREEN_MESH,
            ],
        ),
        object(
            OBJECT_COPPER_REINFORCEMENT,
            "cold-worked copper reinforcement",
            &[TEXTURE_COPPER_HAMMERED],
        ),
        object(
            OBJECT_COPPER_SCREEN_PLATE,
            "perforated copper sizing screen plate",
            &[TEXTURE_SCREEN_MESH, TEXTURE_COPPER_HAMMERED],
        ),
        object(
            OBJECT_COPPER_SAW_BLADE,
            "toothed copper frame-saw blade",
            &[TEXTURE_COPPER_HAMMERED, TEXTURE_WORKING_METAL],
        ),
        object(
            OBJECT_STONE_ROTARY_QUERN,
            "stone rotary quern",
            &[TEXTURE_STONE, TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
        ),
        object(
            OBJECT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
            "copper-reinforced stone rotary quern",
            &[
                TEXTURE_STONE,
                TEXTURE_COPPER_HAMMERED,
                TEXTURE_WOOD_SIDE,
                TEXTURE_WOOD_END,
            ],
        ),
        object(
            OBJECT_COPPER_PLATE_SIZING_SCREEN,
            "timber-framed copper shaker screen",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END, TEXTURE_SCREEN_MESH],
        ),
        object(
            OBJECT_STONE_GEOLOGICAL_HAMMER,
            "stone geological sampling hammer",
            &[TEXTURE_STONE, TEXTURE_WOOD_SIDE],
        ),
        object(
            OBJECT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
            "copper-reinforced geological sampling hammer",
            &[TEXTURE_STONE, TEXTURE_COPPER_HAMMERED, TEXTURE_WOOD_SIDE],
        ),
        object(
            OBJECT_STONE_WOODWORKING_ADZE,
            "hafted stone woodworking adze",
            &[TEXTURE_STONE, TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
        ),
        object(
            OBJECT_COPPER_REINFORCED_WOODWORKING_ADZE,
            "copper-reinforced stone woodworking adze",
            &[TEXTURE_STONE, TEXTURE_COPPER_HAMMERED, TEXTURE_WOOD_SIDE],
        ),
        object(
            OBJECT_TIMBER_FRAME_SAW_BENCH,
            "timber frame saw bench",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END, TEXTURE_COPPER_HAMMERED],
        ),
        object(
            OBJECT_NATIVE_COPPER,
            "native copper",
            &[TEXTURE_COPPER_HAMMERED, TEXTURE_COPPER_ORE],
        ),
        object(
            OBJECT_COPPER_SCRAP,
            "copper scrap",
            &[TEXTURE_WORKING_METAL, TEXTURE_COPPER_HAMMERED],
        ),
        object(
            OBJECT_TAILINGS,
            "mineral tailings",
            &[TEXTURE_STONE, TEXTURE_SLAG],
        ),
    ]
}

fn object(
    id: ObjectAppearanceId,
    name: &'static str,
    textures: &[TextureId],
) -> ObjectAppearanceDefinition {
    ObjectAppearanceDefinition::new(id, name, textures.to_vec())
}
