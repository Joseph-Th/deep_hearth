//! Seeded temple presence, candidate resident footprints, and local discovery.

use super::{ChunkCoord, WorldGenerationProfile, WorldRegistry, temple};
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

pub(super) fn temple_site_for_chunk(
    registry: &WorldRegistry,
    profile: WorldGenerationProfile,
    coord: ChunkCoord,
) -> Option<temple::TempleSite> {
    // Optional capability registries may omit this entire content set.
    // Never instantiate only half a temple and then unwrap missing relic mass.
    [
        crate::content::BLOCK_STATE_TEMPLE_ARCHIVE,
        crate::content::BLOCK_STATE_TEMPLE_DRIVE,
        crate::content::BLOCK_STATE_TEMPLE_SEAL,
        crate::content::BLOCK_STATE_TEMPLE_TIN_DATA_PLATE,
        crate::content::BLOCK_STATE_TEMPLE_IRON_BEARING_HOUSING,
        crate::content::BLOCK_STATE_TEMPLE_REFRACTORY_FIXTURE,
        crate::content::BLOCK_STATE_TEMPLE_ZINC_CORROSION_PANEL,
    ]
    .into_iter()
    .all(|state| registry.get_state(state).is_some())
    .then(|| temple::TempleSite::for_chunk(profile.seed(), coord))
    .flatten()
}

/// A not-yet-generated temple is already part of the deterministic world
/// identity. Check its finite resident footprint before publishing the chunk,
/// including when the prospective terrain cells themselves are air.
pub(super) fn first_player_prospective_temple_collision(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    coord: ChunkCoord,
) -> Option<VoxelCoord> {
    state.logistics().player()?;
    first_prospective_temple_overlap(registries.world(), profile, coord, |voxel| {
        crate::logistics::player_body_occupies_voxel(registries, state, voxel)
    })
}

pub(super) fn first_infrastructure_prospective_temple_collision(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    coord: ChunkCoord,
) -> Option<VoxelCoord> {
    first_prospective_temple_overlap(registries.world(), profile, coord, |voxel| {
        state.logistics().has_stationary_occupant_at(voxel)
            || state
                .structures()
                .elements_covering(voxel)
                .any(|element| !element.embodied_material().is_empty())
    })
}

/// Use one finite resident footprint for both player and durable-custody
/// admission. All 36 candidate bounding cells are checked without scanning
/// unrelated world history or constructing a temporary generated world.
pub(super) fn first_prospective_temple_overlap(
    registry: &WorldRegistry,
    profile: WorldGenerationProfile,
    coord: ChunkCoord,
    mut overlaps: impl FnMut(VoxelCoord) -> bool,
) -> Option<VoxelCoord> {
    let site = temple_site_for_chunk(registry, profile, coord)?;
    let warden = site.warden();
    let archivist = site.archivist();
    for y in warden.y()..=warden.y() + 2 {
        for x in warden.x()..=warden.x() + 1 {
            for z in archivist.z() - 1..=warden.z() {
                let voxel = VoxelCoord::new(x, y, z);
                if crate::kobolds::initial_temple_caste_occupies_voxel(voxel, warden, archivist)
                    && overlaps(voxel)
                {
                    return Some(voxel);
                }
            }
        }
    }
    None
}

/// Resolves one site in an already-generated seeded world column. Absent
/// authored relic definitions never produce an ancient sanctuary.
fn temple_site_in_chunk(
    registries: &Registries,
    state: &AppState,
    coord: ChunkCoord,
) -> Option<temple::TempleSite> {
    let profile = state.world().generation_profile()?;
    // A seeded site is deterministic, but its heavy occupants do not exist
    // before the containing chunk is generated. Otherwise generating an
    // unrelated distant chunk can retroactively overlap an admitted player
    // at an unloaded sanctuary.
    state.world().get_chunk(coord)?;
    temple_site_for_chunk(registries.world(), profile, coord)
}

fn temple_site_at(
    registries: &Registries,
    state: &AppState,
    observer: VoxelCoord,
) -> Option<temple::TempleSite> {
    temple_site_in_chunk(registries, state, ChunkCoord::containing(observer))
}

/// Returns original temple resident identities, not their current mutable
/// positions. Kobold ownership resolves the warden's displacement; true
/// local sight prevents obstructed or distant actors from being revealed.
pub(crate) fn temple_residents_at(
    registries: &Registries,
    state: &AppState,
    observer: VoxelCoord,
) -> Option<[VoxelCoord; 2]> {
    let site = temple_site_at(registries, state, observer)?;
    Some([site.warden(), site.archivist()])
}

/// Resident candidates in the observer's column and its eight immediate neighbors.
/// A caste may be only a few cells away across a chunk boundary. Return only
/// already-generated sanctuaries; callers still enforce their own physical
/// observation radius or occluded sight before any behavior or presentation.
/// No world-wide temple index or distant generation is consulted.
pub(crate) fn nearby_temple_residents<'a>(
    registries: &'a Registries,
    state: &'a AppState,
    observer: VoxelCoord,
) -> impl Iterator<Item = [VoxelCoord; 2]> + 'a {
    let origin = ChunkCoord::containing(observer);
    (-1_i64..=1).flat_map(move |dx| {
        (-1_i64..=1).filter_map(move |dz| {
            let chunk = ChunkCoord::new(origin.x().checked_add(dx)?, origin.z().checked_add(dz)?);
            temple_site_in_chunk(registries, state, chunk)
                .map(|site| [site.warden(), site.archivist()])
        })
    })
}

/// Three physically adjacent cells, entirely inside a generated ancient hall.
/// The caller still checks collision and line of sight before moving a resident.
pub(crate) fn temple_warden_route_at(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> Option<[VoxelCoord; 3]> {
    temple_site_at(registries, state, position).map(temple::TempleSite::warden_route)
}
