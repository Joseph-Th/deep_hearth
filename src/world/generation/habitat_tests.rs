//! Landscape-level ecology and actual resource ownership, not decorative biome tags.

use super::*;
use crate::content::{BLOCK_STATE_STONE, MATERIAL_BERRIES, MATERIAL_WILD_NUTS, build_registries};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::surface::SurfaceResourceKind;
use crate::world::{ChunkCoord, validate_generate_chunk, validate_generate_player_neighborhood};

fn class_index(kind: HabitatKind) -> usize {
    match kind {
        HabitatKind::DampGrove => 0,
        HabitatKind::MixedWoodland => 1,
        HabitatKind::DryScrub => 2,
    }
}

#[test]
fn seeded_habitat_forms_smooth_provinces_on_both_sides_of_zero() {
    let seed = WorldGenerationSeed::new(0x4841_4249_5441_5453);
    let mut classes = [false; 3];
    for x in (-384..=384).step_by(16) {
        for z in (-384..=384).step_by(16) {
            classes[class_index(habitat_at(seed, x, z))] = true;
        }
    }
    assert_eq!(
        classes, [true; 3],
        "the same world needs contrasting habitats"
    );
    // A continuous moisture field has no discontinuity at a chunk, habitat
    // lattice or signed-coordinate boundary. Terrain elevation can still
    // change the actual habitat classification on a neighboring hillside.
    for x in [-129, -65, -64, -17, -16, -1, 0, 15, 16, 63, 64, 127, 128] {
        for z in [-129, -64, -1, 0, 63, 64, 128] {
            let current = i64::from(moisture_at(seed, x, z));
            let east = i64::from(moisture_at(seed, x + 1, z));
            let south = i64::from(moisture_at(seed, x, z + 1));
            assert!((current - east).abs() <= 4);
            assert!((current - south).abs() <= 4);
        }
    }
    assert_eq!(moisture_at(seed, 64, 64), moisture_lattice(seed, 1, 1));
}

#[test]
fn seed_ground_conditions_change_real_trees_and_generated_food() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x4855_4D49_445F_464F);
    let profile = super::super::WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
    let mut found_trees = [false; 3];
    let mut found_forage = [false; 3];
    let mut initial_totals = [0_u64; 3];
    let blank = AppState::new();
    for cx in -18..=18 {
        for cz in -18..=18 {
            let chunk_coord = ChunkCoord::new(cx, cz);
            let tree = super::super::super::trees::seeded_tree_identity(profile, chunk_coord)
                .expect("stone world has one seeded tree site per chunk");
            let site = tree.root();
            let habitat = habitat_at(seed, site.x(), site.z());
            let index = class_index(habitat);
            let initial = tree.initial_wood().milligrams();
            match habitat {
                HabitatKind::DampGrove => {
                    assert!(initial == 8_000_000 || (480_000_000..720_000_000).contains(&initial));
                }
                HabitatKind::MixedWoodland => {
                    assert!(initial == 8_000_000 || (280_000_000..460_000_000).contains(&initial));
                }
                HabitatKind::DryScrub => {
                    assert!(initial == 8_000_000 || (150_000_000..260_000_000).contains(&initial));
                }
            }
            if !found_trees[index] {
                found_trees[index] = true;
                initial_totals[index] = initial;
            }
            if found_forage.iter().all(|seen| *seen) {
                continue;
            }
            // Full terrain construction is reserved for candidate source sites.
            // A generated resource must pass the real surface-owner admission
            // and become a physical parcel, not merely match an authored roll.
            let hash = mix64(
                seed.value()
                    ^ (cx.div_euclid(2) as u64).wrapping_mul(0x8EBC_6AF0_9C88_C6E3)
                    ^ (cz.div_euclid(2) as u64).wrapping_mul(0x5899_65CC_7537_4CC3)
                    ^ 0x464F_5241_4745_5F57,
            );
            if (cx.rem_euclid(2) as u64 != hash & 1) || (cz.rem_euclid(2) as u64 != (hash >> 1) & 1)
            {
                continue;
            }
            let chunk =
                super::super::build_generated_chunk(registries.world(), profile, chunk_coord)
                    .unwrap();
            let Some(spec) =
                super::super::forage::seeded_forage_spec(&registries, &blank, profile, &chunk)
            else {
                continue;
            };
            let mut resource_state = AppState::new();
            crate::surface::validate_create_surface_resources(&registries, &resource_state, [spec])
                .unwrap()
                .commit(&mut resource_state)
                .unwrap();
            let patch = resource_state.surface().resources().next().unwrap();
            let kind = habitat_at(seed, patch.position().x(), patch.position().z());
            let patch_index = class_index(kind);
            let mass = patch.remaining_mass().milligrams();
            match kind {
                HabitatKind::DampGrove => {
                    assert_eq!(patch.commodity().material(), MATERIAL_BERRIES);
                    assert!((700_000..1_200_000).contains(&mass));
                }
                HabitatKind::DryScrub => {
                    assert_eq!(patch.commodity().material(), MATERIAL_WILD_NUTS);
                    assert!((180_000..500_000).contains(&mass));
                }
                HabitatKind::MixedWoodland => {
                    assert!((250_000..850_000).contains(&mass));
                }
            }
            found_forage[patch_index] = true;
        }
        if found_trees.iter().all(|seen| *seen) && found_forage.iter().all(|seen| *seen) {
            break;
        }
    }
    assert_eq!(found_trees, [true; 3]);
    assert_eq!(
        found_forage, [true; 3],
        "habitats must produce tangible food differences"
    );
    assert!(initial_totals.iter().all(|mass| *mass > 0));
}

#[test]
fn player_discovers_habitat_with_real_generated_land_and_persistent_ecology() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x4841_4249_5441_5445);
    let mut state = AppState::new();
    assert_eq!(assess_player_habitat(&state), None);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(assess_player_habitat(&state), None);
    let spawn = VoxelCoord::new(0, terrain_height(seed, 0, 0) + 1, 0);
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let habitat = habitat_at(seed, spawn.x(), spawn.z());
    assert_eq!(assess_player_habitat(&state), Some(habitat));
    let mut underground = state.clone();
    let cave = VoxelCoord::new(spawn.x(), spawn.y() - 3, spawn.z());
    for y in cave.y()..=cave.y() + 1 {
        crate::world::set_generated_block_for_test(
            registries.world(),
            &mut underground,
            VoxelCoord::new(cave.x(), y, cave.z()),
            crate::world::BlockStateId::AIR,
        );
    }
    crate::logistics::relocate_player_for_test(&mut underground, cave);
    assert_eq!(
        assess_player_habitat(&underground),
        None,
        "the player should not identify unseen surface ecology from inside rock"
    );
    validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let food = state
        .surface()
        .resources()
        .filter(|resource| resource.kind() == SurfaceResourceKind::Opportunity)
        .collect::<Vec<_>>();
    assert!(!food.is_empty());
    assert!(
        food.iter()
            .all(|resource| resource.created_at() == SimulationTick::ZERO)
    );
    assert!(calculate_matter_accounting(&state).is_ok());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(
        assess_player_habitat(&state),
        assess_player_habitat(&loaded)
    );
    assert_eq!(loaded, state);
}

#[test]
fn woodland_and_scrub_create_different_real_daylight_not_just_different_labels() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x4855_4D49_445F_464F);
    let profile = super::super::WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
    let mut grove = None;
    let mut scrub = None;
    for cx in -12..=12 {
        for cz in -12..=12 {
            let coord = ChunkCoord::new(cx, cz);
            let tree = super::super::super::trees::seeded_tree_identity(profile, coord).unwrap();
            match habitat_at(seed, tree.root().x(), tree.root().z()) {
                HabitatKind::DampGrove if tree.initial_wood().milligrams() >= 480_000_000 => {
                    grove.get_or_insert((coord, tree));
                }
                HabitatKind::DryScrub if tree.initial_wood().milligrams() == 8_000_000 => {
                    scrub.get_or_insert((coord, tree));
                }
                _ => {}
            }
            if grove.is_some() && scrub.is_some() {
                break;
            }
        }
        if grove.is_some() && scrub.is_some() {
            break;
        }
    }
    let measure = |(coord, tree): (ChunkCoord, crate::world::TreeRecord)| {
        let mut world = AppState::new();
        validate_generate_chunk(&registries, &world, seed, coord, BLOCK_STATE_STONE)
            .unwrap()
            .commit(&mut world)
            .unwrap();
        assert!(world.world().tree_at(tree.root()).is_some());
        let at = VoxelCoord::new(tree.root().x() + 1, tree.root().y() + 2, tree.root().z());
        assert_eq!(
            world.world().block_at(at),
            Some(crate::world::BlockStateId::AIR)
        );
        crate::world::assess_generated_skylight(&registries, &world, at)
    };
    let grove_light = measure(grove.expect("seed has a mature grove"));
    let scrub_light = measure(scrub.expect("seed has juvenile scrub"));
    assert!(
        grove_light < scrub_light,
        "actual grove canopy must darken understory"
    );
    assert_eq!(scrub_light, 9, "a juvenile scrub tree has no shade canopy");
}

#[test]
fn same_weather_front_feels_cooler_in_groves_than_dry_scrub() {
    let registries = build_registries();
    let locations = (1..=120).find_map(|seed_value| {
        let seed = WorldGenerationSeed::new(seed_value);
        let mut wet = None;
        let mut dry = None;
        for x in (0..128).step_by(8) {
            for z in (0..128).step_by(8) {
                let position = VoxelCoord::new(x, 1, z);
                match habitat_at(seed, x, z) {
                    HabitatKind::DampGrove => {
                        wet.get_or_insert(position);
                    }
                    HabitatKind::DryScrub => {
                        dry.get_or_insert(position);
                    }
                    HabitatKind::MixedWoodland => {}
                }
            }
        }
        wet.zip(dry).map(|(wet, dry)| (seed, wet, dry))
    });
    let (seed, wet, dry) = locations.expect("one weather front spans distinct ground habitats");
    let weather = |position| {
        super::super::super::weather::weather_at(
            &registries,
            seed.value(),
            position,
            SimulationTick::ZERO,
        )
    };
    let grove = weather(wet);
    let scrub = weather(dry);
    assert_eq!(grove.wind(), scrub.wind());
    assert_eq!(grove.wind_direction(), scrub.wind_direction());
    assert_eq!(
        grove.ambient_temperature().millikelvin() + 4_000,
        scrub.ambient_temperature().millikelvin(),
        "ground exposure must modify local survival temperature within one front"
    );
    assert!(grove.cooling_multiplier_ppm() > scrub.cooling_multiplier_ppm());
}
