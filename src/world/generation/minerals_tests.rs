//! Seeded district distribution for exposed finite geological opportunities.

use super::*;

#[test]
fn seeded_districts_contain_zero_or_one_contiguous_outcrop_on_signed_ground() {
    let states = std::array::from_fn(|index| BlockStateId::new((index + 1) as u16));
    let mut present = 0;
    let mut absent = 0;
    for seed in [
        WorldGenerationSeed::new(1),
        WorldGenerationSeed::new(0xB0A1_DA12),
    ] {
        for cell_x in [-5, -3, -1, 0, 2, 4] {
            for cell_z in [-5, -3, -1, 0, 2, 4] {
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
                if sites.is_empty() {
                    absent += 1;
                    continue;
                }
                present += 1;
                assert_eq!(sites.len(), 4, "one 2x2 outcrop per productive district");
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
    assert!(
        present >= 10 && absent >= 10,
        "geology must permit productive and barren districts"
    );
}

#[test]
fn geological_provinces_create_distinct_ore_frequencies_and_species() {
    let seed = WorldGenerationSeed::new(0x4745_4F4C_4F47_5953);
    let states = std::array::from_fn(|index| BlockStateId::new((index + 1) as u16));
    let mut total = [0_u32; 4];
    let mut outcrops = [0_u32; 4];
    let mut species = std::array::from_fn::<_, 4, _>(|_| std::collections::BTreeSet::new());
    for region_x in -8..8_i64 {
        for region_z in -8..8_i64 {
            let province =
                geological_province(seed, region_x * PROVINCE_SIDE, region_z * PROVINCE_SIDE);
            let kind = match province {
                GeologicalProvince::IronBelt => 0,
                GeologicalProvince::TinUplands => 1,
                GeologicalProvince::SulfideBasin => 2,
                GeologicalProvince::PoorBasement => 3,
            };
            for district_x in 0..4_i64 {
                for district_z in 0..4_i64 {
                    total[kind] += 1;
                    let coord = ChunkCoord::new(
                        region_x * 8 + district_x * 2,
                        region_z * 8 + district_z * 2,
                    );
                    let candidate = (0..2)
                        .flat_map(|x| {
                            (0..2).map(move |z| ChunkCoord::new(coord.x() + x, coord.z() + z))
                        })
                        .find_map(|chunk| seeded_surface_lode_for_chunk(seed, chunk, states));
                    if let Some(site) = candidate {
                        outcrops[kind] += 1;
                        species[kind].insert(site.ore);
                    }
                }
            }
        }
    }
    for count in total {
        assert!(
            count >= 500,
            "each geological province needs ample sampled regions"
        );
    }
    // These statistical bounds are deliberately wide; they establish player-
    // meaningful clustering, not a fragile exact histogram.
    assert!(outcrops[0] * total[1] > outcrops[1] * total[0]);
    assert!(outcrops[1] * total[2] > outcrops[2] * total[1]);
    assert!(outcrops[2] * total[3] > 4 * outcrops[3] * total[2]);
    assert!(species[0].contains(&states[0]) && species[0].contains(&states[3]));
    assert!(species[1].contains(&states[1]));
    assert!(species[2].contains(&states[4]) && species[2].contains(&states[5]));
    assert!(
        outcrops[3] > 0,
        "poor basement can contain rare discoveries"
    );
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
                    // Independent district point geometry remains the oracle
                    // for chunk lookup; geology identity is shared across the
                    // whole 128 m province.
                    let hash = mix64(
                        seed.value()
                            ^ (world_x.div_euclid(32) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                            ^ (world_z.div_euclid(32) as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
                            ^ 0x5355_5246_4143_454F,
                    );
                    let start_x = 16 * (hash & 1) as i64 + 3 + ((hash >> 8) % 10) as i64;
                    let start_z = 16 * ((hash >> 1) & 1) as i64 + 3 + ((hash >> 20) % 10) as i64;
                    let province = geological_province(seed, world_x, world_z);
                    let point = (outcrop_present(province, hash)
                        && (start_x..start_x + 2).contains(&world_x.rem_euclid(32))
                        && (start_z..start_z + 2).contains(&world_z.rem_euclid(32)))
                    .then(|| states[mineral_index(province, ((hash >> 36) % 8) as usize, true)]);
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
fn rich_and_poor_geological_provinces_materialize_different_persistent_rock() {
    use crate::content::{BLOCK_STATE_STONE, FORM_ORE, build_registries};
    use crate::core::state::{AppState, validate_loaded_state};
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5155_4152_5259_4745);
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
    let region_with = |wanted| {
        (-6..=6)
            .find_map(|x| {
                (-6..=6)
                    .find(|&z| {
                        geological_province(seed, x * PROVINCE_SIDE, z * PROVINCE_SIDE) == wanted
                    })
                    .map(|z| (x, z))
            })
            .expect("seeded geology should contain all province types")
    };
    let (iron_x, iron_z) = region_with(GeologicalProvince::IronBelt);
    let (poor_x, poor_z) = region_with(GeologicalProvince::PoorBasement);

    let ore_site = (0..4_i64)
        .find_map(|dx| {
            (0..4_i64).find_map(|dz| {
                let base = ChunkCoord::new(iron_x * 8 + dx * 2, iron_z * 8 + dz * 2);
                (0..2_i64).find_map(|x| {
                    (0..2_i64).find_map(|z| {
                        let chunk = ChunkCoord::new(base.x() + x, base.z() + z);
                        seeded_surface_lode_for_chunk(seed, chunk, states).map(|lode| (chunk, lode))
                    })
                })
            })
        })
        .expect("iron belt should expose a shallow lode");
    let (rich_chunk, rich_lode) = ore_site;
    let mut rich = AppState::new();
    super::super::validate_generate_chunk(&registries, &rich, seed, rich_chunk, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut rich)
        .unwrap();
    for x in rich_lode.local_x..rich_lode.local_x + 2 {
        for z in rich_lode.local_z..rich_lode.local_z + 2 {
            let world_x = rich_chunk.world_min_x().unwrap() + x as i64;
            let world_z = rich_chunk.world_min_z().unwrap() + z as i64;
            let top = super::super::terrain::terrain_height(seed, world_x, world_z);
            for y in top - 3..=top {
                let cell = VoxelCoord::new(world_x, y, world_z);
                let expected = select_mineral_grade(registries.world(), seed, cell, rich_lode.ore);
                assert_eq!(
                    rich.world().block_at(cell),
                    Some(expected),
                    "iron province must contain real 4-deep blocks of a contiguous outcrop",
                );
            }
        }
    }

    let mut poor = AppState::new();
    let mut barren_district = None;
    'search: for dx in 0..4_i64 {
        for dz in 0..4_i64 {
            let base = ChunkCoord::new(poor_x * 8 + dx * 2, poor_z * 8 + dz * 2);
            if (0..2_i64).all(|x| {
                (0..2_i64).all(|z| {
                    seeded_surface_lode_for_chunk(
                        seed,
                        ChunkCoord::new(base.x() + x, base.z() + z),
                        states,
                    )
                    .is_none()
                })
            }) {
                barren_district = Some(base);
                break 'search;
            }
        }
    }
    let base = barren_district.expect("poor basement must include a district without an outcrop");
    for dx in 0..2_i64 {
        for dz in 0..2_i64 {
            let chunk = ChunkCoord::new(base.x() + dx, base.z() + dz);
            super::super::validate_generate_chunk(
                &registries,
                &poor,
                seed,
                chunk,
                BLOCK_STATE_STONE,
            )
            .unwrap()
            .commit(&mut poor)
            .unwrap();
            let min_x = chunk.world_min_x().unwrap();
            let min_z = chunk.world_min_z().unwrap();
            for x in min_x..min_x + super::super::CHUNK_SIDE as i64 {
                for z in min_z..min_z + super::super::CHUNK_SIDE as i64 {
                    let surface = super::super::terrain::terrain_height(seed, x, z);
                    let block = poor
                        .world()
                        .block_at(VoxelCoord::new(x, surface, z))
                        .unwrap();
                    assert_ne!(
                        registries
                            .world()
                            .block_for_state(block)
                            .and_then(|definition| definition.commodity())
                            .map(|commodity| commodity.form()),
                        Some(FORM_ORE),
                        "a seeded barren district must not secretly spawn shallow ore",
                    );
                }
            }
        }
    }
    for state in [rich, poor] {
        assert!(calculate_matter_accounting(&state).is_ok());
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        assert_eq!(restored, state);
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
