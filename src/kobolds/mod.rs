//! Persistent kobold scouts at the opening frontier.
//!
//! Scouts notice exposed player materials and carry at most one finite loose parcel along
//! their authored retreat route to a recoverable den. The surface owner moves the same parcel
//! step by step; no mass disappears and player interception remains possible. Combat is separate.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceId, SurfaceResourceKind, SurfaceResourceLifecycle, SurfaceResourceState,
    forage_food_to_adjacent_den, relocate_scavenged_drop_step,
};
use crate::survival::Vitality;

const FIRST_SCOUT_TICK: u64 = 180;
const SCOUT_STEP_TICKS: u64 = 5;
/// One scout moves a portable parcel, not an arbitrarily heavy abandoned bulk pile.
const SCOUT_CARRY_LIMIT: Mass = Mass::from_milligrams(1_000_000);
const SCOUT_FORAGE_BATCH: Mass = Mass::from_milligrams(100_000);
/// Avoid stripping a food patch on every 72-second scout circuit.
const SCOUT_FORAGE_INTERVAL_TICKS: u64 = 100;

/// Player-local observation of a kobold scout; remote dens and patrols are not exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldSighting {
    position: VoxelCoord,
    has_noticed_player_drop: bool,
    scavenged_material: Option<(CommodityKey, Mass)>,
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

    /// The identity and mass of a currently carried loose parcel, if still present.
    #[must_use]
    pub const fn scavenged_material(self) -> Option<(CommodityKey, Mass)> {
        self.scavenged_material
    }
}

/// A committed patrol step, reported only when the scout is locally observable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldScoutEvent {
    position: VoxelCoord,
    discovered_player_drop: bool,
    relocated_drop: Option<SurfaceResourceId>,
    fled_player: bool,
    abandoned_drop: Option<SurfaceResourceId>,
    foraged_food: Option<(SurfaceResourceId, Mass)>,
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

    /// One real parcel physically moved with the scout this tick.
    #[must_use]
    pub const fn relocated_drop(self) -> Option<SurfaceResourceId> {
        self.relocated_drop
    }

    /// A nearby living player forced this step to retreat rather than scavenge.
    #[must_use]
    pub const fn fled_player(self) -> bool {
        self.fled_player
    }

    /// Previously carried material left in its existing loose surface custody.
    #[must_use]
    pub const fn abandoned_drop(self) -> Option<SurfaceResourceId> {
        self.abandoned_drop
    }

    /// Finite wild food gathered into a loose, recoverable den parcel.
    #[must_use]
    pub const fn foraged_food(self) -> Option<(SurfaceResourceId, Mass)> {
        self.foraged_food
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
    scavenged_drop: Option<SurfaceResourceId>,
    next_forage_at: Option<SimulationTick>,
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
            scavenged_drop: None,
            next_forage_at: None,
        });
    }

    fn sightings_at<'a>(
        &'a self,
        position: VoxelCoord,
        surface: &'a SurfaceResourceState,
    ) -> impl Iterator<Item = KoboldSighting> + 'a {
        self.colony
            .iter()
            .filter(move |colony| colony.is_near(position))
            .map(|colony| KoboldSighting {
                position: colony.scout_position(),
                has_noticed_player_drop: colony.noticed_player_drop,
                scavenged_material: colony
                    .scavenged_drop
                    .and_then(|id| surface.get(id))
                    .filter(|drop| drop.lifecycle() == SurfaceResourceLifecycle::Available)
                    .map(|drop| (drop.commodity(), drop.remaining_mass())),
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
        .flat_map(move |position| state.kobold_state().sightings_at(position, state.surface()))
}

fn is_portable_drop(resource: &crate::surface::SurfaceResourceRecord) -> bool {
    resource.kind() == SurfaceResourceKind::Drop && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
}

/// Deterministic slow-tick patrol with finite, interruptible return-trip scavenging.
///
/// The scout waits one visit before collecting an exposed drop. On retreat it takes one
/// identity-preserving adjacent step with the parcel, then leaves it at the den where normal
/// player pickup can recover it. Blocking the route stops the scout and its cargo together.
pub(crate) fn apply_kobold_scout_tick(
    registries: &Registries,
    state: &mut AppState,
    next_tick: SimulationTick,
) -> Option<KoboldScoutEvent> {
    let colony = state.kobold_state().colony.as_ref()?;
    if colony.next_step_at > next_tick {
        return None;
    }
    let from = colony.scout_position();
    let forage_ready = colony.next_forage_at.is_none_or(|due| next_tick >= due);
    let player_position = state.logistics().player().map(|player| player.position());
    let player_near = player_position.is_some_and(|position| {
        from.y() == position.y()
            && from
                .x()
                .abs_diff(position.x())
                .saturating_add(from.z().abs_diff(position.z()))
                <= 1
    });
    let pressured = player_near
        && state
            .survival()
            .player()
            .is_some_and(|player| player.vitality() != Vitality::ZERO);
    let exposed_at_current = state
        .surface()
        .available_resources_at(Some(from))
        .any(is_portable_drop);
    let (step, returning) = if pressured {
        // Presence is an active noncombat defense. Scouts flee toward the den, or
        // stay sheltered there, rather than snatching supplies beside the player.
        (colony.scout_step.saturating_sub(1), true)
    } else {
        match (colony.scout_step, colony.returning) {
            (0, _) => (1, false),
            // Exposed material at the junction changes the route before its next step.
            (1, false) if exposed_at_current => (0, true),
            (1, false) => (2, false),
            (2, _) => (1, true),
            (1, true) => (0, true),
            _ => unreachable!("trusted colony route step is bounded"),
        }
    };
    let position = colony.route[usize::from(step)];
    let physically_clear = step != colony.scout_step
        && player_position != Some(position)
        && (state.world().generation_profile().is_none()
            || state
                .world()
                .has_standing_clearance(registries.world(), position));
    // A scout only scavenges during retreat. Player pickup may have exhausted its
    // previously dragged parcel between steps; never resurrect or replace that mass.
    let available_drop_at = |id: SurfaceResourceId| {
        state.surface().get(id).is_some_and(|resource| {
            resource.position() == from
                && resource.lifecycle() == SurfaceResourceLifecycle::Available
                && is_portable_drop(resource)
        })
    };
    let abandoned_drop = pressured
        .then_some(colony.scavenged_drop)
        .flatten()
        .filter(|id| available_drop_at(*id));
    let dragged_candidate = (returning && !pressured)
        .then(|| {
            colony
                .scavenged_drop
                .filter(|id| available_drop_at(*id))
                .or_else(|| {
                    state
                        .surface()
                        .available_resources_at(Some(from))
                        .find(|resource| is_portable_drop(resource))
                        .map(|resource| resource.id())
                })
        })
        .flatten();
    // An exhausted surface revision must not let a creature move while silently
    // leaving still-held matter behind. No mutation is attempted when obstructed.
    let can_move =
        physically_clear && (dragged_candidate.is_none() || state.can_spend_surface_revisions(1));
    // Inspect the destination before any relocation. A scout dragging its own
    // parcel home has not discovered a second exposed item there.
    let discovered = can_move
        && state
            .surface()
            .available_resources_at(Some(position))
            .any(|resource| is_portable_drop(resource));
    let relocated_drop = if can_move {
        dragged_candidate.filter(|id| relocate_scavenged_drop_step(state, *id, from, position))
    } else {
        None
    };
    // An unattended food patch is genuine competition even if the player has
    // never dropped inventory. Loose parcels have priority; the scout forages at
    // the junction only while returning to its adjacent den, in one conserved
    // transaction that leaves the finite catch recoverable.
    let forage_source = (can_move
        && returning
        && step == 0
        && relocated_drop.is_none()
        && !pressured
        && forage_ready)
        .then(|| {
            state
                .surface()
                .available_resources_at(Some(from))
                .find(|resource| {
                    resource.kind() == SurfaceResourceKind::Opportunity
                        && resource.commodity().form() == crate::content::FORM_FOOD
                })
                .map(|resource| resource.id())
        })
        .flatten();
    let foraged_food = forage_source.and_then(|source| {
        forage_food_to_adjacent_den(
            registries,
            state,
            source,
            from,
            position,
            next_tick,
            SCOUT_FORAGE_BATCH,
        )
    });
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
        // Completing the return leg deposits the same loose parcel at the den.
        colony.scavenged_drop = if step == 0 { None } else { relocated_drop };
    }
    if pressured {
        // A trapped scout also relinquishes the parcel, enabling recovery without
        // inventing inaccessible enemy inventory or a damage/attack verb.
        colony.scavenged_drop = None;
    }
    colony.next_step_at = next_step_at;
    if foraged_food.is_some() {
        colony.next_forage_at = Some(
            next_tick
                .checked_add_span(TickSpan::new(SCOUT_FORAGE_INTERVAL_TICKS))
                .expect("bounded simulation cannot overflow the next forage opportunity"),
        );
    }
    colony.noticed_player_drop |= discovered || relocated_drop.is_some();
    let visible = (can_move || abandoned_drop.is_some())
        && player_position.is_some_and(|player| colony.is_near(player));
    visible.then_some(KoboldScoutEvent {
        position: colony.scout_position(),
        discovered_player_drop: discovered,
        relocated_drop,
        fled_player: pressured && can_move,
        abandoned_drop,
        foraged_food,
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
    if let Some(resource) = colony.scavenged_drop {
        let Some(drop) = state.surface().get(resource) else {
            return Err("kobold dragged parcel does not exist");
        };
        if colony.scout_step == 0
            || !colony.returning
            || !is_portable_drop(drop)
            || drop.position() != colony.scout_position()
        {
            return Err("kobold dragged parcel has invalid route custody");
        }
        // Depleted custody is legitimate when the player intercepted the parcel
        // since the last scout step; the next tick clears the stale carry reference.
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
    if let Some(next_forage) = colony.next_forage_at {
        let earliest = FIRST_SCOUT_TICK + SCOUT_STEP_TICKS + SCOUT_FORAGE_INTERVAL_TICKS;
        let forage_due = next_forage.value();
        if forage_due < earliest
            || (forage_due - FIRST_SCOUT_TICK) % SCOUT_STEP_TICKS != 0
            || forage_due > current.saturating_add(SCOUT_FORAGE_INTERVAL_TICKS)
        {
            return Err("kobold forage interval is not aligned with authoritative time");
        }
    }
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
