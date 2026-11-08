//! Persistent wilderness forage attached to actual generated terrain.
//!
//! One sparse seed-selected site per two-by-two chunk district can become a
//! finite edible opportunity. Terrain and surface ownership commit atomically;
//! no unlimited respawn or player-centered loot roll is involved. A finite
//! root reserve can later support conserved seasonal fruit cycling.

#[cfg(test)]
#[path = "forage_late_tests.rs"]
mod late_tests;
#[cfg(test)]
#[path = "forage_tests.rs"]
mod tests;

use crate::content::{BLOCK_STATE_STONE, FORM_FOOD, MATERIAL_BERRIES, MATERIAL_WILD_NUTS};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, MaterialComposition};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec};

use super::terrain::{mix64, terrain_height};
use super::{BlockStateId, ChunkCoord, WorldChunk, WorldGenerationProfile};

const AMBIENT_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn district_hash(profile: WorldGenerationProfile, coord: ChunkCoord) -> u64 {
    mix64(
        profile.seed().value()
            ^ (coord.x().div_euclid(2) as u64).wrapping_mul(0x8EBC_6AF0_9C88_C6E3)
            ^ (coord.z().div_euclid(2) as u64).wrapping_mul(0x5899_65CC_7537_4CC3)
            ^ 0x464F_5241_4745_5F57,
    )
}

fn seeded_food(profile: WorldGenerationProfile, hash: u64, x: i64, z: i64) -> (CommodityKey, Mass) {
    let (material, mass_mg) = match super::habitat_at(profile.seed(), x, z) {
        super::HabitatKind::DampGrove => (MATERIAL_BERRIES, 700_000 + ((hash >> 40) % 500_000)),
        super::HabitatKind::DryScrub => (MATERIAL_WILD_NUTS, 180_000 + ((hash >> 40) % 320_000)),
        super::HabitatKind::MixedWoodland => (
            if hash & 4 == 0 {
                MATERIAL_BERRIES
            } else {
                MATERIAL_WILD_NUTS
            },
            250_000 + ((hash >> 40) % 600_000),
        ),
    };
    (
        CommodityKey::new(material, FORM_FOOD),
        Mass::from_milligrams(mass_mg),
    )
}

/// Candidate locations and food identity are fixed by the world seed. Runtime
/// obstructions may skip an earlier site, but neither generation nor trusted
/// load can select a site outside this finite set.
fn seeded_forage_candidates(
    profile: WorldGenerationProfile,
    coord: ChunkCoord,
) -> impl Iterator<Item = (VoxelCoord, CommodityKey, Mass)> {
    let origin = (|| {
        if profile.solid_state() != BLOCK_STATE_STONE {
            return None;
        }
        let hash = district_hash(profile, coord);
        if coord.x().rem_euclid(2) as u64 != hash & 1
            || coord.z().rem_euclid(2) as u64 != (hash >> 1) & 1
        {
            return None;
        }
        let base_x = coord
            .world_min_x()?
            .checked_add(3 + ((hash >> 12) % 10) as i64)?;
        let base_z = coord
            .world_min_z()?
            .checked_add(3 + ((hash >> 26) % 10) as i64)?;
        Some((hash, base_x, base_z))
    })();
    [(0, 0), (2, 0), (0, 2), (-2, 0), (0, -2)]
        .into_iter()
        .filter_map(move |(dx, dz)| {
            let (hash, base_x, base_z) = origin?;
            let x = base_x.checked_add(dx)?;
            let z = base_z.checked_add(dz)?;
            let y = terrain_height(profile.seed(), x, z).checked_add(1)?;
            let (commodity, mass) = seeded_food(profile, hash, x, z);
            Some((VoxelCoord::new(x, y, z), commodity, mass))
        })
}

/// Verify decoded rooted biomass against the original world seed without
/// consulting changed terrain, current fruit, exploration order or live
/// occupants. Any of the five original candidates was legal if earlier
/// sites were obstructed at the generation boundary.
pub(crate) fn seeded_perennial_matches(
    profile: WorldGenerationProfile,
    position: VoxelCoord,
    commodity: CommodityKey,
    original_fruit: Mass,
) -> bool {
    seeded_forage_candidates(profile, ChunkCoord::containing(position)).any(
        |(candidate, food, mass)| {
            candidate == position && food == commodity && mass == original_fruit
        },
    )
}

/// The sparse site is selected by district, not by independent chunk rolls.
/// Only already-generated world cells and co-located owned matter may prevent
/// its admission; missing neighbors do not change its identity.
pub(super) fn seeded_forage_spec(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    chunk: &WorldChunk,
) -> Option<SurfaceResourceSpec> {
    let coord = chunk.coord();
    // The district has already received its single seeded shrub if even a
    // depleted perennial record survives. Do not reroll alternate sites or
    // mint new biomass on expansion after that patch was harvested.
    if state
        .surface()
        .perennial_resources_in_chunk(coord)
        .next()
        .is_some()
    {
        return None;
    }
    let tree = super::super::generated_tree(profile, chunk);
    for (position, commodity, mass) in seeded_forage_candidates(profile, coord) {
        let floor = VoxelCoord::new(position.x(), position.y().checked_sub(1)?, position.z());
        let head = VoxelCoord::new(position.x(), position.y().checked_add(1)?, position.z());
        if chunk.block_at(floor) != BLOCK_STATE_STONE
            || chunk.block_at(position) != BlockStateId::AIR
            || chunk.block_at(head) != BlockStateId::AIR
            || tree.is_some_and(|tree| tree.occupies_trunk(position))
            || state.structures().blocks_full_cell(position)
            || state.logistics().has_stationary_occupant_at(position)
            || state
                .surface()
                .available_resources_at(Some(position))
                .next()
                .is_some()
        {
            continue;
        }
        if !registries.materials().has_commodity(commodity) {
            return None;
        }
        let spec = SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            commodity,
            mass,
            AMBIENT_TEMPERATURE,
            MaterialComposition::pure(commodity.material()),
        )
        .expect("built-in forage has nonzero mass and pure registered material")
        .with_perennial_reserve(Mass::from_milligrams(mass.milligrams() / 2));
        // A chunk generated after the first senescence still represents the
        // same plant born at world tick zero. Do not offer an old crop again
        // merely because the player reached this location late. Subsequent
        // ripening remains an actual weather-dependent conserved transfer.
        let first_senescence =
            SimulationTick::new(super::super::forage_cycle::FRUIT_SENESCENCE_TICKS);
        return Some(if state.tick() >= first_senescence {
            spec.with_elapsed_perennial_senescence(first_senescence)
        } else {
            spec
        });
    }
    None
}
