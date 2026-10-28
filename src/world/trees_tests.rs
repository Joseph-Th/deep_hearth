//! Woodland generation, calendar growth, atomic felling, and trusted replay.

use super::*;
use crate::content::{BLOCK_STATE_STONE, FORM_LOG, MATERIAL_WOOD, build_registries};
use crate::core::quantity::Mass;
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;

fn standing_tree_fixture() -> (crate::registry::Registries, AppState, VoxelCoord) {
    let registries = build_registries();
    // Select level neighboring ground so a control step tests lateral sub-voxel motion,
    // not the player falling from a pre-generation spawn suspended above terrain.
    let (seed, root) = (1..100)
        .find_map(|value| {
            let seed = WorldGenerationSeed::new(value);
            let profile = WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
            let (x, z) = trees::seeded_tree_column(profile, ChunkCoord::new(0, 0))?;
            let root_y = generation::terrain_height(seed, x, z).checked_add(1)?;
            let near_x = x.checked_sub(1)?;
            (generation::terrain_height(seed, near_x, z) == root_y - 1)
                .then_some((seed, VoxelCoord::new(x, root_y, z)))
        })
        .expect("test woodland must admit a neighbor without carving terrain");
    let mut state = AppState::new();
    validate_admit_player(
        &registries,
        &state,
        VoxelCoord::new(root.x() - 1, root.y(), root.z()),
    )
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
    assert!(state.world().tree_at(root).is_some());
    (registries, state, root)
}

#[test]
fn multi_chunk_woodland_is_seed_deterministic_independent_of_generation_order() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(41);
    let coordinates = [
        ChunkCoord::new(0, 0),
        ChunkCoord::new(-1, 1),
        ChunkCoord::new(1, 0),
        ChunkCoord::new(0, -1),
        ChunkCoord::new(-2, 0),
    ];
    let mut forward = AppState::new();
    let mut reverse = AppState::new();
    for coord in coordinates {
        validate_generate_chunk(&registries, &forward, seed, coord, BLOCK_STATE_STONE)
            .unwrap()
            .commit(&mut forward)
            .unwrap();
    }
    for coord in coordinates.into_iter().rev() {
        validate_generate_chunk(&registries, &reverse, seed, coord, BLOCK_STATE_STONE)
            .unwrap()
            .commit(&mut reverse)
            .unwrap();
    }
    assert_eq!(forward.world(), reverse.world());
    let forward_json = serde_json::to_value(SaveEnvelope::new(&registries, &forward)).unwrap();
    let reverse_json = serde_json::to_value(SaveEnvelope::new(&registries, &reverse)).unwrap();
    let forward_world = &forward_json["state"]["systems"]["world"];
    assert_eq!(forward_world, &reverse_json["state"]["systems"]["world"]);
    assert!(
        forward_world["trees"].is_array(),
        "save uses arrays, not JSON map keys"
    );
    assert!(forward_world["felled_trees"].is_array());
    assert!(forward.world().trees().count() >= 2);
    for tree in forward.world().trees() {
        assert_eq!(forward.world().tree_at(tree.root()), Some(tree));
        assert!(forward.world().tree_trunk_at(tree.root()).is_some());
    }
    assert_eq!(validate_loaded_state(&registries, &forward), Ok(()));
}

#[test]
fn generated_full_cell_collision_distinguishes_trunks_rock_air_and_unknown_space() {
    let (registries, mut state, root) = standing_tree_fixture();
    let registry = registries.world();
    let floor = VoxelCoord::new(root.x(), root.y() - 1, root.z());
    let upper = VoxelCoord::new(root.x(), root.y() + 1, root.z());
    let clear = VoxelCoord::new(root.x(), root.y() + 2, root.z());
    let unknown = VoxelCoord::new(100, root.y(), 100);
    let check = |state: &AppState, tree_standing| {
        assert_eq!(
            state.world().full_cell_collision_at(registry, root),
            Some(tree_standing)
        );
        assert_eq!(
            state.world().full_cell_collision_at(registry, upper),
            Some(tree_standing)
        );
        assert_eq!(
            state.world().full_cell_collision_at(registry, floor),
            Some(true)
        );
        assert_eq!(
            state.world().full_cell_collision_at(registry, clear),
            Some(false)
        );
        assert_eq!(
            state.world().full_cell_collision_at(registry, unknown),
            None
        );
        assert_eq!(
            state.world().full_cell_collision_at(
                registry,
                VoxelCoord::new(root.x(), crate::world::WORLD_MAX_Y_EXCLUSIVE, root.z())
            ),
            None
        );
    };
    assert_eq!(state.world().block_at(root), Some(BlockStateId::AIR));
    check(&state, true);
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    check(&loaded, true);

    let _ = validate_fell_tree(&registries, &state, root)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    check(&state, false);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn trunk_bounds_queries_find_exact_cells_and_stop_after_felling() {
    let (registries, mut state, root) = standing_tree_fixture();
    let lower = crate::spatial::VoxelBounds::new(
        root,
        VoxelCoord::new(root.x() + 1, root.y() + 1, root.z() + 1),
    )
    .unwrap();
    let upper = crate::spatial::VoxelBounds::new(
        VoxelCoord::new(root.x(), root.y() + 1, root.z()),
        VoxelCoord::new(root.x() + 1, root.y() + 2, root.z() + 1),
    )
    .unwrap();
    let adjacent = crate::spatial::VoxelBounds::new(
        VoxelCoord::new(root.x() + 1, root.y(), root.z()),
        VoxelCoord::new(root.x() + 2, root.y() + 2, root.z() + 1),
    )
    .unwrap();
    assert_eq!(state.world().first_tree_trunk_in_bounds(lower), Some(root));
    assert_eq!(
        state.world().first_tree_trunk_in_bounds(upper),
        Some(VoxelCoord::new(root.x(), root.y() + 1, root.z()))
    );
    assert_eq!(state.world().first_tree_trunk_in_bounds(adjacent), None);
    let felling = validate_fell_tree(&registries, &state, root).unwrap();
    let _ = felling.commit(&mut state).unwrap();
    assert_eq!(state.world().first_tree_trunk_in_bounds(lower), None);
    assert_eq!(state.world().first_tree_trunk_in_bounds(upper), None);
}

#[test]
fn woodland_growth_is_lazy_deterministic_bounded_and_not_control_rate() {
    let root = VoxelCoord::new(12, 1, 12);
    let tree = TreeRecord::new(root, Mass::from_milligrams(300_000));
    assert_eq!(tree.wood_at(SimulationTick::ZERO), tree.initial_wood());
    assert_eq!(
        tree.wood_at(SimulationTick::new(BIOLOGICAL_YEAR_TICKS)),
        Mass::from_milligrams(4_300_000)
    );
    assert_eq!(
        tree.wood_at(SimulationTick::new(BIOLOGICAL_YEAR_TICKS / 2)),
        Mass::from_milligrams(2_300_000)
    );
    assert_eq!(
        tree.wood_at(SimulationTick::new(u64::MAX)),
        Mass::from_milligrams(1_600_000_000)
    );
    assert_eq!(tree.canopy_radius_voxels(SimulationTick::ZERO), 0);
    assert_eq!(
        tree.canopy_radius_voxels(SimulationTick::new(BIOLOGICAL_YEAR_TICKS)),
        0
    );
    assert!(tree.occupies_trunk(root));
    assert!(tree.occupies_trunk(VoxelCoord::new(12, 2, 12)));
    assert!(!tree.occupies_trunk(VoxelCoord::new(13, 1, 12)));
}

#[test]
fn woodland_stage_growth_crosses_thresholds_continuously_and_slows_with_maturity() {
    let origin = VoxelCoord::new(1, 2, 3);
    let sapling = TreeRecord::new(origin, Mass::from_milligrams(8_000_000));
    assert_eq!(
        sapling.wood_at(SimulationTick::new(BIOLOGICAL_YEAR_TICKS)),
        Mass::from_milligrams(19_000_000)
    );
    assert_eq!(
        sapling.canopy_radius_voxels(SimulationTick::new(BIOLOGICAL_YEAR_TICKS)),
        1
    );
    let pole = TreeRecord::new(origin, Mass::from_milligrams(300_000_000));
    assert_eq!(
        pole.wood_at(SimulationTick::new(BIOLOGICAL_YEAR_TICKS)),
        Mass::from_milligrams(340_000_000)
    );
    let near_mature = TreeRecord::new(origin, Mass::from_milligrams(599_999_999));
    let at = SimulationTick::new(BIOLOGICAL_YEAR_TICKS);
    assert!(near_mature.wood_at(at).milligrams() >= 621_999_980);
    assert!(near_mature.wood_at(at).milligrams() <= 622_000_010);
    for candidate in [sapling, pole, near_mature] {
        let checkpoints = [
            0,
            1,
            20,
            10_000,
            BIOLOGICAL_YEAR_TICKS / 2,
            BIOLOGICAL_YEAR_TICKS,
            40 * BIOLOGICAL_YEAR_TICKS,
            u64::MAX,
        ];
        let mut previous = 0;
        for instant in checkpoints {
            let mass = candidate.wood_at(SimulationTick::new(instant)).milligrams();
            assert!(mass >= previous, "growth must never consume standing wood");
            assert!(
                mass <= 1_600_000_000,
                "standing biomass has a biological ceiling"
            );
            previous = mass;
        }
    }
}

#[test]
fn generated_tree_is_solid_and_felling_moves_exact_wood_without_creating_matter() {
    let (registries, mut state, root) = standing_tree_fixture();
    assert!(state.world().tree_trunk_at(root).is_some());
    assert_eq!(
        state
            .available_adjacent_trees()
            .map(|tree| tree.root())
            .collect::<Vec<_>>(),
        vec![root]
    );
    let total_before = calculate_matter_accounting(&state).unwrap().total();
    let tree_mass = state.world().tree_at(root).unwrap().wood_at(state.tick());
    let validated = validate_fell_tree(&registries, &state, root).unwrap();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    let outcome = validated.commit(&mut state).unwrap();
    assert_eq!(outcome.root(), root);
    assert_eq!(outcome.wood(), tree_mass);
    assert!(state.world().tree_at(root).is_none());
    let logs = state.surface().get(outcome.drop()).unwrap();
    assert_eq!(logs.position(), root);
    assert_eq!(logs.remaining_mass(), tree_mass);
    assert_eq!(logs.commodity(), CommodityKey::new(MATERIAL_WOOD, FORM_LOG));
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        total_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(replay, state);
    assert!(matches!(
        validate_fell_tree(&registries, &state, root),
        Err(TreeFellingError::TreeUnavailable { .. })
    ));
}

#[test]
fn tree_felling_rejects_stale_world_without_losing_or_doubling_wood() {
    let (registries, mut state, root) = standing_tree_fixture();
    let first = validate_fell_tree(&registries, &state, root).unwrap();
    let second = validate_fell_tree(&registries, &state, root).unwrap();
    let _ = first.commit(&mut state).unwrap();
    let after = state.clone();
    assert!(matches!(
        second.commit(&mut state),
        Err(TreeFellingCommitError::StaleWorldRevision { .. })
    ));
    assert_eq!(after, state);
}

#[test]
fn validated_felling_cannot_commit_after_sub_voxel_player_movement() {
    use crate::logistics::{HorizontalControlInput, PlayerControlInput};
    use crate::simulation::{PlayerDirectControlInput, advance_control_tick_with_direct_input};
    use crate::spatial::VIEW_DIRECTION_PARTS_PER_MILLION;

    let (registries, mut state, root) = standing_tree_fixture();
    let origin = state.logistics().player().unwrap().body().foot_position();
    let planned = validate_fell_tree(&registries, &state, root).unwrap();
    let movement = PlayerDirectControlInput::new(PlayerControlInput::new(
        HorizontalControlInput::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0).unwrap(),
    ));
    let _ = advance_control_tick_with_direct_input(&registries, &mut state, movement).unwrap();
    let moved = state.logistics().player().unwrap().body().foot_position();
    assert_ne!(moved, origin);
    assert_eq!(moved.containing_voxel(), origin.containing_voxel());
    let after_movement = state.clone();
    assert!(matches!(
        planned.commit(&mut state),
        Err(TreeFellingCommitError::StaleLogisticsRevision { .. }
            | TreeFellingCommitError::PlayerPositionChanged)
    ));
    assert_eq!(state, after_movement);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn generated_tree_refuses_to_materialize_through_an_admitted_player() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(7);
    let profile = WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
    let (x, z) = trees::seeded_tree_column(profile, ChunkCoord::new(0, 0)).unwrap();
    let root = VoxelCoord::new(x, generation::terrain_height(seed, x, z) + 1, z);
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, root)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE
        )
        .err(),
        Some(ChunkGenerationError::TreePlacementObstructed { voxel: root })
    );
    assert!(state.world().trees().next().is_none());
}

#[test]
fn woodland_position_changes_with_seed_without_rng_order_dependence() {
    let mut positions = std::collections::BTreeSet::new();
    for seed_value in 1..=32 {
        let profile =
            WorldGenerationProfile::new(WorldGenerationSeed::new(seed_value), BLOCK_STATE_STONE);
        let position = trees::seeded_tree_column(profile, ChunkCoord::new(2, -3)).unwrap();
        positions.insert(position);
        assert_eq!(
            trees::seeded_tree_column(profile, ChunkCoord::new(2, -3)),
            Some(position)
        );
    }
    assert!(
        positions.len() > 8,
        "forest tree locations cannot form a fixed grid"
    );
}

#[test]
fn external_biomass_ledger_is_preserved_when_a_grown_tree_is_felled() {
    let (_, mut state, root) = standing_tree_fixture();
    let future = SimulationTick::new(BIOLOGICAL_YEAR_TICKS);
    let before = state.world().assimilated_wood_at(future).unwrap();
    assert!(before.milligrams() > 0);
    let revision = state.world().revision();
    state
        .world_state_mut()
        .remove_felled_tree(root, revision, revision + 1, future);
    assert!(state.world().tree_at(root).is_none());
    assert_eq!(state.world().assimilated_wood_at(future), Some(before));
}

#[test]
fn trusted_load_rejects_forged_tree_origin_and_initial_biomass() {
    let (registries, state, root) = standing_tree_fixture();
    for mutation in ["initial_wood", "root"] {
        let mut value = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
        if mutation == "initial_wood" {
            // Still nonzero, still below the biological cap; only seed provenance catches it.
            value["state"]["systems"]["world"]["trees"][0]["initial_wood"] =
                serde_json::json!(175_000_000u64);
        } else {
            value["state"]["systems"]["world"]["trees"][0]["root"]["x"] =
                serde_json::json!(root.x() + 1);
        }
        let decoded: LoadedSaveEnvelope = serde_json::from_value(value).unwrap();
        assert!(matches!(
            decoded.into_state(&registries),
            Err(crate::persistence::LoadError::InvalidState(
                StateValidationError::World(WorldValidationError::InvalidStandingTree { .. })
            ))
        ));
    }
}

#[test]
fn trusted_load_rejects_forged_felling_receipts_and_future_felling() {
    let (registries, mut state, root) = standing_tree_fixture();
    let felled = validate_fell_tree(&registries, &state, root).unwrap();
    let _ = felled.commit(&mut state).unwrap();
    assert_eq!(state.world().felled_trees().count(), 1);
    let baseline = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let original = trees::seeded_tree_identity(
        state.world().generation_profile().unwrap(),
        ChunkCoord::containing(root),
    )
    .unwrap();
    let future_growth = original.wood_at(SimulationTick::new(1)).milligrams()
        - original.initial_wood().milligrams();
    for mutation in ["root", "future", "duplicate"] {
        let mut value = baseline.clone();
        let history = &mut value["state"]["systems"]["world"]["felled_trees"];
        match mutation {
            "root" => history[0]["root"]["x"] = serde_json::json!(root.x() + 1),
            "future" => history[0]["felled_at"] = serde_json::json!(1u64),
            "duplicate" => {
                let duplicate = history[0].clone();
                history.as_array_mut().unwrap().push(duplicate);
            }
            _ => unreachable!(),
        }
        if mutation == "future" {
            // Preserve the exact seed-derived ledger, isolating the future-time rejection.
            value["state"]["systems"]["world"]["assimilated_wood_milligrams"] =
                serde_json::json!(future_growth);
        }
        let decoded = serde_json::from_value::<LoadedSaveEnvelope>(value);
        if mutation == "duplicate" {
            assert!(
                decoded.is_err(),
                "duplicate history must fail during ordered-index decoding"
            );
            continue;
        }
        let result = decoded.unwrap().into_state(&registries);
        assert!(
            matches!(
                result,
                Err(crate::persistence::LoadError::InvalidState(
                    StateValidationError::World(WorldValidationError::InvalidFelledTree { .. })
                ))
            ),
            "mutation {mutation}: {result:?}"
        );
    }
}

#[test]
fn trusted_load_rejects_forged_photosynthetic_input() {
    let (registries, state, _) = standing_tree_fixture();
    for forgery in [1u64, u64::MAX] {
        let mut value = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
        value["state"]["systems"]["world"]["assimilated_wood_milligrams"] =
            serde_json::json!(forgery);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(value).unwrap();
        assert!(matches!(
            decoded.into_state(&registries),
            Err(crate::persistence::LoadError::InvalidState(
                StateValidationError::World(WorldValidationError::InvalidPhotosyntheticAccounting)
            ))
        ));
    }
}

#[test]
fn trusted_load_rejects_malformed_tree_mass_and_unsupported_roots() {
    let (registries, state, root) = standing_tree_fixture();
    for forged_mass in [true, false] {
        let mut value = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
        if forged_mass {
            value["state"]["systems"]["world"]["trees"][0]["initial_wood"] = serde_json::json!(0);
        } else {
            value["state"]["systems"]["world"]["trees"][0]["root"]["y"] =
                serde_json::json!(root.y() + 20);
        }
        let decoded: LoadedSaveEnvelope = serde_json::from_value(value).unwrap();
        assert!(matches!(
            decoded.into_state(&registries),
            Err(crate::persistence::LoadError::InvalidState(
                StateValidationError::World(WorldValidationError::InvalidStandingTree { .. })
            ))
        ));
    }
}
