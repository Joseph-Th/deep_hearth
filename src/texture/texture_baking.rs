//! Bakes immutable texture definitions into compact deterministic GPU upload data.

use super::{
    BLOCK_FACE_COUNT, BlockAppearanceId, ColorRgba8, CubeFace, ObjectAppearanceId,
    ObjectTextureSlot, PALETTE_RAMP_COLOR_COUNT, PackedTexel, TEXTURE_PALETTE_SLOT_COUNT,
    TextureAlphaMode, TextureId, TextureRegistry,
};

mod appearance;
mod layout;
mod mip;

pub use mip::IndexedMipLevel;

#[cfg(test)]
use super::{PaletteSlot, ShadeIndex, TEXTURE_SIDE};
#[cfg(test)]
use mip::resolve_mip_texel;

/// Dense GPU array layer containing one unique indexed pattern and all of its mip levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureLayer(u16);

impl TextureLayer {
    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Dense row in the texture-local-slot to global-ramp lookup table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TexturePaletteRow(u16);

impl TexturePaletteRow {
    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Constant-sized draw descriptor resolved once while building block faces or object meshes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BakedTextureDescriptor {
    layer: TextureLayer,
    palette_row: TexturePaletteRow,
    alpha_mode: TextureAlphaMode,
}

impl BakedTextureDescriptor {
    #[must_use]
    pub const fn layer(self) -> TextureLayer {
        self.layer
    }

    #[must_use]
    pub const fn palette_row(self) -> TexturePaletteRow {
        self.palette_row
    }

    #[must_use]
    pub const fn alpha_mode(self) -> TextureAlphaMode {
        self.alpha_mode
    }

    /// Packs the two shader-facing lookup coordinates into one mesh-friendly `u32` value.
    #[must_use]
    pub const fn gpu_key(self) -> u32 {
        self.layer.value() as u32 | ((self.palette_row.value() as u32) << 16)
    }
}

/// Six already-resolved draw descriptors for one block appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BakedBlockAppearance {
    textures: [BakedTextureDescriptor; BLOCK_FACE_COUNT],
}

impl BakedBlockAppearance {
    #[must_use]
    pub const fn texture(self, face: CubeFace) -> BakedTextureDescriptor {
        self.textures[face.index()]
    }
}

/// Already-resolved draw descriptors for one object's ordered mesh material slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BakedObjectAppearance {
    textures: Vec<BakedTextureDescriptor>,
}

impl BakedObjectAppearance {
    #[must_use]
    pub fn get_texture(&self, slot: ObjectTextureSlot) -> Option<BakedTextureDescriptor> {
        self.textures.get(usize::from(slot.value())).copied()
    }

    #[must_use]
    pub fn textures(&self) -> &[BakedTextureDescriptor] {
        &self.textures
    }
}

/// Renderer-neutral GPU upload payload for indexed tiles, palette ramps, and palette rows.
///
/// Mip texels are `R8_UINT`-compatible bytes and must use nearest/point sampling because interpolated
/// indices are meaningless. A shader decodes `slot = texel >> 4` and `shade = texel & 15`, adds any
/// clamped face or world-light shade delta, loads a ramp ID from
/// `palette_rows[palette_row * 16 + slot]`, then loads RGBA from
/// `palette_colors[ramp_id * 16 + shade]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BakedTextureArray {
    descriptors_by_texture: Vec<Option<BakedTextureDescriptor>>,
    blocks_by_id: Vec<Option<BakedBlockAppearance>>,
    objects_by_id: Vec<Option<BakedObjectAppearance>>,
    mip_levels: Vec<IndexedMipLevel>,
    palette_rows: Vec<u16>,
    palette_color_bytes: Vec<u8>,
    pattern_layer_count: u16,
    palette_row_count: u16,
}

impl TextureRegistry {
    /// Bakes immutable definitions into compact deterministic GPU upload arrays.
    #[must_use]
    pub fn bake_texture_array(&self) -> BakedTextureArray {
        let layout::BakedTextureLayout {
            descriptors_by_texture,
            patterns,
            palette_rows,
        } = layout::bake_texture_layout(self);
        let pattern_layer_count = u16::try_from(patterns.len())
            .unwrap_or_else(|_| panic!("baked texture layer count exceeds lookup limit"));
        let palette_row_count = u16::try_from(palette_rows.len())
            .unwrap_or_else(|_| panic!("baked palette row count exceeds lookup limit"));
        let mip_levels = mip::build_mip_levels(patterns);
        let palette_color_bytes = layout::bake_palette_color_bytes(self);

        let blocks_by_id = appearance::bake_block_appearances(self, &descriptors_by_texture);
        let objects_by_id = appearance::bake_object_appearances(self, &descriptors_by_texture);

        BakedTextureArray {
            descriptors_by_texture,
            blocks_by_id,
            objects_by_id,
            mip_levels,
            palette_rows: palette_rows.into_iter().flatten().collect(),
            palette_color_bytes,
            pattern_layer_count,
            palette_row_count,
        }
    }
}

impl BakedTextureArray {
    #[must_use]
    pub fn get_descriptor(&self, texture: TextureId) -> Option<BakedTextureDescriptor> {
        self.descriptors_by_texture
            .get(usize::from(texture.value()))
            .copied()
            .flatten()
    }

    #[must_use]
    pub fn get_block(&self, id: BlockAppearanceId) -> Option<BakedBlockAppearance> {
        self.blocks_by_id
            .get(usize::from(id.value()))
            .copied()
            .flatten()
    }

    #[must_use]
    pub fn get_object(&self, id: ObjectAppearanceId) -> Option<&BakedObjectAppearance> {
        self.objects_by_id
            .get(usize::from(id.value()))
            .and_then(Option::as_ref)
    }

    #[must_use]
    pub fn mip_levels(&self) -> &[IndexedMipLevel] {
        &self.mip_levels
    }

    #[must_use]
    pub fn palette_rows(&self) -> &[u16] {
        &self.palette_rows
    }

    /// Returns tightly packed RGBA8 bytes ready for the global ramp lookup texture.
    #[must_use]
    pub fn palette_color_bytes(&self) -> &[u8] {
        &self.palette_color_bytes
    }

    #[must_use]
    pub const fn pattern_layer_count(&self) -> u16 {
        self.pattern_layer_count
    }

    #[must_use]
    pub const fn palette_row_count(&self) -> u16 {
        self.palette_row_count
    }

    /// Resolves an indexed sample for adapter tests, previews, or a CPU renderer.
    #[must_use]
    pub fn sample(&self, texture: TextureId, mip_level: usize, x: u8, y: u8) -> Option<ColorRgba8> {
        let descriptor = self.get_descriptor(texture)?;
        let texel = self
            .mip_levels
            .get(mip_level)?
            .get_texel(descriptor.layer(), x, y)?;
        self.resolve_texel(descriptor.palette_row(), texel)
    }

    #[must_use]
    pub fn indexed_texel_bytes(&self) -> usize {
        self.mip_levels
            .iter()
            .map(|level| level.texels().len())
            .sum()
    }

    #[must_use]
    pub fn palette_lookup_bytes(&self) -> usize {
        self.palette_rows.len() * std::mem::size_of::<u16>() + self.palette_color_bytes.len()
    }

    #[must_use]
    pub fn total_gpu_bytes(&self) -> usize {
        self.indexed_texel_bytes() + self.palette_lookup_bytes()
    }

    #[must_use]
    pub fn expanded_rgba_texel_bytes(&self) -> usize {
        self.mip_levels
            .iter()
            .map(|level| level.texels().len() * std::mem::size_of::<ColorRgba8>())
            .sum()
    }

    fn resolve_texel(
        &self,
        palette_row: TexturePaletteRow,
        texel: PackedTexel,
    ) -> Option<ColorRgba8> {
        let row_index = usize::from(palette_row.value()) * TEXTURE_PALETTE_SLOT_COUNT;
        let ramp = *self
            .palette_rows
            .get(row_index + usize::from(texel.palette_slot().value()))?;
        if ramp == 0 {
            return None;
        }
        let color_index =
            usize::from(ramp) * PALETTE_RAMP_COLOR_COUNT + usize::from(texel.shade().value());
        let byte_index = color_index.checked_mul(std::mem::size_of::<ColorRgba8>())?;
        let channels = self.palette_color_bytes.get(byte_index..byte_index + 4)?;
        Some(ColorRgba8::new(
            channels[0],
            channels[1],
            channels[2],
            channels[3],
        ))
    }
}

#[cfg(test)]
#[path = "texture_baking_tests.rs"]
mod tests;
