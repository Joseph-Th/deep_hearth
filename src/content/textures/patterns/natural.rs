//! Natural terrain and living-vegetation texture patterns.

use crate::texture::TEXTURE_SIDE;

use super::{TexturePattern, base_noise_pattern, hash_2d, packed, varied_shade};

/// Toroidal squared distance keeps every procedural feature seamless across tile edges.
fn wrapped_squared_distance(x: usize, y: usize, center_x: usize, center_y: usize) -> usize {
    let dx = x.abs_diff(center_x);
    let dy = y.abs_diff(center_y);
    let dx = dx.min(TEXTURE_SIDE - dx);
    let dy = dy.min(TEXTURE_SIDE - dy);
    dx * dx + dy * dy
}

/// Jittered feature points on a 3x3 lattice; nearest-two distances outline rock fractures.
fn fracture_points() -> [(usize, usize); 9] {
    std::array::from_fn(|index| {
        let cell = TEXTURE_SIDE / 3;
        let jitter = hash_2d(0x2c1b_3c6d, index, 7);
        let x = (index % 3) * cell + (jitter as usize % cell);
        let y = (index / 3) * cell + ((jitter >> 12) as usize % cell);
        (x, y)
    })
}

/// Undressed host rock: broad mottling, faint bedding and a seamless fracture network.
pub(in crate::content::textures) fn host_rock_pattern() -> TexturePattern {
    let mut texels = base_noise_pattern(0x4b1d_9e27, 7, 3);
    let points = fracture_points();
    for y in 0..TEXTURE_SIDE {
        let bedding = hash_2d(0x77e1_2a0b, 0, y / 5) % 3;
        for x in 0..TEXTURE_SIDE {
            let index = y * TEXTURE_SIDE + x;
            let mut nearest = usize::MAX;
            let mut second = usize::MAX;
            for &(point_x, point_y) in &points {
                let distance = wrapped_squared_distance(x, y, point_x, point_y);
                if distance < nearest {
                    second = nearest;
                    nearest = distance;
                } else if distance < second {
                    second = distance;
                }
            }
            let fine = hash_2d(0x3d9a_6c51, x, y);
            if second - nearest <= 3 && !fine.is_multiple_of(4) {
                texels[index] = packed(1, varied_shade(6, 1, fine));
            } else if second - nearest <= 10 && fine.is_multiple_of(3) {
                texels[index] = packed(0, varied_shade(6, 1, fine));
            } else if bedding == 0 && fine.is_multiple_of(5) {
                texels[index] = packed(0, varied_shade(10, 1, fine));
            }
        }
    }
    texels
}

/// Overlapping leaf clusters with open sky between them. Slot 0 is the transparent ramp.
pub(in crate::content::textures) fn foliage_pattern() -> TexturePattern {
    const CLUSTERS: usize = 22;
    let clusters: [(usize, usize, usize); CLUSTERS] = std::array::from_fn(|index| {
        let hash = hash_2d(0x5f0e_a7c3, index, 3);
        let x = hash as usize % TEXTURE_SIDE;
        let y = (hash >> 8) as usize % TEXTURE_SIDE;
        let radius = 3 + (hash >> 16) as usize % 3;
        (x, y, radius)
    });
    std::array::from_fn(|index| {
        let x = index % TEXTURE_SIDE;
        let y = index / TEXTURE_SIDE;
        let fine = hash_2d(0x9a4c_1e83, x, y);
        let covering = clusters
            .iter()
            .filter_map(|&(center_x, center_y, radius)| {
                let distance = wrapped_squared_distance(x, y, center_x, center_y);
                (distance <= radius * radius).then_some((distance, radius))
            })
            .min_by_key(|&(distance, radius)| distance * 16 / (radius * radius));
        match covering {
            Some((distance, radius)) => {
                // Cluster centers catch more light than their shaded rims.
                let rim = (distance * 6 / (radius * radius)) as u8;
                packed(1, varied_shade(11_u8.saturating_sub(rim), 2, fine))
            }
            None => packed(0, 0),
        }
    })
}
