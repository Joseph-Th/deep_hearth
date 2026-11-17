//! Sparse natural loose material, separate from rooted food and buried ore.
//!
//! Seeded 32 m districts expose finite stone, flint, clay, or fallen timber
//! depending on their real habitat. This supports open-ended primitive play
//! without the guaranteed resources or hand-pickable copper of the tutorial.
//! No resource appears just because the player is low on supplies.

use crate::content::{
    BLOCK_STATE_STONE, FORM_LOG, FORM_LUMP, MATERIAL_CLAY, MATERIAL_FLINT, MATERIAL_STONE,
    MATERIAL_WOOD,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::material::{CommodityKey, MaterialComposition};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec};

use super::terrain::{mix64, terrain_height};
use super::{BlockStateId, ChunkCoord, HabitatKind, WorldChunk, WorldGenerationProfile};

const SITE_SALT: u64 = 0x4C4F_4F53_455F_5349;
const AMBIENT_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn district_hash(profile: WorldGenerationProfile, coord: ChunkCoord) -> u64 {
    mix64(
        profile.seed().value()
            ^ (coord.x().div_euclid(2) as u64).wrapping_mul(0x69A2_3F35_860D_9D3B)
            ^ (coord.z().div_euclid(2) as u64).wrapping_mul(0x7B71_9D36_602A_FC3F)
            ^ SITE_SALT,
    )
}

fn district_site(profile: WorldGenerationProfile, coord: ChunkCoord) -> Option<(u64, i64, i64)> {
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
        .checked_add(3 + ((hash >> 9) % 10) as i64)?;
    let z = coord
        .world_min_z()?
        .checked_add(3 + ((hash >> 23) % 10) as i64)?;
    Some((hash, x, z))
}

fn loose_commodity(habitat: HabitatKind, choice: u64) -> CommodityKey {
    let (material, form) = match (habitat, choice % 8) {
        (HabitatKind::DampGrove, 0..=2) => (MATERIAL_CLAY, FORM_LUMP),
        (HabitatKind::DampGrove, 3..=5) => (MATERIAL_WOOD, FORM_LOG),
        (HabitatKind::DampGrove, 6) => (MATERIAL_STONE, FORM_LUMP),
        (HabitatKind::DampGrove, _) => (MATERIAL_FLINT, FORM_LUMP),
        (HabitatKind::DryScrub, 0..=2) => (MATERIAL_STONE, FORM_LUMP),
        (HabitatKind::DryScrub, 3..=5) => (MATERIAL_FLINT, FORM_LUMP),
        (HabitatKind::DryScrub, 6) => (MATERIAL_CLAY, FORM_LUMP),
        (HabitatKind::DryScrub, _) => (MATERIAL_WOOD, FORM_LOG),
        (HabitatKind::MixedWoodland, 0..=1) => (MATERIAL_STONE, FORM_LUMP),
        (HabitatKind::MixedWoodland, 2..=3) => (MATERIAL_FLINT, FORM_LUMP),
        (HabitatKind::MixedWoodland, 4..=5) => (MATERIAL_CLAY, FORM_LUMP),
        (HabitatKind::MixedWoodland, _) => (MATERIAL_WOOD, FORM_LOG),
    };
    CommodityKey::new(material, form)
}

fn loose_mass(hash: u64, commodity: CommodityKey) -> Mass {
    let variability = (hash >> 40) % 4_000_000;
    let minimum = if commodity.material() == MATERIAL_FLINT {
        750_000
    } else {
        2_000_000
    };
    Mass::from_milligrams(minimum + variability)
}

/// The terrain chunk is still provisional. Check its own actual floor, the
/// single prospective tree, and previously persisted occupants, rather than
/// assuming the old uniform-height terrain or overriding another resource.
pub(super) fn seeded_loose_material_spec(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    chunk: &WorldChunk,
    forage_site: Option<VoxelCoord>,
) -> Option<SurfaceResourceSpec> {
    let (hash, base_x, base_z) = district_site(profile, chunk.coord())?;
    let tree = super::super::generated_tree(profile, chunk);
    for (dx, dz) in [(0, 0), (2, 0), (0, 2), (-2, 0), (0, -2)] {
        let x = base_x.checked_add(dx)?;
        let z = base_z.checked_add(dz)?;
        let y = terrain_height(profile.seed(), x, z).checked_add(1)?;
        let position = VoxelCoord::new(x, y, z);
        let floor = VoxelCoord::new(x, y - 1, z);
        let head = VoxelCoord::new(x, y + 1, z);
        if chunk.block_at(floor) != BLOCK_STATE_STONE
            || chunk.block_at(position) != BlockStateId::AIR
            || chunk.block_at(head) != BlockStateId::AIR
            || tree.is_some_and(|record| record.occupies_trunk(position))
            || forage_site == Some(position)
            || state.structures().blocks_full_cell(position)
            || state.logistics().has_stationary_occupant_at(position)
            || state
                .surface()
                .available_resources_at(Some(position))
                .next()
                .is_some()
            || state.surface().rooted_biomass_occupies_voxel(position)
        {
            continue;
        }
        let commodity = loose_commodity(super::habitat_at(profile.seed(), x, z), hash >> 32);
        if !registries.materials().has_commodity(commodity) {
            return None;
        }
        return Some(
            SurfaceResourceSpec::new(
                SurfaceResourceKind::Opportunity,
                position,
                commodity,
                loose_mass(hash, commodity),
                AMBIENT_TEMPERATURE,
                MaterialComposition::pure(commodity.material()),
            )
            .expect("seeded loose resource has a positive mass and pure material"),
        );
    }
    None
}

#[cfg(test)]
#[path = "loose_materials_tests.rs"]
mod tests;
