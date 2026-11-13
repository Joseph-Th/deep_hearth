//! Seed-derived tree identity, transplant provenance, and exact biomass receipts.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::time::SimulationTick;
use crate::spatial::VoxelCoord;

use super::super::{BlockStateId, ChunkCoord, WorldRegistry, WorldState};
use super::WorldValidationError;

pub(super) fn validate_loaded_woodland(
    registry: &WorldRegistry,
    state: &WorldState,
) -> Result<(), WorldValidationError> {
    let transplant_paths = validate_transplant_paths(state)?;
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
            && tree.initial_wood().milligrams() > 0
            && tree.initial_wood().milligrams() <= 1_600_000_000
            && state.has_tree_root_terrain_floor(registry, root)
            && state.block_at(root) == Some(BlockStateId::AIR)
            && root.y().checked_add(1).is_some_and(|head| {
                state.block_at(crate::spatial::VoxelCoord::new(root.x(), head, root.z()))
                    == Some(BlockStateId::AIR)
            });
        if !valid {
            return Err(WorldValidationError::InvalidStandingTree { root });
        }
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
            || state.get_chunk(ChunkCoord::containing(root)).is_none()
            || previous_felled.is_some_and(|previous| previous >= origin)
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
        previous_felled = Some(origin);
    }
    for origin in transplant_paths.keys() {
        if !occupied_seed_origins.contains(origin) {
            return Err(WorldValidationError::InvalidTreeTransplant { origin: *origin });
        }
    }
    if verified_input != state.committed_assimilated_wood_milligrams() {
        return Err(WorldValidationError::InvalidPhotosyntheticAccounting);
    }
    validate_tree_site_history(state)
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

/// Every recorded tree initially occupied its unique seeded root. Reconstruct all
/// intermediate stays, not just the last site on a felling receipt: a forged
/// transplant path can be individually valid while passing through another
/// tree that did not leave until a later tick.
#[derive(Clone, Copy)]
struct SiteVisit {
    origin: VoxelCoord,
    arrived: SimulationTick,
    departed: Option<SimulationTick>,
    felled_here: bool,
}

fn validate_tree_site_history(state: &WorldState) -> Result<(), WorldValidationError> {
    // Seed-derived trees have distinct natural roots. Only relocation can put
    // different biological identities at the same site, and all unmoved
    // felling identities have already been checked against their seed roots.
    if state.transplants().next().is_none() {
        return Ok(());
    }
    let mut current_sites = BTreeMap::<VoxelCoord, (VoxelCoord, SimulationTick)>::new();
    for tree in state.trees() {
        current_sites.insert(
            tree.origin_root(),
            (tree.origin_root(), SimulationTick::ZERO),
        );
    }
    for felled in state.felled_trees() {
        current_sites.insert(
            felled.origin_root(),
            (felled.origin_root(), SimulationTick::ZERO),
        );
    }

    let mut visits = BTreeMap::<VoxelCoord, Vec<SiteVisit>>::new();
    let mut moved_origins = BTreeSet::new();
    for transplant in state.transplants() {
        let origin = transplant.origin();
        moved_origins.insert(origin);
        let Some((site, arrived)) = current_sites.get_mut(&origin) else {
            return Err(WorldValidationError::InvalidTreeTransplant { origin });
        };
        if *site != transplant.from() || *arrived > transplant.transplanted_at() {
            return Err(WorldValidationError::InvalidTreeTransplant { origin });
        }
        visits.entry(*site).or_default().push(SiteVisit {
            origin,
            arrived: *arrived,
            departed: Some(transplant.transplanted_at()),
            felled_here: false,
        });
        *site = transplant.to();
        *arrived = transplant.transplanted_at();
    }

    for tree in state.trees() {
        let origin = tree.origin_root();
        let (site, arrived) = current_sites[&origin];
        visits.entry(site).or_default().push(SiteVisit {
            origin,
            arrived,
            departed: None,
            felled_here: false,
        });
    }
    for felled in state.felled_trees() {
        let origin = felled.origin_root();
        let (site, arrived) = current_sites[&origin];
        visits.entry(site).or_default().push(SiteVisit {
            origin,
            arrived,
            departed: Some(felled.felled_at()),
            felled_here: true,
        });
    }

    for (site, intervals) in &mut visits {
        validate_site_visits(*site, intervals, &moved_origins)?;
    }
    Ok(())
}

/// Compare one planting site's full history. Immediate handoffs may share
/// a slow timestamp, but intervals of persistent residence cannot overlap.
fn validate_site_visits(
    site: VoxelCoord,
    intervals: &mut [SiteVisit],
    moved_origins: &BTreeSet<VoxelCoord>,
) -> Result<(), WorldValidationError> {
    // A natural tree that never moved continuously occupied its seeded
    // root from world origin. A second tree cannot even briefly pass through
    // it before the natural tree leaves or is felled.
    if let Some(natural) = intervals.iter().find(|visit| {
        visit.origin == site
            && visit.arrived == SimulationTick::ZERO
            && !moved_origins.contains(&site)
    }) && let Some(other) = intervals.iter().find(|visit| {
        visit.origin != site
            && natural
                .departed
                .is_none_or(|departure| visit.arrived < departure)
    }) {
        return Err(if natural.felled_here {
            WorldValidationError::InvalidFelledTree { root: site }
        } else {
            WorldValidationError::InvalidTreeTransplant {
                origin: other.origin,
            }
        });
    }

    // Completed same-tick visits sort before open-ended occupation. This
    // permits legitimate sequential felling and replanting at one tick.
    intervals.sort_unstable_by_key(|visit| {
        (
            visit.arrived,
            visit.departed.is_none(),
            visit.departed,
            visit.origin,
        )
    });
    for pair in intervals.windows(2) {
        if pair[0]
            .departed
            .is_none_or(|departed| departed > pair[1].arrived)
        {
            return Err(if pair[0].felled_here || pair[1].felled_here {
                WorldValidationError::InvalidFelledTree { root: site }
            } else {
                WorldValidationError::InvalidTreeTransplant {
                    origin: pair[1].origin,
                }
            });
        }
    }
    Ok(())
}
