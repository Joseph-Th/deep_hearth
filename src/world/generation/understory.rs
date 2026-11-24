//! Independent ground-layer biology alongside berry and nut-bearing shrubs.
//!
//! Dry scrub supports buried starchy tubers; damp groves support edible fungal
//! fruiting bodies. Mixed woodland supports both in seed-varying patches.
//! Digging tubers removes finite one-time plant matter. Fungal fruiting bodies
//! instead have an explicitly owned finite mycelium reserve, so subsequent
//! seasonal crops must draw down the same real organic material.

use crate::content::{BLOCK_STATE_STONE, FORM_FOOD, MATERIAL_EDIBLE_FUNGI, MATERIAL_WILD_TUBERS};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, MaterialComposition};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec};

use super::terrain::{mix64, terrain_height};
use super::{BlockStateId, ChunkCoord, HabitatKind, WorldChunk, WorldGenerationProfile};

const UNDERSTORY_SALT: u64 = 0x554E_4445_5253_5452;
const AMBIENT_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn district_hash(profile: WorldGenerationProfile, coord: ChunkCoord) -> u64 {
    mix64(
        profile.seed().value()
            ^ (coord.x().div_euclid(2) as u64).wrapping_mul(0xD9AB_BABC_D548_3F2B)
            ^ (coord.z().div_euclid(2) as u64).wrapping_mul(0xB9C4_325F_46A1_922E)
            ^ UNDERSTORY_SALT,
    )
}

/// A district has at most one low-growth site. Unlike artificial starter
/// resources, missing plants never get rerolled around the player.
fn site_origin(profile: WorldGenerationProfile, coord: ChunkCoord) -> Option<(u64, i64, i64)> {
    if profile.solid_state() != BLOCK_STATE_STONE {
        return None;
    }
    let hash = district_hash(profile, coord);
    if coord.x().rem_euclid(2) as u64 != hash & 1
        || coord.z().rem_euclid(2) as u64 != (hash >> 1) & 1
    {
        return None;
    }
    let x = coord
        .world_min_x()?
        .checked_add(3 + ((hash >> 10) % 10) as i64)?;
    let z = coord
        .world_min_z()?
        .checked_add(3 + ((hash >> 23) % 10) as i64)?;
    Some((hash, x, z))
}

fn understory_crop(habitat: HabitatKind, hash: u64) -> Option<(CommodityKey, Mass)> {
    let (material, base, spread) = match habitat {
        HabitatKind::DryScrub if (hash >> 43) & 7 != 0 => (MATERIAL_WILD_TUBERS, 350_000, 450_000),
        HabitatKind::DampGrove if (hash >> 43) & 7 < 6 => (MATERIAL_EDIBLE_FUNGI, 160_000, 400_000),
        HabitatKind::MixedWoodland if (hash >> 43) & 3 != 0 => {
            if (hash >> 47) & 1 == 0 {
                (MATERIAL_WILD_TUBERS, 250_000, 300_000)
            } else {
                (MATERIAL_EDIBLE_FUNGI, 120_000, 220_000)
            }
        }
        _ => return None,
    };
    Some((
        CommodityKey::new(material, FORM_FOOD),
        Mass::from_milligrams(base + ((hash >> 30) % spread)),
    ))
}

/// Enumerate the same seed-bound sites for natural generation and trusted load.
/// A candidate's habitat may not support this district's crop even when a
/// nearby alternative does.
fn seeded_understory_candidates(
    profile: WorldGenerationProfile,
    coord: ChunkCoord,
) -> impl Iterator<Item = (VoxelCoord, CommodityKey, Mass)> {
    let origin = site_origin(profile, coord);
    [(0, 0), (2, 0), (0, 2), (-2, 0), (0, -2)]
        .into_iter()
        .filter_map(move |(dx, dz)| {
            let (hash, base_x, base_z) = origin?;
            let x = base_x.checked_add(dx)?;
            let z = base_z.checked_add(dz)?;
            let y = terrain_height(profile.seed(), x, z).checked_add(1)?;
            let (commodity, mass) = understory_crop(super::habitat_at(profile.seed(), x, z), hash)?;
            Some((VoxelCoord::new(x, y, z), commodity, mass))
        })
}

/// Trusted-load validation checks the original seed and species without
/// trusting harvest history, elapsed seasons, or changed physical occupancy.
pub(crate) fn seeded_fungal_colony_matches(
    profile: WorldGenerationProfile,
    position: VoxelCoord,
    commodity: CommodityKey,
    original_fruit: Mass,
) -> bool {
    if commodity != CommodityKey::new(MATERIAL_EDIBLE_FUNGI, FORM_FOOD) {
        return false;
    }
    seeded_understory_candidates(profile, ChunkCoord::containing(position)).any(
        |(candidate, food, mass)| {
            candidate == position && food == commodity && mass == original_fruit
        },
    )
}

/// The same deterministic candidate geometry is used for generation and
/// negative terrain-coordinate coverage. No generated plant occupies a trunk,
/// standing spring, existing shrub or independently placed raw material.
pub(super) fn seeded_understory_spec(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    chunk: &WorldChunk,
    reserved: &[VoxelCoord],
) -> Option<SurfaceResourceSpec> {
    let tree = super::super::generated_tree(profile, chunk);
    for (position, commodity, mass) in seeded_understory_candidates(profile, chunk.coord()) {
        let (x, y, z) = (position.x(), position.y(), position.z());
        let floor = VoxelCoord::new(x, y - 1, z);
        let head = VoxelCoord::new(x, y + 1, z);
        if chunk.block_at(floor) != BLOCK_STATE_STONE
            || chunk.block_at(position) != BlockStateId::AIR
            || chunk.block_at(head) != BlockStateId::AIR
            || tree.is_some_and(|tree| tree.occupies_trunk(position))
            || reserved.contains(&position)
            || state.structures().blocks_full_cell(position)
            || state.logistics().has_stationary_occupant_at(position)
            || state.logistics().fluid_stores_at(position).next().is_some()
            || state
                .surface()
                .available_resources_at(Some(position))
                .next()
                .is_some()
            || state.surface().rooted_biomass_occupies_voxel(position)
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
        .expect("seeded edible understory has positive physical biomass");
        if commodity.material() == MATERIAL_EDIBLE_FUNGI {
            let spec = spec.with_perennial_reserve(Mass::from_milligrams(mass.milligrams() / 2));
            let first_senescence =
                SimulationTick::new(super::super::forage_cycle::FRUIT_SENESCENCE_TICKS);
            return Some(if state.tick() >= first_senescence {
                spec.with_elapsed_perennial_senescence(first_senescence)
            } else {
                spec
            });
        }
        return Some(spec);
    }
    None
}

#[cfg(test)]
#[path = "understory_tests.rs"]
mod tests;
