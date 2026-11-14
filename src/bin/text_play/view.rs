//! Visible, renderer-neutral readout only. Never inspect undiscovered world resources.

use std::fmt::Write as _;

use super::activity;
mod actor_inventory;
mod sensory;

use actor_inventory::hand_status;
pub use actor_inventory::{hand_label, inventory};
use deep_hearth::core::quantity::{Mass, Volume};
use deep_hearth::core::time::{CONTROL_TICK_DURATION, CONTROL_TICKS_PER_SIMULATION_TICK, Season};
use deep_hearth::kobolds::{
    KoboldFootfall, KoboldSighting, KoboldTrack, available_nearby_kobold_footfalls,
    available_nearby_kobold_scouts, available_nearby_kobold_tracks,
};
use deep_hearth::logistics::{
    PlayerActiveSelection, PlayerOverheadCoverAssessment, assess_player_overhead_cover,
};
use deep_hearth::spatial::ViewDirection;
use deep_hearth::spatial::{HorizontalVelocity, VerticalVelocity, VoxelCoord};
use deep_hearth::structural::StructuralLifecycle;
use deep_hearth::surface::{
    AdjacentSurfaceResource, PlayerSurfaceGatheringOption, SurfaceResourceKind,
    player_surface_gathering_options,
};
use deep_hearth::survival::{
    FoodFreshness, HungerState, HydrationState, ThermalComfortState, Vitality,
    assess_player_resting_provision_runway, assess_player_unheated_comfort_runway, assess_survival,
    available_local_drink_sources,
};
use deep_hearth::world::{
    DayPhase, ForageScent, VisibleWaterSource, WindCondition, WindDirection,
    assess_player_forage_scents, assess_player_weather, available_visible_generated_water,
    raycast_player_block,
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
    let mut output = String::new();
    let sources = available_local_drink_sources(registries, state).collect::<Vec<_>>();
    if sources.is_empty() {
        return "No drinkable source at your current location.\n".to_owned();
    }
    output.push_str("Drink here (drink N):\n");
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
    output
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

/// Player-visible cues only. Weather alone need not interrupt a requested work completion;
/// approaching creatures, tracks, and changing local supplies still demand attention.
#[derive(Clone, PartialEq, Eq)]
pub struct LocalCues {
    resources: Vec<AdjacentSurfaceResource>,
    water: Vec<VisibleWaterSource>,
    kobolds: Vec<KoboldSighting>,
    footfalls: Vec<KoboldFootfall>,
    tracks: Vec<KoboldTrack>,
    scents: Vec<ForageScent>,
    weather: Option<(Season, DayPhase, WindCondition, Option<WindDirection>)>,
}

impl LocalCues {
    /// Returns the most actionable changed cue and whether it should stop a work wait.
    pub fn changed(&self, next: &Self) -> Option<(&'static str, bool)> {
        if self.kobolds != next.kobolds {
            Some(("A nearby kobold moved or changed behavior.", true))
        } else if self.footfalls != next.footfalls {
            Some(("Nearby footsteps have changed.", true))
        } else if self.tracks != next.tracks {
            Some(("Nearby tracks have changed.", true))
        } else if self.resources != next.resources {
            Some(("Nearby loose resources have changed.", true))
        } else if self.water != next.water {
            Some(("Nearby water has changed.", true))
        } else if self.scents != next.scents {
            Some(("Scents in the area have shifted.", true))
        } else if self.weather != next.weather {
            Some(("The wind or weather has shifted.", false))
        } else {
            None
        }
    }
}

/// Do not inspect private geology, colony schedules, or all generated blocks to stop a wait.
pub fn local_cues(registries: &Registries, state: &AppState) -> LocalCues {
    LocalCues {
        resources: AdjacentSurfaceResource::visible_in_generated_world(registries, state).collect(),
        water: available_visible_generated_water(registries, state),
        kobolds: available_nearby_kobold_scouts(registries, state).collect(),
        footfalls: available_nearby_kobold_footfalls(registries, state),
        tracks: available_nearby_kobold_tracks(registries, state).collect(),
        scents: assess_player_forage_scents(registries, state),
        weather: assess_player_weather(registries, state).map(|weather| {
            (
                weather.season(),
                weather.phase(),
                weather.wind(),
                weather.wind_direction(),
            )
        }),
    }
}

/// Threshold transitions, not tiny per-tick resource changes, interrupt long waits.
pub fn urgency(registries: &Registries, state: &AppState) -> (bool, bool, bool, bool) {
    let Some(assessment) = assess_survival(registries, state) else {
        return (true, true, true, true);
    };
    (
        assessment.hunger() != HungerState::Fed,
        assessment.hydration_state() != HydrationState::Hydrated,
        assessment.vitality() < Vitality::MAXIMUM,
        assessment.thermal_comfort_state() != ThermalComfortState::Comfortable,
    )
}

fn survival_status(registries: &Registries, state: &AppState) -> String {
    let mut output = String::new();
    if let Some(survival) = assess_survival(registries, state) {
        let (food_percent, water_percent) = survival.reserve_percentages(registries);
        let _ = writeln!(
            output,
            "Condition: vitality {}%, food reserve {}%, hydration {}%, warmth {}%",
            survival.vitality().parts_per_million() / 10_000,
            food_percent,
            water_percent,
            survival.thermal_comfort_ppm() / 10_000,
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
            if hungry || thirsty || injured || cold {
                let mut warnings = Vec::new();
                if hungry {
                    warnings.push("hunger");
                }
                if thirsty {
                    warnings.push("thirst");
                }
                if injured {
                    warnings.push("injury");
                }
                if cold {
                    warnings.push("cold exposure");
                }
                let _ = writeln!(output, "Warnings: {}", warnings.join(", "));
            }
        }
    }
    output
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
    for source in state.available_local_energy_stores() {
        if let Some(definition) = registries.energy().get_store(source.definition())
            && definition.is_local_warmth_source()
            && !source.stored().is_zero()
        {
            let _ = writeln!(
                output,
                "Local warmth: {} | {} stored",
                definition.name(),
                super::fire::energy_label(source.stored()),
            );
        }
    }
    if let Some(work) = state.player_work().active() {
        let _ = writeln!(
            output,
            "Work: {}",
            activity::remaining_duration(registries, state, work),
        );
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
            "Nearby roof does not cover your standing position. Shelter requires a reachable space directly beneath a panel, with clearance for your body.\n",
        );
    }
    output
}

pub fn look(registries: &Registries, state: &AppState, aim: Option<ViewDirection>) -> String {
    let mut output = status(registries, state);
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
    for water in available_visible_generated_water(registries, state) {
        let _ = writeln!(
            output,
            "Visible spring water {} at {}",
            from.map_or_else(
                || "unlocated".to_owned(),
                |from| bearing(from, water.position())
            ),
            pos(water.position())
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
    for track in available_nearby_kobold_tracks(registries, state) {
        let _ = writeln!(output, "Track: {}", sensory::track(track));
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
            }
            Ok(None) => output.push_str("No targetable block within reach in that direction.\n"),
            Err(error) => {
                let _ = writeln!(output, "Pointing unavailable: {error}");
            }
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
    }
    if let Some(runway) = assess_player_unheated_comfort_runway(registries, state) {
        let _ = writeln!(
            output,
            "Without heat, time until cold warning: {}; cover: {:?}",
            runway_label(runway.current().map(|span| span.value())),
            runway.overhead_cover()
        );
    }
    output
}

fn runway_label(slow_ticks: Option<u64>) -> String {
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
        let mass = default_gather_mass(state, option);
        let duration = option
            .duration_for_mass(mass)
            .map(|span| activity::gathering_duration(span.value()))
            .unwrap_or_else(|| "unavailable".to_owned());
        let _ = writeln!(
            output,
            "  {}. {} | available {} g | default {} g | work {}{}",
            index + 1,
            name(registries, option.commodity()),
            grams(option.source_mass()),
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
            let _ = writeln!(output, "     Food: {:?}, {freshness}", food.category(),);
        }
    }
    output
}

/// A suggested mass fits both the gathering method and the current unreserved
/// carried capacity. It does not replace the inventory owner's admission proof.
pub(super) fn default_gather_mass(state: &AppState, option: PlayerSurfaceGatheringOption) -> Mass {
    let capacity = state
        .logistics()
        .player()
        .and_then(|player| state.inventory().get_stockpile(player.carried_stockpile()))
        .map_or(Mass::ZERO, |stockpile| stockpile.available_capacity());
    bounded_gather_mass(option, capacity)
}

fn bounded_gather_mass(option: PlayerSurfaceGatheringOption, capacity: Mass) -> Mass {
    option
        .source_mass()
        .min(option.maximum_batch_mass())
        .min(option.direct_batch_mass())
        .min(capacity)
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
