//! Compact deterministic texture-array baking; sibling definitions own authored visual content.

use std::collections::BTreeMap;

use super::{
    BLOCK_FACE_COUNT, BlockAppearanceId, ColorRgba8, CubeFace, ObjectAppearanceId,
    ObjectTextureSlot, PALETTE_RAMP_COLOR_COUNT, PackedTexel, PaletteSlot, ShadeIndex,
    TEXTURE_PALETTE_SLOT_COUNT, TEXTURE_SIDE, TextureAlphaMode, TextureId, TextureRegistry,
};

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
        match face {
            CubeFace::Top => self.textures[0],
            CubeFace::Bottom => self.textures[1],
            CubeFace::North => self.textures[2],
            CubeFace::South => self.textures[3],
            CubeFace::East => self.textures[4],
            CubeFace::West => self.textures[5],
        }
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
        let mut patterns = Vec::<Vec<PackedTexel>>::new();
        let mut pattern_layers = BTreeMap::<Vec<PackedTexel>, TextureLayer>::new();
        let mut palette_rows = Vec::<[u16; TEXTURE_PALETTE_SLOT_COUNT]>::new();
        let mut palette_row_ids =
            BTreeMap::<[u16; TEXTURE_PALETTE_SLOT_COUNT], TexturePaletteRow>::new();

        let maximum_texture_id = self
            .textures_in_id_order()
            .map(|definition| usize::from(definition.id().value()))
            .max()
            .unwrap_or(0);
        let mut descriptors_by_texture = vec![None; maximum_texture_id + 1];

        for definition in self.textures_in_id_order() {
            let pattern = definition.texels().to_vec();
            let layer = match pattern_layers.get(&pattern) {
                Some(layer) => *layer,
                None => {
                    let layer = TextureLayer(patterns.len() as u16);
                    patterns.push(pattern.clone());
                    pattern_layers.insert(pattern, layer);
                    layer
                }
            };

            let mut palette_row = [0_u16; TEXTURE_PALETTE_SLOT_COUNT];
            for (slot, ramp) in definition.palette().ramps().iter().enumerate() {
                palette_row[slot] = ramp.value();
            }
            let palette_row_id = match palette_row_ids.get(&palette_row) {
                Some(row) => *row,
                None => {
                    let row = TexturePaletteRow(palette_rows.len() as u16);
                    palette_rows.push(palette_row);
                    palette_row_ids.insert(palette_row, row);
                    row
                }
            };

            descriptors_by_texture[usize::from(definition.id().value())] =
                Some(BakedTextureDescriptor {
                    layer,
                    palette_row: palette_row_id,
                    alpha_mode: definition.alpha_mode(),
                });
        }

        let mip_levels = build_mip_levels(patterns);
        let maximum_ramp_id = self
            .ramps_in_id_order()
            .map(|definition| usize::from(definition.id().value()))
            .max()
            .unwrap_or(0);
        let mut palette_colors =
            vec![ColorRgba8::default(); (maximum_ramp_id + 1) * PALETTE_RAMP_COLOR_COUNT];
        for ramp in self.ramps_in_id_order() {
            let start = usize::from(ramp.id().value()) * PALETTE_RAMP_COLOR_COUNT;
            palette_colors[start..start + PALETTE_RAMP_COLOR_COUNT].copy_from_slice(ramp.colors());
        }

        let palette_color_bytes = palette_colors
            .into_iter()
            .flat_map(ColorRgba8::channels)
            .collect();
        let blocks_by_id = bake_block_appearances(self, &descriptors_by_texture);
        let objects_by_id = bake_object_appearances(self, &descriptors_by_texture);

        BakedTextureArray {
            descriptors_by_texture,
            blocks_by_id,
            objects_by_id,
            mip_levels,
            palette_rows: palette_rows.into_iter().flatten().collect(),
            palette_color_bytes,
            pattern_layer_count: pattern_layers.len() as u16,
            palette_row_count: palette_row_ids.len() as u16,
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

fn bake_block_appearances(
    registry: &TextureRegistry,
    descriptors: &[Option<BakedTextureDescriptor>],
) -> Vec<Option<BakedBlockAppearance>> {
    let maximum_id = registry
        .blocks_in_id_order()
        .map(|definition| usize::from(definition.id().value()))
        .max()
        .unwrap_or(0);
    let mut baked = vec![None; maximum_id + 1];
    for definition in registry.blocks_in_id_order() {
        let textures = definition
            .textures()
            .map(|texture| required_descriptor(descriptors, texture));
        baked[usize::from(definition.id().value())] = Some(BakedBlockAppearance { textures });
    }
    baked
}

fn bake_object_appearances(
    registry: &TextureRegistry,
    descriptors: &[Option<BakedTextureDescriptor>],
) -> Vec<Option<BakedObjectAppearance>> {
    let maximum_id = registry
        .objects_in_id_order()
        .map(|definition| usize::from(definition.id().value()))
        .max()
        .unwrap_or(0);
    let mut baked = vec![None; maximum_id + 1];
    for definition in registry.objects_in_id_order() {
        let textures = definition
            .textures()
            .iter()
            .map(|texture| required_descriptor(descriptors, *texture))
            .collect();
        baked[usize::from(definition.id().value())] = Some(BakedObjectAppearance { textures });
    }
    baked
}

fn required_descriptor(
    descriptors: &[Option<BakedTextureDescriptor>],
    texture: TextureId,
) -> BakedTextureDescriptor {
    match descriptors
        .get(usize::from(texture.value()))
        .copied()
        .flatten()
    {
        Some(descriptor) => descriptor,
        None => panic!(
            "validated texture appearance references missing texture {} during bake",
            texture.value()
        ),
    }
}

fn build_mip_levels(mut layers: Vec<Vec<PackedTexel>>) -> Vec<IndexedMipLevel> {
    let mut mip_levels = Vec::new();
    let mut side = TEXTURE_SIDE;
    loop {
        mip_levels.push(IndexedMipLevel {
            side: side as u8,
            texels: layers
                .iter()
                .flatten()
                .map(|texel| texel.raw_value())
                .collect(),
        });
        if side == 1 {
            break;
        }
        layers = layers
            .iter()
            .map(|layer| downsample_layer(layer, side))
            .collect();
        side /= 2;
    }
    mip_levels
}

fn downsample_layer(source: &[PackedTexel], source_side: usize) -> Vec<PackedTexel> {
    let target_side = source_side / 2;
    let mut target = Vec::with_capacity(target_side * target_side);
    for y in 0..target_side {
        for x in 0..target_side {
            let source_x = x * 2;
            let source_y = y * 2;
            let samples = [
                source[source_y * source_side + source_x],
                source[source_y * source_side + source_x + 1],
                source[(source_y + 1) * source_side + source_x],
                source[(source_y + 1) * source_side + source_x + 1],
            ];
            target.push(resolve_mip_texel(samples));
        }
    }
    target
}

fn resolve_mip_texel(samples: [PackedTexel; 4]) -> PackedTexel {
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
        PaletteSlot::new(selected_slot as u8),
        ShadeIndex::new(rounded_shade as u8),
    )
}

#[cfg(all(
    test,
    any(not(feature = "test-unit-sharded"), feature = "test-unit-render")
))]
mod tests {
    use super::*;
    use crate::texture::{
        PaletteRampDefinition, PaletteRampId, TEXTURE_MIP_LEVEL_COUNT, TextureDefinition,
        TexturePalette,
    };

    const RAMP_A: PaletteRampId = PaletteRampId::new(1);
    const RAMP_B: PaletteRampId = PaletteRampId::new(2);

    fn ramp(id: PaletteRampId, value: u8) -> PaletteRampDefinition {
        PaletteRampDefinition::new(
            id,
            "fixture",
            [ColorRgba8::opaque(value, value, value); PALETTE_RAMP_COLOR_COUNT],
        )
    }

    fn pattern() -> [PackedTexel; TEXTURE_SIDE * TEXTURE_SIDE] {
        std::array::from_fn(|index| {
            PackedTexel::new(
                PaletteSlot::new((index % 2) as u8),
                ShadeIndex::new((index % PALETTE_RAMP_COLOR_COUNT) as u8),
            )
        })
    }

    #[test]
    fn bake_deduplicates_patterns_and_palette_rows_independently() {
        let texture_a = TextureDefinition::new(
            TextureId::new(1),
            "a",
            TexturePalette::new(vec![RAMP_A, RAMP_B]),
            TextureAlphaMode::Opaque,
            pattern(),
        );
        let texture_b = TextureDefinition::new(
            TextureId::new(2),
            "b",
            TexturePalette::new(vec![RAMP_B, RAMP_A]),
            TextureAlphaMode::Opaque,
            pattern(),
        );
        let registry = TextureRegistry::new(
            [ramp(RAMP_A, 20), ramp(RAMP_B, 200)],
            [texture_a, texture_b],
            std::iter::empty(),
            std::iter::empty(),
            std::iter::empty(),
            std::iter::empty(),
        );

        let baked = registry.bake_texture_array();

        assert_eq!(baked.pattern_layer_count(), 1);
        assert_eq!(baked.palette_row_count(), 2);
        assert_eq!(baked.mip_levels().len(), TEXTURE_MIP_LEVEL_COUNT);
        assert_eq!(baked.mip_levels()[TEXTURE_MIP_LEVEL_COUNT - 1].side(), 1);
        assert_eq!(
            baked
                .get_descriptor(TextureId::new(1))
                .map(|item| item.layer()),
            baked
                .get_descriptor(TextureId::new(2))
                .map(|item| item.layer())
        );
        assert_ne!(
            baked.sample(TextureId::new(1), 0, 0, 0),
            baked.sample(TextureId::new(2), 0, 0, 0)
        );
    }

    #[test]
    fn mip_resolution_prefers_majority_slot_and_rounds_its_shades() {
        let mip = resolve_mip_texel([
            PackedTexel::new(PaletteSlot::new(1), ShadeIndex::new(2)),
            PackedTexel::new(PaletteSlot::new(1), ShadeIndex::new(5)),
            PackedTexel::new(PaletteSlot::new(1), ShadeIndex::new(8)),
            PackedTexel::new(PaletteSlot::new(0), ShadeIndex::new(15)),
        ]);

        assert_eq!(mip.palette_slot(), PaletteSlot::new(1));
        assert_eq!(mip.shade(), ShadeIndex::new(5));
    }

    #[test]
    fn descriptor_gpu_key_packs_layer_and_palette_row_without_loss() {
        let descriptor = BakedTextureDescriptor {
            layer: TextureLayer(0x1234),
            palette_row: TexturePaletteRow(0xabcd),
            alpha_mode: TextureAlphaMode::Cutout,
        };

        assert_eq!(descriptor.gpu_key(), 0xabcd_1234);
    }
}
