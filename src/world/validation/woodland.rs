//! Seed-derived tree identity, transplant provenance, and exact biomass receipts.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{BlockStateId, ChunkCoord, WorldRegistry, WorldState};
use super::WorldValidationError;

pub(super) fn validate_loaded_woodland(
    registry: &WorldRegistry,
    state: &WorldState,
) -> Result<(), WorldValidationError> {
    let transplant_paths = validate_transplant_paths(state)?;
    let mut previous_tree = None;
    let mut occupied_seed_origins = BTreeSet::new();
    for tree in state.trees() {
        let root = tree.root();
        let origin = tree.origin_root();
        let valid = state
            .generation_profile()
            .and_then(|profile| {
                super::super::trees::seeded_tree_identity(profile, ChunkCoord::containing(origin))
            })
            .map(|original| original.transplanted_to(root))
            == Some(*tree)
            && transplant_paths
                .get(&origin)
                .is_none_or(|(last, _)| *last == root)
            && (transplant_paths.contains_key(&origin) || root == origin)
            && state.get_chunk(ChunkCoord::containing(origin)).is_some()
            && occupied_seed_origins.insert(origin)
            && previous_tree.is_none_or(|previous| previous < root)
            && tree.initial_wood().milligrams() > 0
            && tree.initial_wood().milligrams() <= 1_600_000_000
            && root.y().checked_sub(1).is_some_and(|floor| {
                state
                    .block_at(crate::spatial::VoxelCoord::new(root.x(), floor, root.z()))
                    .and_then(|block| registry.block_for_state(block))
                    .is_some_and(|block| {
                        block.collision() == super::super::BlockCollisionShape::FullCube
                    })
            })
            && state.block_at(root) == Some(BlockStateId::AIR)
            && root.y().checked_add(1).is_some_and(|head| {
                state.block_at(crate::spatial::VoxelCoord::new(root.x(), head, root.z()))
                    == Some(BlockStateId::AIR)
            });
        if !valid {
            return Err(WorldValidationError::InvalidStandingTree { root });
        }
        previous_tree = Some(root);
    }
    // The cumulative biomass-input ledger is a cache of exactly reproducible felled-tree
    // records. Decode cannot forge atmospheric mass by tweaking an unaudited total.
    let mut previous_felled = None;
    let mut verified_input = 0u128;
    for record in state.felled_trees() {
        let root = record.root();
        let origin = record.origin_root();
        let Some(original) = state.generation_profile().and_then(|profile| {
            super::super::trees::seeded_tree_identity(profile, ChunkCoord::containing(origin))
        }) else {
            return Err(WorldValidationError::InvalidFelledTree { root });
        };
        if original.root() != origin
            || transplant_paths
                .get(&origin)
                .is_some_and(|(last, at)| *last != root || *at > record.felled_at())
            || (!transplant_paths.contains_key(&origin) && root != origin)
            || !occupied_seed_origins.insert(origin)
            || state.tree_at(root).is_some()
            || state.get_chunk(ChunkCoord::containing(root)).is_none()
            || previous_felled.is_some_and(|previous| previous >= root)
        {
            return Err(WorldValidationError::InvalidFelledTree { root });
        }
        let grown = original
            .wood_at(record.felled_at())
            .milligrams()
            .checked_sub(original.initial_wood().milligrams())
            .ok_or(WorldValidationError::InvalidFelledTree { root })?;
        verified_input = verified_input
            .checked_add(u128::from(grown))
            .ok_or(WorldValidationError::InvalidPhotosyntheticAccounting)?;
        previous_felled = Some(root);
    }
    for origin in transplant_paths.keys() {
        if !occupied_seed_origins.contains(origin) {
            return Err(WorldValidationError::InvalidTreeTransplant { origin: *origin });
        }
    }
    if verified_input != state.committed_assimilated_wood_milligrams() {
        return Err(WorldValidationError::InvalidPhotosyntheticAccounting);
    }
    Ok(())
}

/// Replay immutable transplants from each seed-derived root. Current coordinates alone
/// cannot authorize a decoded move or felling receipt.
fn validate_transplant_paths(
    state: &WorldState,
) -> Result<
    BTreeMap<
        crate::spatial::VoxelCoord,
        (
            crate::spatial::VoxelCoord,
            crate::core::time::SimulationTick,
        ),
    >,
    WorldValidationError,
> {
    let mut paths = BTreeMap::new();
    let mut previous_tick = None;
    for record in state.transplants() {
        let origin = record.origin();
        let from = record.from();
        let to = record.to();
        let at = record.transplanted_at();
        let original = state.generation_profile().and_then(|profile| {
            super::super::trees::seeded_tree_identity(profile, ChunkCoord::containing(origin))
        });
        let valid = original.is_some_and(|tree| {
            tree.root() == origin && tree.wood_at(at).milligrams() < 10_000_000
        }) && previous_tick.is_none_or(|previous| previous <= at)
            && from.y() == to.y()
            && from
                .x()
                .abs_diff(to.x())
                .checked_add(from.z().abs_diff(to.z()))
                == Some(2)
            && state.get_chunk(ChunkCoord::containing(origin)).is_some()
            && state.get_chunk(ChunkCoord::containing(from)).is_some()
            && state.get_chunk(ChunkCoord::containing(to)).is_some()
            && paths.get(&origin).is_none_or(|(last, _)| *last == from)
            && (paths.contains_key(&origin) || from == origin);
        if !valid {
            return Err(WorldValidationError::InvalidTreeTransplant { origin });
        }
        paths.insert(origin, (to, at));
        previous_tick = Some(at);
    }
    Ok(paths)
}
