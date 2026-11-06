//! Deterministic local outdoor conditions from persisted world seed and calendar time.
//!
//! Conditions are read-only. A forecast does not materialize weather, air, water,
//! or vegetation, and never requires a background update or save-side RNG state.

use crate::core::quantity::Temperature;
use crate::core::state::AppState;
use crate::core::time::{Season, SimulationTick, TickSpan};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::generation::terrain::mix64;
use super::generation::{HabitatKind, WorldGenerationSeed, habitat_at};

/// Physical time between changes in a regional wind/temperature front: twelve minutes.
pub(crate) const WEATHER_FRONT_TICKS: u64 = 200;
pub(super) const REGION_SIDE_VOXELS: i64 = 128;
const COOLING_REFERENCE_CELSIUS: i32 = 12;
const COOLING_PER_DEGREE_PPM: i32 = 30_000;
const BREEZE_COOLING_PPM: u32 = 180_000;
const GUST_COOLING_PPM: u32 = 400_000;
const MINIMUM_COOLING_PPM: u32 = 400_000;
const MAXIMUM_COOLING_PPM: u32 = 2_500_000;

fn wind_cooling_ppm(wind: WindCondition) -> u32 {
    match wind {
        WindCondition::Calm => 0,
        WindCondition::Breeze => BREEZE_COOLING_PPM,
        WindCondition::Gusting => GUST_COOLING_PPM,
    }
}

fn air_cooling_multiplier_ppm(celsius: i32, wind_cooling_ppm: u32) -> u32 {
    let cooling = (1_000_000_i64
        + i64::from(COOLING_REFERENCE_CELSIUS - celsius) * i64::from(COOLING_PER_DEGREE_PPM)
        + i64::from(wind_cooling_ppm))
    .clamp(
        i64::from(MINIMUM_COOLING_PPM),
        i64::from(MAXIMUM_COOLING_PPM),
    );
    u32::try_from(cooling).expect("authored weather cooling bound fits u32")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindCondition {
    Calm,
    Breeze,
    Gusting,
}

/// A regional front's tendency to exchange finite water with a shallow
/// catchment. This does not model new precipitation or create water.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpringMoistureFlux {
    Drying,
    Returning,
    Balanced,
}

pub(crate) fn spring_moisture_flux(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    at: SimulationTick,
) -> SpringMoistureFlux {
    let Some(weather) = exposed_air_at(registries, state, position, at) else {
        return SpringMoistureFlux::Balanced;
    };
    let temperature = weather.ambient_temperature().millikelvin();
    if weather.wind() == WindCondition::Gusting || temperature >= 295_150 {
        SpringMoistureFlux::Drying
    } else if weather.wind() == WindCondition::Calm && temperature <= 291_150 {
        SpringMoistureFlux::Returning
    } else {
        SpringMoistureFlux::Balanced
    }
}

/// Generated, sky-exposed air at an actually inhabited voxel. Indoor residents
/// and unexplored regions have no asserted outdoor thermal exposure.
pub(crate) fn exposed_air_at(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    at: SimulationTick,
) -> Option<PlayerWeather> {
    let seed = state.world().generation_profile()?.seed().value();
    super::sky::has_open_sky(registries, state, position)
        .then(|| weather_at(registries, seed, position, at))
}

/// Horizontal direction in which air moves, not the direction it originates from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindDirection {
    East,
    West,
    North,
    South,
}

impl WindDirection {
    #[must_use]
    pub(crate) const fn delta(self) -> (i64, i64) {
        match self {
            Self::East => (1, 0),
            Self::West => (-1, 0),
            Self::North => (0, -1),
            Self::South => (0, 1),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayPhase {
    Morning,
    Afternoon,
    Evening,
    Night,
}

/// Actor-local outdoor conditions at the actual current simulation tick.
/// The multiplier is a thermal-demand parameter, not a claim of instantaneous
/// body temperature. Cover and local heat remain owned by survival.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerWeather {
    season: Season,
    phase: DayPhase,
    ambient_temperature: Temperature,
    wind: WindCondition,
    wind_direction: Option<WindDirection>,
    cooling_multiplier_ppm: u32,
    changes_in: TickSpan,
}

impl PlayerWeather {
    #[must_use]
    pub const fn season(self) -> Season {
        self.season
    }
    #[must_use]
    pub const fn phase(self) -> DayPhase {
        self.phase
    }
    #[must_use]
    pub const fn ambient_temperature(self) -> Temperature {
        self.ambient_temperature
    }
    #[must_use]
    pub const fn wind(self) -> WindCondition {
        self.wind
    }
    #[must_use]
    pub const fn wind_direction(self) -> Option<WindDirection> {
        self.wind_direction
    }
    #[must_use]
    pub const fn cooling_multiplier_ppm(self) -> u32 {
        self.cooling_multiplier_ppm
    }

    /// Number of player-height cells shielded directly upwind (zero, one or two).
    /// A one-voxel berm protects the lower body; a two-voxel wall or standing
    /// tree trunk blocks wind across the whole standing profile. Missing terrain
    /// is never treated as a protective wall.
    pub(crate) fn upwind_windbreak_levels(self, registries: &Registries, state: &AppState) -> u8 {
        let (Some(direction), Some(player)) = (self.wind_direction, state.logistics().player())
        else {
            return 0;
        };
        let (dx, dz) = direction.delta();
        let position = player.position();
        let (Some(x), Some(z), Some(head_y)) = (
            position.x().checked_sub(dx),
            position.z().checked_sub(dz),
            position.y().checked_add(1),
        ) else {
            return 0;
        };
        [position.y(), head_y]
            .into_iter()
            .filter(|&y| {
                let voxel = VoxelCoord::new(x, y, z);
                state
                    .world()
                    .full_cell_collision_at(registries.world(), voxel)
                    == Some(true)
                    || state.structures().blocks_full_cell(voxel)
            })
            .count() as u8
    }

    /// Ambient temperature remains unchanged behind a windbreak. Only the
    /// convective wind surcharge is reduced, by half per sheltered body level;
    /// both the survival tick and the player's forecast use this same rule.
    pub(crate) fn felt_cooling_multiplier_ppm(
        self,
        registries: &Registries,
        state: &AppState,
    ) -> u32 {
        let wind = wind_cooling_ppm(self.wind);
        let exposed_levels = 2 - u32::from(self.upwind_windbreak_levels(registries, state));
        let celsius = (i64::from(self.ambient_temperature.millikelvin()) - 273_150) / 1_000;
        air_cooling_multiplier_ppm(
            i32::try_from(celsius).expect("authored weather temperature fits i32 Celsius"),
            wind * exposed_levels / 2,
        )
    }

    /// Earliest possible change from a new regional weather front or day phase.
    #[must_use]
    pub const fn changes_in(self) -> TickSpan {
        self.changes_in
    }
}

/// Only an admitted player standing in a generated column has an observable
/// weather context. A dead player receives no live environmental observation;
/// missing world truth likewise cannot invent conditions.
#[must_use]
pub fn assess_player_weather(registries: &Registries, state: &AppState) -> Option<PlayerWeather> {
    state.player_observation_position()?;
    assess_player_weather_at(registries, state, state.tick())
}

/// Next-tick survival uses the same observable physical conditions without
/// lagging a front transition by one simulation quantum.
pub(crate) fn assess_player_weather_at(
    registries: &Registries,
    state: &AppState,
    at: SimulationTick,
) -> Option<PlayerWeather> {
    let position = state.logistics().player()?.position();
    let profile = state.world().generation_profile()?;
    state
        .world()
        .has_generated_terrain_at(position)
        .then(|| weather_at(registries, profile.seed().value(), position, at))
}

pub(super) fn weather_at(
    registries: &Registries,
    seed: u64,
    position: VoxelCoord,
    at: SimulationTick,
) -> PlayerWeather {
    let calendar = registries.core().calendar();
    let date = calendar.date_at(at);
    let day_ticks = calendar.ticks_per_day();
    let day_tick = date.day_tick();
    let phase_index = (u128::from(day_tick) * 4 / u128::from(day_ticks)) as u8;
    let phase = match phase_index {
        0 => DayPhase::Morning,
        1 => DayPhase::Afternoon,
        2 => DayPhase::Evening,
        _ => DayPhase::Night,
    };
    let base_celsius: i32 = match date.season() {
        Season::Spring => [12, 17, 11, 7],
        Season::Summer => [20, 26, 21, 16],
        Season::Autumn => [9, 14, 8, 3],
        Season::Winter => [-3, 2, -4, -8],
    }[usize::from(phase_index)];
    let weather_window = at.value() / WEATHER_FRONT_TICKS;
    let hash = mix64(
        seed ^ (position.x().div_euclid(REGION_SIDE_VOXELS) as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (position.z().div_euclid(REGION_SIDE_VOXELS) as u64)
                .wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ weather_window.wrapping_mul(0xD6E8_FEB8_6659_FD93)
            ^ 0x5745_4154_4845_5253,
    );
    let wind = match hash % 5 {
        0 | 1 => WindCondition::Calm,
        2 | 3 => WindCondition::Breeze,
        _ => WindCondition::Gusting,
    };
    let wind_direction = if wind == WindCondition::Calm {
        None
    } else {
        Some(match (hash >> 8) & 3 {
            0 => WindDirection::East,
            1 => WindDirection::West,
            2 => WindDirection::North,
            _ => WindDirection::South,
        })
    };
    // Broad ground moisture and shading affect outdoor temperature even
    // when the same weather front crosses adjacent habitat provinces.
    let local_ground_delta =
        match habitat_at(WorldGenerationSeed::new(seed), position.x(), position.z()) {
            HabitatKind::DampGrove => -2,
            HabitatKind::MixedWoodland => 0,
            HabitatKind::DryScrub => 2,
        };
    let celsius = base_celsius + ((hash >> 20) % 5) as i32 - 2 + local_ground_delta;
    let cooling = air_cooling_multiplier_ppm(celsius, wind_cooling_ppm(wind));
    let front_in = WEATHER_FRONT_TICKS - at.value() % WEATHER_FRONT_TICKS;
    let day_phase_end = ((u128::from(phase_index) + 1) * u128::from(day_ticks))
        .div_ceil(4)
        .min(u128::from(day_ticks)) as u64;
    PlayerWeather {
        season: date.season(),
        phase,
        ambient_temperature: Temperature::from_millikelvin(
            u32::try_from(273_150 + celsius * 1_000)
                .expect("authored weather temperature is above absolute zero"),
        ),
        wind,
        wind_direction,
        cooling_multiplier_ppm: cooling,
        changes_in: TickSpan::new(front_in.min(day_phase_end - day_tick)),
    }
}

/// Source-local outdoor wind for physical sound attenuation. Generated rock
/// caverns are not exposed to surface gusts; missing chunk truth does not
/// invent atmospheric masking for an unobserved place.
pub(crate) fn wind_at_generated_position(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    at: SimulationTick,
) -> Option<WindCondition> {
    exposed_air_at(registries, state, position, at).map(PlayerWeather::wind)
}

#[cfg(test)]
#[path = "weather_tests.rs"]
mod tests;
