//! Persistent wilderness forage attached to actual generated terrain.
//!
//! One sparse seed-selected site per two-by-two chunk district can become a
//! finite edible opportunity. Terrain and surface ownership commit atomically;
//! no background respawn or player-centered loot roll is involved.

#[cfg(test)]
#[path = "forage_tests.rs"]
mod tests;

use crate::content::{BLOCK_STATE_STONE, FORM_FOOD, MATERIAL_BERRIES, MATERIAL_WILD_NUTS};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::material::{CommodityKey, MaterialComposition};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec};

use super::terrain::{mix64, terrain_height};
use super::{BlockStateId, WorldChunk, WorldGenerationProfile};

const AMBIENT_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

/// The sparse site is selected by district, not by independent chunk rolls.
/// Only already-generated world cells and co-located owned matter may prevent
/// its admission; missing neighbors do not change its identity.
pub(super) fn seeded_forage_spec(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    chunk: &WorldChunk,
) -> Option<SurfaceResourceSpec> {
    if profile.solid_state() != BLOCK_STATE_STONE {
        return None;
    }
    let coord = chunk.coord();
    let hash = mix64(
        profile.seed().value()
            ^ (coord.x().div_euclid(2) as u64).wrapping_mul(0x8EBC_6AF0_9C88_C6E3)
            ^ (coord.z().div_euclid(2) as u64).wrapping_mul(0x5899_65CC_7537_4CC3)
            ^ 0x464F_5241_4745_5F57,
    );
    if coord.x().rem_euclid(2) as u64 != hash & 1
        || coord.z().rem_euclid(2) as u64 != (hash >> 1) & 1
    {
        return None;
    }
    let material = if hash & 4 == 0 {
        MATERIAL_BERRIES
    } else {
        MATERIAL_WILD_NUTS
    };
    let commodity = CommodityKey::new(material, FORM_FOOD);
    if !registries.materials().has_commodity(commodity) {
        return None;
    }
    let min_x = coord.world_min_x()?;
    let min_z = coord.world_min_z()?;
    // A small number of alternate rooted sites prevents a single tree or
    // already placed structure from erasing every nearby food opportunity.
    let base_x = min_x.checked_add(3 + ((hash >> 12) % 10) as i64)?;
    let base_z = min_z.checked_add(3 + ((hash >> 26) % 10) as i64)?;
    let tree = super::super::generated_tree(profile, chunk);
    for (offset_x, offset_z) in [(0, 0), (2, 0), (0, 2), (-2, 0), (0, -2)] {
        let x = base_x.checked_add(offset_x)?;
        let z = base_z.checked_add(offset_z)?;
        let floor = VoxelCoord::new(x, terrain_height(profile.seed(), x, z), z);
        let position = VoxelCoord::new(x, floor.y().checked_add(1)?, z);
        if chunk.block_at(floor) != BLOCK_STATE_STONE
            || chunk.block_at(position) != BlockStateId::AIR
            || chunk.block_at(VoxelCoord::new(x, position.y().checked_add(1)?, z))
                != BlockStateId::AIR
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
        // Each patch supplies a few finite, human-scale servings. Existing
        // food physiology and spoilage own its utility and deterioration.
        let mass = Mass::from_milligrams(250_000 + ((hash >> 40) % 600_000));
        return Some(
            SurfaceResourceSpec::new(
                SurfaceResourceKind::Opportunity,
                position,
                commodity,
                mass,
                AMBIENT_TEMPERATURE,
                MaterialComposition::pure(material),
            )
            .expect("built-in forage has nonzero mass and pure registered material"),
        );
    }
    None
}
