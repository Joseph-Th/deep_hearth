//! Contract tests for deterministic texture baking.

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
fn empty_registry_bakes_without_phantom_lookup_or_palette_data() {
    let registry = TextureRegistry::new(
        std::iter::empty(),
        std::iter::empty(),
        std::iter::empty(),
        std::iter::empty(),
        std::iter::empty(),
        std::iter::empty(),
    );

    let baked = registry.bake_texture_array();

    assert!(baked.descriptors_by_texture.is_empty());
    assert!(baked.blocks_by_id.is_empty());
    assert!(baked.objects_by_id.is_empty());
    assert!(baked.palette_rows().is_empty());
    assert!(baked.palette_color_bytes().is_empty());
    assert_eq!(baked.pattern_layer_count(), 0);
    assert_eq!(baked.palette_row_count(), 0);
    assert_eq!(baked.total_gpu_bytes(), 0);
    assert_eq!(baked.get_descriptor(TextureId::new(1)), None);
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
fn cutout_pattern_deduplication_distinguishes_alpha_palettes() {
    let transparent = PaletteRampDefinition::new(
        RAMP_B,
        "transparent",
        [ColorRgba8::new(0, 0, 0, 0); PALETTE_RAMP_COLOR_COUNT],
    );
    let first = TextureId::new(1);
    let second = TextureId::new(2);
    let registry = TextureRegistry::new(
        [ramp(RAMP_A, 80), transparent],
        [
            TextureDefinition::new(
                first,
                "steel at even pixels",
                TexturePalette::new(vec![RAMP_A, RAMP_B]),
                TextureAlphaMode::Cutout,
                pattern(),
            ),
            TextureDefinition::new(
                second,
                "steel at odd pixels",
                TexturePalette::new(vec![RAMP_B, RAMP_A]),
                TextureAlphaMode::Cutout,
                pattern(),
            ),
        ],
        std::iter::empty(),
        std::iter::empty(),
        std::iter::empty(),
        std::iter::empty(),
    );
    let baked = registry.bake_texture_array();
    assert_ne!(
        baked.get_descriptor(first).unwrap().layer(),
        baked.get_descriptor(second).unwrap().layer(),
        "one indexed pattern needs separate mips when its palette changes its alpha mask"
    );
    for texture in [first, second] {
        for mip in 0..=4 {
            let side = baked.mip_levels()[mip].side();
            let mut opaque = 0;
            for y in 0..side {
                for x in 0..side {
                    opaque +=
                        usize::from(baked.sample(texture, mip, x, y).unwrap().alpha() == u8::MAX);
                }
            }
            assert_eq!(
                opaque,
                usize::from(side) * usize::from(side) / 2,
                "cutout coverage changed at mip {mip}"
            );
        }
    }
}

#[test]
fn cutout_representative_shades_cannot_cross_an_alpha_boundary() {
    let mut colors = [ColorRgba8::new(0, 0, 0, 0); PALETTE_RAMP_COLOR_COUNT];
    colors[0] = ColorRgba8::opaque(20, 30, 40);
    colors[2] = ColorRgba8::opaque(40, 50, 60);
    let texture = TextureId::new(1);
    let texels = std::array::from_fn(|index| {
        let shade = match ((index / TEXTURE_SIDE) % 2, (index % TEXTURE_SIDE) % 2) {
            (0, 0) => 0,
            (0, 1) => 2,
            _ => 1,
        };
        PackedTexel::new(PaletteSlot::new(0), ShadeIndex::new(shade))
    });
    let registry = TextureRegistry::new(
        [PaletteRampDefinition::new(
            RAMP_A,
            "alternating alpha",
            colors,
        )],
        [TextureDefinition::new(
            texture,
            "shade-dependent alpha",
            TexturePalette::new(vec![RAMP_A]),
            TextureAlphaMode::Cutout,
            texels,
        )],
        std::iter::empty(),
        std::iter::empty(),
        std::iter::empty(),
        std::iter::empty(),
    );
    let baked = registry.bake_texture_array();
    for mip in 1..=4 {
        let side = baked.mip_levels()[mip].side();
        let mut opaque = 0;
        for y in 0..side {
            for x in 0..side {
                opaque += usize::from(baked.sample(texture, mip, x, y).unwrap().alpha() == u8::MAX);
            }
        }
        assert_eq!(
            opaque,
            usize::from(side) * usize::from(side) / 2,
            "averaging opaque shades across a transparent intermediate shade loses coverage"
        );
    }
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
