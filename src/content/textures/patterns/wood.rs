//! Wood grain and end-grain texture patterns.

use crate::texture::{PackedTexel, TEXTURE_SIDE, TEXTURE_TEXEL_COUNT};

use super::{TexturePattern, base_noise_pattern, hash_2d, layered_shade, packed, varied_shade};

fn wood_grain_texel(x: usize, y: usize, hash: u32) -> Option<PackedTexel> {
    let grain_offset = usize::from((hash_2d(91, 0, y / 3) & 3) as u8);
    match (x + grain_offset) % 7 {
        0 => Some(packed(0, varied_shade(5, 1, hash))),
        1 if hash.is_multiple_of(3) => Some(packed(0, varied_shade(7, 1, hash >> 5))),
        _ => None,
    }
}

fn wood_knot_texel_at_center(
    x: usize,
    y: usize,
    center_x: usize,
    center_y: usize,
    hash: u32,
) -> Option<PackedTexel> {
    let dx = x.abs_diff(center_x);
    let dy = y.abs_diff(center_y);
    let elliptical_distance = dx * dx + dy * dy * 2;
    if (8..=18).contains(&elliptical_distance) {
        Some(packed(1, varied_shade(7, 1, hash)))
    } else if elliptical_distance <= 3 {
        Some(packed(1, varied_shade(4, 1, hash)))
    } else if dy <= 1 && dx <= 6 && hash & 1 == 0 {
        Some(packed(0, varied_shade(6, 1, hash)))
    } else {
        None
    }
}

fn wood_knot_texel(x: usize, y: usize, hash: u32) -> Option<PackedTexel> {
    let mut overlay = None;
    for (center_x, center_y) in [(9, 10), (24, 23)] {
        if let Some(texel) = wood_knot_texel_at_center(x, y, center_x, center_y, hash) {
            overlay = Some(texel);
        }
    }
    overlay
}

pub(in crate::content::textures) fn wood_side_pattern() -> TexturePattern {
    let mut texels = base_noise_pattern(0x7e11_4a2d, 9, 2);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let hash = hash_2d(0x293a_51c7, x, y);
            let index = y * TEXTURE_SIDE + x;
            if let Some(grain) = wood_grain_texel(x, y, hash) {
                texels[index] = grain;
            }
            if let Some(knot) = wood_knot_texel(x, y, hash) {
                texels[index] = knot;
            }
        }
    }
    texels
}

pub(in crate::content::textures) fn wood_end_pattern() -> TexturePattern {
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
