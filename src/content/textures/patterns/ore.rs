//! In-situ ore texture pattern.

use crate::texture::TEXTURE_SIDE;

use super::{TexturePattern, base_noise_pattern, hash_2d, packed, varied_shade};

pub(in crate::content::textures) fn ore_pattern() -> TexturePattern {
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
