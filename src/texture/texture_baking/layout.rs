//! Deduplicates authored texture patterns and palette data into dense bake-time lookup layouts.

use std::collections::HashMap;

use crate::texture::{
    PALETTE_RAMP_COLOR_COUNT, PackedTexel, TEXTURE_PALETTE_SLOT_COUNT, TEXTURE_TEXEL_COUNT,
    TextureRegistry,
};

use super::{BakedTextureDescriptor, TextureLayer, TexturePaletteRow};

pub(super) struct BakedTextureLayout<'registry> {
    pub(super) descriptors_by_texture: Vec<Option<BakedTextureDescriptor>>,
    pub(super) patterns: Vec<&'registry [PackedTexel; TEXTURE_TEXEL_COUNT]>,
    pub(super) palette_rows: Vec<[u16; TEXTURE_PALETTE_SLOT_COUNT]>,
}

pub(super) fn bake_texture_layout(registry: &TextureRegistry) -> BakedTextureLayout<'_> {
    let texture_count = registry.textures_in_id_order().len();
    let mut patterns = Vec::<&[PackedTexel; TEXTURE_TEXEL_COUNT]>::new();
    let mut pattern_layers =
        HashMap::<&[PackedTexel; TEXTURE_TEXEL_COUNT], TextureLayer>::with_capacity(texture_count);
    let mut palette_rows = Vec::<[u16; TEXTURE_PALETTE_SLOT_COUNT]>::new();
    let mut palette_row_ids =
        HashMap::<[u16; TEXTURE_PALETTE_SLOT_COUNT], TexturePaletteRow>::with_capacity(
            texture_count,
        );

    let texture_lookup_len = registry
        .textures_in_id_order()
        .map(|definition| usize::from(definition.id().value()))
        .max()
        .map_or(0, |maximum_id| maximum_id + 1);
    let mut descriptors_by_texture = vec![None; texture_lookup_len];

    for definition in registry.textures_in_id_order() {
        let pattern = definition.texels();
        let layer = match pattern_layers.get(pattern) {
            Some(layer) => *layer,
            None => {
                let layer =
                    TextureLayer(u16::try_from(patterns.len()).unwrap_or_else(|_| {
                        panic!("baked texture layer count exceeds lookup limit")
                    }));
                patterns.push(pattern);
                pattern_layers.insert(pattern, layer);
                layer
            }
        };

        let mut palette_row = [0_u16; TEXTURE_PALETTE_SLOT_COUNT];
        for (slot, ramp) in definition.palette().ramps().iter().enumerate() {
            palette_row[slot] = ramp.value();
        }
        let palette_row = match palette_row_ids.get(&palette_row) {
            Some(row) => *row,
            None => {
                let row =
                    TexturePaletteRow(u16::try_from(palette_rows.len()).unwrap_or_else(|_| {
                        panic!("baked palette row count exceeds lookup limit")
                    }));
                palette_rows.push(palette_row);
                palette_row_ids.insert(palette_row, row);
                row
            }
        };

        descriptors_by_texture[usize::from(definition.id().value())] =
            Some(BakedTextureDescriptor {
                layer,
                palette_row,
                alpha_mode: definition.alpha_mode(),
            });
    }

    BakedTextureLayout {
        descriptors_by_texture,
        patterns,
        palette_rows,
    }
}

pub(super) fn bake_palette_color_bytes(registry: &TextureRegistry) -> Vec<u8> {
    let ramp_lookup_len = registry
        .ramps_in_id_order()
        .map(|definition| usize::from(definition.id().value()))
        .max()
        .map_or(0, |maximum_id| maximum_id + 1);
    let palette_color_count = ramp_lookup_len
        .checked_mul(PALETTE_RAMP_COLOR_COUNT)
        .unwrap_or_else(|| panic!("baked palette color count exceeds addressable memory"));
    let mut palette_color_bytes = vec![
        0_u8;
        palette_color_count.checked_mul(4).unwrap_or_else(|| {
            panic!("baked palette byte count exceeds addressable memory")
        })
    ];
    for ramp in registry.ramps_in_id_order() {
        let color_start = usize::from(ramp.id().value())
            .checked_mul(PALETTE_RAMP_COLOR_COUNT)
            .unwrap_or_else(|| panic!("baked palette color offset overflowed"));
        for (offset, color) in ramp.colors().iter().enumerate() {
            let byte_start = color_start
                .checked_add(offset)
                .and_then(|index| index.checked_mul(4))
                .unwrap_or_else(|| panic!("baked palette byte offset overflowed"));
            palette_color_bytes[byte_start..byte_start + 4].copy_from_slice(&color.channels());
        }
    }
    palette_color_bytes
}
