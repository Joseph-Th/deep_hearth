//! Contract tests for built-in texture content.

use std::collections::BTreeSet;

use super::*;
use crate::texture::{TEXTURE_MIP_LEVEL_COUNT, TEXTURE_SIDE};

const BUILT_IN_TEXTURES: [TextureId; 27] = [
    TEXTURE_WOOD_SIDE,
    TEXTURE_WOOD_END,
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
    TEXTURE_TEMPLE_DATA_PLATE,
    TEXTURE_TEMPLE_BEARING_HOUSING,
    TEXTURE_TEMPLE_REFRACTORY_FIXTURE,
    TEXTURE_TEMPLE_CORROSION_PANEL,
    TEXTURE_HEMATITE_ORE,
    TEXTURE_CASSITERITE_ORE,
    TEXTURE_SPHALERITE_ORE,
    TEXTURE_MOLTEN_IRON,
    TEXTURE_MOLTEN_TIN,
    TEXTURE_MOLTEN_ZINC,
    TEXTURE_MAGNETITE_ORE,
    TEXTURE_GALENA_ORE,
    TEXTURE_PYRITE_ORE,
    TEXTURE_GRAPHITE_ORE,
    TEXTURE_QUARTZ_ORE,
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

#[test]
fn screen_mesh_cutout_mips_retain_openings_and_steel_at_a_distance() {
    let baked = build_texture_registry().bake_texture_array();
    let texture = TEXTURE_SCREEN_MESH;
    let base = baked.mip_levels()[0].side();
    let opaque_at = |mip: usize| {
        let side = baked.mip_levels()[mip].side();
        let mut opaque = 0_usize;
        for y in 0..side {
            for x in 0..side {
                if baked.sample(texture, mip, x, y).unwrap().alpha() == u8::MAX {
                    opaque += 1;
                }
            }
        }
        (opaque, usize::from(side) * usize::from(side))
    };
    assert_eq!(base, 32);
    let (base_opaque, base_pixels) = opaque_at(0);
    assert!(base_opaque > 0 && base_opaque < base_pixels);
    for mip in 1..=4 {
        let (opaque, pixels) = opaque_at(mip);
        assert!(
            opaque > 0 && opaque < pixels,
            "cutout mesh mip {mip} became either fully opaque or completely invisible"
        );
        assert!(
            (opaque * base_pixels).abs_diff(base_opaque * pixels) <= base_pixels,
            "cutout mesh mip {mip} lost its approximate projected wire coverage"
        );
    }
}

#[test]
fn service_bay_material_textures_are_distinct_from_generic_blocks() {
    let atlas = build_texture_registry().bake_texture_array();
    for (artifact, ordinary) in [
        (TEXTURE_TEMPLE_DATA_PLATE, TEXTURE_COPPER_HAMMERED),
        (TEXTURE_TEMPLE_BEARING_HOUSING, TEXTURE_STONE),
        (TEXTURE_TEMPLE_REFRACTORY_FIXTURE, TEXTURE_STONE),
        (TEXTURE_TEMPLE_CORROSION_PANEL, TEXTURE_COPPER_HAMMERED),
    ] {
        assert!(
            (0..TEXTURE_SIDE).any(|y| (0..TEXTURE_SIDE).any(|x| {
                let x = u8::try_from(x).unwrap();
                let y = u8::try_from(y).unwrap();
                atlas.sample(artifact, 0, x, y) != atlas.sample(ordinary, 0, x, y)
            })),
            "caste relic texture {} cannot duplicate generic block {}",
            artifact.value(),
            ordinary.value()
        );
    }
}
