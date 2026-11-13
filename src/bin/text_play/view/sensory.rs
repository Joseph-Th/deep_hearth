//! Player-facing names for observable weather, scent, and resident activity.
//! Typed observations remain authoritative; debug records are not player text.

use deep_hearth::kobolds::{
    KoboldFootfall, KoboldFootfallBearing, KoboldFootfallKind, KoboldRole, KoboldSighting,
    KoboldStature, KoboldTrack, KoboldTrailKind,
};
use deep_hearth::world::{
    ForageScent, ForageScentIntensity, ForageScentKind, PlayerWeather, WindCondition, WindDirection,
};

pub(super) fn weather(weather: PlayerWeather) -> String {
    let celsius = (i64::from(weather.ambient_temperature().millikelvin()) - 273_150) / 1_000;
    let wind = match weather.wind() {
        WindCondition::Calm => "calm".to_owned(),
        WindCondition::Breeze => wind_heading("breeze", weather.wind_direction()),
        WindCondition::Gusting => wind_heading("gusting wind", weather.wind_direction()),
    };
    format!(
        "{:?} {:?}, {celsius} C, {wind}",
        weather.season(),
        weather.phase()
    )
}

fn wind_heading(strength: &str, direction: Option<WindDirection>) -> String {
    match direction {
        Some(WindDirection::North) => format!("{strength} toward north"),
        Some(WindDirection::South) => format!("{strength} toward south"),
        Some(WindDirection::East) => format!("{strength} toward east"),
        Some(WindDirection::West) => format!("{strength} toward west"),
        None => strength.to_owned(),
    }
}

pub(super) fn scent(scent: ForageScent) -> String {
    let intensity = match scent.intensity() {
        ForageScentIntensity::Faint => "faint",
        ForageScentIntensity::Distinct => "distinct",
    };
    let kind = match scent.kind() {
        ForageScentKind::SweetFruit => "sweet fruit",
        ForageScentKind::WildNuts => "wild nuts",
    };
    format!("{intensity} {kind}")
}

pub(super) fn role(role: KoboldRole) -> &'static str {
    match role {
        KoboldRole::Scout => "scout",
        KoboldRole::Watchman => "watchman",
        KoboldRole::TempleWarden => "temple warden",
        KoboldRole::TempleArchivist => "temple archivist",
    }
}

pub(super) fn sighting_condition(sighting: KoboldSighting) -> String {
    let stature = match sighting.stature() {
        KoboldStature::Small => "small",
        KoboldStature::Heavy => "heavy",
        KoboldStature::Massive => "massive",
    };
    let mut description = format!("; {stature} stature");
    if sighting.is_wary() {
        description.push_str(", wary");
    }
    if sighting.is_defending() {
        description.push_str(", defending");
    }
    description
}

pub(super) fn footfall(footfall: KoboldFootfall) -> String {
    let sound = match footfall.kind() {
        KoboldFootfallKind::Footsteps => "footsteps",
        KoboldFootfallKind::Dragging => "something dragging",
        KoboldFootfallKind::Foraging => "foraging noises",
        KoboldFootfallKind::Splashing => "splashing",
        KoboldFootfallKind::Hurrying => "hurried footsteps",
    };
    let bearing = match footfall.bearing() {
        KoboldFootfallBearing::Nearby => "nearby",
        KoboldFootfallBearing::North => "to the north",
        KoboldFootfallBearing::South => "to the south",
        KoboldFootfallBearing::East => "to the east",
        KoboldFootfallBearing::West => "to the west",
        KoboldFootfallBearing::Above => "above",
        KoboldFootfallBearing::Below => "below",
    };
    format!("{sound} {bearing}")
}

pub(super) fn track(track: KoboldTrack) -> String {
    let activity = match track.observed_activity() {
        KoboldTrailKind::Walking => "footprints",
        KoboldTrailKind::Hauling => "drag marks",
        KoboldTrailKind::Foraging => "foraging traces",
        KoboldTrailKind::Drinking => "water-side footprints",
        KoboldTrailKind::Fleeing => "hurried footprints",
    };
    format!(
        "{} {activity} at {}, heading {}",
        role(track.role()),
        super::pos(track.position()),
        super::bearing(track.position(), track.toward()),
    )
}
