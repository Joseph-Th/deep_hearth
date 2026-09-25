//! Deterministic indexed-texture mip construction.

use super::TextureLayer;
use crate::texture::{
    PackedTexel, PaletteSlot, ShadeIndex, TEXTURE_MIP_LEVEL_COUNT, TEXTURE_PALETTE_SLOT_COUNT,
    TEXTURE_SIDE, TEXTURE_TEXEL_COUNT,
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
) -> Vec<IndexedMipLevel> {
    let layer_count = layers.len();
    let mut current = Vec::with_capacity(layer_count.saturating_mul(TEXTURE_TEXEL_COUNT));
    for layer in layers {
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
    }
    mip_levels
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
    let mut slot_counts = [0_u8; TEXTURE_PALETTE_SLOT_COUNT];
    for sample in samples {
        slot_counts[usize::from(sample.palette_slot().value())] += 1;
    }
    let mut selected_slot = 0_usize;
    for slot in 1..TEXTURE_PALETTE_SLOT_COUNT {
        if slot_counts[slot] > slot_counts[selected_slot] {
            selected_slot = slot;
        }
    }

    let mut shade_sum = 0_u16;
    let mut shade_count = 0_u16;
    for sample in samples {
        if usize::from(sample.palette_slot().value()) == selected_slot {
            shade_sum += u16::from(sample.shade().value());
            shade_count += 1;
        }
    }
    let rounded_shade = (shade_sum + shade_count / 2) / shade_count;
    PackedTexel::new(
        PaletteSlot::new(
            u8::try_from(selected_slot)
                .unwrap_or_else(|_| panic!("palette slot exceeds texel range")),
        ),
        ShadeIndex::new(
            u8::try_from(rounded_shade).unwrap_or_else(|_| panic!("shade exceeds texel range")),
        ),
    )
}
