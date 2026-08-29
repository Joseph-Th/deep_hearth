//! Contract tests for texture definitions.

use super::*;

const RAMP: PaletteRampId = PaletteRampId::new(1);
const TEXTURE: TextureId = TextureId::new(1);

fn ramp(alpha: u8) -> PaletteRampDefinition {
    PaletteRampDefinition::from_anchors(
        RAMP,
        "fixture",
        [
            ColorRgba8::new(10, 20, 30, alpha),
            ColorRgba8::new(30, 40, 50, alpha),
            ColorRgba8::new(60, 70, 80, alpha),
            ColorRgba8::new(100, 110, 120, alpha),
        ],
    )
}

fn texture(alpha_mode: TextureAlphaMode) -> TextureDefinition {
    TextureDefinition::new(
        TEXTURE,
        "fixture",
        TexturePalette::new(vec![RAMP]),
        alpha_mode,
        [PackedTexel::new(PaletteSlot::new(0), ShadeIndex::new(8)); TEXTURE_TEXEL_COUNT],
    )
}

#[test]
fn packed_texel_round_trip_uses_exactly_one_byte() {
    let texel = PackedTexel::new(PaletteSlot::new(11), ShadeIndex::new(6));

    assert_eq!(std::mem::size_of::<PackedTexel>(), 1);
    assert_eq!(std::mem::size_of::<ColorRgba8>(), 4);
    assert_eq!(texel.raw_value(), 0xb6);
    assert_eq!(texel.palette_slot(), PaletteSlot::new(11));
    assert_eq!(texel.shade(), ShadeIndex::new(6));
    assert_eq!(texel.with_shade_offset(20).raw_value(), 0xbf);
    assert_eq!(texel.with_shade_offset(-20).raw_value(), 0xb0);
}

#[test]
fn authored_ramp_anchors_are_preserved_exactly() {
    let ramp = ramp(u8::MAX);

    assert_eq!(ramp.colors()[0], ColorRgba8::opaque(10, 20, 30));
    assert_eq!(ramp.colors()[5], ColorRgba8::opaque(30, 40, 50));
    assert_eq!(ramp.colors()[10], ColorRgba8::opaque(60, 70, 80));
    assert_eq!(ramp.colors()[15], ColorRgba8::opaque(100, 110, 120));
}

#[test]
fn opaque_texture_rejects_transparent_palette_resolution() {
    let result = std::panic::catch_unwind(|| {
        TextureRegistry::new(
            [ramp(0)],
            [texture(TextureAlphaMode::Opaque)],
            std::iter::empty(),
            std::iter::empty(),
            std::iter::empty(),
            std::iter::empty(),
        )
    });

    assert!(result.is_err());
}

#[test]
fn block_face_mapping_is_explicit_and_stable() {
    let other = TextureId::new(2);
    let block = BlockAppearanceDefinition::top_side_bottom(
        BlockAppearanceId::new(1),
        "fixture",
        TEXTURE,
        other,
        TextureId::new(3),
    );

    assert_eq!(block.texture(CubeFace::Top), TEXTURE);
    assert_eq!(block.texture(CubeFace::North), other);
    assert_eq!(block.texture(CubeFace::West), other);
    assert_eq!(block.texture(CubeFace::Bottom), TextureId::new(3));
}
