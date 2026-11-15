//! Species identity survives a transplant; different crowns shape real light.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::{AppState, validate_loaded_state};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::world::{ChunkCoord, WorldGenerationSeed, validate_generate_chunk};
use std::collections::BTreeSet;

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
                _ => {}
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
fn nearby_tree_species_are_visible_only_through_a_real_open_trunk_face() {
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
        if state.world().block_at(face) != Some(BlockStateId::AIR)
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
        available_visible_generated_trees(&registries, &state).all(|seen| seen.root() != root),
        "an opaque face on the only local sightline must conceal the tree",
    );
}
