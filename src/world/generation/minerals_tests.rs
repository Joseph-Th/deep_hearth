//! Seeded district distribution for exposed finite geological opportunities.

use super::*;

#[test]
fn each_seeded_district_has_one_contiguous_ore_outcrop_even_at_negative_coordinates() {
    let states = [
        BlockStateId::new(1),
        BlockStateId::new(2),
        BlockStateId::new(3),
        BlockStateId::new(4),
        BlockStateId::new(5),
        BlockStateId::new(6),
        BlockStateId::new(7),
        BlockStateId::new(8),
    ];
    for seed in [
        WorldGenerationSeed::new(1),
        WorldGenerationSeed::new(0xB0A1_DA12),
    ] {
        for cell_x in [-3, -1, 0, 2] {
            for cell_z in [-3, -1, 0, 2] {
                let mut sites = Vec::new();
                let mut selected = None;
                for x in cell_x * 32..(cell_x + 1) * 32 {
                    for z in cell_z * 32..(cell_z + 1) * 32 {
                        if let Some(ore) = seeded_surface_lode_state(seed, x, z, states) {
                            assert_eq!(*selected.get_or_insert(ore), ore);
                            sites.push((x, z));
                        }
                    }
                }
                assert_eq!(
                    sites.len(),
                    4,
                    "district must offer one finite shallow lode"
                );
                let min_x = sites.iter().map(|site| site.0).min().unwrap();
                let min_z = sites.iter().map(|site| site.1).min().unwrap();
                for x in min_x..min_x + 2 {
                    for z in min_z..min_z + 2 {
                        assert!(sites.contains(&(x, z)), "outcrop must be contiguous");
                    }
                }
            }
        }
    }
}

#[test]
fn chunk_shallow_outcrop_matches_independent_district_point_formula() {
    let states = [
        BlockStateId::new(1),
        BlockStateId::new(2),
        BlockStateId::new(3),
        BlockStateId::new(4),
        BlockStateId::new(5),
        BlockStateId::new(6),
        BlockStateId::new(7),
        BlockStateId::new(8),
    ];
    for seed in [
        WorldGenerationSeed::new(0),
        WorldGenerationSeed::new(0x5A17_D1F0),
        WorldGenerationSeed::new(u64::MAX),
    ] {
        for (cx, cz) in [
            (0, 0),
            (1, 1),
            (2, -1),
            (-3, -2),
            (-101, 100),
            (i64::MIN / 16, i64::MAX / 16),
        ] {
            let chunk = ChunkCoord::new(cx, cz);
            let (min_x, min_z) = chunk.bounded_world_origin().unwrap();
            let projected = seeded_surface_lode_for_chunk(seed, chunk, states);
            for z in 0..super::super::CHUNK_SIDE {
                for x in 0..super::super::CHUNK_SIDE {
                    let world_x = min_x + x as i64;
                    let world_z = min_z + z as i64;
                    // Independent original district formula is intentionally
                    // retained here as an oracle, not runtime generation code.
                    let hash = mix64(
                        seed.value()
                            ^ (world_x.div_euclid(32) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                            ^ (world_z.div_euclid(32) as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
                            ^ 0x5355_5246_4143_454F,
                    );
                    let start_x = 16 * (hash & 1) as i64 + 3 + ((hash >> 8) % 10) as i64;
                    let start_z = 16 * ((hash >> 1) & 1) as i64 + 3 + ((hash >> 20) % 10) as i64;
                    let point = ((start_x..start_x + 2).contains(&world_x.rem_euclid(32))
                        && (start_z..start_z + 2).contains(&world_z.rem_euclid(32)))
                    .then(|| states[((hash >> 36) % states.len() as u64) as usize]);
                    assert_eq!(
                        projected.and_then(|site| site.at(x, z)),
                        point,
                        "seed={seed:?}, position=({world_x},{world_z})"
                    );
                    assert_eq!(
                        seeded_surface_lode_state(seed, world_x, world_z, states),
                        point
                    );
                }
            }
        }
    }
}

#[test]
fn chunk_lens_cache_matches_independent_seeded_ore_and_grade_at_every_depth() {
    let registries = crate::content::build_registries();
    let registry = registries.world();
    let states = [
        crate::content::BLOCK_STATE_HEMATITE_ORE,
        crate::content::BLOCK_STATE_CASSITERITE_ORE,
        crate::content::BLOCK_STATE_SPHALERITE_ORE,
        crate::content::BLOCK_STATE_MAGNETITE_ORE,
        crate::content::BLOCK_STATE_GALENA_ORE,
        crate::content::BLOCK_STATE_PYRITE_ORE,
        crate::content::BLOCK_STATE_GRAPHITE_ORE,
        crate::content::BLOCK_STATE_QUARTZ_ORE,
    ];
    for seed in [
        WorldGenerationSeed::new(0),
        WorldGenerationSeed::new(u64::MAX),
        WorldGenerationSeed::new(0x2E00_6A17_2026_FFFF),
    ] {
        for (chunk_x, chunk_z) in [
            (0, 0),
            (-1, -1),
            (10, -100),
            (
                i64::MIN / super::super::CHUNK_SIDE as i64,
                i64::MAX / super::super::CHUNK_SIDE as i64,
            ),
        ] {
            let coord = super::super::ChunkCoord::new(chunk_x, chunk_z);
            let (min_x, min_z) = coord.bounded_world_origin().unwrap();
            let cached = ChunkMineralLenses::new(registry, seed, min_x, min_z, states);
            for z in 0..super::super::CHUNK_SIDE {
                for x in 0..super::super::CHUNK_SIDE {
                    for y in (MINERAL_MIN_Y - 1)..=(MINERAL_MAX_Y + 1) {
                        let position = VoxelCoord::new(min_x + x as i64, y, min_z + z as i64);
                        let expected = seeded_mineral_state(seed, position, states)
                            .map(|ore| select_mineral_grade(registry, seed, position, ore));
                        assert_eq!(
                            cached.at(x, y, z),
                            expected,
                            "seed={seed:?}, position={position:?}"
                        );
                    }
                }
            }
        }
    }
}
