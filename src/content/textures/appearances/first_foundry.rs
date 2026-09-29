//! Ordinary first-foundry electrical generation, crucible, and mold object appearances.

use crate::texture::ObjectAppearanceDefinition;

use super::super::{
    OBJECT_CLAY_FACED_STONE_CASTING_BED, OBJECT_DOUBLE_WOUND_TREADLE_DYNAMO,
    OBJECT_FOUR_CAVITY_STONE_INGOT_MOLD, OBJECT_FOUR_POT_ARC_CRUCIBLE_FURNACE,
    OBJECT_STONE_ARC_CRUCIBLE_FURNACE, OBJECT_STONE_INGOT_MOLD, OBJECT_TIMBER_TREADLE_DYNAMO,
    TEXTURE_CLAY_EARTH, TEXTURE_COPPER_HAMMERED, TEXTURE_MOLTEN_COPPER, TEXTURE_REFRACTORY,
    TEXTURE_STONE, TEXTURE_WOOD_END, TEXTURE_WOOD_SIDE,
};
use super::object;

pub(super) fn definitions() -> impl Iterator<Item = ObjectAppearanceDefinition> {
    [
        object(
            OBJECT_TIMBER_TREADLE_DYNAMO,
            "copper-wound treadle dynamo",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END, TEXTURE_COPPER_HAMMERED],
        ),
        object(
            OBJECT_STONE_ARC_CRUCIBLE_FURNACE,
            "stone arc crucible furnace",
            &[
                TEXTURE_STONE,
                TEXTURE_REFRACTORY,
                TEXTURE_COPPER_HAMMERED,
                TEXTURE_MOLTEN_COPPER,
            ],
        ),
        object(
            OBJECT_STONE_INGOT_MOLD,
            "carved stone ingot mold",
            &[TEXTURE_STONE, TEXTURE_MOLTEN_COPPER],
        ),
        object(
            OBJECT_CLAY_FACED_STONE_CASTING_BED,
            "clay-faced stone casting bed",
            &[
                TEXTURE_WOOD_SIDE,
                TEXTURE_STONE,
                TEXTURE_CLAY_EARTH,
                TEXTURE_MOLTEN_COPPER,
            ],
        ),
        object(
            OBJECT_DOUBLE_WOUND_TREADLE_DYNAMO,
            "geared double-wound treadle dynamo",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END, TEXTURE_COPPER_HAMMERED],
        ),
        object(
            OBJECT_FOUR_POT_ARC_CRUCIBLE_FURNACE,
            "four-pot arc crucible furnace",
            &[
                TEXTURE_STONE,
                TEXTURE_REFRACTORY,
                TEXTURE_COPPER_HAMMERED,
                TEXTURE_MOLTEN_COPPER,
            ],
        ),
        object(
            OBJECT_FOUR_CAVITY_STONE_INGOT_MOLD,
            "four-cavity stone ingot mold",
            &[
                TEXTURE_STONE,
                TEXTURE_COPPER_HAMMERED,
                TEXTURE_MOLTEN_COPPER,
            ],
        ),
    ]
    .into_iter()
}
