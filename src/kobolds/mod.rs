//! Persistent kobold scouts at the opening frontier.
//!
//! Scouts notice exposed player materials and carry at most one finite loose parcel along
//! their authored retreat route to a recoverable den. The surface owner moves the same parcel
//! step by step; no mass disappears and player interception remains possible. Combat is separate.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceId, SurfaceResourceLifecycle};

mod patrol;
mod scavenging;
mod visibility;

pub(crate) use patrol::apply_kobold_scout_tick;
#[cfg(test)]
use scavenging::den_cache_space;
use scavenging::is_portable_drop;
use visibility::scout_visible_from;

const FIRST_SCOUT_TICK: u64 = 180;
const SCOUT_STEP_TICKS: u64 = 5;
/// One scout moves a portable parcel, not an arbitrarily heavy abandoned bulk pile.
const SCOUT_CARRY_LIMIT: Mass = Mass::from_milligrams(1_000_000);
/// Limited loose staging space at the den; source scarcity alone is not its capacity.
const DEN_LOOSE_CACHE_LIMIT: Mass = Mass::from_milligrams(500_000);
const SCOUT_FORAGE_BATCH: Mass = Mass::from_milligrams(100_000);
/// Scavengers can divide an exposed bulk parcel, but only into small carried lots.
const SCOUT_BULK_SPLIT_BATCH: Mass = Mass::from_milligrams(250_000);
/// Avoid stripping a food patch on every 72-second scout circuit.
const SCOUT_FORAGE_INTERVAL_TICKS: u64 = 100;
/// A chased scout stays wary for 2.4 simulation minutes after the most recent threat.
const SCOUT_WARY_INTERVAL_TICKS: u64 = 40;
/// A low, dying ember does not repel the same scout as a maintained bright fire.
const SCOUT_DETERRENT_LIGHT_LEVEL: u8 = 3;

/// Player-local observation of a kobold scout; remote dens and patrols are not exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldSighting {
    position: VoxelCoord,
    has_noticed_player_drop: bool,
    is_wary: bool,
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

    /// The nearby scout is still avoiding danger after player or firelight pressure.
    #[must_use]
    pub const fn is_wary(self) -> bool {
        self.is_wary
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
    fled_light: bool,
    turned_back_at_barrier: bool,
    cancelled_empty_cache_raid: bool,
    abandoned_drop: Option<SurfaceResourceId>,
    foraged_food: Option<(SurfaceResourceId, Mass)>,
    split_drop: Option<(SurfaceResourceId, Mass)>,
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

    /// The scout retreated because a finite local light source illuminated its position.
    #[must_use]
    pub const fn fled_light(self) -> bool {
        self.fled_light
    }

    /// An inaccessible or illuminated outbound cell forced the scout to turn home.
    #[must_use]
    pub const fn turned_back_at_barrier(self) -> bool {
        self.turned_back_at_barrier
    }

    /// The planned farther cache no longer has any loose matter it can recover.
    #[must_use]
    pub const fn cancelled_empty_cache_raid(self) -> bool {
        self.cancelled_empty_cache_raid
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

    /// A loose bulk pile lost this much mass into a new recoverable parcel.
    #[must_use]
    pub const fn split_drop(self) -> Option<(SurfaceResourceId, Mass)> {
        self.split_drop
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KoboldColony {
    /// The three authored patrol voxels never change after wilderness generation.
    route: [VoxelCoord; 3],
    /// One temporary branch diversion, valid only until the scout returns to the den.
    /// Its presence in the new save schema is mandatory even when the value is null.
    #[serde(deserialize_with = "required_optional_detour")]
    detour: Option<VoxelCoord>,
    /// Second traversable branch cell when a scout investigates a farther exposed cache.
    #[serde(deserialize_with = "required_optional_detour")]
    detour_goal: Option<VoxelCoord>,
    scout_step: u8,
    returning: bool,
    next_step_at: SimulationTick,
    noticed_player_drop: bool,
    scavenged_drop: Option<SurfaceResourceId>,
    next_forage_at: Option<SimulationTick>,
    /// Caution survives saves and expires on the same five-tick scout clock.
    #[serde(deserialize_with = "required_optional_wary_tick")]
    wary_until: Option<SimulationTick>,
}

fn required_optional_detour<'de, D>(deserializer: D) -> Result<Option<VoxelCoord>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<VoxelCoord>::deserialize(deserializer)
}

fn required_optional_wary_tick<'de, D>(deserializer: D) -> Result<Option<SimulationTick>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<SimulationTick>::deserialize(deserializer)
}

impl KoboldColony {
    fn position_at_step(&self, step: u8) -> VoxelCoord {
        match step {
            0..=1 => self.route[usize::from(step)],
            2 => self.detour.unwrap_or(self.route[2]),
            3 => self
                .detour_goal
                .expect("a fourth scout step requires a planned goal"),
            _ => unreachable!("trusted scout step remains in its bounded route"),
        }
    }

    fn scout_position(&self) -> VoxelCoord {
        self.position_at_step(self.scout_step)
    }

    /// Return toward safety when confronted; otherwise keep the patrol or turn
    /// home with an unattended loose parcel at the junction.
    fn next_scout_step(&self, pressured: bool, exposed_at_current: bool) -> (u8, bool) {
        if pressured {
            return (self.scout_step.saturating_sub(1), true);
        }
        match (self.scout_step, self.returning) {
            (0, _) => (1, false),
            (1, false) if exposed_at_current => (0, true),
            (1, false) => (2, false),
            (2, false) if self.detour_goal.is_some() && !exposed_at_current => (3, false),
            (3, false) => (2, true),
            (2, _) => (1, true),
            (1, true) => (0, true),
            _ => unreachable!("trusted colony route step is bounded"),
        }
    }

    fn apply_step(
        &mut self,
        step: u8,
        returning: bool,
        detour: Option<(VoxelCoord, Option<VoxelCoord>)>,
        carried: Option<SurfaceResourceId>,
    ) {
        if let Some((first, goal)) = detour {
            self.detour = Some(first);
            self.detour_goal = goal;
        } else if step == 0 {
            self.detour = None;
            self.detour_goal = None;
        }
        self.scout_step = step;
        self.returning = returning;
        // Completing the return leg deposits the same loose parcel at the den.
        self.scavenged_drop = if step == 0 { None } else { carried };
    }

    fn validate_detour(&self, state: &AppState) -> Result<(), &'static str> {
        if let Some(detour) = self.detour {
            if (self.scout_step != 2
                && self.scout_step != 3
                && !(self.scout_step == 1 && self.returning))
                || detour == self.route[0]
                || detour == self.route[1]
                || detour == self.route[2]
                || !state.logistics().has_traversal_edge(self.route[1], detour)
            {
                return Err("invalid kobold temporary detour or traversal edge");
            }
        }
        if let Some(goal) = self.detour_goal {
            let Some(first) = self.detour else {
                return Err("kobold far detour has no intermediate route cell");
            };
            if (self.scout_step == 1 && !self.returning)
                || self.scout_step == 0
                || goal == first
                || goal == self.route[0]
                || goal == self.route[1]
                || goal == self.route[2]
                || !state.logistics().has_traversal_edge(first, goal)
            {
                return Err("invalid kobold second detour edge or cursor");
            }
        } else if self.scout_step == 3 {
            return Err("kobold far detour is missing its destination");
        }
        Ok(())
    }

    fn is_visible_from(
        &self,
        registries: &Registries,
        state: &AppState,
        observer: VoxelCoord,
    ) -> bool {
        scout_visible_from(registries, state, self.scout_position(), observer)
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
            detour: None,
            detour_goal: None,
            scout_step: 0,
            returning: false,
            next_step_at: SimulationTick::new(FIRST_SCOUT_TICK),
            noticed_player_drop: false,
            scavenged_drop: None,
            next_forage_at: None,
            wary_until: None,
        });
    }

    fn sightings_at<'a>(
        &'a self,
        registries: &'a Registries,
        state: &'a AppState,
        position: VoxelCoord,
    ) -> impl Iterator<Item = KoboldSighting> + 'a {
        self.colony
            .iter()
            .filter(move |colony| colony.is_visible_from(registries, state, position))
            .map(move |colony| KoboldSighting {
                position: colony.scout_position(),
                has_noticed_player_drop: colony.noticed_player_drop,
                is_wary: colony.wary_until.is_some_and(|until| until > state.tick()),
                scavenged_material: colony
                    .scavenged_drop
                    .and_then(|id| state.surface().get(id))
                    .filter(|drop| drop.lifecycle() == SurfaceResourceLifecycle::Available)
                    .map(|drop| (drop.commodity(), drop.remaining_mass())),
            })
    }
}

/// Local-only observation, suitable for an adapter's entity/sighting presentation.
pub fn available_nearby_kobold_scouts<'a>(
    registries: &'a Registries,
    state: &'a AppState,
) -> impl Iterator<Item = KoboldSighting> + 'a {
    let observer = state.logistics().player().map(|player| player.position());
    observer.into_iter().flat_map(move |position| {
        state
            .kobold_state()
            .sightings_at(registries, state, position)
    })
}

/// Full save-admission validation of persistent colony topology and patrol continuation.
pub(crate) fn validate_loaded_kobolds(state: &AppState) -> Result<(), &'static str> {
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return Ok(());
    };
    if colony.scout_step > 3
        || (colony.scout_step == 3 && (colony.returning || colony.detour_goal.is_none()))
        || !colony.route[0].is_face_adjacent_to(colony.route[1])
        || !colony.route[1].is_face_adjacent_to(colony.route[2])
        || colony.route[0] == colony.route[2]
    {
        return Err("invalid kobold patrol route or cursor");
    }
    colony.validate_detour(state)?;
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
    if let Some(wary_until) = colony.wary_until {
        let due = wary_until.value();
        if due < FIRST_SCOUT_TICK + SCOUT_WARY_INTERVAL_TICKS
            || (due - FIRST_SCOUT_TICK) % SCOUT_STEP_TICKS != 0
            || due > current.saturating_add(SCOUT_WARY_INTERVAL_TICKS)
        {
            return Err("kobold caution deadline is not aligned with authoritative time");
        }
    }
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
