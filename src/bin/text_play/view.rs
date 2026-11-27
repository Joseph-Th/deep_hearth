//! Visible, renderer-neutral readout only. Never inspect undiscovered world resources.

use std::fmt::Write as _;

mod actor_inventory;
mod attention;
mod condition;
mod provisions;
mod sensory;
mod targeting;

pub use actor_inventory::{hand_label, inventory};
pub use attention::{LocalCues, ResidentWatch, Urgency, local_cues, urgency};
pub use condition::status;
use deep_hearth::core::quantity::{Mass, Volume};
use deep_hearth::core::time::{CONTROL_TICK_DURATION, CONTROL_TICKS_PER_SIMULATION_TICK, TickSpan};
use deep_hearth::kobolds::{
    available_nearby_kobold_footfalls, available_nearby_kobold_scouts,
    available_nearby_kobold_tracks,
};
use deep_hearth::logistics::{
    HorizontalControlInput, PlayerOverheadCoverAssessment, assess_player_overhead_cover,
    player_body_is_grounded, project_player_climb_target, project_player_descent_approach,
    project_player_jump_rise_bound, project_player_level_approach, project_player_standing_space,
};
use deep_hearth::spatial::ViewDirection;
use deep_hearth::spatial::{VerticalVelocity, VoxelCoord};
use deep_hearth::structural::StructuralLifecycle;
use deep_hearth::surface::{
    AdjacentSurfaceResource, SurfaceResourceKind, available_visible_dormant_shrub_landmarks,
    available_visible_surface_landmarks, player_surface_gathering_options,
};
use deep_hearth::survival::{
    FoodFreshness, assess_player_resting_provision_runway, assess_player_unheated_comfort_runway,
    available_local_drink_sources,
};
use deep_hearth::world::{
    assess_player_forage_scents, available_visible_generated_trees,
    available_visible_generated_water, available_visible_water_landmarks,
};
use deep_hearth::{AppState, Registries};
pub use provisions::{drink_options, gather_options};
use provisions::{drink_sources_report, natural_spring_report};

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

pub fn unsafe_handling_material(
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
            && project_player_descent_approach(registries, state, VoxelCoord::new(x, y, z))
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
    if !player_body_is_grounded(registries, state)
        || player.body().vertical_velocity() != VerticalVelocity::ZERO
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
    for shrub in available_visible_dormant_shrub_landmarks(registries, state) {
        let _ = writeln!(
            output,
            "Dormant {} shrub {} at {} (no harvestable crop)",
            name(registries, shrub.commodity()),
            from.map_or_else(
                || "unlocated".to_owned(),
                |from| bearing(from, shrub.position())
            ),
            pos(shrub.position()),
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
    output.push_str(&natural_spring_report(registries, state));
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
        targeting::pointed_report(registries, state, aim, &mut output);
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

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
