//! Bounded local percussion through actual open voxels, independently of sight.
//!
//! A sound may follow a bent air corridor but not cut across solid rock, built panels, or an
//! unknown/unloaded chunk along a generated route. In ungenerated opening space,
//! even when distant chunks exist, the opening uses a local
//! Manhattan bound when unobstructed, or a bounded air detour around real built
//! walls. This is not a world-wide noise map or signal network; rig transmission
//! has separate physical rules.

use std::collections::{BTreeSet, VecDeque};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::logistics::{PlayerStance, player_body_is_grounded};
use crate::registry::Registries;
use crate::spatial::{CubeFace, VoxelCoord};
use crate::survival::Vitality;
use crate::world::{BlockCollisionShape, ChunkCoord, WindCondition, wind_at_generated_position};

use super::junkworks::{KoboldJunkRig, KoboldJunkRigKind};
use super::tracks::KoboldTrailKind;
use super::visibility::within_manhattan_reach;

/// Only an actual recent movement makes a sound. Tracks remain visible much
/// longer; they must not turn into a permanent noise source.
const FOOTFALL_AUDIBLE_TICKS: u64 = 2;

/// What a nearby living resident can be heard doing without identifying them
/// or disclosing the coordinates of an unseen den or patrol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KoboldFootfallKind {
    Footsteps,
    Dragging,
    Foraging,
    Splashing,
    Hurrying,
}

/// Broad bearing only; sound can bend through air corridors and does not
/// provide a target, a distance measurement, or the resident's route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KoboldFootfallBearing {
    Nearby,
    North,
    South,
    East,
    West,
    Above,
    Below,
}

/// A short-lived auditory clue from one physically recorded resident step.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldFootfall {
    kind: KoboldFootfallKind,
    bearing: KoboldFootfallBearing,
    occurred_at: SimulationTick,
}

impl KoboldFootfall {
    #[must_use]
    pub const fn kind(self) -> KoboldFootfallKind {
        self.kind
    }

    #[must_use]
    pub const fn bearing(self) -> KoboldFootfallBearing {
        self.bearing
    }

    /// Allows an audio adapter to play each real step once instead of
    /// retriggering it on every 20 Hz observation.
    #[must_use]
    pub const fn occurred_at(self) -> SimulationTick {
        self.occurred_at
    }
}

fn footfall_kind(activity: KoboldTrailKind) -> (KoboldFootfallKind, u8) {
    match activity {
        KoboldTrailKind::Walking => (KoboldFootfallKind::Footsteps, 4),
        KoboldTrailKind::Hauling => (KoboldFootfallKind::Dragging, 5),
        KoboldTrailKind::Foraging => (KoboldFootfallKind::Foraging, 3),
        KoboldTrailKind::Drinking => (KoboldFootfallKind::Splashing, 3),
        KoboldTrailKind::Fleeing => (KoboldFootfallKind::Hurrying, 5),
    }
}

/// Open-air gusts mask softer movement; underground rock passages retain
/// their own air-borne acoustics independent of the surface weather.
fn wind_masked_reach(
    registries: &Registries,
    state: &AppState,
    origin: VoxelCoord,
    at: SimulationTick,
    reach: u8,
) -> u8 {
    match wind_at_generated_position(registries, state, origin, at) {
        Some(WindCondition::Breeze) => reach.saturating_sub(1),
        Some(WindCondition::Gusting) => reach.saturating_sub(2),
        Some(WindCondition::Calm) | None => reach,
    }
}

fn footfall_bearing(source: VoxelCoord, listener: VoxelCoord) -> KoboldFootfallBearing {
    let dx = source.x().abs_diff(listener.x());
    let dy = source.y().abs_diff(listener.y());
    let dz = source.z().abs_diff(listener.z());
    if dy > dx.max(dz) {
        return if source.y() > listener.y() {
            KoboldFootfallBearing::Above
        } else {
            KoboldFootfallBearing::Below
        };
    }
    if dx > dz {
        if source.x() > listener.x() {
            KoboldFootfallBearing::East
        } else {
            KoboldFootfallBearing::West
        }
    } else if dz > dx {
        if source.z() > listener.z() {
            KoboldFootfallBearing::South
        } else {
            KoboldFootfallBearing::North
        }
    } else {
        KoboldFootfallBearing::Nearby
    }
}

/// Present only real footfalls on the current resident clock, with bounded
/// propagation through the same physical air as a material-built den alarm.
/// The speaker's role and exact location are deliberately not returned.
#[must_use]
pub fn available_nearby_kobold_footfalls(
    registries: &Registries,
    state: &AppState,
) -> Vec<KoboldFootfall> {
    let Some(listener) = state.logistics().player().map(|player| player.position()) else {
        return Vec::new();
    };
    if state
        .survival()
        .player()
        .is_none_or(|player| player.vitality() == Vitality::ZERO)
    {
        return Vec::new();
    }
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return Vec::new();
    };
    [colony.tracks.last(), colony.watch_tracks.last()]
        .into_iter()
        .flatten()
        .filter_map(|track| {
            let age = state.tick().checked_duration_since(track.left_at())?;
            if age.value() > FOOTFALL_AUDIBLE_TICKS {
                return None;
            }
            let source = track.toward();
            let (kind, nominal_reach) = footfall_kind(track.observed_activity());
            let reach =
                wind_masked_reach(registries, state, source, track.left_at(), nominal_reach);
            audible_with_reach(registries, state, source, listener, reach).then_some(
                KoboldFootfall {
                    kind,
                    bearing: footfall_bearing(source, listener),
                    occurred_at: track.left_at(),
                },
            )
        })
        .collect()
}

/// A sprinting player's actual grounded body motion can alert a nearby scout
/// even around a stone corner. Crouching, stationary, airborne and ordinary
/// walking actors do not make this extra warning. The resident still knows
/// only that something noisy is near, not a target path or remote inventory.
pub(super) fn scout_hears_running_player(
    registries: &Registries,
    state: &AppState,
    listener: VoxelCoord,
    at: SimulationTick,
) -> bool {
    let Some(player) = state.logistics().player() else {
        return false;
    };
    if state
        .survival()
        .player()
        .is_none_or(|record| record.vitality() == Vitality::ZERO)
        || player.body().stance() != PlayerStance::Standing
        || !player_body_is_grounded(registries, state)
    {
        return false;
    }
    let velocity = player.body().horizontal_velocity();
    let x = u128::from(velocity.x_nanometers_per_second().unsigned_abs());
    let z = u128::from(velocity.z_nanometers_per_second().unsigned_abs());
    let walking = u128::from(
        registries
            .survival()
            .physiology()
            .locomotion()
            .walking_speed()
            .nanometers_per_second(),
    );
    let speed_squared = x.saturating_mul(x).saturating_add(z.saturating_mul(z));
    speed_squared > walking * walking
        && audible_with_reach(
            registries,
            state,
            player.position(),
            listener,
            wind_masked_reach(registries, state, player.position(), at, 6),
        )
}

fn acoustic_reach(kind: KoboldJunkRigKind) -> u8 {
    match kind {
        // The wooden brace dampens the crack; a bare stone clapper carries farther.
        KoboldJunkRigKind::TimberRatchet => 3,
        KoboldJunkRigKind::StoneClatter => 5,
        // Shaped iron rings clearly beyond the radius of a raw stone clapper.
        KoboldJunkRigKind::IronChime => 6,
    }
}

fn neighbors(voxel: VoxelCoord) -> impl Iterator<Item = VoxelCoord> {
    [
        CubeFace::East,
        CubeFace::West,
        CubeFace::Top,
        CubeFace::Bottom,
        CubeFace::South,
        CubeFace::North,
    ]
    .into_iter()
    .filter_map(move |face| face.adjacent_voxel(voxel))
}

/// Sound travels along actual face-connected, empty voxels; a short bend is
/// audible even when the ordinary two-cell visual ray is blocked. Opaque
/// terrain always obstructs; missing terrain fails closed on generated local
/// routes and follows the authored opening graph elsewhere. Search is capped at six steps
/// and cannot discover a remote region or leak a blocked source location.
pub(super) fn can_hear_junk_rig(
    registries: &Registries,
    state: &AppState,
    rig: KoboldJunkRig,
    listener: VoxelCoord,
) -> bool {
    audible_path(registries, state, rig.position(), listener, rig.kind())
}

fn audible_path(
    registries: &Registries,
    state: &AppState,
    source: VoxelCoord,
    listener: VoxelCoord,
    kind: KoboldJunkRigKind,
) -> bool {
    audible_with_reach(registries, state, source, listener, acoustic_reach(kind))
}

fn audible_with_reach(
    registries: &Registries,
    state: &AppState,
    source: VoxelCoord,
    listener: VoxelCoord,
    reach: u8,
) -> bool {
    if !within_manhattan_reach(source, listener, u64::from(reach)) {
        return false;
    }
    // The authored graph has no terrain opacity. Built full-cell members
    // still block sound, and may lengthen its physical path beyond the
    // authored hearing radius. Use the indexed structural broadphase before
    // deciding whether a bounded air-corridor search is necessary.
    let generated_at_endpoint = [source, listener]
        .into_iter()
        .any(|position| state.world().has_generated_terrain_at(position));
    let passable = |voxel| {
        !state.structures().blocks_full_cell(voxel)
            && state.world().tree_trunk_at(voxel).is_none()
            && match state.world().block_at(voxel) {
                Some(id) => registries
                    .world()
                    .block_for_state(id)
                    .is_some_and(|block| block.collision() == BlockCollisionShape::Empty),
                None => !generated_at_endpoint,
            }
    };
    if !passable(source) || !passable(listener) {
        return false;
    }
    if !generated_at_endpoint
        && ChunkCoord::containing(source) == ChunkCoord::containing(listener)
        && !state
            .structures()
            .has_full_cell_member_in_voxel_box(source, listener)
    {
        return true;
    }
    let mut visited = BTreeSet::from([source]);
    let mut queue = VecDeque::from([(source, 0_u8)]);
    while let Some((current, distance)) = queue.pop_front() {
        if current == listener {
            return true;
        }
        if distance == reach {
            continue;
        }
        for neighbor in neighbors(current) {
            if visited.insert(neighbor) && passable(neighbor) {
                queue.push_back((neighbor, distance + 1));
            }
        }
    }
    false
}

#[cfg(test)]
#[path = "acoustics_tests.rs"]
mod tests;
