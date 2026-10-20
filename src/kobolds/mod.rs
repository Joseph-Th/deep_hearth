//! Persistent kobold scouts at the opening frontier.
//!
//! Scouts notice exposed player materials and carry at most one finite loose parcel along
//! their authored retreat route to a recoverable den. The surface owner moves the same parcel
//! step by step; no mass disappears and player interception remains possible. Combat is separate.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::assess_local_illumination;
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceId, SurfaceResourceKind, SurfaceResourceLifecycle, forage_food_to_adjacent_den,
    relocate_scavenged_drop_step, split_bulk_scavenged_drop_step,
};
use crate::survival::Vitality;

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

    /// The scout found that its planned farther cache no longer has portable loot.
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

/// Close-range scout sightings use authoritative block opacity, not distance alone.
/// At two voxels, all intermediate axis steps must be transparent; a diagonal
/// corner is concealed by a blocking wall on either side. Missing generated
/// terrain fails closed. Pre-generation authored graphs have no voxel authority
/// to occlude their local observations.
fn scout_visible_from(
    registries: &Registries,
    state: &AppState,
    scout: VoxelCoord,
    observer: VoxelCoord,
) -> bool {
    let distance = scout
        .x()
        .abs_diff(observer.x())
        .saturating_add(scout.y().abs_diff(observer.y()))
        .saturating_add(scout.z().abs_diff(observer.z()));
    if distance > 2 {
        return false;
    }
    if state.world().generation_profile().is_none() {
        return true;
    }
    let transparent = |voxel| {
        state
            .world()
            .block_at(voxel)
            .and_then(|id| registries.world().block_for_state(id))
            .is_some_and(|block| !block.is_opaque())
    };
    if !transparent(scout) || !transparent(observer) {
        return false;
    }
    if distance < 2 {
        return true;
    }
    let toward = |first: i64, second: i64| {
        if first < second {
            first.checked_add(1)
        } else {
            first.checked_sub(1)
        }
    };
    (scout.x() == observer.x()
        || toward(scout.x(), observer.x())
            .is_some_and(|x| transparent(VoxelCoord::new(x, scout.y(), scout.z()))))
        && (scout.y() == observer.y()
            || toward(scout.y(), observer.y())
                .is_some_and(|y| transparent(VoxelCoord::new(scout.x(), y, scout.z()))))
        && (scout.z() == observer.z()
            || toward(scout.z(), observer.z())
                .is_some_and(|z| transparent(VoxelCoord::new(scout.x(), scout.y(), z))))
}

/// Food takes precedence over inert loose cargo at equal route distance.
/// The stable surface ID breaks ties without introducing random state.
fn scout_loot_priority(
    resource: &crate::surface::SurfaceResourceRecord,
) -> (bool, SurfaceResourceId) {
    (
        resource.commodity().form() != crate::content::FORM_FOOD,
        resource.id(),
    )
}

fn is_portable_drop(resource: &crate::surface::SurfaceResourceRecord) -> bool {
    resource.kind() == SurfaceResourceKind::Drop && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
}

/// A reachable loose pile is unsafe even if too large to move as one parcel.
fn is_contestable_drop(resource: &crate::surface::SurfaceResourceRecord, space: Mass) -> bool {
    resource.kind() == SurfaceResourceKind::Drop
        && !resource.remaining_mass().is_zero()
        && !space.is_zero()
}

/// The den uses ordinary surface custody. Its finite loose storage includes all available
/// player and scout drops, not just parcels labelled as belonging to the colony.
fn den_cache_space(state: &AppState, den: VoxelCoord) -> Mass {
    let occupied: u128 = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|resource| resource.kind() == SurfaceResourceKind::Drop)
        .map(|resource| u128::from(resource.remaining_mass().milligrams()))
        .sum();
    let available = u128::from(DEN_LOOSE_CACHE_LIMIT.milligrams()).saturating_sub(occupied);
    Mass::from_milligrams(
        u64::try_from(available).expect("remaining den space fits its fixed mass limit"),
    )
}

/// From the junction, an exposed parcel on a traversable neighboring branch can
/// displace the routine outer patrol stop. Candidate order is stable surface ID,
/// and inaccessible, illuminated, occupied, or too-heavy parcels cannot lure a
/// scout through an obstructed route. This is one physically adjacent detour, not
/// global knowledge, teleportation, or unrestricted pathfinding.
fn reachable_side_drop(
    registries: &Registries,
    state: &AppState,
    junction: VoxelCoord,
    den: VoxelCoord,
    authored_tip: VoxelCoord,
    player_position: Option<VoxelCoord>,
    capacity: Mass,
) -> Option<VoxelCoord> {
    state
        .surface()
        .resources()
        .filter(|resource| {
            let destination = resource.position();
            resource.kind() == SurfaceResourceKind::Drop
                && resource.lifecycle() == SurfaceResourceLifecycle::Available
                && !resource.remaining_mass().is_zero()
                && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
                && resource.remaining_mass() <= capacity
                && destination != den
                && destination != junction
                && destination != authored_tip
                && state.logistics().has_traversal_edge(junction, destination)
                && player_position != Some(destination)
                && assess_local_illumination(registries, state, destination).level()
                    < SCOUT_DETERRENT_LIGHT_LEVEL
                && (state.world().generation_profile().is_none()
                    || state
                        .world()
                        .has_standing_clearance(registries.world(), destination))
        })
        .min_by_key(|resource| scout_loot_priority(resource))
        .map(|resource| resource.position())
}

/// Search one further authored traversal edge only after no directly adjacent
/// unattended portable parcel was found. The same stable resource order picks
/// the goal; the first physically clear, unlit intermediate path wins.
fn reachable_far_drop(
    registries: &Registries,
    state: &AppState,
    junction: VoxelCoord,
    den: VoxelCoord,
    authored_tip: VoxelCoord,
    player_position: Option<VoxelCoord>,
    capacity: Mass,
) -> Option<(VoxelCoord, VoxelCoord)> {
    state
        .surface()
        .resources()
        .filter_map(|resource| {
            let goal = resource.position();
            if resource.kind() != SurfaceResourceKind::Drop
                || resource.lifecycle() != SurfaceResourceLifecycle::Available
                || resource.remaining_mass().is_zero()
                || resource.remaining_mass() > SCOUT_CARRY_LIMIT
                || resource.remaining_mass() > capacity
                || goal == den
                || goal == junction
                || goal == authored_tip
                || player_position == Some(goal)
                || assess_local_illumination(registries, state, goal).level()
                    >= SCOUT_DETERRENT_LIGHT_LEVEL
                || (state.world().generation_profile().is_some()
                    && !state
                        .world()
                        .has_standing_clearance(registries.world(), goal))
            {
                return None;
            }
            state
                .logistics()
                .traversable_destinations(junction)
                .find(|first| {
                    *first != den
                        && *first != authored_tip
                        && *first != goal
                        && player_position != Some(*first)
                        && state.logistics().has_traversal_edge(*first, goal)
                        && assess_local_illumination(registries, state, *first).level()
                            < SCOUT_DETERRENT_LIGHT_LEVEL
                        && (state.world().generation_profile().is_none()
                            || state
                                .world()
                                .has_standing_clearance(registries.world(), *first))
                })
                .map(|first| (scout_loot_priority(resource), first, goal))
        })
        .min_by_key(|(priority, _, _)| *priority)
        .map(|(_, first, goal)| (first, goal))
}

/// When the junction's own wild food is exhausted, investigate one reachable
/// neighboring food patch. Foraging stays source-owned and physically adjacent:
/// the scout creates its catch only while retreating from the patch.
fn reachable_side_food(
    registries: &Registries,
    state: &AppState,
    junction: VoxelCoord,
    den: VoxelCoord,
    authored_tip: VoxelCoord,
    player_position: Option<VoxelCoord>,
) -> Option<VoxelCoord> {
    state.surface().resources().find_map(|resource| {
        let destination = resource.position();
        (resource.kind() == SurfaceResourceKind::Opportunity
            && resource.lifecycle() == SurfaceResourceLifecycle::Available
            && resource.commodity().form() == crate::content::FORM_FOOD
            && !resource.remaining_mass().is_zero()
            && destination != den
            && destination != junction
            && destination != authored_tip
            && state.logistics().has_traversal_edge(junction, destination)
            && scout_can_enter(registries, state, destination, player_position)
            && assess_local_illumination(registries, state, destination).level()
                < SCOUT_DETERRENT_LIGHT_LEVEL)
            .then_some(destination)
    })
}

fn choose_outbound_detour(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    step: u8,
    returning: bool,
    player_position: Option<VoxelCoord>,
    den_space: Mass,
    forage_ready: bool,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    if step != 2 || returning || colony.scout_step != 1 {
        return None;
    }
    let [den, junction, tip] = colony.route;
    reachable_side_drop(
        registries,
        state,
        junction,
        den,
        tip,
        player_position,
        den_space,
    )
    .map(|first| (first, None))
    .or_else(|| {
        reachable_far_drop(
            registries,
            state,
            junction,
            den,
            tip,
            player_position,
            den_space,
        )
        .map(|(first, goal)| (first, Some(goal)))
    })
    .or_else(|| {
        let local_food_remains =
            state
                .surface()
                .available_resources_at(Some(junction))
                .any(|resource| {
                    resource.kind() == SurfaceResourceKind::Opportunity
                        && resource.commodity().form() == crate::content::FORM_FOOD
                        && !resource.remaining_mass().is_zero()
                });
        (forage_ready && !den_space.is_zero() && !local_food_remains)
            .then(|| reachable_side_food(registries, state, junction, den, tip, player_position))
            .flatten()
            .map(|destination| (destination, None))
    })
}

fn scout_can_enter(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    player_position: Option<VoxelCoord>,
) -> bool {
    player_position != Some(position)
        && (state.world().generation_profile().is_none()
            || state
                .world()
                .has_standing_clearance(registries.world(), position))
}

fn living_player_deters_scout(state: &AppState, from: VoxelCoord) -> bool {
    let near = state.logistics().player().is_some_and(|player| {
        let position = player.position();
        from.y() == position.y()
            && from
                .x()
                .abs_diff(position.x())
                .saturating_add(from.z().abs_diff(position.z()))
                <= 1
    });
    near && state
        .survival()
        .player()
        .is_some_and(|player| player.vitality() != Vitality::ZERO)
}

#[derive(Clone, Copy)]
struct ScoutMovePlan {
    step: u8,
    returning: bool,
    detour: Option<(VoxelCoord, Option<VoxelCoord>)>,
    position: VoxelCoord,
    ahead_is_lit: bool,
    turned_back_at_barrier: bool,
    cancelled_empty_cache_raid: bool,
    physically_clear: bool,
}

/// Movement and retreat planning does not mutate the authoritative colony.
/// An obstructed outbound scout returns by real edges; at the den it waits
/// for a cleared route rather than inventing a bypass.
fn plan_scout_move(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    next_tick: SimulationTick,
    pressured: bool,
    exposed_at_current: bool,
    player_position: Option<VoxelCoord>,
    den_space: Mass,
    forage_ready: bool,
) -> ScoutMovePlan {
    let wary_at_den = colony.scout_step == 0
        && !pressured
        && colony.wary_until.is_some_and(|until| next_tick < until);
    let (proposed_step, proposed_returning) = if wary_at_den {
        (0, false)
    } else {
        colony.next_scout_step(pressured, exposed_at_current)
    };
    let proposed_detour = choose_outbound_detour(
        registries,
        state,
        colony,
        proposed_step,
        proposed_returning,
        player_position,
        den_space,
        forage_ready,
    );
    let proposed_position = proposed_detour.map_or_else(
        || colony.position_at_step(proposed_step),
        |(first, _)| first,
    );
    let ahead_is_lit = !proposed_returning
        && assess_local_illumination(registries, state, proposed_position).level()
            >= SCOUT_DETERRENT_LIGHT_LEVEL;
    let ahead_is_blocked = !scout_can_enter(registries, state, proposed_position, player_position);
    // A detour chosen at the junction is not a reservation. Player pickup or
    // another canonical surface transaction can remove or alter the target
    // during the scout's intervening physical step. Recheck its neighboring
    // goal rather than walking into an empty camp by stale distant knowledge.
    let cancelled_empty_cache_raid = proposed_step == 3
        && !proposed_returning
        && !state
            .surface()
            .available_resources_at(Some(proposed_position))
            .any(|resource| {
                resource.kind() == SurfaceResourceKind::Drop
                    && !resource.remaining_mass().is_zero()
                    && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
                    && resource.remaining_mass() <= den_space
            });
    let turned_back_at_barrier = !proposed_returning
        && proposed_step > colony.scout_step
        && colony.scout_step > 0
        && (ahead_is_lit || ahead_is_blocked);
    let (step, returning, detour, position) =
        if turned_back_at_barrier || cancelled_empty_cache_raid {
            let retreat_step = colony.scout_step - 1;
            (
                retreat_step,
                true,
                None,
                colony.position_at_step(retreat_step),
            )
        } else {
            (
                proposed_step,
                proposed_returning,
                proposed_detour,
                proposed_position,
            )
        };
    ScoutMovePlan {
        step,
        returning,
        detour,
        position,
        ahead_is_lit,
        turned_back_at_barrier,
        cancelled_empty_cache_raid,
        physically_clear: step != colony.scout_step
            && (returning || !ahead_is_lit)
            && scout_can_enter(registries, state, position, player_position),
    }
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
    let pressured_by_player = living_player_deters_scout(state, from);
    // World-owned opaque blocks and live finite energy stores control this field.
    // Neither player-held torches nor hidden ambient/skylight are invented here.
    let pressured_by_light =
        assess_local_illumination(registries, state, from).level() >= SCOUT_DETERRENT_LIGHT_LEVEL;
    let pressured = pressured_by_player || pressured_by_light;
    let den_space = den_cache_space(state, colony.route[0]);
    let exposed_at_current = state
        .surface()
        .available_resources_at(Some(from))
        .any(|resource| is_contestable_drop(resource, den_space));
    let move_plan = plan_scout_move(
        registries,
        state,
        colony,
        next_tick,
        pressured,
        exposed_at_current,
        player_position,
        den_space,
        forage_ready,
    );
    let ScoutMovePlan {
        step,
        returning,
        detour,
        position,
        ahead_is_lit,
        turned_back_at_barrier,
        cancelled_empty_cache_raid,
        physically_clear,
    } = move_plan;
    // A scout only scavenges during retreat. Player pickup may have exhausted its
    // previously dragged parcel between steps; never resurrect or replace that mass.
    let available_drop_at = |id: SurfaceResourceId| {
        state.surface().get(id).is_some_and(|resource| {
            resource.position() == from
                && resource.lifecycle() == SurfaceResourceLifecycle::Available
                && is_portable_drop(resource)
                && resource.remaining_mass() <= den_space
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
                        .filter(|resource| {
                            is_portable_drop(resource) && resource.remaining_mass() <= den_space
                        })
                        .min_by_key(|resource| scout_loot_priority(resource))
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
            .any(|resource| is_contestable_drop(resource, den_space));
    let relocated_drop = if can_move {
        dragged_candidate.filter(|id| relocate_scavenged_drop_step(state, *id, from, position))
    } else {
        None
    };
    // Whole portable parcels retain identity. Oversized exposed piles instead
    // yield a bounded daughter parcel, leaving the exact remainder at the source.
    let bulk_source = (can_move && returning && !pressured && relocated_drop.is_none())
        .then(|| {
            state
                .surface()
                .available_resources_at(Some(from))
                .filter(|resource| {
                    is_contestable_drop(resource, den_space)
                        && (!is_portable_drop(resource) || resource.remaining_mass() > den_space)
                })
                .min_by_key(|resource| scout_loot_priority(resource))
                .map(|resource| resource.id())
        })
        .flatten();
    let split_drop = bulk_source.and_then(|source| {
        split_bulk_scavenged_drop_step(
            registries,
            state,
            source,
            from,
            position,
            next_tick,
            SCOUT_BULK_SPLIT_BATCH.min(den_space),
        )
    });
    // An unattended food patch is genuine competition even if the player has
    // never dropped inventory. Loose parcels have priority; the scout forages at
    // the junction only while returning to its adjacent den, in one conserved
    // transaction that leaves the finite catch recoverable.
    let forage_source = (can_move
        && returning
        && step <= 1
        && relocated_drop.is_none()
        && split_drop.is_none()
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
            SCOUT_FORAGE_BATCH.min(den_space),
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
        colony.apply_step(
            step,
            returning,
            detour,
            relocated_drop
                .or(split_drop.map(|(id, _)| id))
                .or_else(|| (step > 0).then(|| foraged_food.map(|(id, _)| id)).flatten()),
        );
    }
    if pressured {
        // A trapped scout also relinquishes the parcel, enabling recovery without
        // inventing inaccessible enemy inventory or a damage/attack verb.
        colony.scavenged_drop = None;
    }
    colony.next_step_at = next_step_at;
    if pressured || (turned_back_at_barrier && ahead_is_lit && can_move) {
        colony.wary_until = Some(
            next_tick
                .checked_add_span(TickSpan::new(SCOUT_WARY_INTERVAL_TICKS))
                .expect("bounded simulation cannot overflow a scout caution deadline"),
        );
    }
    if foraged_food.is_some() {
        colony.next_forage_at = Some(
            next_tick
                .checked_add_span(TickSpan::new(SCOUT_FORAGE_INTERVAL_TICKS))
                .expect("bounded simulation cannot overflow the next forage opportunity"),
        );
    }
    colony.noticed_player_drop |= discovered || relocated_drop.is_some() || split_drop.is_some();
    let scout_position = colony.scout_position();
    let visible = (can_move || abandoned_drop.is_some())
        && player_position
            .is_some_and(|player| scout_visible_from(registries, state, scout_position, player));
    visible.then_some(KoboldScoutEvent {
        position: scout_position,
        discovered_player_drop: discovered,
        relocated_drop,
        fled_player: pressured_by_player && can_move,
        fled_light: (pressured_by_light || (turned_back_at_barrier && ahead_is_lit)) && can_move,
        turned_back_at_barrier: turned_back_at_barrier && can_move,
        cancelled_empty_cache_raid: cancelled_empty_cache_raid && can_move,
        abandoned_drop,
        foraged_food,
        split_drop,
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
