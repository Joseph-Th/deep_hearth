//! Local sensory changes, resident-watch attention, and survival urgency.
//!
//! All snapshots use player-visible production read models. A watch responds to
//! changed threats, not routine patrol motion or irrelevant weather churn.

use deep_hearth::core::quantity::Temperature;
use deep_hearth::core::time::Season;
use deep_hearth::kobolds::{
    KoboldFootfall, KoboldRole, KoboldSighting, KoboldTrack, available_nearby_kobold_footfalls,
    available_nearby_kobold_scouts, available_nearby_kobold_tracks,
};
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::surface::AdjacentSurfaceResource;
use deep_hearth::survival::{
    HungerState, HydrationState, LocalDrinkSource, ThermalComfortState, Vitality, assess_survival,
    available_local_drink_sources,
};
use deep_hearth::world::{
    DayPhase, ForageScent, VisibleWaterSource, WindCondition, WindDirection,
    assess_player_forage_scents, assess_player_weather, available_visible_generated_water,
};
use deep_hearth::{AppState, Registries};

/// Stable observable weather, excluding the front countdown which changes
/// every tick without implying a change in actual conditions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocalWeatherCue {
    season: Season,
    phase: DayPhase,
    ambient_temperature: Temperature,
    wind: WindCondition,
    wind_direction: Option<WindDirection>,
}

/// Player-visible cues only. Routine patrol traces can interrupt ordinary short
/// waits without repeatedly disrupting a requested work completion.
#[derive(Clone, PartialEq, Eq)]
pub struct LocalCues {
    resources: Vec<AdjacentSurfaceResource>,
    water: Vec<VisibleWaterSource>,
    local_water: Vec<LocalDrinkSource>,
    kobolds: Vec<KoboldSighting>,
    footfalls: Vec<KoboldFootfall>,
    tracks: Vec<KoboldTrack>,
    scents: Vec<ForageScent>,
    weather: Option<LocalWeatherCue>,
}

impl LocalCues {
    /// Returns the most actionable changed cue for a passive local observation.
    pub fn changed(&self, next: &Self) -> Option<&'static str> {
        if self.kobolds != next.kobolds {
            Some("Nearby resident sightings have changed.")
        } else if self.footfalls != next.footfalls {
            Some("Nearby footsteps have changed.")
        } else if self.tracks != next.tracks {
            Some("Nearby tracks have changed.")
        } else if self.resources != next.resources {
            Some("Nearby loose resources have changed.")
        } else if self.local_water != next.local_water {
            Some("Drinkable water at your position has changed.")
        } else if self.water != next.water {
            Some("Nearby water sightings have changed.")
        } else if self.scents != next.scents {
            Some("Scents in the area have shifted.")
        } else if self.weather != next.weather {
            Some("The wind or weather has shifted.")
        } else {
            None
        }
    }

    /// Long watches ignore routine patrol movement, footsteps, tracks, and weather.
    /// The stepping owner handles real events, extinguished fires, and survival
    /// warnings separately; only a new close approach or defensive posture
    /// requires attention from the otherwise passive local observations.
    pub fn watch_attention(&self, next: &Self, player: VoxelCoord) -> Option<&'static str> {
        resident_watch_attention(&self.kobolds, &next.kobolds, player, player)
    }

    /// During hand work, ignore passive patrol motion, traces, and drifting
    /// scents. A new close approach, defensive resident, or changing physical
    /// supply remains actionable; canonical events and survival warnings are
    /// handled by the control-step owner independently of these observations.
    pub fn work_attention(&self, next: &Self, player: VoxelCoord) -> Option<&'static str> {
        self.watch_attention(next, player).or_else(|| {
            if self.resources != next.resources {
                Some("Nearby loose resources have changed.")
            } else if self
                .local_water
                .iter()
                .map(|source| source.store())
                .ne(next.local_water.iter().map(|source| source.store()))
            {
                Some("Drinkable water at your position has changed.")
            } else if next.water.iter().any(|source| !self.water.contains(source)) {
                Some("New nearby water is visible.")
            } else {
                None
            }
        })
    }
}

/// Passive daylight watches only need visible resident behavior. Avoid a full
/// food, water, track, scent, and weather survey at every slow boundary.
pub fn local_resident_sightings(registries: &Registries, state: &AppState) -> Vec<KoboldSighting> {
    available_nearby_kobold_scouts(registries, state).collect()
}

/// Successive local observations for commands that deliberately ignore
/// departures and ordinary patrol motion. A departure must still reset the
/// baseline so that a later close approach or defensive posture is noticed.
pub struct ResidentWatch {
    position: VoxelCoord,
    residents: Vec<KoboldSighting>,
}

impl ResidentWatch {
    pub fn capture(registries: &Registries, state: &AppState, position: VoxelCoord) -> Self {
        Self {
            position,
            residents: local_resident_sightings(registries, state),
        }
    }

    pub fn observe(&mut self, next: Self) -> Option<&'static str> {
        let attention = resident_watch_attention(
            &self.residents,
            &next.residents,
            self.position,
            next.position,
        );
        *self = next;
        attention
    }
}

/// Watch snapshots only expose role, physical position, and guarding posture.
/// Same-role head counts do not identify who entered or became defensive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResidentAttentionRecord {
    role: KoboldRole,
    position: VoxelCoord,
    defending: bool,
}

impl From<KoboldSighting> for ResidentAttentionRecord {
    fn from(sighting: KoboldSighting) -> Self {
        Self {
            role: sighting.role(),
            position: sighting.position(),
            defending: sighting.is_defending(),
        }
    }
}

/// One observable resident and whether it satisfies the current attention rule.
/// Identity is intentionally not exposed by the sighting projection.
type QualifiedResident = (ResidentAttentionRecord, bool);

/// Reserve stationary matches globally, including residents that no longer qualify.
/// For co-located same-role sightings, preserve equal qualification first: their
/// individual identities cannot be distinguished by observation.
fn reserve_stationary_matches(
    previous: &mut [Option<QualifiedResident>],
    next: &mut [Option<QualifiedResident>],
) -> bool {
    for same_qualification in [true, false] {
        for sighting in &mut *next {
            let Some((current, qualifies)) = *sighting else {
                continue;
            };
            let matched = previous.iter_mut().find(|old| {
                old.is_some_and(|(record, qualified)| {
                    record.role == current.role
                        && record.position == current.position
                        && (!same_qualification || qualified == qualifies)
                })
            });
            let Some(old) = matched else {
                continue;
            };
            let (_, was_qualified) = old.take().expect("matched resident exists");
            if qualifies && !was_qualified {
                return true;
            }
            *sighting = None;
        }
    }
    false
}

/// Find a one-to-one explanation for a moved sighting. Reassign an earlier
/// ambiguous match if necessary instead of letting iteration order create
/// a false warning. Every recursive step visits a previously unvisited old
/// sighting, so the search is bounded by the actual local observations.
fn match_patrol_step(
    previous: &[ResidentAttentionRecord],
    next: &[ResidentAttentionRecord],
    current: usize,
    assignments: &mut [Option<usize>],
    visited: &mut [bool],
) -> bool {
    let sighting = next[current];
    for (index, old) in previous.iter().enumerate() {
        if visited[index]
            || old.role != sighting.role
            || !old.position.is_face_adjacent_to(sighting.position)
        {
            continue;
        }
        visited[index] = true;
        if assignments[index].is_none_or(|assigned| {
            match_patrol_step(previous, next, assigned, assignments, visited)
        }) {
            assignments[index] = Some(current);
            return true;
        }
    }
    false
}

/// Exact-position matches take precedence over plausible one-step patrol motion.
/// Each old sighting can explain at most one new sighting, regardless of order.
fn newly_qualifying_resident<T: Copy + Into<ResidentAttentionRecord>>(
    previous: &[T],
    next: &[T],
    before_qualifies: impl Fn(ResidentAttentionRecord) -> bool,
    after_qualifies: impl Fn(ResidentAttentionRecord) -> bool,
) -> bool {
    let mut previous: Vec<_> = previous
        .iter()
        .copied()
        .map(Into::into)
        .map(|record| Some((record, before_qualifies(record))))
        .collect();
    let mut next: Vec<_> = next
        .iter()
        .copied()
        .map(Into::into)
        .map(|record| Some((record, after_qualifies(record))))
        .collect();
    if reserve_stationary_matches(&mut previous, &mut next) {
        return true;
    }
    let remaining_qualified = |records: Vec<Option<QualifiedResident>>| {
        records
            .into_iter()
            .flatten()
            .filter_map(|(record, qualifies)| qualifies.then_some(record))
            .collect::<Vec<_>>()
    };
    let previous = remaining_qualified(previous);
    let next = remaining_qualified(next);
    if next.len() > previous.len() {
        return true;
    }
    let mut assignments = vec![None; previous.len()];
    let mut visited = vec![false; previous.len()];
    for current in 0..next.len() {
        visited.fill(false);
        if !match_patrol_step(&previous, &next, current, &mut assignments, &mut visited) {
            return true;
        }
    }
    false
}

fn resident_attention<T: Copy + Into<ResidentAttentionRecord>>(
    previous: &[T],
    next: &[T],
    previous_player: VoxelCoord,
    player: VoxelCoord,
) -> Option<&'static str> {
    let near = |position: VoxelCoord, observer: VoxelCoord| {
        position.y().abs_diff(observer.y()) <= 1
            && position.x().abs_diff(observer.x()) <= 1
            && position.z().abs_diff(observer.z()) <= 1
    };
    if newly_qualifying_resident(
        previous,
        next,
        |record| record.defending,
        |record| record.defending,
    ) {
        Some("A nearby resident is visibly defensive.")
    } else if newly_qualifying_resident(
        previous,
        next,
        |record| near(record.position, previous_player),
        |record| near(record.position, player),
    ) {
        Some("A resident is now visible near your immediate position.")
    } else {
        None
    }
}

/// Compare the same real resident observations used by work and daylight watches.
/// Walking patrols remain quiet; a replacement near resident or defender alerts.
pub fn resident_watch_attention(
    previous: &[KoboldSighting],
    next: &[KoboldSighting],
    previous_player: VoxelCoord,
    player: VoxelCoord,
) -> Option<&'static str> {
    resident_attention(previous, next, previous_player, player)
}

/// Do not inspect private geology, colony schedules, or all generated blocks to stop a wait.
pub fn local_cues(registries: &Registries, state: &AppState) -> LocalCues {
    LocalCues {
        resources: AdjacentSurfaceResource::visible_in_generated_world(registries, state).collect(),
        water: available_visible_generated_water(registries, state),
        local_water: available_local_drink_sources(registries, state).collect(),
        kobolds: local_resident_sightings(registries, state),
        footfalls: available_nearby_kobold_footfalls(registries, state),
        tracks: available_nearby_kobold_tracks(registries, state).collect(),
        scents: assess_player_forage_scents(registries, state),
        weather: assess_player_weather(registries, state).map(|weather| LocalWeatherCue {
            season: weather.season(),
            phase: weather.phase(),
            ambient_temperature: weather.ambient_temperature(),
            wind: weather.wind(),
            wind_direction: weather.wind_direction(),
        }),
    }
}

/// Threshold transitions, not tiny per-tick resource changes, interrupt travel
/// and long waits. A worsening warning (hunger -> starvation, etc.) is a
/// separate change even when a warning was already present.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Urgency {
    hunger: HungerState,
    hydration: HydrationState,
    thermal: ThermalComfortState,
    injured: bool,
    dead: bool,
}

impl Urgency {
    pub fn requires_provisions_or_warmth(self) -> bool {
        self.hunger != HungerState::Fed
            || self.hydration != HydrationState::Hydrated
            || self.thermal != ThermalComfortState::Comfortable
            || self.dead
    }
}

pub fn urgency(registries: &Registries, state: &AppState) -> Option<Urgency> {
    let assessment = assess_survival(registries, state)?;
    Some(Urgency {
        hunger: assessment.hunger(),
        hydration: assessment.hydration_state(),
        thermal: assessment.thermal_comfort_state(),
        injured: assessment.vitality() < Vitality::MAXIMUM,
        dead: assessment.vitality() == Vitality::ZERO,
    })
}

#[cfg(test)]
#[path = "attention_tests.rs"]
mod tests;
