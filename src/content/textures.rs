//! Built-in palette ramps, compact texture tiles, and block/object appearance bindings.

use crate::material::CommodityKey;
use crate::texture::{
    BlockAppearanceDefinition, BlockAppearanceId, ColorRgba8, CommodityAppearanceBinding,
    EquipmentAppearanceBinding, ObjectAppearanceDefinition, ObjectAppearanceId, PackedTexel,
    PaletteRampDefinition, PaletteRampId, PaletteSlot, ShadeIndex, TEXTURE_SIDE,
    TEXTURE_TEXEL_COUNT, TextureAlphaMode, TextureDefinition, TextureId, TexturePalette,
    TextureRegistry,
};

use super::equipment::{
    EQUIPMENT_CASTING_MOLD, EQUIPMENT_DRY_SCREEN, EQUIPMENT_ELECTRIC_FURNACE,
    EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER,
};
use super::materials::{
    FORM_CONCENTRATE, FORM_CRUSHED, FORM_INGOT, FORM_LOG, FORM_LUMP, FORM_MOLTEN, FORM_ORE,
    MATERIAL_CHARCOAL, MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_WOOD,
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
pub const OBJECT_JAW_CRUSHER: ObjectAppearanceId = ObjectAppearanceId::new(10);
pub const OBJECT_ELECTRIC_FURNACE: ObjectAppearanceId = ObjectAppearanceId::new(11);
pub const OBJECT_CASTING_MOLD: ObjectAppearanceId = ObjectAppearanceId::new(12);
pub const OBJECT_DRY_SCREEN: ObjectAppearanceId = ObjectAppearanceId::new(13);
pub const OBJECT_GRINDING_MILL: ObjectAppearanceId = ObjectAppearanceId::new(14);

pub(crate) fn build_texture_registry() -> TextureRegistry {
    TextureRegistry::new(
        build_palette_ramps(),
        build_textures(),
        build_block_appearances(),
        build_object_appearances(),
        build_commodity_bindings(),
        build_equipment_bindings(),
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

fn build_object_appearances() -> Vec<ObjectAppearanceDefinition> {
    vec![
        object(OBJECT_LOG, "log", &[TEXTURE_WOOD_SIDE, TEXTURE_WOOD_END]),
        object(OBJECT_CHARCOAL, "charcoal lump", &[TEXTURE_CHARCOAL]),
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
    ]
}

fn object(
    id: ObjectAppearanceId,
    name: &'static str,
    textures: &[TextureId],
) -> ObjectAppearanceDefinition {
    ObjectAppearanceDefinition::new(id, name, textures.to_vec())
}

fn build_commodity_bindings() -> Vec<CommodityAppearanceBinding> {
    vec![
        CommodityAppearanceBinding::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Some(BLOCK_TIMBER),
            Some(OBJECT_LOG),
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
    ]
}

fn build_equipment_bindings() -> Vec<EquipmentAppearanceBinding> {
    vec![
        EquipmentAppearanceBinding::new(EQUIPMENT_JAW_CRUSHER, OBJECT_JAW_CRUSHER),
        EquipmentAppearanceBinding::new(EQUIPMENT_ELECTRIC_FURNACE, OBJECT_ELECTRIC_FURNACE),
        EquipmentAppearanceBinding::new(EQUIPMENT_CASTING_MOLD, OBJECT_CASTING_MOLD),
        EquipmentAppearanceBinding::new(EQUIPMENT_DRY_SCREEN, OBJECT_DRY_SCREEN),
        EquipmentAppearanceBinding::new(EQUIPMENT_GRINDING_MILL, OBJECT_GRINDING_MILL),
    ]
}

fn packed(slot: u8, shade: u8) -> PackedTexel {
    PackedTexel::new(PaletteSlot::new(slot), ShadeIndex::new(shade))
}

fn varied_shade(base: u8, amplitude: u8, noise: u32) -> u8 {
    let width = u32::from(amplitude) * 2 + 1;
    let delta = (noise % width) as i16 - i16::from(amplitude);
    (i16::from(base) + delta).clamp(0, 15) as u8
}

fn hash_2d(seed: u32, x: usize, y: usize) -> u32 {
    let mut value =
        seed ^ (x as u32).wrapping_mul(0x9e37_79b9) ^ (y as u32).wrapping_mul(0x85eb_ca6b);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

fn base_noise_pattern(seed: u32, base_shade: u8, amplitude: u8) -> [PackedTexel; 256] {
    std::array::from_fn(|index| {
        let x = index % TEXTURE_SIDE;
        let y = index / TEXTURE_SIDE;
        packed(0, varied_shade(base_shade, amplitude, hash_2d(seed, x, y)))
    })
}

fn wood_side_pattern() -> [PackedTexel; 256] {
    let mut texels = base_noise_pattern(0x7e11_4a2d, 8, 1);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let hash = hash_2d(0x293a_51c7, x, y);
            let grain = (x + usize::from((hash_2d(91, x, 0) & 3) as u8)) % 6;
            let index = y * TEXTURE_SIDE + x;
            if grain == 0 {
                texels[index] = packed(0, varied_shade(5, 1, hash));
            }
            let knot_x = x.abs_diff(11);
            let knot_y = y.abs_diff(7);
            let knot_radius = knot_x * knot_x + knot_y * knot_y;
            if knot_radius == 1 || knot_radius == 4 || (knot_radius == 5 && hash & 1 == 0) {
                texels[index] = packed(1, varied_shade(7, 1, hash));
            }
        }
    }
    texels[0] = packed(0, 8);
    texels[1] = packed(1, 7);
    texels
}

fn wood_end_pattern() -> [PackedTexel; 256] {
    let mut texels = [packed(0, 8); TEXTURE_TEXEL_COUNT];
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let dx = x as i32 * 2 - 15;
            let dy = y as i32 * 2 - 15;
            let radius = ((dx * dx + dy * dy) as u32 + hash_2d(17, x, y) % 13) / 18;
            let index = y * TEXTURE_SIDE + x;
            texels[index] = if radius.is_multiple_of(4) {
                packed(1, 6 + (radius % 3) as u8)
            } else {
                packed(0, 7 + (radius % 4) as u8)
            };
        }
    }
    texels
}

fn charcoal_pattern() -> [PackedTexel; 256] {
    let mut texels = base_noise_pattern(0x3bca_0197, 6, 3);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let hash = hash_2d(0x9021_bef3, x / 2, y / 2);
            let index = y * TEXTURE_SIDE + x;
            if hash.is_multiple_of(19) || (x + y * 3).is_multiple_of(23) {
                texels[index] = packed(1, varied_shade(7, 2, hash));
            } else if (x + y + usize::from((hash & 3) as u8)).is_multiple_of(9) {
                texels[index] = packed(0, 2);
            }
        }
    }
    texels[0] = packed(0, 5);
    texels[1] = packed(1, 8);
    texels
}

fn ore_pattern() -> [PackedTexel; 256] {
    let mut texels = base_noise_pattern(0x16ac_48d2, 7, 2);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let coarse = hash_2d(0x991d_2883, x / 2, y / 2);
            let fine = hash_2d(0x1b87_3593, x, y);
            if coarse.is_multiple_of(7)
                || (x + y * 2 + usize::from((coarse & 3) as u8)).is_multiple_of(17)
            {
                texels[y * TEXTURE_SIDE + x] = packed(1, varied_shade(8, 3, fine));
            }
        }
    }
    texels[0] = packed(0, 7);
    texels[1] = packed(1, 9);
    texels
}

fn panel_pattern() -> [PackedTexel; 256] {
    let mut texels = base_noise_pattern(0xa7b3_3141, 8, 1);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let index = y * TEXTURE_SIDE + x;
            let edge = x == 0 || y == 0 || x == TEXTURE_SIDE - 1 || y == TEXTURE_SIDE - 1;
            if edge {
                texels[index] = packed(0, if x == 0 || y == 0 { 5 } else { 11 });
            }
            if matches!((x, y), (2, 2) | (13, 2) | (2, 13) | (13, 13)) {
                texels[index] = packed(1, 9);
            }
            if (x == 6 || x == 7) && (4..12).contains(&y) && hash_2d(71, x, y).is_multiple_of(5) {
                texels[index] = packed(1, 6);
            }
        }
    }
    texels
}

fn slag_pattern() -> [PackedTexel; 256] {
    let mut texels = base_noise_pattern(0x52d8_b779, 7, 3);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let hash = hash_2d(0x19b5_0a63, x, y);
            if hash.is_multiple_of(13) || hash.is_multiple_of(29) {
                texels[y * TEXTURE_SIDE + x] = packed(1, varied_shade(3, 2, hash));
            } else if hash.is_multiple_of(17) {
                texels[y * TEXTURE_SIDE + x] = packed(0, 12);
            }
        }
    }
    texels[0] = packed(0, 7);
    texels[1] = packed(1, 3);
    texels
}

fn molten_pattern() -> [PackedTexel; 256] {
    let mut texels = base_noise_pattern(0x628f_c921, 9, 2);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let wave = (x + y * 2 + usize::from((hash_2d(83, x / 3, y) & 3) as u8)) % 11;
            let index = y * TEXTURE_SIDE + x;
            if wave <= 1 {
                texels[index] = packed(0, 13 + wave as u8);
            } else if hash_2d(0x91aa_6721, x, y).is_multiple_of(23) {
                texels[index] = packed(1, 4);
            }
        }
    }
    texels[0] = packed(0, 10);
    texels[1] = packed(1, 4);
    texels
}

fn aggregate_pattern() -> [PackedTexel; 256] {
    let mut texels = base_noise_pattern(0xb741_c38d, 6, 3);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let coarse = hash_2d(0x2424_9911, x / 2, y / 2);
            let fine = hash_2d(0x837a_1705, x, y);
            if coarse.is_multiple_of(5) && fine & 3 != 0 {
                texels[y * TEXTURE_SIDE + x] = packed(1, varied_shade(7, 3, fine));
            }
        }
    }
    texels[0] = packed(0, 6);
    texels[1] = packed(1, 7);
    texels
}

fn working_metal_pattern() -> [PackedTexel; 256] {
    let mut texels = base_noise_pattern(0x22ce_7419, 7, 2);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let hash = hash_2d(0x78a5_18d3, x, y);
            let index = y * TEXTURE_SIDE + x;
            if (x + y * 3 + usize::from((hash & 7) as u8)).is_multiple_of(19) {
                texels[index] = packed(0, 13);
            } else if hash.is_multiple_of(31) {
                texels[index] = packed(1, 7);
            }
        }
    }
    texels[0] = packed(0, 7);
    texels[1] = packed(1, 7);
    texels
}

fn refractory_pattern() -> [PackedTexel; 256] {
    let mut texels = base_noise_pattern(0xd12b_6059, 8, 2);
    for y in 0..TEXTURE_SIDE {
        let row_offset = if (y / 4).is_multiple_of(2) { 0 } else { 4 };
        for x in 0..TEXTURE_SIDE {
            let is_mortar = y.is_multiple_of(4) || (x + row_offset).is_multiple_of(8);
            if is_mortar {
                texels[y * TEXTURE_SIDE + x] = packed(1, 5);
            } else if hash_2d(0x29d9_1e17, x, y).is_multiple_of(17) {
                texels[y * TEXTURE_SIDE + x] = packed(1, 8);
            }
        }
    }
    texels[0] = packed(0, 8);
    texels[1] = packed(1, 5);
    texels
}

fn screen_pattern() -> [PackedTexel; 256] {
    std::array::from_fn(|index| {
        let x = index % TEXTURE_SIDE;
        let y = index / TEXTURE_SIDE;
        if x.is_multiple_of(4) || y.is_multiple_of(4) {
            packed(1, varied_shade(8, 3, hash_2d(0x8841_329b, x, y)))
        } else {
            packed(0, 0)
        }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn built_in_tiles_have_palette_detail_without_rgba_duplication() {
        let registry = build_texture_registry();
        for texture in [
            TEXTURE_WOOD_SIDE,
            TEXTURE_WOOD_END,
            TEXTURE_CHARCOAL,
            TEXTURE_COPPER_ORE,
            TEXTURE_COPPER_HAMMERED,
            TEXTURE_SLAG,
            TEXTURE_MOLTEN_COPPER,
            TEXTURE_CRUSHED_ORE,
            TEXTURE_MACHINE_PANEL,
            TEXTURE_WORKING_METAL,
            TEXTURE_REFRACTORY,
            TEXTURE_SCREEN_MESH,
        ] {
            let definition = match registry.get_texture(texture) {
                Some(definition) => definition,
                None => panic!("missing built-in texture {}", texture.value()),
            };
            let unique: BTreeSet<_> = definition.texels().iter().copied().collect();
            assert!(
                unique.len() >= 5,
                "texture {} lacks authored shade detail",
                texture.value()
            );
        }
    }

    #[test]
    fn built_in_bake_is_deterministic_compact_and_deduplicates_panel_geometry() {
        let registry = build_texture_registry();
        let first = registry.bake_texture_array();
        let second = registry.bake_texture_array();

        assert_eq!(first, second);
        assert!(first.pattern_layer_count() < 12);
        assert!(first.total_gpu_bytes() * 2 < first.expanded_rgba_texel_bytes());
        assert_eq!(
            first.indexed_texel_bytes(),
            usize::from(first.pattern_layer_count()) * (256 + 64 + 16 + 4 + 1)
        );
        assert_eq!(first.mip_levels().len(), 5);
        assert_eq!(
            first
                .get_descriptor(TEXTURE_COPPER_HAMMERED)
                .map(|descriptor| descriptor.layer()),
            first
                .get_descriptor(TEXTURE_MACHINE_PANEL)
                .map(|descriptor| descriptor.layer())
        );
        assert_ne!(
            first.sample(TEXTURE_COPPER_HAMMERED, 0, 8, 8),
            first.sample(TEXTURE_MACHINE_PANEL, 0, 8, 8)
        );
    }
}
