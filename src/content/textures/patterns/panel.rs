//! Riveted panel texture pattern shared by metal surfaces.

use crate::texture::{PackedTexel, TEXTURE_SIDE};

use super::{TexturePattern, base_noise_pattern, hash_2d, packed, squared_distance, varied_shade};

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

fn panel_overlay_texel(x: usize, y: usize) -> Option<PackedTexel> {
    // Preserve the original overlay precedence while avoiding work for layers that will be hidden.
    panel_scratch_texel(x, y)
        .or_else(|| panel_seam_texel(x))
        .or_else(|| panel_rivet_texel(x, y))
        .or_else(|| panel_frame_texel(x, y))
}

pub(in crate::content::textures) fn panel_pattern() -> TexturePattern {
    let mut texels = base_noise_pattern(0xa7b3_3141, 8, 2);
    for y in 0..TEXTURE_SIDE {
        for x in 0..TEXTURE_SIDE {
            let index = y * TEXTURE_SIDE + x;
            if let Some(overlay) = panel_overlay_texel(x, y) {
                texels[index] = overlay;
            }
        }
    }
    texels
}
