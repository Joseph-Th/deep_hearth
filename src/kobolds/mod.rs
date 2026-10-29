//! Persistent kobold scouts at the opening frontier.
//!
//! Scouts notice exposed player materials and carry at most one finite loose parcel along
//! their authored retreat route to a recoverable den. The surface owner moves the same parcel
//! step by step; no mass disappears and player interception remains possible. Combat is separate.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::quantity::{AggregateMass, Mass, Temperature};
use crate::core::state::AppState;
use crate::core::time::{ControlTick, SimulationTick};
use crate::inventory::StockpileId;
use crate::material::{CommodityKey, MaterialId};
use crate::registry::Registries;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::surface::{SurfaceResourceId, SurfaceResourceLifecycle};

mod acoustics;
mod defense;
mod feeding;
mod junkworks;
mod mobilization;
mod patrol;
mod scavenging;
mod scrap_tools;
mod temple_archivist;
mod temple_guard;
mod tracks;
mod validation;
mod visibility;
mod watchman;

pub use junkworks::{
    KoboldAlarmPulse, KoboldJunkRig, KoboldJunkRigKind, available_nearby_kobold_junk_relay,
    available_nearby_kobold_junk_rig,
};
pub use scrap_tools::{
    KoboldPryHeadKind, KoboldPryWorkSighting, KoboldScrapLever, available_nearby_kobold_pry_work,
    available_nearby_kobold_scrap_lever,
};
pub use tracks::{KoboldTrack, KoboldTrailKind, available_nearby_kobold_tracks};

pub(crate) use patrol::apply_kobold_scout_tick;
#[cfg(test)]
use scavenging::den_cache_space;
pub(crate) use validation::validate_loaded_kobolds;
use visibility::scout_visible_from;

const FIRST_SCOUT_TICK: u64 = 180;
const SCOUT_STEP_TICKS: u64 = 5;
/// One scout moves a portable parcel, not an arbitrarily heavy abandoned bulk pile.
const SCOUT_CARRY_LIMIT: Mass = Mass::from_milligrams(1_000_000);
/// Bare-handed scavengers cannot safely retrieve hot workshop output.
const SCOUT_SAFE_SCAVENGE_TEMPERATURE: Temperature = Temperature::from_millikelvin(333_150);
/// Limited loose staging space at the den; source scarcity alone is not its capacity.
const DEN_LOOSE_CACHE_LIMIT: Mass = Mass::from_milligrams(500_000);
const SCOUT_FORAGE_BATCH: Mass = Mass::from_milligrams(100_000);
/// Scavengers can divide an exposed bulk parcel, but only into small carried lots.
const SCOUT_BULK_SPLIT_BATCH: Mass = Mass::from_milligrams(250_000);
const MOBILIZED_BULK_SPLIT_BATCH: Mass = Mass::from_milligrams(500_000);
const MOBILIZED_FORAGE_BATCH: Mass = Mass::from_milligrams(200_000);
const MOBILIZED_FORAGE_INTERVAL_TICKS: u64 = 50;
/// Avoid stripping a food patch on every 72-second scout circuit.
const SCOUT_FORAGE_INTERVAL_TICKS: u64 = 100;
/// One 20 g meal every 24 physical minutes is about 1.2 kg/day for a small
/// humanoid. Food stays recoverable between visits and frees finite den space.
const SCOUT_MEAL_INTERVAL_TICKS: u64 = 400;
const SCOUT_MEAL_MASS: Mass = Mass::from_milligrams(20_000);
/// A chased scout stays wary for 2.4 simulation minutes after the most recent threat.
const SCOUT_WARY_INTERVAL_TICKS: u64 = 40;
/// Stance is a transient fraction of secure footing, never tissue damage or virtual hit points.
const FULL_BALANCE_PPM: u32 = 1_000_000;
pub(crate) const STAGGERED_BELOW_PPM: u32 = 350_000;
/// One recovery increment per five slow ticks; a heavily staggered resident needs real time.
const BALANCE_RECOVERY_PPM_PER_STEP: u32 = 120_000;
/// A low, dying ember does not repel the same scout as a maintained bright fire.
const SCOUT_DETERRENT_LIGHT_LEVEL: u8 = 3;
/// The second opening resident can watch the junction for twenty slow ticks per full meal.
const WATCH_DURATION_TICKS: u64 = 20;
/// Bounded recuperation between patrols; no free watch duty every scout step.
const WATCH_RECOVERY_TICKS: u64 = 100;

/// Identity of a locally seen resident, including the fixed den watchman.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KoboldRole {
    Scout,
    Watchman,
    /// A large, armored temple-bred resident; no combat system is claimed.
    TempleWarden,
    /// A longer-lived heavy caste attending the surviving mnemonic machinery.
    TempleArchivist,
}

/// Distinct physical presence, independent of an unimplemented damage/combat model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KoboldStature {
    Small,
    Heavy,
    Massive,
}

/// Player-local observation of a physical kobold; remote dens and patrols are not exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldSighting {
    role: KoboldRole,
    position: VoxelCoord,
    has_noticed_player_drop: bool,
    is_wary: bool,
    is_defending: bool,
    balance_ppm: Option<u32>,
    mobilized: bool,
    scavenged_material: Option<(CommodityKey, Mass)>,
}

impl KoboldSighting {
    #[must_use]
    pub const fn role(self) -> KoboldRole {
        self.role
    }

    #[must_use]
    pub const fn balance_ppm(self) -> Option<u32> {
        self.balance_ppm
    }

    #[must_use]
    pub const fn is_staggered(self) -> bool {
        match self.balance_ppm {
            Some(balance) => balance < STAGGERED_BELOW_PPM,
            None => false,
        }
    }

    #[must_use]
    pub const fn stature(self) -> KoboldStature {
        match self.role {
            KoboldRole::Scout | KoboldRole::Watchman => KoboldStature::Small,
            KoboldRole::TempleWarden => KoboldStature::Heavy,
            KoboldRole::TempleArchivist => KoboldStature::Massive,
        }
    }

    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub const fn has_noticed_player_drop(self) -> bool {
        self.has_noticed_player_drop
    }

    /// Scouts avoid recent danger; wardens guard visibly breached machinery
    /// or heed a directly observed archivist's damaged-archive vigil.
    #[must_use]
    pub const fn is_wary(self) -> bool {
        self.is_wary
    }

    /// Visible guarding includes a fitted scout at a threatened industrial
    /// junction, a temple warden's challenge or an archivist's archive vigil.
    /// It conveys physical readiness, not a remote combat buff.
    #[must_use]
    pub const fn is_defending(self) -> bool {
        self.is_defending
    }

    /// An actual local sighting of engineered machinery mobilized this colony.
    #[must_use]
    pub const fn is_mobilized(self) -> bool {
        self.mobilized
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
    raided_stockpile: Option<(StockpileId, Mass)>,
    relocated_drop: Option<SurfaceResourceId>,
    fled_player: bool,
    fled_light: bool,
    turned_back_at_barrier: bool,
    cancelled_empty_cache_raid: bool,
    abandoned_drop: Option<SurfaceResourceId>,
    foraged_food: Option<(SurfaceResourceId, Mass)>,
    split_drop: Option<(SurfaceResourceId, Mass)>,
    consumed_food: Option<(SurfaceResourceId, Mass)>,
    discarded_food: Option<SurfaceResourceId>,
    cleared_cache: Option<SurfaceResourceId>,
    heard_junk_alarm: bool,
    defensive_contact: Option<(SurfaceResourceId, u32)>,
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

    /// Finite matter taken from a visibly reached, unenclosed ground store.
    /// Extraction is a separate work step before the scout carries the parcel.
    #[must_use]
    pub const fn raided_stockpile(self) -> Option<(StockpileId, Mass)> {
        self.raided_stockpile
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

    /// A nearby scout consumed finite food at its den on this step.
    #[must_use]
    pub const fn consumed_food(self) -> Option<(SurfaceResourceId, Mass)> {
        self.consumed_food
    }

    /// Food unfit for consumption physically discarded onto the adjacent route.
    /// For bulky cargo, this names the moved split portion, not its source.
    #[must_use]
    pub const fn discarded_food(self) -> Option<SurfaceResourceId> {
        self.discarded_food
    }

    /// A blocked den released inert cargo for nearby food or visible salvage.
    /// For oversized cargo, this identifies the moved split portion, not its source.
    #[must_use]
    pub const fn cleared_cache(self) -> Option<SurfaceResourceId> {
        self.cleared_cache
    }

    /// A nearby scout heard a real den clatter-rig warning and changed course.
    #[must_use]
    pub const fn heard_junk_alarm(self) -> bool {
        self.heard_junk_alarm
    }
    /// The actual fitted head used and nonlethal vitality loss in ppm.
    #[must_use]
    pub const fn defensive_contact(self) -> Option<(SurfaceResourceId, u32)> {
        self.defensive_contact
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KoboldColony {
    /// The three authored patrol voxels never change after wilderness generation.
    route: [VoxelCoord; 3],
    /// One temporary branch diversion, valid only until the scout returns to the den.
    /// Persisted colony records must include this field even when its value is null.
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    detour: Option<VoxelCoord>,
    /// Second traversable branch cell when a scout investigates a farther exposed cache.
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    detour_goal: Option<VoxelCoord>,
    scout_step: u8,
    returning: bool,
    next_step_at: SimulationTick,
    noticed_player_drop: bool,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    scavenged_drop: Option<SurfaceResourceId>,
    /// Work spent fitting the exact copper head and handle, never virtual equipment.
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    pry_fitting: Option<scrap_tools::KoboldPryFitting>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    next_forage_at: Option<SimulationTick>,
    /// Regular meal opportunity, advanced only on the low-frequency scout clock.
    next_meal_at: SimulationTick,
    /// A meal was missed and the scout still needs recoverable food.
    /// This need survives save/load and clears only after an actual den meal.
    hungry: bool,
    /// Food actually eaten toward this outstanding 20 g ration. Required by
    /// persisted colony schema; missing values are rejected, never defaulted.
    partial_meal: Mass,
    /// A scout's first direct sighting of engineered machinery, still unreported until home.
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    witnessed_machinery_at: Option<SimulationTick>,
    /// Colony action changes only after the scout physically returns with that information.
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    mobilized_at: Option<SimulationTick>,
    /// The den's second, fixed resident watches the real junction after eating.
    /// Null means it is at home, not that a new kobold can be spawned.
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    watch_deployed_at: Option<SimulationTick>,
    /// Earliest possible next fed watch patrol; starts at the opening scout deadline.
    watch_ready_at: SimulationTick,
    /// Danger seen by the guard, not shared with a scout until both return home.
    watch_alarm_pending: bool,
    /// A delivered player warning authorizes one later, food-funded outer picket.
    watch_counterpatrol_pending: bool,
    /// 0 = junction, 1 = outer post, 2 = junction on actual return; home uses 0.
    watch_leg: u8,
    /// Only the latest real departed route steps; old traces expire on observation.
    tracks: Vec<KoboldTrack>,
    /// A separate small history of actual watchman steps, never invented idle spoor.
    watch_tracks: Vec<KoboldTrack>,
    /// Caution survives saves and expires on the same five-tick scout clock.
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    wary_until: Option<SimulationTick>,
    /// A real close-contact blow causes temporary caution, not imaginary HP loss.
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    scout_shoved_at: Option<ControlTick>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    watch_shoved_at: Option<ControlTick>,
    /// Required current-schema physical stance, not replenishable health or blood.
    scout_balance_ppm: u32,
    watch_balance_ppm: u32,
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

    fn seeks_food_at_junction(&self) -> bool {
        self.hungry && self.scout_step == 1 && !self.returning
    }

    fn can_forage_on_return(&self, next_step: u8) -> bool {
        // A distant patch is harvested only during the actual goal-to-first
        // return leg; later legs carry the exact same newly materialized parcel.
        next_step <= 1 || (self.scout_step == 3 && next_step == 2)
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
        if let Some(detour) = self.detour
            && ((self.scout_step != 2
                && self.scout_step != 3
                && !(self.scout_step == 1 && self.returning))
                || detour == self.route[0]
                || detour == self.route[1]
                || detour == self.route[2]
                || !state.logistics().has_traversal_edge(self.route[1], detour))
        {
            return Err("invalid kobold temporary detour or traversal edge");
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

    fn balance(&self, role: KoboldRole) -> u32 {
        match role {
            KoboldRole::Scout => self.scout_balance_ppm,
            KoboldRole::Watchman => self.watch_balance_ppm,
            KoboldRole::TempleWarden | KoboldRole::TempleArchivist => {
                unreachable!("only opening residents have provisional balance")
            }
        }
    }

    fn is_staggered(&self, role: KoboldRole) -> bool {
        self.balance(role) < STAGGERED_BELOW_PPM
    }

    fn recover_balance(&mut self) {
        self.scout_balance_ppm = self
            .scout_balance_ppm
            .saturating_add(BALANCE_RECOVERY_PPM_PER_STEP)
            .min(FULL_BALANCE_PPM);
        self.watch_balance_ppm = self
            .watch_balance_ppm
            .saturating_add(BALANCE_RECOVERY_PPM_PER_STEP)
            .min(FULL_BALANCE_PPM);
    }

    fn recently_shoved(&self, role: KoboldRole, now: ControlTick) -> bool {
        let at = match role {
            KoboldRole::Scout => self.scout_shoved_at,
            KoboldRole::Watchman => self.watch_shoved_at,
            KoboldRole::TempleWarden | KoboldRole::TempleArchivist => None,
        };
        at.is_some_and(|at| {
            now.checked_steps_since(at)
                .is_some_and(|elapsed| elapsed < 1_800)
        })
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
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    colony: Option<KoboldColony>,
    /// Only off-post wardens need persistent cursors. Absence means the
    /// resident remains at its original authored hall cell.
    temple_wardens: Vec<temple_guard::TempleWardenRecord>,
    /// Only archivists physically attending the archive shell need a cursor.
    temple_archivists: Vec<temple_archivist::TempleArchivistRecord>,
    /// Terminal consumed-food custody retained by original material, not deleted matter.
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    consumed_matter: BTreeMap<MaterialId, AggregateMass>,
}

impl KoboldState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            colony: None,
            temple_wardens: Vec::new(),
            temple_archivists: Vec::new(),
            consumed_matter: BTreeMap::new(),
        }
    }

    #[must_use]
    pub(crate) fn is_pristine(&self) -> bool {
        self.colony.is_none()
            && self.temple_wardens.is_empty()
            && self.temple_archivists.is_empty()
            && self.consumed_matter.is_empty()
    }

    pub(crate) fn consumed_matter(&self) -> impl Iterator<Item = (MaterialId, AggregateMass)> + '_ {
        self.consumed_matter.iter().map(|(id, mass)| (*id, *mass))
    }

    fn temple_warden_step(&self, chunk: crate::world::ChunkCoord) -> u8 {
        self.temple_wardens
            .binary_search_by_key(&chunk, |resident| resident.chunk)
            .ok()
            .map_or(0, |index| self.temple_wardens[index].step)
    }

    fn temple_archivist_step(&self, chunk: crate::world::ChunkCoord) -> u8 {
        self.temple_archivists
            .binary_search_by_key(&chunk, |resident| resident.chunk)
            .ok()
            .map_or(0, |index| self.temple_archivists[index].step)
    }

    fn apply_meal(&mut self, material: MaterialId, total: AggregateMass) {
        self.consumed_matter.insert(material, total);
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
            pry_fitting: None,
            next_forage_at: None,
            next_meal_at: SimulationTick::new(FIRST_SCOUT_TICK + SCOUT_MEAL_INTERVAL_TICKS),
            hungry: false,
            partial_meal: Mass::ZERO,
            witnessed_machinery_at: None,
            mobilized_at: None,
            watch_deployed_at: None,
            watch_ready_at: SimulationTick::new(FIRST_SCOUT_TICK),
            watch_alarm_pending: false,
            watch_counterpatrol_pending: false,
            watch_leg: 0,
            tracks: Vec::new(),
            watch_tracks: Vec::new(),
            wary_until: None,
            scout_shoved_at: None,
            watch_shoved_at: None,
            scout_balance_ppm: FULL_BALANCE_PPM,
            watch_balance_ppm: FULL_BALANCE_PPM,
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
                role: KoboldRole::Scout,
                position: colony.scout_position(),
                has_noticed_player_drop: colony.noticed_player_drop,
                is_wary: colony.wary_until.is_some_and(|until| until > state.tick())
                    || colony.recently_shoved(KoboldRole::Scout, state.control_tick()),
                is_defending: defense::scout_defends_visible_worksite(registries, state, colony),
                balance_ppm: Some(colony.scout_balance_ppm),
                mobilized: colony.mobilized_at.is_some(),
                scavenged_material: colony
                    .scavenged_drop
                    .and_then(|id| state.surface().get(id))
                    .filter(|drop| drop.lifecycle() == SurfaceResourceLifecycle::Available)
                    .map(|drop| (drop.commodity(), drop.remaining_mass())),
            })
            .chain(self.colony.iter().filter_map(move |colony| {
                let guard = colony.watch_position();
                scout_visible_from(registries, state, guard, position).then_some(KoboldSighting {
                    role: KoboldRole::Watchman,
                    position: guard,
                    has_noticed_player_drop: false,
                    is_wary: colony.watch_alarm_pending
                        || colony.recently_shoved(KoboldRole::Watchman, state.control_tick()),
                    is_defending: false,
                    balance_ppm: Some(colony.watch_balance_ppm),
                    mobilized: colony.mobilized_at.is_some(),
                    scavenged_material: None,
                })
            }))
            .chain(
                crate::world::nearby_temple_residents(registries, state, position)
                    .flat_map(move |[post, archivist]| {
                        let step =
                            self.temple_warden_step(crate::world::ChunkCoord::containing(post));
                        let route = crate::world::temple_warden_route_at(registries, state, post)
                            .expect("generated temple residents retain a real warden route");
                        let warden = temple_guard::warden_position(route, step)
                            .expect("trusted temple cursor names a real cell");
                        let archivist_step =
                            self.temple_archivist_step(crate::world::ChunkCoord::containing(post));
                        let archivist_actual =
                            temple_archivist::archivist_position(archivist, archivist_step)
                                .expect("trusted archive cursor names a physical cell");
                        [
                            (
                                warden,
                                KoboldRole::TempleWarden,
                                step != 0
                                    && temple_guard::warden_is_challenging(
                                        registries, state, post, warden, position,
                                    ),
                            ),
                            (
                                archivist_actual,
                                KoboldRole::TempleArchivist,
                                temple_archivist::archivist_is_guarding_breach(
                                    registries,
                                    state,
                                    archivist,
                                    archivist_step,
                                    position,
                                ),
                            ),
                        ]
                    })
                    .filter(move |(resident, role, _)| match role {
                        KoboldRole::TempleArchivist => temple_archivist::archivist_visible_from(
                            registries, state, *resident, position,
                        ),
                        KoboldRole::Scout | KoboldRole::Watchman | KoboldRole::TempleWarden => {
                            scout_visible_from(registries, state, *resident, position)
                        }
                    })
                    .map(|(position, role, is_defending)| KoboldSighting {
                        role,
                        position,
                        has_noticed_player_drop: false,
                        is_wary: role == KoboldRole::TempleWarden
                            && temple_guard::warden_has_damage_warning(registries, state, position),
                        is_defending,
                        balance_ppm: None,
                        mobilized: false,
                        scavenged_material: None,
                    }),
            )
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

/// Selects only the physical opening residents occupying an actual ray cell.
/// Co-located scout and watchman at the den are deliberately ambiguous.
pub(crate) fn close_contact_at(
    state: &AppState,
    voxel: VoxelCoord,
) -> Result<Option<(KoboldRole, VoxelCoord)>, ()> {
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return Ok(None);
    };
    let occupies = |anchor: VoxelCoord| {
        voxel.x() == anchor.x()
            && voxel.z() == anchor.z()
            && (voxel.y() == anchor.y() || anchor.y().checked_add(1) == Some(voxel.y()))
    };
    match (
        occupies(colony.scout_position()),
        occupies(colony.watch_position()),
    ) {
        (true, true) => Err(()),
        (true, false) => Ok(Some((KoboldRole::Scout, colony.scout_position()))),
        (false, true) => Ok(Some((KoboldRole::Watchman, colony.watch_position()))),
        (false, false) => Ok(None),
    }
}

/// Exposes only the unique opening victim's stance to the already-local combat operation.
pub(crate) fn opening_balance(state: &AppState, role: KoboldRole) -> u32 {
    state
        .kobold_state()
        .colony
        .as_ref()
        .expect("validated opening contact requires colony")
        .balance(role)
}

/// Changes only the struck resident's local caution and footing after validated contact.
/// Equipment wear and attack tempo remain owned by combat/equipment.
pub(crate) fn record_close_contact(state: &mut AppState, role: KoboldRole, impulse_ppm: u32) {
    let at = state.control_tick();
    let colony = state
        .kobold_state_mut()
        .colony
        .as_mut()
        .expect("validated opening resident");
    assert!((1..=500_000).contains(&impulse_ppm));
    match role {
        KoboldRole::Scout => {
            colony.scout_shoved_at = Some(at);
            colony.scout_balance_ppm = colony.scout_balance_ppm.saturating_sub(impulse_ppm);
        }
        KoboldRole::Watchman => {
            colony.watch_shoved_at = Some(at);
            colony.watch_balance_ppm = colony.watch_balance_ppm.saturating_sub(impulse_ppm);
        }
        KoboldRole::TempleWarden | KoboldRole::TempleArchivist => {
            unreachable!("primitive contact targets only opening residents")
        }
    }
}

/// Physical occupancy of the two small opening residents, independently of
/// sight, warning state and surface cargo. Both stand two cells tall; the
/// watchman has his own saved patrol position even while the scout is away.
pub(crate) fn opening_kobold_positions(state: &AppState) -> Option<[VoxelCoord; 2]> {
    let colony = state.kobold_state().colony.as_ref()?;
    Some([colony.scout_position(), colony.watch_position()])
}

pub(crate) fn opening_kobold_occupies_voxel(state: &AppState, voxel: VoxelCoord) -> bool {
    let Some(residents) = opening_kobold_positions(state) else {
        return false;
    };
    residents.into_iter().any(|resident| {
        voxel.x() == resident.x()
            && voxel.z() == resident.z()
            && (voxel.y() == resident.y() || resident.y().checked_add(1) == Some(voxel.y()))
    })
}

/// Materialization checks generated temple caste bodies and the opening
/// residents. Narrow posts can share the small caste's cells; temple castes
/// reject all embodied structural members.
pub(crate) fn first_kobold_member_overlap(
    registries: &Registries,
    state: &AppState,
    bounds: VoxelBounds,
    full_footprint: bool,
) -> Option<VoxelCoord> {
    if let Some(cell) = temple_guard::first_temple_member_overlap(registries, state, bounds) {
        return Some(cell);
    }
    if !full_footprint {
        return None;
    }
    let colony = state.kobold_state().colony.as_ref()?;
    for resident in [colony.scout_position(), colony.watch_position()] {
        for dy in 0..=1 {
            let cell = VoxelCoord::new(resident.x(), resident.y().checked_add(dy)?, resident.z());
            if bounds.has_voxel(cell) {
                return Some(cell);
            }
        }
    }
    None
}

/// Authoritative occupancy in this generated voxel, independent of local
/// sight. A concealed caste is still solid; no remote-creature scan occurs.
pub(crate) fn temple_caste_occupies_voxel(
    registries: &Registries,
    state: &AppState,
    voxel: VoxelCoord,
) -> bool {
    let Some([post, archivist]) = crate::world::temple_residents_at(registries, state, voxel)
    else {
        return false;
    };
    let step = state
        .kobold_state()
        .temple_warden_step(crate::world::ChunkCoord::containing(post));
    let Some(route) = crate::world::temple_warden_route_at(registries, state, post) else {
        return false;
    };
    let Some(warden) = temple_guard::warden_position(route, step) else {
        return false;
    };
    let Some(archivist_actual) = temple_archivist::archivist_position(
        archivist,
        state
            .kobold_state()
            .temple_archivist_step(crate::world::ChunkCoord::containing(post)),
    ) else {
        return false;
    };
    initial_temple_caste_occupies_voxel(voxel, warden, archivist_actual)
}

/// Pure body geometry shared by existing castes and the not-yet-instantiated
/// residents of a prospective generated chunk. No generated-world profile
/// or visibility observation is required to check the latter.
pub(crate) fn initial_temple_caste_occupies_voxel(
    voxel: VoxelCoord,
    warden: VoxelCoord,
    archivist: VoxelCoord,
) -> bool {
    let heavy_guard = voxel.x() == warden.x()
        && voxel.z() == warden.z()
        && warden
            .y()
            .checked_add(1)
            .is_some_and(|head| (warden.y()..=head).contains(&voxel.y()));
    // The old archivist caste is truly larger: two-by-two hall cells across,
    // three cells high, all inside the architect's existing excavated chamber.
    // The combat model remains absent, but the body footprint is real.
    let massive_archivist = (voxel.x() == archivist.x()
        || archivist.x().checked_add(1) == Some(voxel.x()))
        && (voxel.z() == archivist.z() || archivist.z().checked_sub(1) == Some(voxel.z()))
        && archivist
            .y()
            .checked_add(2)
            .is_some_and(|head| (archivist.y()..=head).contains(&voxel.y()));
    heavy_guard || massive_archivist
}

#[cfg(test)]
#[path = "den_care_tests.rs"]
mod den_care_tests;
#[cfg(test)]
#[path = "foraging_freshness_tests.rs"]
mod foraging_freshness_tests;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
