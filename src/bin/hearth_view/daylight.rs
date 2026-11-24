//! Sun path and atmosphere colors derived from the simulation day cycle.
//!
//! The simulation day starts at dawn; its first three quarters are daylight phases and the last
//! quarter is night (`world::sky`). The sun therefore crosses the sky over the first 75% of the
//! day and stays below the horizon for the final quarter.

use glam::Vec3;

/// Continuous adapter-local time of day; never quantized to simulation ticks.
pub(super) struct DisplayClock {
    hours: f64,
}

impl DisplayClock {
    pub(super) fn new(day_fraction: f32) -> Self {
        Self {
            hours: f64::from(day_fraction) * 24.0,
        }
    }

    pub(super) fn shift_hours(&mut self, hours: f64) {
        self.hours = (self.hours + hours).rem_euclid(24.0);
    }

    pub(super) fn day_fraction(&self) -> f32 {
        (self.hours / 24.0) as f32
    }
}

#[cfg(test)]
#[path = "daylight_tests.rs"]
mod tests;

pub struct Daylight {
    /// Unit vector from the ground toward the sun.
    pub to_sun: Vec3,
    /// Normalized 0..1 direct-sun strength (drives palette shading and shadow use).
    pub sun_intensity: f32,
    /// Linear HDR sun radiance color.
    pub sun_color: Vec3,
    pub sky_ambient: Vec3,
    pub zenith: Vec3,
    pub horizon: Vec3,
    pub night: f32,
    /// Scene exposure: a fixed stand-in for eye adaptation between noon and moonlight.
    pub exposure: f32,
    /// Color saturation; dim scenes lose color as scotopic vision does.
    pub saturation: f32,
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Daylight {
    pub fn at(day_fraction: f32) -> Self {
        let fraction = day_fraction.rem_euclid(1.0);
        let angle = if fraction < 0.75 {
            fraction / 0.75 * std::f32::consts::PI
        } else {
            std::f32::consts::PI * (1.0 + (fraction - 0.75) / 0.25)
        };
        // East to west, leaning south so midday shadows are never perfectly vertical.
        let to_sun = Vec3::new(angle.cos(), angle.sin() * 0.92, 0.38).normalize();
        let elevation = to_sun.y;
        let day = smoothstep(-0.10, 0.20, elevation);
        // Capped below 1 so full sun lifts palette shades by one step rather than bleaching them.
        let sun_intensity = smoothstep(-0.02, 0.16, elevation) * 0.7;
        let warm = Vec3::new(1.0, 0.46, 0.20);
        let white = Vec3::new(1.0, 0.94, 0.84);
        // Fades through twilight so clouds and the sun glow do not stay lit after sunset.
        let twilight = smoothstep(-0.16, 0.04, elevation);
        let sun_color = warm.lerp(white, smoothstep(0.02, 0.45, elevation)) * 2.4 * twilight;
        let sunset = (-(elevation / 0.14).powi(2)).exp() * day;
        let zenith = Vec3::new(0.010, 0.014, 0.028).lerp(Vec3::new(0.10, 0.26, 0.66), day);
        let horizon = Vec3::new(0.020, 0.025, 0.040)
            .lerp(Vec3::new(0.58, 0.70, 0.86), day)
            .lerp(Vec3::new(0.95, 0.46, 0.24), sunset * 0.7);
        // Night keeps a dim cool moonlit ambient so terrain stays legible without a light source.
        let sky_ambient = Vec3::new(0.085, 0.095, 0.125).lerp(Vec3::new(0.40, 0.48, 0.62), day);
        Self {
            to_sun,
            sun_intensity,
            sun_color,
            sky_ambient,
            zenith,
            horizon,
            night: 1.0 - day,
            exposure: 0.85 + (1.0 - day) * 2.1,
            saturation: 0.45 + day * 0.67,
        }
    }
}
