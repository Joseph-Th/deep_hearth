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
