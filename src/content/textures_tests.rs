//! Tests for the sibling textures module; isolated so test-only edits do not invalidate production builds.

use std::collections::BTreeSet;

use super::*;
use crate::texture::TEXTURE_MIP_LEVEL_COUNT;

const BUILT_IN_TEXTURES: [TextureId; 13] = [
    TEXTURE_WOOD_SIDE,
    TEXTURE_WOOD_END,
    TEXTURE_CHARCOAL,
    TEXTURE_COPPER_ORE,
    TEXTURE_COPPER_HAMMERED,
    TEXTURE_SLAG,
    TEXTURE_MOLTEN_COPPER,
    TEXTURE_CRUSHED_ORE,
    TEXTURE_MACHINE_PANEL,
    TEXTURE_WORKING_METAL,
    TEXTURE_REFRACTORY,
    TEXTURE_SCREEN_MESH,
    TEXTURE_STONE,
];

#[test]
fn built_in_tiles_have_palette_detail_without_rgba_duplication() {
    let registry = build_texture_registry();
    assert_eq!(TEXTURE_SIDE, 32);
    assert_eq!(TEXTURE_MIP_LEVEL_COUNT, 6);
    assert_eq!(TEXTURE_SIDE >> (TEXTURE_MIP_LEVEL_COUNT - 1), 1);
    for texture in BUILT_IN_TEXTURES {
        let definition = match registry.get_texture(texture) {
            Some(definition) => definition,
            None => panic!("missing built-in texture {}", texture.value()),
        };
        let unique: BTreeSet<_> = definition.texels().iter().copied().collect();
        assert!(
            unique.len() >= 8,
            "texture {} lacks authored shade detail",
            texture.value()
        );
        for region_y in 0..4 {
            for region_x in 0..4 {
                let mut local_detail = BTreeSet::new();
                for y in region_y * 8..region_y * 8 + 8 {
                    for x in region_x * 8..region_x * 8 + 8 {
                        local_detail.insert(definition.texels()[y * TEXTURE_SIDE + x]);
                    }
                }
                assert!(
                    local_detail.len() >= 3,
                    "texture {} lacks local detail in region {},{}",
                    texture.value(),
                    region_x,
                    region_y
                );
            }
        }
    }
}

#[test]
fn built_in_bake_is_deterministic_compact_and_deduplicates_panel_geometry() {
    let registry = build_texture_registry();
    let first = registry.bake_texture_array();
    let second = registry.bake_texture_array();

    assert_eq!(first, second);
    assert!(first.pattern_layer_count() < 12);
    assert!(first.total_gpu_bytes() * 2 < first.expanded_rgba_texel_bytes());
    assert!(first.total_gpu_bytes() <= 16 * 1_024);
    let indexed_texels_per_layer = (0..TEXTURE_MIP_LEVEL_COUNT)
        .map(|level| {
            let side = TEXTURE_SIDE >> level;
            side * side
        })
        .sum::<usize>();
    assert_eq!(
        first.indexed_texel_bytes(),
        usize::from(first.pattern_layer_count()) * indexed_texels_per_layer
    );
    assert_eq!(first.mip_levels().len(), TEXTURE_MIP_LEVEL_COUNT);
    for (level, mip) in first.mip_levels().iter().enumerate() {
        assert_eq!(usize::from(mip.side()), TEXTURE_SIDE >> level);
    }
    for texture in BUILT_IN_TEXTURES {
        let descriptor = match first.get_descriptor(texture) {
            Some(descriptor) => descriptor,
            None => panic!("missing baked texture descriptor {}", texture.value()),
        };
        let expected_alpha_mode = if texture == TEXTURE_SCREEN_MESH {
            TextureAlphaMode::Cutout
        } else {
            TextureAlphaMode::Opaque
        };
        assert_eq!(descriptor.alpha_mode(), expected_alpha_mode);
        for (mip_level, minimum_detail) in [(1, 5), (2, 3)] {
            let mip = &first.mip_levels()[mip_level];
            let mut detail = BTreeSet::new();
            for y in 0..mip.side() {
                for x in 0..mip.side() {
                    let texel = match mip.get_texel(descriptor.layer(), x, y) {
                        Some(texel) => texel,
                        None => panic!(
                            "texture {} mip {} sample {},{} did not resolve",
                            texture.value(),
                            mip_level,
                            x,
                            y
                        ),
                    };
                    detail.insert(texel);
                }
            }
            assert!(
                detail.len() >= minimum_detail,
                "texture {} loses detail by mip {}",
                texture.value(),
                mip_level
            );
        }
    }
    assert_eq!(
        first
            .get_descriptor(TEXTURE_COPPER_HAMMERED)
            .map(|descriptor| descriptor.layer()),
        first
            .get_descriptor(TEXTURE_MACHINE_PANEL)
            .map(|descriptor| descriptor.layer())
    );
    assert_ne!(
        first.sample(TEXTURE_COPPER_HAMMERED, 0, 16, 16),
        first.sample(TEXTURE_MACHINE_PANEL, 0, 16, 16)
    );
}
