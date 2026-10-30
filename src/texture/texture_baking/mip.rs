//! Deterministic indexed-texture mip construction.

use super::TextureLayer;
use crate::texture::{
    PackedTexel, PaletteSlot, ShadeIndex, TEXTURE_MIP_LEVEL_COUNT, TEXTURE_SIDE,
    TEXTURE_TEXEL_COUNT,
};

/// One mip level with all unique pattern layers stored contiguously in layer-major order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedMipLevel {
    side: u8,
    texels: Vec<u8>,
}

impl IndexedMipLevel {
    #[must_use]
    pub const fn side(&self) -> u8 {
        self.side
    }

    /// Returns bytes ready for an `R8_UINT` texture-array upload.
    #[must_use]
    pub fn texels(&self) -> &[u8] {
        &self.texels
    }

    #[must_use]
    pub fn get_texel(&self, layer: TextureLayer, x: u8, y: u8) -> Option<PackedTexel> {
        if x >= self.side || y >= self.side {
            return None;
        }
        let side = usize::from(self.side);
        let layer_stride = side * side;
        let index = usize::from(layer.value())
            .checked_mul(layer_stride)?
            .checked_add(usize::from(y) * side + usize::from(x))?;
        self.texels.get(index).copied().map(PackedTexel::from_raw)
    }
}

pub(super) fn build_mip_levels(
    layers: Vec<&[PackedTexel; TEXTURE_TEXEL_COUNT]>,
    cutout_opacity: Vec<Option<[u8; 256]>>,
) -> Vec<IndexedMipLevel> {
    assert_eq!(layers.len(), cutout_opacity.len());
    let layer_count = layers.len();
    let mut current = Vec::with_capacity(layer_count.saturating_mul(TEXTURE_TEXEL_COUNT));
    for &layer in &layers {
        current.extend_from_slice(layer);
    }

    let mut mip_levels = Vec::with_capacity(TEXTURE_MIP_LEVEL_COUNT);
    let mut side = TEXTURE_SIDE;
    loop {
        let mut texels = Vec::with_capacity(current.len());
        texels.extend(current.iter().map(|texel| texel.raw_value()));
        mip_levels.push(IndexedMipLevel {
            side: u8::try_from(side)
                .unwrap_or_else(|_| panic!("texture side exceeds mip descriptor range")),
            texels,
        });
        if side == 1 {
            break;
        }
        downsample_layers_in_place(&mut current, side, layer_count);
        side /= 2;
        for (layer_index, opacity) in cutout_opacity.iter().enumerate() {
            if let Some(opacity) = opacity {
                preserve_cutout_coverage(
                    &mut current,
                    side,
                    layer_index,
                    layers[layer_index],
                    opacity,
                );
            }
        }
    }
    mip_levels
}

/// A repeated majority vote changes a sparse wire mesh into a solid sheet:
/// at each reduction the few surviving opaque cells win ever more 2x2 votes.
/// Use the *original* alpha mask instead, retaining the nearest representable
/// overall cutout coverage at each level. The most covered footprints win;
/// a stable spatial tie break distributes equally covered fine details.
fn preserve_cutout_coverage(
    target: &mut [PackedTexel],
    side: usize,
    layer_index: usize,
    original: &[PackedTexel; TEXTURE_TEXEL_COUNT],
    opaque_by_texel: &[u8; 256],
) {
    let footprint_side = TEXTURE_SIDE / side;
    let stride = side * side;
    let base_opaque = original
        .iter()
        .filter(|texel| opaque_by_texel[usize::from(texel.raw_value())] != 0)
        .count();
    let wanted = (base_opaque * stride + TEXTURE_TEXEL_COUNT / 2) / TEXTURE_TEXEL_COUNT;
    let mut pixels = Vec::with_capacity(stride);
    for index in 0..stride {
        let x = (index % side) * footprint_side;
        let y = (index / side) * footprint_side;
        let mut opaque = 0_usize;
        for dy in 0..footprint_side {
            for dx in 0..footprint_side {
                let texel = original[(y + dy) * TEXTURE_SIDE + x + dx];
                opaque += usize::from(opaque_by_texel[usize::from(texel.raw_value())]);
            }
        }
        // Tie-breaking uses a fixed, non-periodic coordinate mix rather than
        // palette ID or source iteration order, which could align with a grid.
        let mut tie = (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        tie ^= tie >> 30;
        tie = tie.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        tie ^= tie >> 27;
        pixels.push((index, opaque, tie));
    }
    pixels.sort_unstable_by_key(|(index, opaque, tie)| (std::cmp::Reverse(*opaque), *tie, *index));
    let offset = layer_index * stride;
    for (rank, (index, _, _)) in pixels.into_iter().enumerate() {
        let x = (index % side) * footprint_side;
        let y = (index / side) * footprint_side;
        target[offset + index] = representative_cutout_texel(
            original,
            opaque_by_texel,
            x,
            y,
            footprint_side,
            rank < wanted,
        );
    }
    debug_assert!(wanted <= stride);
}

/// Choose an actual palette slot with the selected physical opacity; shade
/// averages only the eligible source samples, never transparent samples.
fn representative_cutout_texel(
    original: &[PackedTexel; TEXTURE_TEXEL_COUNT],
    opaque_by_texel: &[u8; 256],
    x: usize,
    y: usize,
    footprint_side: usize,
    opaque: bool,
) -> PackedTexel {
    let mut counts = [0_u16; 16];
    let mut shades = [0_u32; 16];
    for dy in 0..footprint_side {
        for dx in 0..footprint_side {
            let texel = original[(y + dy) * TEXTURE_SIDE + x + dx];
            if (opaque_by_texel[usize::from(texel.raw_value())] != 0) == opaque {
                let slot = usize::from(texel.palette_slot().value());
                counts[slot] += 1;
                shades[slot] += u32::from(texel.shade().value());
            }
        }
    }
    let slot = (0..16)
        .max_by_key(|&slot| (counts[slot], std::cmp::Reverse(slot)))
        .unwrap_or_else(|| unreachable!("a palette always has local slots"));
    let count = u32::from(counts[slot]);
    assert!(
        count != 0,
        "coverage selection requires a represented alpha"
    );
    let rounded_shade = (shades[slot] + count / 2) / count;
    // A ramp may itself switch from transparent to opaque between adjacent
    // shades. Averaging opaque samples can produce a transparent shade (or
    // vice versa), so select an actual source shade nearest the mean instead.
    let mut chosen_shade: Option<u32> = None;
    for dy in 0..footprint_side {
        for dx in 0..footprint_side {
            let sample = original[(y + dy) * TEXTURE_SIDE + x + dx];
            if usize::from(sample.palette_slot().value()) == slot
                && (opaque_by_texel[usize::from(sample.raw_value())] != 0) == opaque
            {
                let shade = u32::from(sample.shade().value());
                if chosen_shade.is_none_or(|current| {
                    (shade.abs_diff(rounded_shade), shade)
                        < (current.abs_diff(rounded_shade), current)
                }) {
                    chosen_shade = Some(shade);
                }
            }
        }
    }
    PackedTexel::new(
        PaletteSlot::new(slot as u8),
        ShadeIndex::new(
            u8::try_from(chosen_shade.expect("selected slot has eligible source shades"))
                .expect("source shade is four bits"),
        ),
    )
}

fn downsample_layers_in_place(
    source: &mut Vec<PackedTexel>,
    source_side: usize,
    layer_count: usize,
) {
    let source_stride = source_side * source_side;
    let target_side = source_side / 2;
    let target_stride = target_side * target_side;
    for layer in 0..layer_count {
        let source_offset = layer * source_stride;
        let target_offset = layer * target_stride;
        for y in 0..target_side {
            for x in 0..target_side {
                let source_x = x * 2;
                let source_y = y * 2;
                let top_left = source_offset + source_y * source_side + source_x;
                let samples = [
                    source[top_left],
                    source[top_left + 1],
                    source[top_left + source_side],
                    source[top_left + source_side + 1],
                ];
                source[target_offset + y * target_side + x] = resolve_mip_texel(samples);
            }
        }
    }
    source.truncate(layer_count.saturating_mul(target_stride));
}

pub(super) fn resolve_mip_texel(samples: [PackedTexel; 4]) -> PackedTexel {
    // A 2x2 mip footprint has only four candidates. Counting those candidates directly avoids
    // zeroing and scanning the full 16-slot palette histogram for every output texel.
    let mut selected_slot = samples[0].palette_slot().value();
    let mut selected_count = 0_u8;
    for candidate in samples {
        let slot = candidate.palette_slot().value();
        let count = samples
            .iter()
            .filter(|sample| sample.palette_slot().value() == slot)
            .count() as u8;
        if count > selected_count || (count == selected_count && slot < selected_slot) {
            selected_slot = slot;
            selected_count = count;
        }
    }

    let mut shade_sum = 0_u16;
    let mut shade_count = 0_u16;
    for sample in samples {
        if sample.palette_slot().value() == selected_slot {
            shade_sum += u16::from(sample.shade().value());
            shade_count += 1;
        }
    }
    let rounded_shade = (shade_sum + shade_count / 2) / shade_count;
    PackedTexel::new(
        PaletteSlot::new(selected_slot),
        ShadeIndex::new(
            u8::try_from(rounded_shade).unwrap_or_else(|_| panic!("shade exceeds texel range")),
        ),
    )
}
