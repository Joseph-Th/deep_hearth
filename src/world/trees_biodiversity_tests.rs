//! Species identity survives a transplant; different crowns shape real light.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::{AppState, validate_loaded_state};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::world::{ChunkCoord, WorldGenerationSeed, validate_generate_chunk};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn each_seeded_habitat_supports_two_mature_tree_species_and_pioneers() {
    let seed = WorldGenerationSeed::new(0x504C_414E_5453_3236);
    let profile = WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
    let mut kinds = std::array::from_fn::<_, 3, _>(|_| BTreeSet::new());
    for cx in -24..=24 {
        for cz in -24..=24 {
            let tree = seeded_tree_identity(profile, ChunkCoord::new(cx, cz)).unwrap();
            let habitat =
                super::super::generation::habitat_at(seed, tree.root().x(), tree.root().z());
            let species = tree.species();
            let index = match habitat {
                super::super::HabitatKind::DampGrove => 0,
                super::super::HabitatKind::MixedWoodland => 1,
                super::super::HabitatKind::DryScrub => 2,
            };
            kinds[index].insert(species);
            if species != TreeSpecies::Pioneer {
                let fits = match index {
                    0 => matches!(species, TreeSpecies::Alder | TreeSpecies::Willow),
                    1 => matches!(species, TreeSpecies::Oak | TreeSpecies::Birch),
                    _ => matches!(species, TreeSpecies::Pinyon | TreeSpecies::Juniper),
                };
                assert!(fits, "{habitat:?} has wrong mature {species:?}");
            }
            let moved = tree.transplanted_to(VoxelCoord::new(
                tree.root().x() + 2,
                tree.root().y(),
                tree.root().z(),
            ));
            assert_eq!(moved.species(), species);
        }
    }
    assert_eq!(
        kinds[0],
        BTreeSet::from([
            TreeSpecies::Alder,
            TreeSpecies::Willow,
            TreeSpecies::Pioneer,
        ])
    );
    assert_eq!(
        kinds[1],
        BTreeSet::from([TreeSpecies::Oak, TreeSpecies::Birch, TreeSpecies::Pioneer,])
    );
    assert_eq!(
        kinds[2],
        BTreeSet::from([
            TreeSpecies::Pinyon,
            TreeSpecies::Juniper,
            TreeSpecies::Pioneer,
        ])
    );
}

#[test]
fn native_species_accumulate_different_real_timber_at_exact_year_boundaries() {
    // Pick trees from real seeded terrain rather than constructing identities
    // disconnected from the generated ecosystem. Keep mature samples below
    // the next stage ceiling to measure one full year of species growth.
    let mut examples = BTreeMap::<TreeSpecies, TreeRecord>::new();
    for seed_value in 0..4 {
        let seed = WorldGenerationSeed::new(seed_value);
        let profile = WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
        for x in -24..=24 {
            for z in -24..=24 {
                let tree = seeded_tree_identity(profile, ChunkCoord::new(x, z)).unwrap();
                let suitable = tree.species() == TreeSpecies::Pioneer
                    || tree.initial_wood().milligrams() < 530_000_000;
                if suitable {
                    examples.entry(tree.species()).or_insert(tree);
                }
            }
        }
    }
    let expected_annual_gain_mg = [
        (TreeSpecies::Alder, 50_000_000),
        (TreeSpecies::Willow, 60_000_000),
        (TreeSpecies::Oak, 30_000_000),
        (TreeSpecies::Birch, 50_000_000),
        (TreeSpecies::Pinyon, 20_000_000),
        (TreeSpecies::Juniper, 16_000_000),
        (TreeSpecies::Pioneer, 11_000_000),
    ];
    for (species, gain) in expected_annual_gain_mg {
        let tree = *examples
            .get(&species)
            .unwrap_or_else(|| panic!("real generated world lacks a suitable {species:?}"));
        let grown = tree.wood_at(SimulationTick::new(BIOLOGICAL_YEAR_TICKS));
        assert_eq!(
            grown.checked_sub(tree.initial_wood()),
            Some(Mass::from_milligrams(gain)),
            "{species:?} must use its own distinct biological growth rate"
        );
        assert_eq!(tree.wood_at(SimulationTick::ZERO), tree.initial_wood());
        assert_eq!(
            tree.wood_at(SimulationTick::new(u64::MAX)),
            Mass::from_milligrams(1_600_000_000),
            "all species must respect the finite standing-wood ceiling"
        );
        let transplanted = tree.transplanted_to(VoxelCoord::new(
            tree.root().x() + 1,
            tree.root().y(),
            tree.root().z() + 1,
        ));
        assert_eq!(transplanted.species(), species);
        assert_eq!(
            transplanted.wood_at(SimulationTick::new(BIOLOGICAL_YEAR_TICKS)),
            grown
        );
    }
}

#[test]
fn mature_willow_oak_and_juniper_felling_conserves_species_specific_growth() {
    use crate::content::{FORM_LOG, MATERIAL_WOOD};
    use crate::core::state::apply_clock_advance;
    use crate::material::CommodityKey;
    use crate::matter::calculate_matter_accounting;
    use crate::player::validate_admit_player;

    let registries = build_registries();
    for species in [TreeSpecies::Willow, TreeSpecies::Oak, TreeSpecies::Juniper] {
        let (seed, tree, player_position) = (0..1_024_u64)
            .find_map(|value| {
                let seed = WorldGenerationSeed::new(value);
                let profile = WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
                let tree = seeded_tree_identity(profile, ChunkCoord::new(0, 0))?;
                if tree.species() != species {
                    return None;
                }
                let root = tree.root();
                [(-1_i64, 0_i64), (1, 0), (0, -1), (0, 1)]
                    .into_iter()
                    .find_map(|(dx, dz)| {
                        let x = root.x() + dx;
                        let z = root.z() + dz;
                        (super::super::generation::terrain_height(seed, x, z) == root.y() - 1)
                            .then_some((seed, tree, VoxelCoord::new(x, root.y(), z)))
                    })
            })
            .unwrap_or_else(|| panic!("must find a grounded, reachable native {species:?}"));
        let mut state = AppState::new();
        validate_admit_player(&registries, &state, player_position)
            .unwrap()
            .commit(&mut state)
            .unwrap();
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
        assert_eq!(state.world().tree_at(tree.root()), Some(&tree));

        let at = SimulationTick::new(BIOLOGICAL_YEAR_TICKS * 8);
        apply_clock_advance(&mut state, at);
        let grown = tree.wood_at(at);
        assert!(grown > tree.initial_wood(), "old {species:?} must grow");
        let before = calculate_matter_accounting(&state).unwrap().total();
        let felled = super::super::felling::validate_fell_tree(&registries, &state, tree.root())
            .unwrap()
            .commit(&mut state)
            .unwrap();
        assert_eq!(felled.wood(), grown);
        assert_eq!(
            state.surface().get(felled.drop()).unwrap().commodity(),
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        );
        assert_eq!(
            state.surface().get(felled.drop()).unwrap().remaining_mass(),
            grown,
        );
        assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        assert_eq!(loaded, state);
    }
}

#[test]
fn evergreen_crowns_keep_real_winter_shade_and_broadleaf_trees_lose_it() {
    let registries = build_registries();
    let calendar = registries.core().calendar();
    let season = calendar.ticks_per_day()
        * u64::from(calendar.days_per_month())
        * u64::from(calendar.months_per_year() / 4);
    let winter = SimulationTick::new(season * 3);
    let seed = WorldGenerationSeed::new(0x5348_4144_4553_3236);
    let profile = WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
    let mut evergreen = None;
    let mut deciduous = None;
    for cx in -20..=20 {
        for cz in -20..=20 {
            let chunk = ChunkCoord::new(cx, cz);
            let tree = seeded_tree_identity(profile, chunk).unwrap();
            match tree.species() {
                TreeSpecies::Pinyon | TreeSpecies::Juniper => {
                    evergreen.get_or_insert((chunk, tree));
                }
                TreeSpecies::Alder | TreeSpecies::Willow => {
                    deciduous.get_or_insert((chunk, tree));
                }
                TreeSpecies::Oak | TreeSpecies::Birch | TreeSpecies::Pioneer => {}
            }
            if evergreen.is_some() && deciduous.is_some() {
                break;
            }
        }
        if evergreen.is_some() && deciduous.is_some() {
            break;
        }
    }
    let measured = |(chunk, tree): (ChunkCoord, TreeRecord)| {
        let mut state = AppState::new();
        validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        assert_eq!(state.world().tree_at(tree.root()), Some(&tree));
        let summer = super::super::sky::living_canopy_shade_at(
            &registries,
            &state,
            tree.root(),
            SimulationTick::new(season),
        );
        let winter_shade =
            super::super::sky::living_canopy_shade_at(&registries, &state, tree.root(), winter);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        assert_eq!(loaded, state);
        (summer, winter_shade)
    };
    let (needle_summer, needle_winter) = measured(evergreen.unwrap());
    let (leaf_summer, leaf_winter) = measured(deciduous.unwrap());
    assert!(needle_summer > 0 && needle_winter > 0);
    assert!(needle_winter < needle_summer);
    assert!(leaf_summer > 0);
    assert_eq!(leaf_winter, 0);
}

#[test]
fn nearby_tree_remains_visible_when_only_lower_trunk_is_occluded() {
    use crate::player::validate_admit_player;

    let registries = build_registries();
    let mut witness = None;
    for n in 0..128 {
        let seed = WorldGenerationSeed::new(n);
        let mut state = AppState::new();
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
        let Some(tree) = state.world().trees().next().copied() else {
            continue;
        };
        let root = tree.root();
        let face = VoxelCoord::new(root.x() + 1, root.y(), root.z());
        let from = VoxelCoord::new(root.x() + 2, root.y(), root.z());
        let floor = VoxelCoord::new(from.x(), from.y() - 1, from.z());
        let head = VoxelCoord::new(from.x(), from.y() + 1, from.z());
        let upper_face = VoxelCoord::new(face.x(), face.y() + 1, face.z());
        if state.world().block_at(face) != Some(BlockStateId::AIR)
            || state.world().block_at(upper_face) != Some(BlockStateId::AIR)
            || state.world().block_at(from) != Some(BlockStateId::AIR)
            || state.world().block_at(head) != Some(BlockStateId::AIR)
            || state.world().block_at(floor) != Some(BLOCK_STATE_STONE)
        {
            continue;
        }
        let Ok(plan) = validate_admit_player(&registries, &state, from) else {
            continue;
        };
        plan.commit(&mut state).unwrap();
        witness = Some((state, root, face, tree.species()));
        break;
    }
    let (mut state, root, face, species) =
        witness.expect("sampled real terrain must contain a directly visible tree");
    let observations = available_visible_generated_trees(&registries, &state).collect::<Vec<_>>();
    assert!(
        observations
            .iter()
            .any(|seen| seen.root() == root && seen.species() == species),
        "the visible living tree should be described by its species",
    );
    super::super::set_generated_block_for_test(
        registries.world(),
        &mut state,
        face,
        BLOCK_STATE_STONE,
    );
    assert!(
        available_visible_generated_trees(&registries, &state).any(|seen| seen.root() == root),
        "the exposed upper trunk remains visible above a one-block obstruction",
    );
    super::super::set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(face.x(), face.y() + 1, face.z()),
        BLOCK_STATE_STONE,
    );
    assert!(
        available_visible_generated_trees(&registries, &state).all(|seen| seen.root() != root),
        "a wall covering both trunk cells conceals the tree",
    );
}

#[test]
fn distant_tree_trunks_are_real_daylight_landmarks_not_remote_resource_oracles() {
    use crate::player::validate_admit_player;

    let registries = build_registries();
    let mut witness = None;
    'seeds: for value in 0..192 {
        let seed = WorldGenerationSeed::new(value);
        let mut state = AppState::new();
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
        let Some(root) = state.world().trees().next().map(|tree| tree.root()) else {
            continue;
        };
        for (dx, dz) in [(-5, 0), (5, 0), (0, -5), (0, 5)] {
            let at = VoxelCoord::new(root.x() + dx, root.y(), root.z() + dz);
            if ChunkCoord::containing(at) != ChunkCoord::new(0, 0) {
                continue;
            }
            let Ok(plan) = validate_admit_player(&registries, &state, at) else {
                continue;
            };
            let mut admitted = state.clone();
            plan.commit(&mut admitted).unwrap();
            if available_visible_generated_trees(&registries, &admitted)
                .any(|tree| tree.root() == root)
            {
                witness = Some((admitted, root, at));
                break 'seeds;
            }
        }
    }
    let (mut state, root, at) =
        witness.expect("an unobstructed living trunk must be visible beyond two cells");
    assert_eq!(at.x().abs_diff(root.x()) + at.z().abs_diff(root.z()), 5);

    let mut night = state.clone();
    crate::core::state::apply_clock_advance(
        &mut night,
        crate::core::time::SimulationTick::new(18_000),
    );
    assert!(
        available_visible_generated_trees(&registries, &night).all(|tree| tree.root() != root),
        "a distant unlit trunk must not remain visible at night"
    );

    let (step_x, step_z) = ((root.x() - at.x()).signum(), (root.z() - at.z()).signum());
    let wall = VoxelCoord::new(at.x() + step_x * 2, root.y(), at.z() + step_z * 2);
    for y in [wall.y(), wall.y() + 1, wall.y() + 2] {
        super::super::set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(wall.x(), y, wall.z()),
            BLOCK_STATE_STONE,
        );
    }
    assert!(
        available_visible_generated_trees(&registries, &state).all(|tree| tree.root() != root),
        "a real opaque wall must block the long tree sightline"
    );
}
