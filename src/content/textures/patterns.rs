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

mod ore;
mod panel;
mod processed;
mod wood;
mod workshop;

pub(super) use ore::ore_pattern;
pub(super) use panel::panel_pattern;
pub(super) use processed::{aggregate_pattern, molten_pattern, slag_pattern};
pub(super) use wood::{wood_end_pattern, wood_side_pattern};
pub(super) use workshop::{refractory_pattern, screen_pattern, working_metal_pattern};
