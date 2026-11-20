//! Visible, renderer-neutral readout only. Never inspect undiscovered world resources.

use std::fmt::Write as _;

use super::activity;
mod actor_inventory;
mod sensory;

use actor_inventory::hand_status;
pub use actor_inventory::{hand_label, inventory};
use deep_hearth::content::{FORM_BOARD, MATERIAL_WOOD};
use deep_hearth::core::quantity::{Mass, Temperature, Volume};
use deep_hearth::core::time::{
    CONTROL_TICK_DURATION, CONTROL_TICKS_PER_SIMULATION_TICK, Season, TickSpan,
};
use deep_hearth::energy::project_player_nearby_warmth_sources;
use deep_hearth::kobolds::{
    KoboldFootfall, KoboldSighting, KoboldTrack, available_nearby_kobold_footfalls,
    available_nearby_kobold_scouts, available_nearby_kobold_tracks,
};
use deep_hearth::logistics::{
    HorizontalControlInput, PlayerActiveSelection, PlayerOverheadCoverAssessment,
    assess_player_overhead_cover, project_player_climb_target, project_player_jump_rise_bound,
    project_player_level_approach, project_player_standing_space,
};
use deep_hearth::material::CommodityKey;
use deep_hearth::simulation::{
    SecondaryActionOutcome, project_pointed_structural_break_target,
    project_pointed_timber_cover_target, validate_pointed_terrain_excavation,
};
use deep_hearth::spatial::ViewDirection;
use deep_hearth::spatial::{HorizontalVelocity, VerticalVelocity, VoxelCoord};
use deep_hearth::structural::StructuralLifecycle;
use deep_hearth::surface::{
    AdjacentSurfaceResource, PlayerSurfaceGatheringOption, SurfaceGatheringError,
    SurfaceResourceKind, assess_affordable_player_gathering_mass,
    assess_player_surface_gathering_option_admission, available_visible_surface_landmarks,
    player_surface_gathering_options,
};
use deep_hearth::survival::{
    FoodFreshness, HungerState, HydrationState, LocalDrinkSource, SurvivalAssessment,
    ThermalComfortState, Vitality, assess_player_resting_provision_runway,
    assess_player_unheated_comfort_runway, assess_survival, available_local_drink_sources,
};
use deep_hearth::world::{
    DayPhase, ForageScent, HabitatKind, VisibleWaterSource, WindCondition, WindDirection,
    assess_player_forage_scents, assess_player_habitat, assess_player_weather,
    available_visible_generated_trees, available_visible_generated_water,
    available_visible_water_landmarks, raycast_player_block,
};
use deep_hearth::{AppState, Registries};

fn name<'a>(registries: &'a Registries, commodity: deep_hearth::material::CommodityKey) -> &'a str {
    registries
        .materials()
        .commodity_name(commodity)
        .unwrap_or("unidentified material")
}

/// Translate owner-reported construction constraints without exposing commodity IDs
/// or losing the exact represented material mass.
pub fn missing_construction_material(
    registries: &Registries,
    commodity: deep_hearth::material::CommodityKey,
    available: Mass,
    required: Mass,
) -> String {
    format!(
        "need {} g {} ({} g carried)",
        grams(required),
        name(registries, commodity),
        grams(available),
    )
}

pub fn unsafe_construction_material(
    registries: &Registries,
    commodity: deep_hearth::material::CommodityKey,
    temperature: deep_hearth::core::quantity::Temperature,
    maximum: deep_hearth::core::quantity::Temperature,
    activity: &str,
) -> String {
    format!(
        "{} is too hot for {activity} ({} C; maximum {} C)",
        name(registries, commodity),
        celsius(temperature),
        celsius(maximum),
    )
}

fn pos(position: VoxelCoord) -> String {
    format!("({},{},{})", position.x(), position.y(), position.z())
}

/// Preserve signed millidegrees at safety boundaries. Truncating toward zero
/// can report both a prohibited 60.001 C item and its 60 C limit as "60 C".
pub fn celsius(temperature: deep_hearth::core::quantity::Temperature) -> String {
    let millidegrees = i64::from(temperature.millikelvin()) - 273_150;
    let magnitude = millidegrees.unsigned_abs();
    let sign = if millidegrees < 0 { "-" } else { "" };
    let whole = magnitude / 1_000;
    let fractional = magnitude % 1_000;
    if fractional == 0 {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{fractional:03}")
            .trim_end_matches('0')
            .to_owned()
    }
}

/// Inventory and receipts must not silently discard sub-gram material.
pub fn grams(mass: Mass) -> String {
    let milligrams = mass.milligrams();
    let whole = milligrams / 1_000;
    let remainder = milligrams % 1_000;
    if remainder == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{remainder:03}")
            .trim_end_matches('0')
            .to_owned()
    }
}

pub fn milliliters(volume: Volume) -> String {
    let microliters = volume.microliters();
    let whole = microliters / 1_000;
    let remainder = microliters % 1_000;
    if remainder == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{remainder:03}")
            .trim_end_matches('0')
            .to_owned()
    }
}

/// Numbered exact-local drinking choices, never remote or unsafe water.
/// Choosing one still revalidates the actual finite source on the input tick.
pub fn drink_options(registries: &Registries, state: &AppState) -> String {
    let sources = available_local_drink_sources(registries, state).collect::<Vec<_>>();
    drink_sources_report(&sources)
        .unwrap_or_else(|| "No drinkable source at your current location.\n".to_owned())
}

fn drink_sources_report(sources: &[LocalDrinkSource]) -> Option<String> {
    if sources.is_empty() {
        return None;
    }
    let mut output = String::from("Drink here (drink N):\n");
    for (index, source) in sources.iter().enumerate() {
        let celsius = celsius(source.temperature());
        let _ = writeln!(
            output,
            "  {}. {} mL available | {} C",
            index + 1,
            milliliters(source.available_volume()),
            celsius,
        );
    }
    Some(output)
}

fn meters_from_nanometers(nanometers: i64) -> String {
    let absolute = i128::from(nanometers).abs();
    format!(
        "{}{}.{:03}",
        if nanometers < 0 { "-" } else { "" },
        absolute / 1_000_000_000,
        (absolute % 1_000_000_000) / 1_000_000,
    )
}

/// Relative displacement of a genuinely visible thing, not a reachable path.
fn bearing(from: VoxelCoord, toward: VoxelCoord) -> String {
    let axes = [
        (
            i128::from(toward.x()) - i128::from(from.x()),
            "east",
            "west",
        ),
        (
            i128::from(toward.z()) - i128::from(from.z()),
            "south",
            "north",
        ),
        (i128::from(toward.y()) - i128::from(from.y()), "up", "down"),
    ];
    let mut steps = Vec::new();
    for (offset, positive, negative) in axes {
        if offset > 0 {
            steps.push(format!("{offset} {positive}"));
        } else if offset < 0 {
            steps.push(format!("{} {negative}", -offset));
        }
    }
    if steps.is_empty() {
        "here".to_owned()
    } else {
        steps.join(", ")
    }
}

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

/// Compare the same authoritative resident observations used by work waits.
pub fn resident_watch_attention(
    previous: &[KoboldSighting],
    next: &[KoboldSighting],
    previous_player: VoxelCoord,
    player: VoxelCoord,
) -> Option<&'static str> {
    let near = |sighting: &KoboldSighting, player: VoxelCoord| {
        let position = sighting.position();
        position.y().abs_diff(player.y()) <= 1
            && position.x().abs_diff(player.x()) <= 1
            && position.z().abs_diff(player.z()) <= 1
    };
    // Role names are not resident identities. Multiple temples can have
    // residents of the same caste in sight, so one already-close or
    // defending resident must not conceal a second one's approach.
    for (index, current) in next.iter().enumerate() {
        let role = current.role();
        if next[..index].iter().any(|earlier| earlier.role() == role) {
            continue;
        }
        let before = previous
            .iter()
            .filter(|old| old.role() == role && old.is_defending())
            .count();
        let after = next
            .iter()
            .filter(|seen| seen.role() == role && seen.is_defending())
            .count();
        if after > before {
            return Some("A nearby resident is visibly defensive.");
        }
        let before = previous
            .iter()
            .filter(|old| old.role() == role && near(old, previous_player))
            .count();
        let after = next
            .iter()
            .filter(|seen| seen.role() == role && near(seen, player))
            .count();
        if after > before {
            return Some("A resident is now visible near your immediate position.");
        }
    }
    None
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

/// Never render a living player as zero vitality. Preserve ordinary
/// whole-percent labels outside the final one percent.
fn vitality_label(ppm: u32) -> String {
    match ppm {
        0 => "0".to_owned(),
        1..=99 => "<0.01".to_owned(),
        100..=9_999 => format!("0.{:02}", ppm / 100),
        _ => (ppm / 10_000).to_string(),
    }
}

fn reserve_label(percent: u8, exhausted: bool) -> String {
    if percent == 0 && !exhausted {
        "<1".to_owned()
    } else {
        percent.to_string()
    }
}

fn survival_status(registries: &Registries, state: &AppState) -> String {
    let mut output = String::new();
    if let Some(survival) = assess_survival(registries, state) {
        let (food_percent, water_percent) = survival.reserve_percentages(registries);
        let _ = writeln!(
            output,
            "Condition: vitality {}%, food reserve {}%, hydration {}%, warmth {}%",
            vitality_label(survival.vitality().parts_per_million()),
            reserve_label(food_percent, survival.metabolic_energy().is_zero()),
            reserve_label(water_percent, survival.hydration().is_zero()),
            reserve_label(
                (survival.thermal_comfort_ppm() / 10_000) as u8,
                survival.thermal_comfort_ppm() == 0,
            ),
        );
        if survival.vitality() == Vitality::ZERO {
            output.push_str(
                "You are dead. Use respawn to return; your old pack remains at the death cache.\n",
            );
        } else {
            let hungry = survival.hunger() != HungerState::Fed;
            let thirsty = survival.hydration_state() != HydrationState::Hydrated;
            let injured = survival.vitality() < Vitality::MAXIMUM;
            let cold = survival.thermal_comfort_state() != ThermalComfortState::Comfortable;
            let warnings = [
                (hungry, "hunger"),
                (thirsty, "thirst"),
                (injured, "injury"),
                (cold, "cold exposure"),
            ]
            .into_iter()
            .filter_map(|(active, label)| active.then_some(label))
            .collect::<Vec<_>>();
            if !warnings.is_empty() {
                let _ = writeln!(output, "Warnings: {}", warnings.join(", "));
            }
            if survival.hunger() == HungerState::Starving {
                output.push_str("Starvation: metabolic reserve exhausted. Uncovered metabolic costs cause vitality loss. Eat carried food or gather a small fresh edible portion at risk to vitality; other paid work may be impossible.\n");
            }
            if survival.hydration_state() == HydrationState::Dehydrated {
                output.push_str("Dehydration: water reserve exhausted. Uncovered hydration costs cause vitality loss. Drink local water or forage fresh edible food at risk to vitality.\n");
            }
            if injured {
                injury_recovery_report(registries, survival, &mut output);
            }
        }
    }
    output
}

fn injury_recovery_report(
    registries: &Registries,
    survival: SurvivalAssessment,
    output: &mut String,
) {
    let ppm = survival.vitality().parts_per_million();
    if ppm < 100 {
        output.push_str("Vitality: <0.01%\n");
    } else {
        let _ = writeln!(
            output,
            "Vitality: {}.{:02}%",
            ppm / 10_000,
            (ppm % 10_000) / 100
        );
    }
    if survival.hunger() != HungerState::Fed
        || survival.hydration_state() != HydrationState::Hydrated
        || survival.thermal_comfort_state() != ThermalComfortState::Comfortable
    {
        output.push_str("Injury recovery paused: maintain food, water and warmth above their warning thresholds.\n");
    } else if let Some(duration) = survival.recovery_duration_at_current_diet(registries) {
        let _ = writeln!(
            output,
            "Injury recovery: {} to full vitality at unchanged diet quality, if fed, hydrated and warm without further injury.",
            runway_label(Some(duration.value())),
        );
    } else {
        output.push_str("Injury recovery paused: replenish dietary reserves with varied food.\n");
    }
}

pub fn status(registries: &Registries, state: &AppState) -> String {
    let mut output = String::new();
    let Some(player) = state.logistics().player() else {
        return "No player admitted.\n".to_owned();
    };
    let _ = writeln!(
        output,
        "At {} | control {} | simulation {}",
        pos(player.position()),
        state.control_tick().value(),
        state.tick().value(),
    );
    let _ = writeln!(
        output,
        "Body: {:?} | foot elevation {} m",
        player.body().stance(),
        meters_from_nanometers(player.body().foot_position().y_nanometers()),
    );
    let foot = player.body().foot_position();
    let _ = writeln!(
        output,
        "Within cell: x {} m, z {} m (centre 0.500, 0.500)",
        meters_from_nanometers(foot.x_nanometers().rem_euclid(1_000_000_000)),
        meters_from_nanometers(foot.z_nanometers().rem_euclid(1_000_000_000))
    );
    output.push_str(&survival_status(registries, state));
    let active = state.logistics().active_selection();
    let (hand, material_carried) = hand_status(registries, state, active);
    let _ = writeln!(
        output,
        "Hand: {} (slot {})",
        hand,
        state.logistics().active_quick_access_slot().index() + 1,
    );
    if let Some(PlayerActiveSelection::Material(commodity)) = active
        && registries.survival().get_food(commodity).is_some()
        && material_carried
    {
        output.push_str(
            "Selected food: use eats it; eat N chooses a physical stack; drink N selects local water without changing the hand.\n",
        );
    }
    if player.body().vertical_velocity() != VerticalVelocity::ZERO {
        output.push_str("Airborne: gravity is active; wait until landing before hand work.\n");
    } else if player.body().horizontal_velocity() != HorizontalVelocity::ZERO {
        output.push_str("Still moving: wait 10 to let momentum settle before hand work.\n");
    }
    if let Some(weather) = assess_player_weather(registries, state) {
        let _ = writeln!(output, "Weather: {}", sensory::weather(weather));
    }
    if let Some(habitat) = assess_player_habitat(state) {
        let name = match habitat {
            HabitatKind::DampGrove => "damp grove",
            HabitatKind::MixedWoodland => "mixed woodland",
            HabitatKind::DryScrub => "dry scrub",
        };
        let _ = writeln!(output, "Habitat: {name}");
    }
    for source in project_player_nearby_warmth_sources(registries, state) {
        let definition = registries
            .energy()
            .get_store(source.definition())
            .unwrap_or_else(|| {
                unreachable!("energy owner projected a missing warmth source definition")
            });
        if source.position() == player.position() {
            let _ = writeln!(
                output,
                "Local warmth: {} | {} stored | {}",
                definition.name(),
                super::fire::energy_label(source.stored()),
                super::fire::burn_runway_label(registries, source.definition(), source.stored()),
            );
        } else {
            let _ = writeln!(
                output,
                "Adjacent warmth: {} at {} | {} stored | {}",
                definition.name(),
                pos(source.position()),
                super::fire::energy_label(source.stored()),
                super::fire::burn_runway_label(registries, source.definition(), source.stored()),
            );
        }
    }
    if let Some(work) = state.player_work().active() {
        let _ = writeln!(
            output,
            "Work: {}",
            activity::remaining_duration(registries, state, work),
        );
    } else if state.logistics().has_pending_direct_action() {
        output.push_str("Queued interaction: use 'wait finish' to reach admission at the next simulation boundary; if work starts, wait again to complete it.\n");
    }
    output
}

fn nearby_structure_report(registries: &Registries, state: &AppState) -> String {
    let mut output = String::new();
    let mut active_roof_nearby = false;
    for member in state.available_nearby_structural_members() {
        active_roof_nearby |= member.geometry().covers_horizontal_footprint()
            && member.lifecycle() == StructuralLifecycle::Active;
        let label = if member.geometry().covers_horizontal_footprint() {
            "panel"
        } else {
            "framing"
        };
        let profile = registries
            .structural()
            .get_profile(member.profile())
            .map_or("unidentified", |definition| definition.name());
        let _ = writeln!(
            output,
            "Nearby {label}: {profile} at {} | {:?} | {} g",
            pos(member.bounds().min()),
            member.lifecycle(),
            grams(member.embodied_mass()),
        );
    }
    if active_roof_nearby
        && assess_player_overhead_cover(registries, state)
            == Some(PlayerOverheadCoverAssessment::Open)
    {
        output.push_str(
            "Nearby roof does not cover your standing position. Shelter requires a reachable space directly beneath a panel, with clearance for your whole body footprint. Even inside its voxel, a body straddling the edge may be exposed; use 'center' to move toward the centre without teleporting.\n",
        );
    }
    output
}

/// The ground one pace away is already locally observable. Use actual standing
/// clearance and support rather than implying a generated hillside can always
/// be traversed with a level walk. A jump or descent is still a real movement
/// maneuver, not a pathfinding guarantee.
fn adjacent_footing(registries: &Registries, state: &AppState, dx: i64, dz: i64) -> &'static str {
    let Some(player) = state.logistics().player() else {
        return "unknown";
    };
    let foot = player.position();
    let Some((x, z)) = foot.x().checked_add(dx).zip(foot.z().checked_add(dz)) else {
        return "world edge";
    };
    let next = VoxelCoord::new(x, foot.y(), z);
    if state.world().block_at(next).is_none() {
        return "unseen";
    }
    if project_player_standing_space(registries, state, next) {
        return if project_player_level_approach(registries, state, next) {
            "level"
        } else {
            "level (approach obstructed)"
        };
    }
    for (depth, label) in [(1, "down 1m"), (2, "drop 2m"), (3, "drop 3m")] {
        if let Some(y) = foot.y().checked_sub(depth)
            && project_player_standing_space(registries, state, VoxelCoord::new(x, y, z))
        {
            return label;
        }
    }
    let direction = HorizontalControlInput::new(dx as i32 * 1_000_000, dz as i32 * 1_000_000)
        .expect("cardinal footing input");
    let climb = project_player_climb_target(registries, state, direction);
    if let Some(y) = foot.y().checked_add(1)
        && project_player_standing_space(registries, state, VoxelCoord::new(x, y, z))
    {
        if project_player_jump_rise_bound(registries, state)
            .is_some_and(|height| height.micrometers() < 1_000_000)
        {
            return if climb.is_some() {
                "up 1m (climb)"
            } else {
                "up 1m (load prevents jump)"
            };
        }
        return if climb.is_some() {
            "up 1m (jump or climb)"
        } else {
            "up 1m (jump clearance unverified)"
        };
    }
    if climb.is_some_and(|target| target.y().checked_sub(foot.y()) == Some(2)) {
        return "up 2m (climb)";
    }
    "blocked/steep"
}

fn adjacent_footing_report(registries: &Registries, state: &AppState) -> String {
    let Some(player) = state.logistics().player() else {
        return String::new();
    };
    if player.body().vertical_velocity() != VerticalVelocity::ZERO
        || state.world().block_at(player.position()).is_none()
    {
        return String::new();
    }
    format!(
        "Nearby footing (not a guaranteed route): N {} | E {} | S {} | W {}\n",
        adjacent_footing(registries, state, 0, -1),
        adjacent_footing(registries, state, 1, 0),
        adjacent_footing(registries, state, 0, 1),
        adjacent_footing(registries, state, -1, 0),
    )
}

fn surface_sight_report(registries: &Registries, state: &AppState, output: &mut String) {
    let from = state.logistics().player().map(|player| player.position());
    let nearby =
        AdjacentSurfaceResource::visible_in_generated_world(registries, state).collect::<Vec<_>>();
    if nearby.is_empty() {
        output.push_str("No surface resources in immediate sight.\n");
    } else {
        output.push_str("Visible surface resources (exact quantities require standing at them):\n");
        for source in nearby {
            let kind = match source.kind() {
                SurfaceResourceKind::Opportunity => "",
                SurfaceResourceKind::Drop => " (loose drop)",
            };
            let _ = writeln!(
                output,
                "  {}{kind} {} at {}",
                name(registries, source.commodity()),
                from.map_or_else(
                    || "unlocated".to_owned(),
                    |from| bearing(from, source.position())
                ),
                pos(source.position()),
            );
        }
    }
    for source in available_visible_surface_landmarks(registries, state) {
        let _ = writeln!(
            output,
            "Distant visible resource: {} {} at {}",
            name(registries, source.commodity()),
            from.map_or_else(
                || "unlocated".to_owned(),
                |from| bearing(from, source.position())
            ),
            pos(source.position()),
        );
    }
}

pub fn look(registries: &Registries, state: &AppState, aim: Option<ViewDirection>) -> String {
    let mut output = status(registries, state);
    output.push_str(&adjacent_footing_report(registries, state));
    surface_sight_report(registries, state, &mut output);
    let from = state.logistics().player().map(|player| player.position());
    for site in deep_hearth::geology::visible_weathered_copper_sites(registries, state) {
        let _ = writeln!(
            output,
            "Copper-stained weathered rock {} at {} (survey from the site to assess ore)",
            from.map_or_else(|| "unlocated".to_owned(), |from| bearing(from, site)),
            pos(site),
        );
    }
    // The world visibility query includes placed water vessels as well as
    // natural springs. Sight alone does not establish a natural source.
    let local_water = available_visible_generated_water(registries, state)
        .into_iter()
        .map(|water| ("Visible water", water));
    let landmarks = available_visible_water_landmarks(registries, state)
        .into_iter()
        .map(|water| ("Distant visible water", water));
    for (label, water) in local_water.chain(landmarks) {
        let _ = writeln!(
            output,
            "{label} {} at {}",
            from.map_or_else(
                || "unlocated".to_owned(),
                |from| bearing(from, water.position())
            ),
            pos(water.position())
        );
    }
    // Exact-local access is not a sight ray. A roof or darkness can hide
    // a spring that the actor is physically standing at and can drink from.
    if let Some(report) =
        drink_sources_report(&available_local_drink_sources(registries, state).collect::<Vec<_>>())
    {
        output.push_str(&report);
    }
    for tree in available_visible_generated_trees(registries, state) {
        let _ = writeln!(
            output,
            "Visible {} {} at {}",
            tree.species().name(),
            from.map_or_else(|| "unlocated".to_owned(), |from| bearing(from, tree.root())),
            pos(tree.root()),
        );
    }
    for sighting in available_nearby_kobold_scouts(registries, state) {
        let _ = writeln!(
            output,
            "Visible {} {} at {}{}",
            sensory::role(sighting.role()),
            from.map_or_else(
                || "unlocated".to_owned(),
                |from| bearing(from, sighting.position())
            ),
            pos(sighting.position()),
            sensory::sighting_condition(sighting),
        );
    }
    output.push_str(&nearby_structure_report(registries, state));
    for scent in assess_player_forage_scents(registries, state) {
        let _ = writeln!(
            output,
            "Scent: {} (no exact location)",
            sensory::scent(scent),
        );
    }
    for footfall in available_nearby_kobold_footfalls(registries, state) {
        let _ = writeln!(output, "Heard: {}", sensory::footfall(footfall));
    }
    // The same patrol leg may leave multiple impressions. Keep its location,
    // heading and activity observable without flooding every 'look' with repeats.
    let tracks = group_track_descriptions(
        available_nearby_kobold_tracks(registries, state)
            .into_iter()
            .map(sensory::track),
    );
    for (track, count) in tracks {
        if count == 1 {
            let _ = writeln!(output, "Track: {track}");
        } else {
            let _ = writeln!(output, "Track: {track} ({count} impressions)");
        }
    }
    if let Some(aim) = aim {
        match raycast_player_block(registries, state, aim) {
            Ok(Some(target)) => {
                let block_name = registries
                    .world()
                    .block_for_state(target.state())
                    .map_or("unknown block", |block| block.name());
                let _ = writeln!(
                    output,
                    "Pointed at {block_name} at {}, face {:?}",
                    pos(target.voxel()),
                    target.face()
                );
                if matches!(
                    state.logistics().active_selection(),
                    Some(PlayerActiveSelection::Equipment(_))
                ) && registries
                    .world()
                    .block_for_state(target.state())
                    .is_some_and(|block| block.excavation().is_some())
                {
                    output.push_str(&excavation_preview(registries, state, aim));
                }
            }
            Ok(None) => output.push_str("No targetable block within reach in that direction.\n"),
            Err(error) => {
                let _ = writeln!(output, "Pointing unavailable: {error}");
            }
        }
        output.push_str(&structural_break_preview(registries, state, aim));
        if state.logistics().active_selection()
            == Some(PlayerActiveSelection::material(CommodityKey::new(
                MATERIAL_WOOD,
                FORM_BOARD,
            )))
        {
            output.push_str(&timber_cover_preview(registries, state, aim));
        }
    }
    if let Ok(options) = player_surface_gathering_options(registries, state)
        && !options.is_empty()
    {
        output.push_str(&gather_options(registries, state, &options));
    }
    if let Some(runway) = assess_player_resting_provision_runway(registries, state) {
        let _ = writeln!(
            output,
            "At rest, hunger warning in {}, thirst warning in {}",
            runway_label(runway.hungry_in().map(|time| time.value())),
            runway_label(runway.thirsty_in().map(|time| time.value()))
        );
        let _ = writeln!(
            output,
            "At rest without intake, reserve exhaustion: food in {}, water in {} (work and travel spend reserves faster; provision before exhaustion).",
            runway_label(runway.starving_in().map(|time| time.value())),
            runway_label(runway.dehydrated_in().map(|time| time.value())),
        );
    }
    if let Some(runway) = assess_player_unheated_comfort_runway(registries, state) {
        let _ = writeln!(
            output,
            "At rest without local heat, cold warning in {}; cover: {:?}{}",
            runway_label(runway.current().map(|span| span.value())),
            runway.overhead_cover(),
            weather_forecast_notice(runway.current(), runway.weather_changes_in()),
        );
    }
    output
}

/// The unheated forecast assumes the current weather and airflow persist.
/// A new weather front or daylight phase can change the cooling rate on its
/// first authoritative tick; do not present a longer estimate as a promise.
fn weather_forecast_notice(cold_in: Option<TickSpan>, changes_in: Option<TickSpan>) -> String {
    let Some(change) = changes_in else {
        return String::new();
    };
    if cold_in.is_some_and(|cold| cold.value() < change.value()) {
        return String::new();
    }
    format!(
        "; weather may change in {} (reassess then)",
        runway_label(Some(change.value()))
    )
}

/// Combine identical physical traces while retaining first-observed route order.
fn group_track_descriptions(tracks: impl IntoIterator<Item = String>) -> Vec<(String, usize)> {
    let mut grouped: Vec<(String, usize)> = Vec::new();
    for description in tracks {
        if let Some((_, count)) = grouped.iter_mut().find(|(label, _)| *label == description) {
            *count += 1;
        } else {
            grouped.push((description, 1));
        }
    }
    grouped
}

fn excavation_preview(registries: &Registries, state: &AppState, aim: ViewDirection) -> String {
    match validate_pointed_terrain_excavation(registries, state, aim) {
        Ok(Some(plan)) => {
            let work = plan.work();
            let mass = work.excavated_mass();
            let remaining = work.source_mass_before();
            let cuts = remaining.milligrams().div_ceil(mass.milligrams());
            let duration = work
                .duration_control_ticks()
                .map(|ticks| ticks.div_ceil(CONTROL_TICKS_PER_SIMULATION_TICK));
            let destination = if work.output().carried_stockpile().is_some() {
                "pack"
            } else {
                "ground"
            };
            format!(
                "Excavation preview: {} g solid terrain remains; next cut {} g in {} to {destination}. At this cut size: {cuts} cuts; tool wear and reserves can change subsequent cuts. Use 'break DIRECTION repeat N' for bounded repeated cuts.\n",
                grams(remaining),
                grams(mass),
                runway_label(duration)
            )
        }
        Ok(None) => String::new(),
        Err(error) => format!("Excavation unavailable: {error}\n"),
    }
}

fn structural_break_preview(
    registries: &Registries,
    state: &AppState,
    aim: ViewDirection,
) -> String {
    match project_pointed_structural_break_target(registries, state, aim) {
        Ok(Some(element)) => state
            .structures()
            .get_element(element)
            .map_or_else(String::new, |member| {
                let label = if member.geometry().covers_horizontal_footprint() {
                    "panel"
                } else {
                    "framing"
                };
                format!(
                    "Break target: {label} at {} | {} g. 'break' selects this structure before terrain.\n",
                    pos(member.bounds().min()),
                    grams(member.embodied_mass()),
                )
            }),
        Err(count) => format!("Break target: {count} overlapping structures; aim more precisely.\n"),
        Ok(None) => String::new(),
    }
}

/// Show the production target before the player spends timber boards. A
/// standing-space hint is deliberately not a general pathfinding guarantee.
fn timber_cover_preview(registries: &Registries, state: &AppState, aim: ViewDirection) -> String {
    let target = match project_pointed_timber_cover_target(registries, state, aim) {
        Ok(Some(position)) => position,
        Ok(None) => {
            return "Cover preview: no load-bearing member at this aim. Raise framing above standing height, then aim toward open space beside it.\n".to_owned();
        }
        Err(SecondaryActionOutcome::StructuralPlacementIntersectsPlayer { position }) => {
            return format!(
                "Cover preview: a panel at {} would intersect your body. Move outside that destination and aim at an exposed face of the raised post.\n",
                pos(position),
            );
        }
        Err(
            SecondaryActionOutcome::StructuralPlacementOccupied { position, .. }
            | SecondaryActionOutcome::StructuralPlacementLocationOccupied { position },
        ) => {
            return format!(
                "Cover preview: {} is already occupied. Aim at a free cell beside the raised support, above clear walking space.\n",
                pos(position),
            );
        }
        Err(SecondaryActionOutcome::StructuralPlacementWorldUnavailable { position }) => {
            return format!(
                "Cover preview: terrain at {} is not yet available. Approach the site to load it before building.\n",
                pos(position),
            );
        }
        Err(_) => {
            return "Cover preview: placement unavailable at this aim; try an exposed side of the raised support.\n".to_owned();
        }
    };
    let mut output = format!("Cover preview: panel would occupy {}. ", pos(target));
    let Some(standing_y) = target.y().checked_sub(2) else {
        output.push_str("No standing-height space can be projected below it.\n");
        return output;
    };
    let beneath = VoxelCoord::new(target.x(), standing_y, target.z());
    if !project_player_standing_space(registries, state, beneath) {
        let _ = writeln!(
            output,
            "No clear, grounded standing-height space at {} underneath it. A support post or terrain may occupy the standing column; aim at a free cell beside raised framing to make shelter.",
            pos(beneath),
        );
    } else {
        let _ = writeln!(
            output,
            "Potential standing space at {} below the panel; verify a walkable approach before building.",
            pos(beneath),
        );
    }
    output
}

pub(super) fn runway_label(slow_ticks: Option<u64>) -> String {
    let Some(ticks) = slow_ticks else {
        return "not projected".to_owned();
    };
    if ticks == 0 {
        return "now".to_owned();
    }
    let microseconds = u128::from(ticks)
        * u128::from(CONTROL_TICKS_PER_SIMULATION_TICK)
        * u128::from(CONTROL_TICK_DURATION.microseconds());
    let seconds = microseconds.div_ceil(1_000_000);
    if seconds >= 3_600 {
        format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60)
    } else if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

/// Render one survival-owned freshness assessment consistently for loose
/// gathering opportunities and stored food cohorts.
fn freshness_label(freshness: FoodFreshness) -> String {
    match freshness {
        FoodFreshness::Fresh { remaining, .. } => format!(
            "fresh; estimated spoilage in {}",
            runway_label(Some(remaining.value()))
        ),
        FoodFreshness::Spoiled { .. } => "spoiled".to_owned(),
    }
}

pub fn gather_options(
    registries: &Registries,
    state: &AppState,
    options: &[PlayerSurfaceGatheringOption],
) -> String {
    let mut output = String::new();
    if options.is_empty() {
        return "No gatherable source at your current location.\n".to_owned();
    }
    output.push_str("Gather here (gather N, optional grams to 0.001 g):\n");
    for (index, option) in options.iter().copied().enumerate() {
        let mass = option.suggested_mass_at(state);
        let duration = option
            .duration_for_mass(mass)
            .map(|span| activity::gathering_duration(span.value()))
            .unwrap_or_else(|| "unavailable".to_owned());
        let _ = writeln!(
            output,
            "  {}. {} | available {} g | batch limit {} g | default {} g | work {} for default{}",
            index + 1,
            name(registries, option.commodity()),
            grams(option.source_mass()),
            grams(option.maximum_mass_at(state)),
            grams(mass),
            duration,
            if option.hand_handling_blocker().is_some() {
                " | too hot for bare hands"
            } else if mass.is_zero() {
                " | no carrying space"
            } else {
                ""
            }
        );
        if let Ok(Some(food)) = option.food_value(registries, state) {
            let freshness = freshness_label(food.current_freshness());
            let _ = writeln!(
                output,
                "     Food: {:?}, {freshness}; {} gross dietary energy per {} g",
                food.category(),
                super::fire::energy_label(food.dietary_energy()),
                grams(food.batch_mass()),
            );
        }
        if !mass.is_zero()
            && let Err(SurfaceGatheringError::PlayerWork(error)) =
                assess_player_surface_gathering_option_admission(registries, state, option, mass)
        {
            let _ = writeln!(
                output,
                "     Default batch currently blocked: {}",
                super::errors::work_blocker(&error),
            );
            if let Ok(affordable) =
                assess_affordable_player_gathering_mass(registries, state, option, mass)
            {
                if affordable.is_zero() {
                    output.push_str(
                        "     No positive portion is affordable with current reserves.\n",
                    );
                } else {
                    let _ = writeln!(
                        output,
                        "     Smaller affordable portion now: gather {} {} (revalidated on use).",
                        index + 1,
                        grams(affordable)
                    );
                }
            }
        }
    }
    output
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
