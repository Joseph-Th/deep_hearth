//! Sparse natural loose material, separate from rooted food and buried ore.
//!
//! Seeded 32 m districts expose finite stone, flint, clay, or fallen timber
//! depending on their real habitat. Weathered rubble and fallen deadwood also
//! occur on suitable local terrain, without a guaranteed starter trail.
//! No resource appears just because the player is low on supplies.

use crate::content::{
    BLOCK_STATE_STONE, FORM_LOG, FORM_LUMP, FORM_ORE, MATERIAL_CLAY, MATERIAL_COPPER,
    MATERIAL_FLINT, MATERIAL_STONE, MATERIAL_WOOD,
};
use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::state::AppState;
use crate::geology::GeneratedDepositSpec;
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
use crate::registry::Registries;
use crate::spatial::{VoxelBounds, VoxelCoord};
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

/// All candidates belong to the provisional chunk and occupy real, open
/// standing space. Reservations include other sites in the same generation plan.
struct LooseSiteContext<'a> {
    registries: &'a Registries,
    state: &'a AppState,
    profile: WorldGenerationProfile,
    chunk: &'a WorldChunk,
    tree: Option<crate::world::TreeRecord>,
}

impl LooseSiteContext<'_> {
    fn position(&self, x: i64, z: i64) -> Option<VoxelCoord> {
        let y = terrain_height(self.profile.seed(), x, z).checked_add(1)?;
        let position = VoxelCoord::new(x, y, z);
        (ChunkCoord::containing(position) == self.chunk.coord()).then_some(position)
    }

    fn clear(&self, position: VoxelCoord, reserved: &[VoxelCoord]) -> bool {
        let (x, y, z) = (position.x(), position.y(), position.z());
        self.chunk.block_at(VoxelCoord::new(x, y - 1, z)) == BLOCK_STATE_STONE
            && self.chunk.block_at(position) == BlockStateId::AIR
            && self.chunk.block_at(VoxelCoord::new(x, y + 1, z)) == BlockStateId::AIR
            && !self.tree.is_some_and(|tree| tree.occupies_trunk(position))
            && !reserved.contains(&position)
            && !self.state.structures().blocks_full_cell(position)
            && !self.state.logistics().has_stationary_occupant_at(position)
            && self
                .state
                .surface()
                .available_resources_at(Some(position))
                .next()
                .is_none()
            && !self.state.surface().rooted_biomass_occupies_voxel(position)
    }

    fn spec(
        &self,
        position: VoxelCoord,
        commodity: CommodityKey,
        mass: Mass,
    ) -> Option<SurfaceResourceSpec> {
        self.registries
            .materials()
            .has_commodity(commodity)
            .then(|| {
                SurfaceResourceSpec::new(
                    SurfaceResourceKind::Opportunity,
                    position,
                    commodity,
                    mass,
                    AMBIENT_TEMPERATURE,
                    MaterialComposition::pure(commodity.material()),
                )
                .expect("seeded loose resource has a positive mass and pure material")
            })
    }
}

const SITE_OFFSETS: [(i64, i64); 5] = [(0, 0), (2, 0), (0, 2), (-2, 0), (0, -2)];

fn mixed_district_spec(
    context: &LooseSiteContext<'_>,
    reserved: &[VoxelCoord],
) -> Option<SurfaceResourceSpec> {
    let (hash, base_x, base_z) = district_site(context.profile, context.chunk.coord())?;
    for (dx, dz) in SITE_OFFSETS {
        let position = context.position(base_x.checked_add(dx)?, base_z.checked_add(dz)?)?;
        if !context.clear(position, reserved) {
            continue;
        }
        let commodity = loose_commodity(
            super::habitat_at(context.profile.seed(), position.x(), position.z()),
            hash >> 32,
        );
        return context.spec(position, commodity, loose_mass(hash, commodity));
    }
    None
}

fn patch_hash(profile: WorldGenerationProfile, coord: ChunkCoord, salt: u64) -> u64 {
    mix64(
        profile.seed().value()
            ^ (coord.x() as u64).wrapping_mul(0x69A2_3F35_860D_9D3B)
            ^ (coord.z() as u64).wrapping_mul(0x7B71_9D36_602A_FC3F)
            ^ salt,
    )
}

/// Weathered rubble collects below exposed slopes. Search a seed-permuted
/// lattice of interior sites: a rock face above the parcel and a level
/// neighboring surface make a much more useful find than a parcel on a cliff.
fn weathered_stone_spec(
    context: &LooseSiteContext<'_>,
    reserved: &[VoxelCoord],
) -> Option<SurfaceResourceSpec> {
    let coord = context.chunk.coord();
    let hash = patch_hash(context.profile, coord, 0x5745_4154_4845_5244);
    let min_x = coord.world_min_x()?;
    let min_z = coord.world_min_z()?;
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    // One finite rubble site in a suitable chunk; the permutation is independent
    // of exploration order and never creates loose rock in response to scarcity.
    for index in 0..49_u64 {
        let offset = (index * 17 + hash % 49) % 49;
        let position = context.position(
            min_x.checked_add(1 + (offset % 7) as i64 * 2)?,
            min_z.checked_add(1 + (offset / 7) as i64 * 2)?,
        )?;
        if context.clear(position, reserved) && weathered_stone_site(context, position) {
            return context.spec(position, commodity, loose_mass(hash, commodity));
        }
    }
    None
}

fn weathered_stone_site(context: &LooseSiteContext<'_>, position: VoxelCoord) -> bool {
    let mut exposed_face = false;
    let mut level_approach = false;
    for (dx, dz) in [(1_i64, 0_i64), (-1, 0), (0, 1), (0, -1)] {
        let (Some(x), Some(z)) = (position.x().checked_add(dx), position.z().checked_add(dz))
        else {
            continue;
        };
        let Some(neighbor) = context.position(x, z) else {
            continue;
        };
        exposed_face |= neighbor.y() > position.y()
            && context.chunk.block_at(VoxelCoord::new(x, position.y(), z)) == BLOCK_STATE_STONE;
        level_approach |= neighbor.y() == position.y()
            && context.chunk.block_at(neighbor) == BlockStateId::AIR
            && context
                .chunk
                .block_at(VoxelCoord::new(x, neighbor.y() + 1, z))
                == BlockStateId::AIR
            && !context
                .tree
                .is_some_and(|tree| tree.occupies_trunk(neighbor));
    }
    exposed_face && level_approach
}

/// Weathered copper-bearing rubble is a separate finite geological body at a
/// reachable rock face, not a duplicate of the solid block under the player.
/// Seeded independently of player needs; barren chunks remain possible.
pub(super) fn seeded_copper_spec(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    chunk: &WorldChunk,
    reserved: &[VoxelCoord],
) -> Option<GeneratedDepositSpec> {
    let commodity = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
    if profile.solid_state() != BLOCK_STATE_STONE
        || !registries.materials().has_commodity(commodity)
    {
        return None;
    }
    let coord = chunk.coord();
    let hash = patch_hash(profile, coord, 0x434F_5050_4552_534D);
    if !hash.is_multiple_of(3) {
        return None;
    }
    let context = LooseSiteContext {
        registries,
        state,
        profile,
        chunk,
        tree: super::super::generated_tree(profile, chunk),
    };
    let min_x = coord.world_min_x()?;
    let min_z = coord.world_min_z()?;
    for index in 0..49_u64 {
        let offset = (index * 17 + (hash >> 8) % 49) % 49;
        let position = context.position(
            min_x.checked_add(1 + (offset % 7) as i64 * 2)?,
            min_z.checked_add(1 + (offset / 7) as i64 * 2)?,
        )?;
        if !context.clear(position, reserved) || !weathered_stone_site(&context, position) {
            continue;
        }
        let bounds = VoxelBounds::new(
            position,
            VoxelCoord::new(
                position.x().checked_add(1)?,
                position.y().checked_add(1)?,
                position.z().checked_add(1)?,
            ),
        )
        .ok()?;
        if state
            .geology()
            .deposits()
            .any(|deposit| deposit.bounds().has_intersection(bounds))
        {
            continue;
        }
        let composition = MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_COPPER, 100_000),
            CompositionComponent::new(MATERIAL_STONE, 900_000),
        ])
        .expect("weathered copper composition is complete");
        return Some(
            GeneratedDepositSpec::new(
                bounds,
                commodity,
                Mass::from_milligrams(24_000_000 + (hash >> 32) % 12_000_000),
                AMBIENT_TEMPERATURE,
                Pressure::from_pascals(450_000_000),
                composition,
            )
            .expect("weathered copper specification is valid"),
        );
    }
    None
}

/// Fallen deadwood belongs to the seeded landscape, not to the living tree's
/// current biomass. A nearby tree identifies a plausible deadwood catchment;
/// a small seed-dependent share has no collectible deadwood.
fn deadwood_spec(
    context: &LooseSiteContext<'_>,
    reserved: &[VoxelCoord],
) -> Option<SurfaceResourceSpec> {
    let tree = context.tree?;
    let hash = patch_hash(
        context.profile,
        context.chunk.coord(),
        0x4445_4144_574F_4F44,
    );
    if hash.is_multiple_of(4) {
        return None;
    }
    let root = tree.root();
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let offsets = [(2_i64, 0_i64), (0, 2), (-2, 0), (0, -2), (2, 2), (-2, -2)];
    for index in 0..offsets.len() {
        let (dx, dz) = offsets[(index + (hash % offsets.len() as u64) as usize) % offsets.len()];
        let Some(position) = root
            .x()
            .checked_add(dx)
            .zip(root.z().checked_add(dz))
            .and_then(|(x, z)| context.position(x, z))
        else {
            continue;
        };
        if context.clear(position, reserved) {
            return context.spec(position, commodity, loose_mass(hash, commodity));
        }
    }
    None
}

/// Keep the scarce mixed-material district identity, then add physically
/// justified rubble and deadwood. All sites share the same custody admission
/// and are created only during first-time chunk generation.
pub(super) fn seeded_loose_material_specs(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    chunk: &WorldChunk,
    forage_site: Option<VoxelCoord>,
) -> Vec<SurfaceResourceSpec> {
    if profile.solid_state() != BLOCK_STATE_STONE {
        return Vec::new();
    }
    let context = LooseSiteContext {
        registries,
        state,
        profile,
        chunk,
        tree: super::super::generated_tree(profile, chunk),
    };
    let mut reserved = forage_site.into_iter().collect::<Vec<_>>();
    let mut specs = Vec::new();
    for generate in [mixed_district_spec, weathered_stone_spec, deadwood_spec] {
        if let Some(spec) = generate(&context, &reserved) {
            reserved.push(spec.position());
            specs.push(spec);
        }
    }
    specs
}

#[cfg(test)]
#[path = "loose_materials_tests.rs"]
mod tests;
