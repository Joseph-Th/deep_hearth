//! Terrain, resident, structure, and custody collision checks for generated cells.

use super::temple_presence::{
    first_infrastructure_prospective_temple_collision, first_player_prospective_temple_collision,
    first_prospective_temple_overlap,
};
use super::{
    CHUNK_SIDE, ChunkCoord, ChunkGenerationError, WORLD_MIN_Y, WorldChunk, WorldGenerationProfile,
    WorldRegistry,
};
use crate::core::state::AppState;
use crate::logistics::first_player_body_collision_in_chunk;
use crate::registry::Registries;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::surface::SurfaceResourceLifecycle;
use crate::world::{BlockCollisionShape, WORLD_MAX_Y_EXCLUSIVE};

/// One shared physical admission for both direct and atomic neighborhood
/// generation; future occupied-matter categories must be checked in both.
pub(super) fn validate_generated_chunk_occupancy(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    chunk: &WorldChunk,
) -> Result<(), ChunkGenerationError> {
    let registry = registries.world();
    if let Some(voxel) = first_surface_resource_collision(registry, state, chunk) {
        return Err(ChunkGenerationError::SurfaceResourceCollision { voxel });
    }
    if let Some(voxel) = first_structure_collision_in_generated_chunk(registry, state, chunk) {
        return Err(ChunkGenerationError::StructureBuriedByTerrain { voxel });
    }
    if let Some(voxel) =
        first_stationary_enclosure_collision_in_generated_chunk(registry, state, chunk)
    {
        return Err(ChunkGenerationError::StorageEnclosureBuriedByTerrain { voxel });
    }
    if let Some(voxel) = first_ground_matter_collision_in_generated_chunk(registry, state, chunk) {
        return Err(ChunkGenerationError::GroundMatterBuriedByTerrain { voxel });
    }
    if let Some(voxel) = first_fluid_store_collision_in_generated_chunk(registry, state, chunk) {
        return Err(ChunkGenerationError::FluidStoreBuriedByTerrain { voxel });
    }
    if let Some(voxel) = first_energy_store_collision_in_generated_chunk(registry, state, chunk) {
        return Err(ChunkGenerationError::EnergyStoreBuriedByTerrain { voxel });
    }
    if let Some(voxel) = first_equipment_collision_in_generated_chunk(registry, state, chunk) {
        return Err(ChunkGenerationError::EquipmentBuriedByTerrain { voxel });
    }
    if let Some(player) = state.logistics().player()
        && let Some(voxel) = first_player_body_collision_in_chunk(
            registry,
            chunk,
            player.body(),
            registries.survival().physiology().locomotion(),
        )
    {
        return Err(ChunkGenerationError::PlayerBodyCollision { voxel });
    }
    if let Some(voxel) = first_opening_resident_collision_in_chunk(registry, state, chunk) {
        return Err(ChunkGenerationError::OpeningResidentBodyCollision { voxel });
    }
    let coord = chunk.coord();
    if let Some(voxel) = first_prospective_temple_overlap(registry, profile, coord, |cell| {
        crate::kobolds::opening_kobold_occupies_voxel(state, cell)
    }) {
        return Err(ChunkGenerationError::OpeningResidentTempleCasteCollision { voxel });
    }
    if let Some(voxel) =
        first_player_prospective_temple_collision(registries, state, profile, coord)
    {
        return Err(ChunkGenerationError::PlayerBodyTempleCasteCollision { voxel });
    }
    if let Some(voxel) =
        first_infrastructure_prospective_temple_collision(registries, state, profile, coord)
    {
        return Err(ChunkGenerationError::TempleCasteInfrastructureCollision { voxel });
    }
    if let Some(tree) = super::super::generated_tree(profile, chunk) {
        for voxel in [
            tree.root(),
            VoxelCoord::new(tree.root().x(), tree.root().y() + 1, tree.root().z()),
        ] {
            if crate::logistics::player_body_occupies_voxel(registries, state, voxel)
                || crate::kobolds::opening_kobold_occupies_voxel(state, voxel)
                || crate::kobolds::temple_caste_occupies_voxel(registries, state, voxel)
                || state.structures().has_embodied_member_at(voxel)
                || state.logistics().has_stationary_occupant_at(voxel)
                || state
                    .surface()
                    .available_resources_at(Some(voxel))
                    .next()
                    .is_some()
                || state.surface().rooted_biomass_occupies_voxel(voxel)
            {
                return Err(ChunkGenerationError::TreePlacementObstructed { voxel });
            }
        }
    }
    Ok(())
}

/// A live opening colony can inhabit authored route cells before chunk
/// generation. Do not create full-cube terrain through the scout or the
/// watchman's two-cell body; unoccupied future route cells remain subject
/// to the ordinary movement clearance gate.
fn first_opening_resident_collision_in_chunk(
    registry: &WorldRegistry,
    state: &AppState,
    chunk: &WorldChunk,
) -> Option<VoxelCoord> {
    let residents = crate::kobolds::opening_kobold_positions(state)?;
    for resident in residents {
        for y in [Some(resident.y()), resident.y().checked_add(1)]
            .into_iter()
            .flatten()
        {
            let cell = VoxelCoord::new(resident.x(), y, resident.z());
            if ChunkCoord::containing(cell) == chunk.coord()
                && (!(WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&cell.y())
                    || registry
                        .block_for_state(chunk.block_at(cell))
                        .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube))
            {
                return Some(cell);
            }
        }
    }
    None
}

/// Newly generated solid cells cannot bury exposed matter or still-living
/// roots. A fully exhausted historical parcel has no physical residue.
fn first_surface_resource_collision(
    registry: &WorldRegistry,
    state: &AppState,
    chunk: &WorldChunk,
) -> Option<VoxelCoord> {
    let surface = state.surface();
    surface
        .available_resources_in_chunk(chunk.coord())
        .chain(surface.perennial_resources_in_chunk(chunk.coord()).filter(|resource| {
            resource.lifecycle() == SurfaceResourceLifecycle::Depleted
                && resource.rooted_biomass_occupies_site()
        }))
        .filter(|resource| {
            registry
                .block_for_state(chunk.block_at(resource.position()))
                .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
        })
        // Report the lowest parcel ID when multiple resources conflict,
        // independently of spatial-index traversal order.
        .min_by_key(|resource| resource.id())
        .map(|resource| resource.position())
}

/// A located fluid vessel has physical presence even if its contents are empty.
/// New solid rock cannot appear around an already-located vessel.
fn first_fluid_store_collision_in_generated_chunk(
    registry: &WorldRegistry,
    state: &AppState,
    chunk: &WorldChunk,
) -> Option<VoxelCoord> {
    state
        .logistics()
        .fluid_stores_in_chunk(chunk.coord())
        .filter(|&(_, voxel)| {
            (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&voxel.y())
                && registry
                    .block_for_state(chunk.block_at(voxel))
                    .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
        })
        .min_by_key(|(id, _)| *id)
        .map(|(_, voxel)| voxel)
}

/// Material-built campfires, engines and energy buffers occupy real stationary
/// matter. Fixture-only stores without embodied material do not obstruct rock.
fn first_energy_store_collision_in_generated_chunk(
    registry: &WorldRegistry,
    state: &AppState,
    chunk: &WorldChunk,
) -> Option<VoxelCoord> {
    state
        .logistics()
        .energy_stores_in_chunk(chunk.coord())
        .filter(|&(id, voxel)| {
            state
                .energy()
                .get_store(id)
                .is_some_and(|store| !store.embodied_material().is_empty())
                && (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&voxel.y())
                && registry
                    .block_for_state(chunk.block_at(voxel))
                    .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
        })
        .min_by_key(|(id, _)| *id)
        .map(|(_, voxel)| voxel)
}

/// Mass-bearing stationary equipment is physical even when its definition
/// came from a controlled fixture rather than material-traced assembly.
fn first_equipment_collision_in_generated_chunk(
    registry: &WorldRegistry,
    state: &AppState,
    chunk: &WorldChunk,
) -> Option<VoxelCoord> {
    state
        .logistics()
        .equipment_in_chunk(chunk.coord())
        .filter(|&(id, voxel)| {
            state
                .equipment()
                .get_equipment(id)
                .is_some_and(|record| !record.embodied_mass().is_zero())
                && (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&voxel.y())
                && registry
                    .block_for_state(chunk.block_at(voxel))
                    .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
        })
        .min_by_key(|(id, _)| *id)
        .map(|(_, voxel)| voxel)
}

/// Finished stationary enclosure bodies are physical custody, unlike loose
/// unenclosed stockpiles. Only their actual located voxels obstruct a future
/// full-cube terrain block; no remote stockpile inventory is scanned.
fn first_stationary_enclosure_collision_in_generated_chunk(
    registry: &WorldRegistry,
    state: &AppState,
    chunk: &WorldChunk,
) -> Option<VoxelCoord> {
    state
        .logistics()
        .stockpiles_in_chunk(chunk.coord())
        .filter(|&(id, voxel)| {
            state
                .inventory()
                .get_stockpile(id)
                .is_some_and(|stockpile| stockpile.enclosure().is_some())
                && (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&voxel.y())
                && registry
                    .block_for_state(chunk.block_at(voxel))
                    .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
        })
        .min_by_key(|(id, _)| *id)
        .map(|(_, voxel)| voxel)
}

/// Loose ground stockpiles may be empty location markers, but their stored
/// matter and committed incoming deliveries cannot be embedded in new rock.
/// Enclosures use their own earlier physical-collision diagnostic.
fn first_ground_matter_collision_in_generated_chunk(
    registry: &WorldRegistry,
    state: &AppState,
    chunk: &WorldChunk,
) -> Option<VoxelCoord> {
    state
        .logistics()
        .stockpiles_in_chunk(chunk.coord())
        .filter(|&(id, voxel)| {
            state.inventory().get_stockpile(id).is_some_and(|pile| {
                pile.enclosure().is_none()
                    && (!pile.stored_mass().is_zero() || !pile.reserved_inbound().is_zero())
            }) && (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&voxel.y())
                && registry
                    .block_for_state(chunk.block_at(voxel))
                    .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
        })
        .min_by_key(|(id, _)| *id)
        .map(|(_, voxel)| voxel)
}

/// A built member has physical matter even when its narrow cross-section
/// does not fill the entire voxel. A newly generated full cube cannot
/// intersect any such portion. Limit the scan to the finite new chunk rather
/// than expanding potentially huge, user-authored member bounds.
fn first_structure_collision_in_generated_chunk(
    registry: &WorldRegistry,
    state: &AppState,
    chunk: &WorldChunk,
) -> Option<VoxelCoord> {
    let min_x = chunk.coord().world_min_x()?;
    let min_z = chunk.coord().world_min_z()?;
    let max_x = min_x + (CHUNK_SIDE - 1) as i64;
    let max_z = min_z + (CHUNK_SIDE - 1) as i64;
    state
        .structures()
        .elements_intersecting_horizontal_columns(min_x, max_x, min_z, max_z)
        .filter(|element| !element.embodied_material().is_empty())
        .find_map(|element| {
            first_solid_block_intersecting_bounds(registry, chunk, element.bounds())
        })
}

fn first_solid_block_intersecting_bounds(
    registry: &WorldRegistry,
    chunk: &WorldChunk,
    bounds: VoxelBounds,
) -> Option<VoxelCoord> {
    let min_x = chunk.coord().world_min_x()?;
    let min_z = chunk.coord().world_min_z()?;
    let last_x = min_x + (CHUNK_SIDE - 1) as i64;
    let last_z = min_z + (CHUNK_SIDE - 1) as i64;
    let start = bounds.min();
    let end = bounds.max_exclusive();
    let x0 = start.x().max(min_x);
    let x1 = (end.x() - 1).min(last_x);
    let y0 = start.y().max(WORLD_MIN_Y);
    let y1 = (end.y() - 1).min(WORLD_MAX_Y_EXCLUSIVE - 1);
    let z0 = start.z().max(min_z);
    let z1 = (end.z() - 1).min(last_z);
    if x0 > x1 || y0 > y1 || z0 > z1 {
        return None;
    }
    for z in z0..=z1 {
        for x in x0..=x1 {
            for y in y0..=y1 {
                let voxel = VoxelCoord::new(x, y, z);
                if registry
                    .block_for_state(chunk.block_at(voxel))
                    .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
                {
                    return Some(voxel);
                }
            }
        }
    }
    None
}

/// Prospective materialization clearance. Inert plans may overlap stone;
/// embodying their material must first prove the entire reserved volume clear.
pub(crate) fn first_solid_block_in_structural_bounds(
    registries: &Registries,
    state: &AppState,
    bounds: VoxelBounds,
) -> Option<VoxelCoord> {
    if let Some(voxel) = state.world().first_tree_trunk_in_bounds(bounds) {
        return Some(voxel);
    }
    state
        .world()
        .chunks_intersecting_horizontal_bounds(bounds)
        .find_map(|chunk| first_solid_block_intersecting_bounds(registries.world(), chunk, bounds))
}

/// Trusted-load counterpart of prospective terrain admission. Both paths
/// test the same physical bounds against exactly the existing block truth.
pub(crate) fn first_embodied_structure_collision_in_world(
    registries: &Registries,
    state: &AppState,
) -> Option<VoxelCoord> {
    state.world().chunks().find_map(|chunk| {
        first_structure_collision_in_generated_chunk(registries.world(), state, chunk)
    })
}

/// Trusted-load counterpart of prospective enclosure terrain admission.
/// Only generated block truth may bury a completed stationary enclosure.
pub(crate) fn first_buried_stationary_enclosure_in_world(
    registries: &Registries,
    state: &AppState,
) -> Option<VoxelCoord> {
    // This validation runs on every trusted load, including worlds with many
    // generated columns and no enclosures. Query each located enclosure against
    // the indexed block world once instead of scanning every generated column.
    // Preserve chunk-first, then stockpile-ID-first diagnostic ordering.
    state
        .logistics()
        .stockpile_locations()
        .filter(|&(stockpile, _)| {
            state
                .inventory()
                .get_stockpile(stockpile)
                .is_some_and(|record| record.enclosure().is_some())
        })
        .filter(|&(_, voxel)| {
            state
                .world()
                .block_at(voxel)
                .and_then(|block| registries.world().block_for_state(block))
                .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
        })
        .min_by_key(|(stockpile, voxel)| (ChunkCoord::containing(*voxel), *stockpile))
        .map(|(_, voxel)| voxel)
}
