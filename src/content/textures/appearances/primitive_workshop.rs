//! Primitive workshop, mining, processing, and first-mechanization object appearances.

use crate::texture::ObjectAppearanceDefinition;

use super::super::{
    OBJECT_COPPER_PLATE_SIZING_SCREEN, OBJECT_COPPER_REINFORCED_HAND_CRANK,
    OBJECT_COPPER_REINFORCED_PICK, OBJECT_COPPER_REINFORCED_STONE_CRUSHER,
    OBJECT_COPPER_REINFORCED_STONE_QUARRY_PICK, OBJECT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
    OBJECT_COPPER_REINFORCED_STONE_SEPARATOR, OBJECT_COPPER_REINFORCEMENT, OBJECT_COPPER_SAW_BLADE,
    OBJECT_COPPER_SCREEN_PLATE, OBJECT_GRAVITY_SEPARATOR, OBJECT_STONE_COBBING_HAMMER,
    OBJECT_STONE_CRUSHER, OBJECT_STONE_FLYWHEEL_PUMP_DRILL, OBJECT_STONE_GEOLOGICAL_HAMMER,
    OBJECT_STONE_HAND_CRANK, OBJECT_STONE_PICK, OBJECT_STONE_QUARRY_PICK,
    OBJECT_STONE_ROTARY_QUERN, OBJECT_STONE_SEPARATOR, OBJECT_TIMBER_FLYWHEEL_GRINDING_BENCH,
    OBJECT_TIMBER_FLYWHEEL_LATHE, OBJECT_TIMBER_RIDDLE_PANEL, OBJECT_TIMBER_RIDDLE_SIZING_SCREEN,
    OBJECT_TIMBER_SPINDLE_DRILL, OBJECT_TIMBER_SPRING_POLE_LATHE, OBJECT_TIMBER_TREADLE_DRIVE,
    OBJECT_TIMBER_TREADLE_GRINDSTONE, TEXTURE_COPPER_HAMMERED, TEXTURE_MACHINE_PANEL,
    TEXTURE_SCREEN_MESH, TEXTURE_STONE, TEXTURE_WOOD_END, TEXTURE_WOOD_SIDE, TEXTURE_WORKING_METAL,
};
use super::object;

pub(super) fn definitions() -> impl Iterator<Item = ObjectAppearanceDefinition> {
    [
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
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
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
            OBJECT_TIMBER_RIDDLE_PANEL,
            "slatted timber riddle panel",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
        ),
        object(
            OBJECT_TIMBER_RIDDLE_SIZING_SCREEN,
            "timber riddle sizing screen",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
        ),
        object(
            OBJECT_STONE_GEOLOGICAL_HAMMER,
            "stone geological sampling hammer",
            &[TEXTURE_STONE, TEXTURE_WOOD_SIDE],
        ),
        object(
            OBJECT_STONE_COBBING_HAMMER,
            "hafted stone cobbing hammer",
            &[TEXTURE_STONE, TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
        ),
        object(
            OBJECT_STONE_FLYWHEEL_PUMP_DRILL,
            "stone-flywheel pump drill",
            &[TEXTURE_STONE, TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END],
        ),
        object(
            OBJECT_TIMBER_SPINDLE_DRILL,
            "timber spindle drill",
            &[
                TEXTURE_WOOD_SIDE,
                TEXTURE_WOOD_END,
                TEXTURE_STONE,
                TEXTURE_COPPER_HAMMERED,
            ],
        ),
        object(
            OBJECT_TIMBER_SPRING_POLE_LATHE,
            "timber spring-pole lathe",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END, TEXTURE_STONE],
        ),
        object(
            OBJECT_TIMBER_FLYWHEEL_LATHE,
            "flywheel-driven timber lathe",
            &[
                TEXTURE_WOOD_SIDE,
                TEXTURE_WOOD_END,
                TEXTURE_STONE,
                TEXTURE_COPPER_HAMMERED,
            ],
        ),
        object(
            OBJECT_TIMBER_TREADLE_GRINDSTONE,
            "timber treadle grindstone",
            &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END, TEXTURE_STONE],
        ),
        object(
            OBJECT_TIMBER_FLYWHEEL_GRINDING_BENCH,
            "flywheel-driven toolroom grindstone",
            &[
                TEXTURE_WOOD_SIDE,
                TEXTURE_WOOD_END,
                TEXTURE_STONE,
                TEXTURE_COPPER_HAMMERED,
            ],
        ),
    ]
    .into_iter()
}
