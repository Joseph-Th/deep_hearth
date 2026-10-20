//! Sparse physical route evidence: a player can follow actual recent scout footsteps.

use serde::{Deserialize, Serialize};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::visibility::scout_visible_from;
use super::{FIRST_SCOUT_TICK, KoboldColony, SCOUT_STEP_TICKS};

const MAX_RECENT_TRACKS: usize = 12;
const TRACK_VISIBLE_TICKS: u64 = 120;

/// What a footprint or disturbed patch says about a scout's physical activity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KoboldTrailKind {
    Walking,
    Hauling,
    Foraging,
    Fleeing,
}

impl KoboldTrailKind {
    pub(super) fn from_step(
        pressured: bool,
        barrier: bool,
        moved_cargo: bool,
        split_cargo: bool,
        gathered_food: bool,
    ) -> Self {
        if pressured || barrier {
            Self::Fleeing
        } else if moved_cargo || split_cargo {
            Self::Hauling
        } else if gathered_food {
            Self::Foraging
        } else {
            Self::Walking
        }
    }
}

/// Evidence left at a voxel where a scout really stood and departed.
///
/// Its direction describes the adjacent step, not any hidden future destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KoboldTrack {
    from: VoxelCoord,
    toward: VoxelCoord,
    at: SimulationTick,
    kind: KoboldTrailKind,
}

impl KoboldTrack {
    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.from
    }

    #[must_use]
    pub const fn toward(self) -> VoxelCoord {
        self.toward
    }

    #[must_use]
    pub const fn observed_activity(self) -> KoboldTrailKind {
        self.kind
    }

    #[must_use]
    pub const fn left_at(self) -> SimulationTick {
        self.at
    }
}

impl KoboldColony {
    pub(super) fn record_track(
        &mut self,
        from: VoxelCoord,
        toward: VoxelCoord,
        at: SimulationTick,
        kind: KoboldTrailKind,
    ) {
        assert!(
            from.is_face_adjacent_to(toward),
            "scout trail requires a real step"
        );
        if self.tracks.len() == MAX_RECENT_TRACKS {
            self.tracks.remove(0);
        }
        self.tracks.push(KoboldTrack {
            from,
            toward,
            at,
            kind,
        });
    }

    pub(super) fn validate_tracks(&self, state: &AppState) -> Result<(), &'static str> {
        if self.tracks.len() > MAX_RECENT_TRACKS {
            return Err("kobold track record exceeds bounded activity history");
        }
        let center = self.route[1];
        let locally_reachable = |position| {
            position == center
                || state.logistics().has_traversal_edge(center, position)
                || state
                    .logistics()
                    .traversable_destinations(center)
                    .any(|first| state.logistics().has_traversal_edge(first, position))
        };
        let mut previous = None;
        for trail in &self.tracks {
            let tick = trail.at.value();
            if tick < FIRST_SCOUT_TICK
                || tick > state.tick().value()
                || (tick - FIRST_SCOUT_TICK) % SCOUT_STEP_TICKS != 0
                || previous.is_some_and(|prior: KoboldTrack| {
                    prior.at >= trail.at || prior.toward != trail.from
                })
                || !state
                    .logistics()
                    .has_traversal_edge(trail.from, trail.toward)
                || !locally_reachable(trail.from)
                || !locally_reachable(trail.toward)
            {
                return Err("kobold track lacks a possible, ordered local physical step");
            }
            previous = Some(*trail);
        }
        if previous.is_some_and(|last: KoboldTrack| last.toward != self.scout_position()) {
            return Err("kobold latest track does not lead to the resident scout");
        }
        Ok(())
    }
}

/// Only visible recent traces at the admitted player's actual location.
///
/// An occluded or faraway den, and the scout's current route, remain unknown.
/// Old signs fade by clock comparison without any per-tick world-wide cleanup.
pub fn available_nearby_kobold_tracks<'a>(
    registries: &'a Registries,
    state: &'a AppState,
) -> impl Iterator<Item = KoboldTrack> + 'a {
    let observer = state.logistics().player().map(|player| player.position());
    state
        .kobold_state()
        .colony
        .iter()
        .flat_map(|colony| colony.tracks.iter().copied())
        .filter(move |track| {
            observer.is_some_and(|player| {
                state.tick().value().saturating_sub(track.at.value()) <= TRACK_VISIBLE_TICKS
                    && scout_visible_from(registries, state, track.from, player)
            })
        })
}

#[cfg(test)]
#[path = "tracks_tests.rs"]
mod tests;
