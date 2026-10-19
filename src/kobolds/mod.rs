//! Persistent kobold scouts at the opening frontier.
//!
//! This first creature slice owns positions, patrol timing and evidence of investigated loose
//! player material. It deliberately does not invent combat damage, loot destruction, or free
//! material conversion. Future theft must cross canonical surface/inventory custody boundaries.

use serde::{Deserialize, Serialize};

use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::SurfaceResourceKind;

const FIRST_SCOUT_TICK: u64 = 600;
const SCOUT_STEP_TICKS: u64 = 20;

/// Player-local observation of a kobold scout; remote dens and patrols are not exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldSighting {
    position: VoxelCoord,
    has_noticed_player_drop: bool,
}

impl KoboldSighting {
    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub const fn has_noticed_player_drop(self) -> bool {
        self.has_noticed_player_drop
    }
}

/// A committed patrol step, reported only when the scout is locally observable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldScoutEvent {
    position: VoxelCoord,
    discovered_player_drop: bool,
}

impl KoboldScoutEvent {
    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub const fn discovered_player_drop(self) -> bool {
        self.discovered_player_drop
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KoboldColony {
    /// A three-voxel route, anchored at a den entrance in the authored opening graph.
    route: [VoxelCoord; 3],
    scout_step: u8,
    returning: bool,
    next_step_at: SimulationTick,
    noticed_player_drop: bool,
}

impl KoboldColony {
    fn scout_position(&self) -> VoxelCoord {
        self.route[usize::from(self.scout_step)]
    }

    fn is_near(&self, observer: VoxelCoord) -> bool {
        let scout = self.scout_position();
        scout
            .x()
            .abs_diff(observer.x())
            .saturating_add(scout.y().abs_diff(observer.y()))
            .saturating_add(scout.z().abs_diff(observer.z()))
            <= 2
    }
}

/// Complete durable kobold owner. The opening creates at most one colony.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KoboldState {
    colony: Option<KoboldColony>,
}

impl KoboldState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self { colony: None }
    }

    #[must_use]
    pub(crate) const fn is_pristine(&self) -> bool {
        self.colony.is_none()
    }

    /// Called within the already prechecked initial-wilderness generation transaction.
    pub(crate) fn apply_opening_colony(&mut self, route: [VoxelCoord; 3]) {
        assert!(self.is_pristine());
        assert!(route[0].is_face_adjacent_to(route[1]));
        assert!(route[1].is_face_adjacent_to(route[2]));
        assert_ne!(route[0], route[2]);
        self.colony = Some(KoboldColony {
            route,
            scout_step: 0,
            returning: false,
            next_step_at: SimulationTick::new(FIRST_SCOUT_TICK),
            noticed_player_drop: false,
        });
    }

    fn sightings_at(&self, position: VoxelCoord) -> impl Iterator<Item = KoboldSighting> + '_ {
        self.colony
            .iter()
            .filter(move |colony| colony.is_near(position))
            .map(|colony| KoboldSighting {
                position: colony.scout_position(),
                has_noticed_player_drop: colony.noticed_player_drop,
            })
    }
}

/// Local-only observation, suitable for an adapter's entity/sighting presentation.
pub fn available_nearby_kobold_scouts(
    state: &AppState,
) -> impl Iterator<Item = KoboldSighting> + '_ {
    let observer = state.logistics().player().map(|player| player.position());
    observer
        .into_iter()
        .flat_map(move |position| state.kobold_state().sightings_at(position))
}

/// Deterministic slow-tick patrol. Scouts notice exposed player-created drops but do not take
/// custody of them until a conserved scavenging operation exists.
pub(crate) fn apply_kobold_scout_tick(
    registries: &Registries,
    state: &mut AppState,
    next_tick: SimulationTick,
) -> Option<KoboldScoutEvent> {
    let colony = state.kobold_state().colony.as_ref()?;
    if colony.next_step_at > next_tick {
        return None;
    }
    let (step, returning) = match (colony.scout_step, colony.returning) {
        (0, _) => (1, false),
        (1, false) => (2, false),
        (2, _) => (1, true),
        (1, true) => (0, true),
        _ => unreachable!("trusted colony route step is bounded"),
    };
    let position = colony.route[usize::from(step)];
    let player_position = state.logistics().player().map(|player| player.position());
    let can_move = player_position != Some(position)
        && (state.world().generation_profile().is_none()
            || state
                .world()
                .has_standing_clearance(registries.world(), position));
    let discovered = can_move
        && state
            .surface()
            .available_resources_at(Some(position))
            .any(|resource| resource.kind() == SurfaceResourceKind::Drop);
    let next_step_at = next_tick
        .checked_add_span(TickSpan::new(SCOUT_STEP_TICKS))
        .expect("bounded control-aligned simulation cannot overflow a scout schedule");

    let colony = state
        .kobold_state_mut()
        .colony
        .as_mut()
        .unwrap_or_else(|| unreachable!("scout decision requires a colony"));
    if can_move {
        colony.scout_step = step;
        colony.returning = returning;
    }
    colony.next_step_at = next_step_at;
    colony.noticed_player_drop |= discovered;
    let visible = can_move && player_position.is_some_and(|player| colony.is_near(player));
    visible.then_some(KoboldScoutEvent {
        position,
        discovered_player_drop: discovered,
    })
}

/// Full save-admission validation of persistent colony topology and patrol continuation.
pub(crate) fn validate_loaded_kobolds(state: &AppState) -> Result<(), &'static str> {
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return Ok(());
    };
    if colony.scout_step > 2
        || (colony.scout_step == 2 && colony.returning)
        || !colony.route[0].is_face_adjacent_to(colony.route[1])
        || !colony.route[1].is_face_adjacent_to(colony.route[2])
        || colony.route[0] == colony.route[2]
    {
        return Err("invalid kobold patrol route or cursor");
    }
    if !state
        .logistics()
        .has_traversal_edge(colony.route[0], colony.route[1])
        || !state
            .logistics()
            .has_traversal_edge(colony.route[1], colony.route[2])
    {
        return Err("kobold patrol lacks authoritative wilderness traversal edges");
    }
    let current = state.tick().value();
    let due = colony.next_step_at.value();
    if due <= current {
        return Err("kobold scout step is overdue");
    }
    if due < FIRST_SCOUT_TICK
        || (due - FIRST_SCOUT_TICK) % SCOUT_STEP_TICKS != 0
        || (current < FIRST_SCOUT_TICK && due != FIRST_SCOUT_TICK)
        || (current >= FIRST_SCOUT_TICK && due - current > SCOUT_STEP_TICKS)
    {
        return Err("kobold patrol schedule is not aligned with authoritative time");
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
