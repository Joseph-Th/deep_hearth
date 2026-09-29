//! Reinforced and settlement-scale workshop object appearances plus processed outputs.

use crate::texture::ObjectAppearanceDefinition;

use super::super::{
    OBJECT_CLAY_EARTH_LUMP, OBJECT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
    OBJECT_COPPER_REINFORCED_WOODWORKING_ADZE, OBJECT_COPPER_SCRAP, OBJECT_FLYWHEEL_WIRE_DRAWBENCH,
    OBJECT_NATIVE_COPPER, OBJECT_PACKED_CLAY_BINDER, OBJECT_STONE_WOODWORKING_ADZE,
    OBJECT_TAILINGS, OBJECT_TIMBER_CHANNEL_SAMPLING_FRAME, OBJECT_TIMBER_DRESSING_BENCH,
    OBJECT_TIMBER_FRAME_COMMINUTION_MILL, OBJECT_TIMBER_FRAME_SAW_BENCH,
    OBJECT_TIMBER_HELVE_HAMMER, OBJECT_TIMBER_ORE_DRESSING_TABLE, OBJECT_TIMBER_SASH_SAWMILL,
    OBJECT_TIMBER_TREADLE_HAMMER, OBJECT_TIMBER_TRIPOD_CORE_DRILL,
    OBJECT_TIMBER_WALKING_WHEEL_DRIVE, OBJECT_TIMBER_WIRE_DRAWBENCH, TEXTURE_CLAY_EARTH,
    TEXTURE_COPPER_HAMMERED, TEXTURE_COPPER_ORE, TEXTURE_SCREEN_MESH, TEXTURE_SLAG, TEXTURE_STONE,
    TEXTURE_WOOD_END, TEXTURE_WOOD_SIDE, TEXTURE_WORKING_METAL,
};
use super::object;

pub(super) fn definitions() -> impl Iterator<Item = ObjectAppearanceDefinition> {
    [
        object(
            OBJECT_CLAY_EARTH_LUMP,
            "clay-rich earth lump",
            &[TEXTURE_CLAY_EARTH, TEXTURE_STONE],
        ),
        object(
            OBJECT_PACKED_CLAY_BINDER,
            "packed clay binder",
            &[TEXTURE_CLAY_EARTH, TEXTURE_STONE],
        ),
        object(
            OBJECT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
            "copper-reinforced geological sampling hammer",
            &[TEXTURE_STONE, TEXTURE_COPPER_HAMMERED, TEXTURE_WOOD_SIDE],
        ),
        object(
            OBJECT_TIMBER_CHANNEL_SAMPLING_FRAME,
            "timber channel-sampling frame",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END, TEXTURE_COPPER_HAMMERED],
        ),
        object(
            OBJECT_TIMBER_TRIPOD_CORE_DRILL,
            "timber tripod shallow-core drill",
            &[
                TEXTURE_WOOD_SIDE,
                TEXTURE_WOOD_END,
                TEXTURE_STONE,
                TEXTURE_COPPER_HAMMERED,
            ],
        ),
        object(
            OBJECT_FLYWHEEL_WIRE_DRAWBENCH,
            "flywheel-driven copper wire drawbench",
            &[
                TEXTURE_WOOD_SIDE,
                TEXTURE_WOOD_END,
                TEXTURE_STONE,
                TEXTURE_COPPER_HAMMERED,
            ],
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
            OBJECT_TIMBER_DRESSING_BENCH,
            "timber cobbing and picking bench",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END, TEXTURE_STONE],
        ),
        object(
            OBJECT_TIMBER_FRAME_COMMINUTION_MILL,
            "timber-framed stone comminution mill",
            &[
                TEXTURE_WOOD_SIDE,
                TEXTURE_WOOD_END,
                TEXTURE_STONE,
                TEXTURE_COPPER_HAMMERED,
            ],
        ),
        object(
            OBJECT_TIMBER_ORE_DRESSING_TABLE,
            "timber ore-dressing table",
            &[
                TEXTURE_WOOD_SIDE,
                TEXTURE_WOOD_END,
                TEXTURE_STONE,
                TEXTURE_SCREEN_MESH,
                TEXTURE_COPPER_HAMMERED,
            ],
        ),
        object(
            OBJECT_TIMBER_WALKING_WHEEL_DRIVE,
            "timber walking-wheel drive",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
        ),
        object(
            OBJECT_TIMBER_TREADLE_HAMMER,
            "timber treadle forging hammer",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END, TEXTURE_STONE],
        ),
        object(
            OBJECT_TIMBER_WIRE_DRAWBENCH,
            "timber copper wire drawbench",
            &[
                TEXTURE_WOOD_SIDE,
                TEXTURE_WOOD_END,
                TEXTURE_STONE,
                TEXTURE_COPPER_HAMMERED,
            ],
        ),
        object(
            OBJECT_TIMBER_SASH_SAWMILL,
            "timber sash sawmill",
            &[
                TEXTURE_WOOD_SIDE,
                TEXTURE_WOOD_END,
                TEXTURE_COPPER_HAMMERED,
                TEXTURE_WORKING_METAL,
            ],
        ),
        object(
            OBJECT_TIMBER_HELVE_HAMMER,
            "timber helve hammer",
            &[
                TEXTURE_WOOD_SIDE,
                TEXTURE_WOOD_END,
                TEXTURE_STONE,
                TEXTURE_COPPER_HAMMERED,
            ],
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
    .into_iter()
}
