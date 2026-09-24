//! Workshop wear, refractory masonry, and screen-mesh texture patterns.

use crate::texture::TEXTURE_SIDE;

use super::{TexturePattern, base_noise_pattern, hash_2d, packed, varied_shade};

pub(in crate::content::textures) fn working_metal_pattern() -> TexturePattern {
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

pub(in crate::content::textures) fn refractory_pattern() -> TexturePattern {
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

pub(in crate::content::textures) fn screen_pattern() -> TexturePattern {
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
