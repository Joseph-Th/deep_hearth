//! Owns built-in renderer-neutral texture content and appearance bindings.

use crate::texture::{
    BlockAppearanceDefinition, BlockAppearanceId, ColorRgba8, ObjectAppearanceId, PackedTexel,
    PaletteRampDefinition, PaletteRampId, TEXTURE_TEXEL_COUNT, TextureAlphaMode, TextureDefinition,
    TextureId, TexturePalette, TextureRegistry,
};

mod appearances;
mod bindings;
mod patterns;

use patterns::{
    aggregate_pattern, charcoal_pattern, molten_pattern, ore_pattern, panel_pattern,
    refractory_pattern, screen_pattern, slag_pattern, wood_end_pattern, wood_side_pattern,
    working_metal_pattern,
};

const RAMP_WOOD: PaletteRampId = PaletteRampId::new(1);
const RAMP_BARK: PaletteRampId = PaletteRampId::new(2);
const RAMP_CHARCOAL: PaletteRampId = PaletteRampId::new(3);
const RAMP_ASH: PaletteRampId = PaletteRampId::new(4);
const RAMP_STONE: PaletteRampId = PaletteRampId::new(5);
const RAMP_COPPER_MINERAL: PaletteRampId = PaletteRampId::new(6);
const RAMP_COPPER_METAL: PaletteRampId = PaletteRampId::new(7);
const RAMP_COPPER_DARK: PaletteRampId = PaletteRampId::new(8);
const RAMP_SLAG: PaletteRampId = PaletteRampId::new(9);
const RAMP_MOLTEN: PaletteRampId = PaletteRampId::new(10);
const RAMP_STEEL: PaletteRampId = PaletteRampId::new(11);
const RAMP_RUST: PaletteRampId = PaletteRampId::new(12);
const RAMP_REFRACTORY: PaletteRampId = PaletteRampId::new(13);
const RAMP_SOOT: PaletteRampId = PaletteRampId::new(14);
const RAMP_TRANSPARENT: PaletteRampId = PaletteRampId::new(15);

pub const TEXTURE_WOOD_SIDE: TextureId = TextureId::new(1);
pub const TEXTURE_WOOD_END: TextureId = TextureId::new(2);
pub const TEXTURE_CHARCOAL: TextureId = TextureId::new(3);
pub const TEXTURE_COPPER_ORE: TextureId = TextureId::new(4);
pub const TEXTURE_COPPER_HAMMERED: TextureId = TextureId::new(5);
pub const TEXTURE_SLAG: TextureId = TextureId::new(6);
pub const TEXTURE_MOLTEN_COPPER: TextureId = TextureId::new(7);
pub const TEXTURE_CRUSHED_ORE: TextureId = TextureId::new(8);
pub const TEXTURE_MACHINE_PANEL: TextureId = TextureId::new(9);
pub const TEXTURE_WORKING_METAL: TextureId = TextureId::new(10);
pub const TEXTURE_REFRACTORY: TextureId = TextureId::new(11);
pub const TEXTURE_SCREEN_MESH: TextureId = TextureId::new(12);
pub const TEXTURE_STONE: TextureId = TextureId::new(13);

pub const BLOCK_TIMBER: BlockAppearanceId = BlockAppearanceId::new(1);
pub const BLOCK_CHARCOAL: BlockAppearanceId = BlockAppearanceId::new(2);
pub const BLOCK_COPPER_ORE: BlockAppearanceId = BlockAppearanceId::new(3);
pub const BLOCK_COPPER: BlockAppearanceId = BlockAppearanceId::new(4);
pub const BLOCK_SLAG: BlockAppearanceId = BlockAppearanceId::new(5);

pub const OBJECT_LOG: ObjectAppearanceId = ObjectAppearanceId::new(1);
pub const OBJECT_CHARCOAL: ObjectAppearanceId = ObjectAppearanceId::new(2);
pub const OBJECT_COPPER_ORE: ObjectAppearanceId = ObjectAppearanceId::new(3);
pub const OBJECT_CRUSHED_ORE: ObjectAppearanceId = ObjectAppearanceId::new(4);
pub const OBJECT_COPPER_INGOT: ObjectAppearanceId = ObjectAppearanceId::new(5);
pub const OBJECT_MOLTEN_COPPER: ObjectAppearanceId = ObjectAppearanceId::new(6);
pub const OBJECT_SLAG: ObjectAppearanceId = ObjectAppearanceId::new(7);
pub const OBJECT_STONE_LUMP: ObjectAppearanceId = ObjectAppearanceId::new(8);
pub const OBJECT_WOOD_HANDLE: ObjectAppearanceId = ObjectAppearanceId::new(9);
pub const OBJECT_JAW_CRUSHER: ObjectAppearanceId = ObjectAppearanceId::new(10);
pub const OBJECT_ELECTRIC_FURNACE: ObjectAppearanceId = ObjectAppearanceId::new(11);
pub const OBJECT_CASTING_MOLD: ObjectAppearanceId = ObjectAppearanceId::new(12);
pub const OBJECT_DRY_SCREEN: ObjectAppearanceId = ObjectAppearanceId::new(13);
pub const OBJECT_GRINDING_MILL: ObjectAppearanceId = ObjectAppearanceId::new(14);
pub const OBJECT_STONE_TOOL: ObjectAppearanceId = ObjectAppearanceId::new(15);
pub const OBJECT_STONE_CHIP: ObjectAppearanceId = ObjectAppearanceId::new(16);
pub const OBJECT_WOOD_CHIP: ObjectAppearanceId = ObjectAppearanceId::new(17);
pub const OBJECT_STONE_FLYWHEEL: ObjectAppearanceId = ObjectAppearanceId::new(18);
pub const OBJECT_STONE_PICK: ObjectAppearanceId = ObjectAppearanceId::new(19);
pub const OBJECT_STONE_HAND_CRANK: ObjectAppearanceId = ObjectAppearanceId::new(20);
pub const OBJECT_COPPER_REINFORCED_PICK: ObjectAppearanceId = ObjectAppearanceId::new(21);
pub const OBJECT_COPPER_REINFORCED_HAND_CRANK: ObjectAppearanceId = ObjectAppearanceId::new(22);
pub const OBJECT_STONE_CRUSHER: ObjectAppearanceId = ObjectAppearanceId::new(23);
pub const OBJECT_COPPER_REINFORCEMENT: ObjectAppearanceId = ObjectAppearanceId::new(24);
pub const OBJECT_NATIVE_COPPER: ObjectAppearanceId = ObjectAppearanceId::new(25);
pub const OBJECT_COPPER_SCRAP: ObjectAppearanceId = ObjectAppearanceId::new(26);
pub const OBJECT_STONE_SEPARATOR: ObjectAppearanceId = ObjectAppearanceId::new(27);
pub const OBJECT_GRAVITY_SEPARATOR: ObjectAppearanceId = ObjectAppearanceId::new(28);
pub const OBJECT_TAILINGS: ObjectAppearanceId = ObjectAppearanceId::new(29);
pub const OBJECT_WOOD_BOARD: ObjectAppearanceId = ObjectAppearanceId::new(30);
pub const OBJECT_TIMBER_CHEST_BODY: ObjectAppearanceId = ObjectAppearanceId::new(31);
pub const OBJECT_COPPER_REINFORCED_STONE_CRUSHER: ObjectAppearanceId = ObjectAppearanceId::new(32);
pub const OBJECT_COPPER_REINFORCED_STONE_SEPARATOR: ObjectAppearanceId =
    ObjectAppearanceId::new(33);
pub const OBJECT_DOUBLE_WALL_TIMBER_CHEST_BODY: ObjectAppearanceId = ObjectAppearanceId::new(34);
pub const OBJECT_STONE_QUARRY_PICK: ObjectAppearanceId = ObjectAppearanceId::new(35);
pub const OBJECT_COPPER_REINFORCED_STONE_QUARRY_PICK: ObjectAppearanceId =
    ObjectAppearanceId::new(36);
pub const OBJECT_TIMBER_TREADLE_DRIVE: ObjectAppearanceId = ObjectAppearanceId::new(37);
pub const OBJECT_BULK_TIMBER_CRATE_BODY: ObjectAppearanceId = ObjectAppearanceId::new(38);
pub const OBJECT_INSULATED_TIMBER_PANTRY_BODY: ObjectAppearanceId = ObjectAppearanceId::new(39);
pub const OBJECT_ROUGH_TIMBER_FIELD_BOX_BODY: ObjectAppearanceId = ObjectAppearanceId::new(40);
pub const OBJECT_STONE_PROVISIONS_CROCK_BODY: ObjectAppearanceId = ObjectAppearanceId::new(41);
pub const OBJECT_COPPER_SCREEN_PLATE: ObjectAppearanceId = ObjectAppearanceId::new(42);
pub const OBJECT_STONE_ROTARY_QUERN: ObjectAppearanceId = ObjectAppearanceId::new(43);
pub const OBJECT_COPPER_REINFORCED_STONE_ROTARY_QUERN: ObjectAppearanceId =
    ObjectAppearanceId::new(44);
pub const OBJECT_COPPER_PLATE_SIZING_SCREEN: ObjectAppearanceId = ObjectAppearanceId::new(45);
pub const OBJECT_STONE_GEOLOGICAL_HAMMER: ObjectAppearanceId = ObjectAppearanceId::new(46);
pub const OBJECT_COPPER_REINFORCED_GEOLOGICAL_HAMMER: ObjectAppearanceId =
    ObjectAppearanceId::new(47);
pub const OBJECT_STONE_WOODWORKING_ADZE: ObjectAppearanceId = ObjectAppearanceId::new(48);
pub const OBJECT_COPPER_REINFORCED_WOODWORKING_ADZE: ObjectAppearanceId =
    ObjectAppearanceId::new(49);
pub const OBJECT_COPPER_SAW_BLADE: ObjectAppearanceId = ObjectAppearanceId::new(50);
pub const OBJECT_TIMBER_FRAME_SAW_BENCH: ObjectAppearanceId = ObjectAppearanceId::new(51);

pub(crate) fn build_texture_registry() -> TextureRegistry {
    TextureRegistry::new(
        build_palette_ramps(),
        build_textures(),
        build_block_appearances(),
        appearances::build_object_appearances(),
        bindings::build_commodity_bindings(),
        bindings::build_equipment_bindings(),
    )
}

fn build_palette_ramps() -> Vec<PaletteRampDefinition> {
    vec![
        ramp(
            RAMP_WOOD,
            "warm cut wood",
            [(31, 21, 35), (86, 47, 35), (164, 101, 48), (242, 204, 126)],
        ),
        ramp(
            RAMP_BARK,
            "weathered bark",
            [(19, 19, 26), (54, 36, 31), (104, 66, 40), (181, 133, 77)],
        ),
        ramp(
            RAMP_CHARCOAL,
            "blue charcoal",
            [(8, 10, 17), (22, 26, 35), (51, 54, 61), (112, 104, 96)],
        ),
        ramp(
            RAMP_ASH,
            "warm ash",
            [(28, 29, 35), (69, 66, 65), (128, 119, 108), (213, 198, 172)],
        ),
        ramp(
            RAMP_STONE,
            "cool host stone",
            [(17, 22, 31), (48, 59, 67), (95, 105, 105), (182, 178, 157)],
        ),
        ramp(
            RAMP_COPPER_MINERAL,
            "copper mineral",
            [(42, 24, 40), (101, 47, 38), (185, 91, 44), (247, 184, 83)],
        ),
        ramp(
            RAMP_COPPER_METAL,
            "polished copper",
            [(45, 22, 37), (119, 49, 39), (205, 101, 51), (255, 209, 118)],
        ),
        ramp(
            RAMP_COPPER_DARK,
            "oxidized copper",
            [(18, 31, 35), (31, 72, 68), (77, 119, 91), (177, 171, 101)],
        ),
        ramp(
            RAMP_SLAG,
            "glassy slag",
            [(12, 16, 24), (37, 44, 51), (75, 78, 75), (146, 137, 111)],
        ),
        ramp(
            RAMP_MOLTEN,
            "molten copper",
            [(91, 17, 25), (194, 48, 24), (255, 126, 21), (255, 244, 142)],
        ),
        ramp(
            RAMP_STEEL,
            "workshop steel",
            [(15, 21, 31), (48, 62, 74), (105, 121, 128), (215, 220, 205)],
        ),
        ramp(
            RAMP_RUST,
            "workshop rust",
            [(37, 23, 27), (91, 43, 31), (161, 76, 40), (222, 145, 74)],
        ),
        ramp(
            RAMP_REFRACTORY,
            "fired refractory",
            [(35, 25, 31), (91, 53, 43), (161, 94, 61), (224, 171, 112)],
        ),
        ramp(
            RAMP_SOOT,
            "furnace soot",
            [(5, 7, 12), (17, 20, 25), (41, 42, 43), (91, 83, 72)],
        ),
        PaletteRampDefinition::new(
            RAMP_TRANSPARENT,
            "transparent cutout",
            [ColorRgba8::new(0, 0, 0, 0); 16],
        ),
    ]
}

fn ramp(
    id: PaletteRampId,
    name: &'static str,
    anchors: [(u8, u8, u8); 4],
) -> PaletteRampDefinition {
    PaletteRampDefinition::from_anchors(
        id,
        name,
        anchors.map(|(red, green, blue)| ColorRgba8::opaque(red, green, blue)),
    )
}

fn build_textures() -> Vec<TextureDefinition> {
    let panel_pattern = panel_pattern();
    vec![
        texture(
            TEXTURE_WOOD_SIDE,
            "wood grain side",
            &[RAMP_WOOD, RAMP_BARK],
            TextureAlphaMode::Opaque,
            wood_side_pattern(),
        ),
        texture(
            TEXTURE_WOOD_END,
            "wood growth rings",
            &[RAMP_WOOD, RAMP_BARK],
            TextureAlphaMode::Opaque,
            wood_end_pattern(),
        ),
        texture(
            TEXTURE_CHARCOAL,
            "fractured charcoal",
            &[RAMP_CHARCOAL, RAMP_ASH],
            TextureAlphaMode::Opaque,
            charcoal_pattern(),
        ),
        texture(
            TEXTURE_COPPER_ORE,
            "copper mineral in host stone",
            &[RAMP_STONE, RAMP_COPPER_MINERAL],
            TextureAlphaMode::Opaque,
            ore_pattern(),
        ),
        texture(
            TEXTURE_COPPER_HAMMERED,
            "hammered copper",
            &[RAMP_COPPER_METAL, RAMP_COPPER_DARK],
            TextureAlphaMode::Opaque,
            panel_pattern,
        ),
        texture(
            TEXTURE_SLAG,
            "porous slag",
            &[RAMP_SLAG, RAMP_CHARCOAL],
            TextureAlphaMode::Opaque,
            slag_pattern(),
        ),
        texture(
            TEXTURE_MOLTEN_COPPER,
            "molten copper surface",
            &[RAMP_MOLTEN, RAMP_SLAG],
            TextureAlphaMode::Opaque,
            molten_pattern(),
        ),
        texture(
            TEXTURE_CRUSHED_ORE,
            "crushed copper ore",
            &[RAMP_STONE, RAMP_COPPER_MINERAL],
            TextureAlphaMode::Opaque,
            aggregate_pattern(),
        ),
        texture(
            TEXTURE_MACHINE_PANEL,
            "riveted workshop panel",
            &[RAMP_STEEL, RAMP_RUST],
            TextureAlphaMode::Opaque,
            panel_pattern,
        ),
        texture(
            TEXTURE_WORKING_METAL,
            "worn working metal",
            &[RAMP_STEEL, RAMP_RUST],
            TextureAlphaMode::Opaque,
            working_metal_pattern(),
        ),
        texture(
            TEXTURE_REFRACTORY,
            "sooted refractory brick",
            &[RAMP_REFRACTORY, RAMP_SOOT],
            TextureAlphaMode::Opaque,
            refractory_pattern(),
        ),
        texture(
            TEXTURE_SCREEN_MESH,
            "steel screen mesh",
            &[RAMP_TRANSPARENT, RAMP_STEEL],
            TextureAlphaMode::Cutout,
            screen_pattern(),
        ),
        texture(
            TEXTURE_STONE,
            "worked stone",
            &[RAMP_STONE, RAMP_SOOT],
            TextureAlphaMode::Opaque,
            refractory_pattern(),
        ),
    ]
}

fn texture(
    id: TextureId,
    name: &'static str,
    ramps: &[PaletteRampId],
    alpha_mode: TextureAlphaMode,
    texels: [PackedTexel; TEXTURE_TEXEL_COUNT],
) -> TextureDefinition {
    TextureDefinition::new(
        id,
        name,
        TexturePalette::new(ramps.to_vec()),
        alpha_mode,
        texels,
    )
}

fn build_block_appearances() -> Vec<BlockAppearanceDefinition> {
    vec![
        BlockAppearanceDefinition::top_side_bottom(
            BLOCK_TIMBER,
            "timber block",
            TEXTURE_WOOD_END,
            TEXTURE_WOOD_SIDE,
            TEXTURE_WOOD_END,
        ),
        BlockAppearanceDefinition::uniform(BLOCK_CHARCOAL, "charcoal block", TEXTURE_CHARCOAL),
        BlockAppearanceDefinition::uniform(
            BLOCK_COPPER_ORE,
            "copper ore block",
            TEXTURE_COPPER_ORE,
        ),
        BlockAppearanceDefinition::uniform(
            BLOCK_COPPER,
            "hammered copper block",
            TEXTURE_COPPER_HAMMERED,
        ),
        BlockAppearanceDefinition::uniform(BLOCK_SLAG, "slag block", TEXTURE_SLAG),
    ]
}

#[cfg(test)]
#[path = "textures_tests.rs"]
mod tests;
