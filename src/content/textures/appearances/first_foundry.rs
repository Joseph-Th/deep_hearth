//! Ordinary first-foundry electrical generation, crucible, and mold object appearances.

use crate::texture::ObjectAppearanceDefinition;

use super::super::{
    OBJECT_STONE_ARC_CRUCIBLE_FURNACE, OBJECT_STONE_INGOT_MOLD, OBJECT_TIMBER_TREADLE_DYNAMO,
    TEXTURE_COPPER_HAMMERED, TEXTURE_MOLTEN_COPPER, TEXTURE_REFRACTORY, TEXTURE_STONE,
    TEXTURE_WOOD_END, TEXTURE_WOOD_SIDE,
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
    ]
    .into_iter()
}
