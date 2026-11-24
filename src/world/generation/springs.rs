//! Sparse, finite surface water pockets in actual moisture-favoured terrain.
//!
//! Each two-by-two chunk district has one independent candidate. Wet hollows
//! are worth seeking; dry scrub has no generated standing water. The volume
//! belongs to the existing fluid owner, is consumed by ordinary drinking,
//! and never respawns when the player revisits a generated chunk.

use crate::content::FLUID_WATER;
use crate::core::quantity::{Temperature, Volume};
use crate::core::state::AppState;
use crate::fluid::GeneratedFluidStoreSpec;
use crate::spatial::VoxelCoord;

use super::super::spring_identity::seeded_spring_candidates;
use super::occupancy::natural_site_is_clear;
use super::{WorldChunk, WorldGenerationProfile};

pub(super) fn seeded_spring_spec(
    state: &AppState,
    profile: WorldGenerationProfile,
    chunk: &WorldChunk,
    reserved: &[VoxelCoord],
) -> Option<(VoxelCoord, GeneratedFluidStoreSpec)> {
    let tree = super::super::generated_tree(profile, chunk);
    // Prefer low ground among nearby valid candidates, so the resource
    // tends to sit in an actual shallow hollow rather than on a ridge.
    let mut best = None;
    for (position, capacity) in seeded_spring_candidates(profile, chunk.coord()) {
        if !natural_site_is_clear(state, chunk, tree, position, reserved) {
            continue;
        }
        let candidate = (position.y(), position, capacity);
        if best.is_none_or(|current: (i64, VoxelCoord, Volume)| candidate < current) {
            best = Some(candidate);
        }
    }
    let (_, position, capacity) = best?;
    Some((
        position,
        GeneratedFluidStoreSpec::new(
            capacity,
            FLUID_WATER,
            capacity,
            Temperature::from_millikelvin(293_150),
        ),
    ))
}

#[cfg(test)]
#[path = "springs_tests.rs"]
mod tests;
