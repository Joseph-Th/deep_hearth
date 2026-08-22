//! Tests for the sibling texture baking module; isolated so test-only edits do not invalidate production builds.

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
