//! Processed mineral, slag, and molten-material texture patterns.

use crate::texture::TEXTURE_SIDE;

use super::{TexturePattern, base_noise_pattern, hash_2d, packed, squared_distance, varied_shade};

pub(in crate::content::textures) fn slag_pattern() -> TexturePattern {
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

pub(in crate::content::textures) fn molten_pattern() -> TexturePattern {
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

pub(in crate::content::textures) fn aggregate_pattern() -> TexturePattern {
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
