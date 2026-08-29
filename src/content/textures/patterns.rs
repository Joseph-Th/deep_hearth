//! Deterministic procedural synthesis for built-in indexed texture patterns.

use crate::texture::{PackedTexel, PaletteSlot, ShadeIndex, TEXTURE_SIDE, TEXTURE_TEXEL_COUNT};

type TexturePattern = [PackedTexel; TEXTURE_TEXEL_COUNT];

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

fn layered_shade(base: u8, amplitude: u8, seed: u32, x: usize, y: usize) -> u8 {
    let broad_amplitude = amplitude.div_ceil(2);
    let medium_amplitude = amplitude / 2;
    let broad = varied_shade(base, broad_amplitude, hash_2d(seed, x / 8, y / 8));
    let medium = varied_shade(
        broad,
        medium_amplitude,
        hash_2d(seed ^ 0x63d8_35a7, x / 3, y / 3),
    );
    let fine = hash_2d(seed ^ 0xb529_7a4d, x, y);
    if amplitude != 0 && fine.is_multiple_of(7) {
        varied_shade(medium, 1, fine >> 8)
    } else {
        medium
    }
}

fn base_noise_pattern(seed: u32, base_shade: u8, amplitude: u8) -> TexturePattern {
    std::array::from_fn(|index| {
        let x = index % TEXTURE_SIDE;
        let y = index / TEXTURE_SIDE;
        packed(0, layered_shade(base_shade, amplitude, seed, x, y))
    })
}

fn squared_distance(x: usize, y: usize, center_x: usize, center_y: usize) -> usize {
    let dx = x.abs_diff(center_x);
    let dy = y.abs_diff(center_y);
    dx * dx + dy * dy
}

pub(super) fn wood_side_pattern() -> TexturePattern {
    let mut texels = base_noise_pattern(0x7e11_4a2d, 9, 2);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let hash = hash_2d(0x293a_51c7, x, y);
            let grain_offset = usize::from((hash_2d(91, 0, y / 3) & 3) as u8);
            let grain = (x + grain_offset) % 7;
            let index = y * TEXTURE_SIDE + x;
            if grain == 0 {
                texels[index] = packed(0, varied_shade(5, 1, hash));
            } else if grain == 1 && hash.is_multiple_of(3) {
                texels[index] = packed(0, varied_shade(7, 1, hash >> 5));
            }
            for (center_x, center_y) in [(9, 10), (24, 23)] {
                let dx = x.abs_diff(center_x);
                let dy = y.abs_diff(center_y);
                let elliptical_distance = dx * dx + dy * dy * 2;
                if (8..=18).contains(&elliptical_distance) {
                    texels[index] = packed(1, varied_shade(7, 1, hash));
                } else if elliptical_distance <= 3 {
                    texels[index] = packed(1, varied_shade(4, 1, hash));
                } else if dy <= 1 && dx <= 6 && hash & 1 == 0 {
                    texels[index] = packed(0, varied_shade(6, 1, hash));
                }
            }
        }
    }
    texels
}

pub(super) fn wood_end_pattern() -> TexturePattern {
    let mut texels = [packed(0, 8); TEXTURE_TEXEL_COUNT];
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let dx = x as i32 * 2 - 31;
            let dy = y as i32 * 2 - 31;
            let radius = ((dx * dx + dy * dy) as u32).isqrt();
            let wobble = hash_2d(17, x / 3, y / 3) % 5;
            let ring_position = (radius + wobble) % 8;
            let radial_crack = radius > 13
                && ((dx > 0 && (dy - dx / 3).abs() <= 1) || (dy > 0 && (dx + dy / 2).abs() <= 1));
            let index = y * TEXTURE_SIDE + x;
            texels[index] = if radius > 30 {
                packed(1, varied_shade(6, 2, hash_2d(0xa84d_2193, x, y)))
            } else if radial_crack {
                packed(1, 3)
            } else if ring_position <= 1 {
                packed(1, 7 + (radius % 3) as u8)
            } else {
                packed(0, layered_shade(9, 2, 0xd733_91a5, x, y))
            };
        }
    }
    texels
}

pub(super) fn charcoal_pattern() -> TexturePattern {
    let mut texels = base_noise_pattern(0x3bca_0197, 6, 3);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let cell_hash = hash_2d(0x9021_bef3, x / 5, y / 5);
            let fine = hash_2d(0xe241_97b5, x, y);
            let index = y * TEXTURE_SIDE + x;
            let fracture = (x + y * 3 + usize::from((cell_hash & 7) as u8)).is_multiple_of(17)
                || (x * 3 + y + usize::from(((cell_hash >> 4) & 7) as u8)).is_multiple_of(23);
            if fracture {
                texels[index] = packed(0, varied_shade(2, 1, fine));
            } else if cell_hash.is_multiple_of(11) && fine.is_multiple_of(5) {
                texels[index] = packed(1, varied_shade(8, 2, fine));
            } else if fine.is_multiple_of(41) {
                texels[index] = packed(1, 5);
            }
        }
    }
    texels
}

pub(super) fn ore_pattern() -> TexturePattern {
    let mut texels = base_noise_pattern(0x16ac_48d2, 7, 3);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let coarse = hash_2d(0x991d_2883, x / 4, y / 4);
            let fine = hash_2d(0x1b87_3593, x, y);
            let vein_offset = usize::from((hash_2d(0x51a7_3c19, 0, y / 3) & 7) as u8);
            let vein = (x + y * 2 + vein_offset) % 19;
            let branch_offset = usize::from((hash_2d(0x6a21_b94f, x / 4, 0) & 7) as u8);
            let branch = (x * 2 + (TEXTURE_SIDE - 1 - y) * 3 + branch_offset) % 31;
            let mineral_patch = coarse.is_multiple_of(9)
                && (x % 4 == 1 || x % 4 == 2)
                && (y % 4 == 1 || y % 4 == 2);
            let index = y * TEXTURE_SIDE + x;
            if (vein <= 1 && !fine.is_multiple_of(11))
                || (branch == 0 && !coarse.is_multiple_of(3))
                || mineral_patch
            {
                texels[index] = packed(1, varied_shade(9, 3, fine));
            } else if vein == 2 && fine.is_multiple_of(3) {
                texels[index] = packed(1, varied_shade(6, 1, fine));
            } else if fine.is_multiple_of(53) {
                texels[index] = packed(0, 3);
            }
        }
    }
    texels
}

fn panel_frame_texel(x: usize, y: usize) -> Option<PackedTexel> {
    if x <= 1 || y <= 1 {
        return Some(packed(0, if x == 0 || y == 0 { 4 } else { 6 }));
    }
    if x >= TEXTURE_SIDE - 2 || y >= TEXTURE_SIDE - 2 {
        return Some(packed(
            0,
            if x == TEXTURE_SIDE - 1 || y == TEXTURE_SIDE - 1 {
                12
            } else {
                10
            },
        ));
    }
    None
}

fn panel_rivet_texel(x: usize, y: usize) -> Option<PackedTexel> {
    for (rivet_x, rivet_y) in [(4, 4), (27, 4), (4, 27), (27, 27)] {
        let distance = squared_distance(x, y, rivet_x, rivet_y);
        if distance == 0 {
            return Some(packed(1, 12));
        }
        if distance <= 2 {
            return Some(packed(1, 7));
        }
    }
    None
}

fn panel_seam_texel(x: usize) -> Option<PackedTexel> {
    match x {
        15 => Some(packed(0, 5)),
        16 => Some(packed(0, 10)),
        _ => None,
    }
}

fn panel_scratch_texel(x: usize, y: usize) -> Option<PackedTexel> {
    let scratch = x > 4
        && x < 27
        && (x + y * 5 + usize::from((hash_2d(71, x / 4, y / 4) & 7) as u8)).is_multiple_of(29);
    scratch.then(|| packed(1, varied_shade(6, 2, hash_2d(71, x, y))))
}

pub(super) fn panel_pattern() -> TexturePattern {
    let mut texels = base_noise_pattern(0xa7b3_3141, 8, 2);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let index = y * TEXTURE_SIDE + x;
            for overlay in [
                panel_frame_texel(x, y),
                panel_rivet_texel(x, y),
                panel_seam_texel(x),
                panel_scratch_texel(x, y),
            ]
            .into_iter()
            .flatten()
            {
                texels[index] = overlay;
            }
        }
    }
    texels
}

pub(super) fn slag_pattern() -> TexturePattern {
    let mut texels = base_noise_pattern(0x52d8_b779, 7, 3);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let hash = hash_2d(0x19b5_0a63, x, y);
            let index = y * TEXTURE_SIDE + x;
            let mut pore_distance = usize::MAX;
            for (pore_x, pore_y) in [(5, 7), (14, 4), (25, 9), (9, 22), (22, 26), (29, 18)] {
                pore_distance = pore_distance.min(squared_distance(x, y, pore_x, pore_y));
            }
            if pore_distance <= 2 {
                texels[index] = packed(1, varied_shade(2, 1, hash));
            } else if pore_distance <= 5 {
                texels[index] = packed(0, varied_shade(12, 1, hash));
            } else if hash.is_multiple_of(47) {
                texels[index] = packed(1, varied_shade(4, 1, hash));
            }
        }
    }
    texels
}

pub(super) fn molten_pattern() -> TexturePattern {
    let mut texels = base_noise_pattern(0x628f_c921, 9, 2);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let flow_offset = usize::from((hash_2d(83, 0, y / 3) & 7) as u8);
            let wave = (x + y * 2 + flow_offset) % 15;
            let island = hash_2d(0x91aa_6721, x / 4, y / 4);
            let fine = hash_2d(0xf837_4b15, x, y);
            let index = y * TEXTURE_SIDE + x;
            if wave <= 1 {
                texels[index] = packed(0, 13 + wave as u8);
            } else if wave == 2 {
                texels[index] = packed(0, varied_shade(11, 1, fine));
            } else if island.is_multiple_of(13) && x % 4 != 0 && y % 4 != 0 {
                texels[index] = packed(1, varied_shade(4, 2, fine));
            } else if fine.is_multiple_of(61) {
                texels[index] = packed(0, 15);
            }
        }
    }
    texels
}

pub(super) fn aggregate_pattern() -> TexturePattern {
    let mut texels = base_noise_pattern(0xb741_c38d, 6, 3);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let cell_x = x / 4;
            let cell_y = y / 4;
            let coarse = hash_2d(0x2424_9911, cell_x, cell_y);
            let fine = hash_2d(0x837a_1705, x, y);
            let local_x = x % 4;
            let local_y = y % 4;
            let edge = local_x == 0 || local_y == 0;
            let index = y * TEXTURE_SIDE + x;
            if coarse.is_multiple_of(4) && !edge {
                texels[index] = packed(1, varied_shade(8, 3, coarse ^ fine));
            } else if edge {
                texels[index] = packed(0, varied_shade(3, 1, fine));
            } else {
                texels[index] = packed(0, varied_shade(7, 2, coarse));
            }
        }
    }
    texels
}

pub(super) fn working_metal_pattern() -> TexturePattern {
    let mut texels = base_noise_pattern(0x22ce_7419, 7, 2);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let hash = hash_2d(0x78a5_18d3, x, y);
            let patch = hash_2d(0xc161_5a27, x / 5, y / 5);
            let index = y * TEXTURE_SIDE + x;
            let bright_scratch = (x * 5 + y + usize::from((patch & 7) as u8)).is_multiple_of(37);
            let dark_scratch =
                (x + y * 7 + usize::from(((patch >> 3) & 7) as u8)).is_multiple_of(43);
            if bright_scratch {
                texels[index] = packed(0, varied_shade(13, 1, hash));
            } else if dark_scratch {
                texels[index] = packed(0, varied_shade(3, 1, hash));
            } else if patch.is_multiple_of(9) && hash.is_multiple_of(3) {
                texels[index] = packed(1, varied_shade(7, 3, hash));
            } else if hash.is_multiple_of(67) {
                texels[index] = packed(1, 4);
            }
        }
    }
    texels
}

pub(super) fn refractory_pattern() -> TexturePattern {
    let mut texels = base_noise_pattern(0xd12b_6059, 8, 2);
    for y in 0..TEXTURE_SIDE {
        let row_offset = if (y / 8).is_multiple_of(2) { 0 } else { 8 };
        for x in 0..TEXTURE_SIDE {
            let is_mortar = y.is_multiple_of(8) || (x + row_offset).is_multiple_of(16);
            let hash = hash_2d(0x29d9_1e17, x, y);
            let index = y * TEXTURE_SIDE + x;
            if is_mortar {
                texels[index] = packed(1, varied_shade(5, 1, hash));
            } else if y % 8 <= 2 && hash.is_multiple_of(5) {
                texels[index] = packed(1, varied_shade(7, 2, hash));
            } else if hash.is_multiple_of(31) {
                texels[index] = packed(0, 12);
            }
        }
    }
    texels
}

pub(super) fn screen_pattern() -> TexturePattern {
    std::array::from_fn(|index| {
        let x = index % TEXTURE_SIDE;
        let y = index / TEXTURE_SIDE;
        let wire_x = x % 8;
        let wire_y = y % 8;
        if wire_x <= 1 || wire_y <= 1 {
            let base = if wire_x == 0 || wire_y == 0 { 6 } else { 11 };
            let intersection = wire_x <= 1 && wire_y <= 1;
            packed(
                1,
                varied_shade(
                    if intersection { 13 } else { base },
                    1,
                    hash_2d(0x8841_329b, x, y),
                ),
            )
        } else {
            packed(0, 0)
        }
    })
}
