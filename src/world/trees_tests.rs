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
    // Select a world whose neighboring terrain does not bury the player.
    let (seed, root) = (1..100)
        .find_map(|value| {
            let seed = WorldGenerationSeed::new(value);
            let profile = WorldGenerationProfile::new(seed, BLOCK_STATE_STONE);
            let (x, z) = trees::seeded_tree_column(profile, ChunkCoord::new(0, 0))?;
            let root_y = generation::terrain_height(seed, x, z).checked_add(1)?;
            let near_x = x.checked_sub(1)?;
            (generation::terrain_height(seed, near_x, z) < root_y)
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
fn woodland_growth_is_lazy_deterministic_bounded_and_not_control_rate() {
    let root = VoxelCoord::new(12, 1, 12);
    let tree = TreeRecord::new(root, Mass::from_milligrams(300_000));
    assert_eq!(tree.wood_at(SimulationTick::ZERO), tree.initial_wood());
    assert_eq!(
        tree.wood_at(SimulationTick::new(BIOLOGICAL_YEAR_TICKS)),
        Mass::from_milligrams(20_300_000)
    );
    assert_eq!(
        tree.wood_at(SimulationTick::new(BIOLOGICAL_YEAR_TICKS / 2)),
        Mass::from_milligrams(10_300_000)
    );
    assert_eq!(
        tree.wood_at(SimulationTick::new(u64::MAX)),
        Mass::from_milligrams(1_600_000_000)
    );
    assert_eq!(tree.canopy_radius_voxels(SimulationTick::ZERO), 0);
    assert_eq!(
        tree.canopy_radius_voxels(SimulationTick::new(BIOLOGICAL_YEAR_TICKS)),
        1
    );
    assert!(tree.occupies_trunk(root));
    assert!(tree.occupies_trunk(VoxelCoord::new(12, 2, 12)));
    assert!(!tree.occupies_trunk(VoxelCoord::new(13, 1, 12)));
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
fn trusted_load_rejects_forged_photosynthetic_input() {
    let (registries, state, _) = standing_tree_fixture();
    let mut value = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    value["state"]["systems"]["world"]["assimilated_wood_milligrams"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(value).unwrap();
    assert!(matches!(
        decoded.into_state(&registries),
        Err(crate::persistence::LoadError::InvalidState(
            StateValidationError::World(WorldValidationError::InvalidPhotosyntheticAccounting)
        ))
    ));
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
