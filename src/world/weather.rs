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

/// Physical time between changes in a regional wind/temperature front: twelve minutes.
const WEATHER_FRONT_TICKS: u64 = 200;
const REGION_SIDE_VOXELS: i64 = 128;
const COOLING_REFERENCE_CELSIUS: i32 = 12;
const COOLING_PER_DEGREE_PPM: i32 = 30_000;
const BREEZE_COOLING_PPM: u32 = 180_000;
const GUST_COOLING_PPM: u32 = 400_000;
const MINIMUM_COOLING_PPM: u32 = 400_000;
const MAXIMUM_COOLING_PPM: u32 = 2_500_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindCondition {
    Calm,
    Breeze,
    Gusting,
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
    pub const fn cooling_multiplier_ppm(self) -> u32 {
        self.cooling_multiplier_ppm
    }
    /// Earliest possible change from a new regional weather front or day phase.
    #[must_use]
    pub const fn changes_in(self) -> TickSpan {
        self.changes_in
    }
}

/// Only an admitted player standing in a generated column has an observable
/// weather context. Missing world truth cannot invent conditions.
#[must_use]
pub fn assess_player_weather(registries: &Registries, state: &AppState) -> Option<PlayerWeather> {
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

fn weather_at(
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
    let celsius = base_celsius + ((hash >> 20) % 5) as i32 - 2;
    let wind_cooling = match wind {
        WindCondition::Calm => 0,
        WindCondition::Breeze => BREEZE_COOLING_PPM,
        WindCondition::Gusting => GUST_COOLING_PPM,
    };
    let cooling = (1_000_000_i64
        + i64::from(COOLING_REFERENCE_CELSIUS - celsius) * i64::from(COOLING_PER_DEGREE_PPM)
        + i64::from(wind_cooling))
    .clamp(
        i64::from(MINIMUM_COOLING_PPM),
        i64::from(MAXIMUM_COOLING_PPM),
    );
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
        cooling_multiplier_ppm: u32::try_from(cooling)
            .expect("authored weather cooling bound fits u32"),
        changes_in: TickSpan::new(front_in.min(day_phase_end - day_tick)),
    }
}

#[cfg(test)]
#[path = "weather_tests.rs"]
mod tests;
