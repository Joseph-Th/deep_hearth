//! Broad seeded habitat provinces, without stored biome labels or hidden spawn rolls.
//!
//! Moisture varies smoothly over sixty-four-metre landscape cells. Local
//! elevation shifts a woodland toward sheltered moist hollows or dry, exposed
//! ground. Trees, forage and microclimate use the *same* regional identity.
//! The physical resources themselves remain finite owner records.

use crate::core::state::AppState;

use super::WorldGenerationSeed;
use super::terrain::{mix64, terrain_height};

const HABITAT_LATTICE_SIDE: i64 = 64;
const MOISTURE_SALT: u64 = 0x4841_4249_5441_5453;

/// Regional growth conditions, not a claim that a particular tree or food
/// parcel remains standing at the observed location.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HabitatKind {
    /// Heavier, shaded trees and wetter, berry-rich understory.
    DampGrove,
    /// Interspersed canopy, berry and nut sources.
    MixedWoodland,
    /// Sparse, younger trees and lower-yield, predominantly nut-bearing scrub.
    DryScrub,
}

fn moisture_lattice(seed: WorldGenerationSeed, x: i64, z: i64) -> u32 {
    let hash = mix64(
        seed.value()
            ^ (x as u64).wrapping_mul(0xA24B_AED4_963E_E407)
            ^ (z as u64).wrapping_mul(0x9FB2_1C65_1E98_DF25)
            ^ MOISTURE_SALT,
    );
    (hash & 255) as u32
}

/// Bilinear interpolation is integer-only and well-defined across negative
/// chunk coordinates. Adjacent lattice cells share corner values, so there
/// is no independent per-chunk biome lottery at their boundary.
fn moisture_at(seed: WorldGenerationSeed, x: i64, z: i64) -> u32 {
    let cell_x = x.div_euclid(HABITAT_LATTICE_SIDE);
    let cell_z = z.div_euclid(HABITAT_LATTICE_SIDE);
    let fx = x.rem_euclid(HABITAT_LATTICE_SIDE) as u64;
    let fz = z.rem_euclid(HABITAT_LATTICE_SIDE) as u64;
    let span = HABITAT_LATTICE_SIDE as u64;
    let x1 = cell_x.saturating_add(1);
    let z1 = cell_z.saturating_add(1);
    let north = u64::from(moisture_lattice(seed, cell_x, cell_z)) * (span - fx)
        + u64::from(moisture_lattice(seed, x1, cell_z)) * fx;
    let south = u64::from(moisture_lattice(seed, cell_x, z1)) * (span - fx)
        + u64::from(moisture_lattice(seed, x1, z1)) * fx;
    ((north * (span - fz) + south * fz) / (span * span)) as u32
}

pub(in crate::world) fn habitat_at(seed: WorldGenerationSeed, x: i64, z: i64) -> HabitatKind {
    let elevation = terrain_height(seed, x, z);
    let moisture = (i64::from(moisture_at(seed, x, z)) - elevation * 9).clamp(0, 255);
    match moisture {
        0..=87 => HabitatKind::DryScrub,
        178..=255 => HabitatKind::DampGrove,
        _ => HabitatKind::MixedWoodland,
    }
}

/// The player's broad, directly experienced local ground conditions. A biome
/// read supplies no remote resource coordinate, underground deposit or
/// guarantee of an available meal. Absent terrain remains unknown.
#[must_use]
pub fn assess_player_habitat(state: &AppState) -> Option<HabitatKind> {
    let position = state.logistics().player()?.position();
    let profile = state.world().generation_profile()?;
    (state.world().has_generated_terrain_at(position)
        && position.y() > terrain_height(profile.seed(), position.x(), position.z()))
    .then(|| habitat_at(profile.seed(), position.x(), position.z()))
}

#[cfg(test)]
#[path = "habitat_tests.rs"]
mod tests;
