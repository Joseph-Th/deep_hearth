//! Physical overlap and trusted occupancy validation for generated temple castes.

use super::warden_body_cells;
use crate::core::state::AppState;
use crate::kobolds::{current_temple_caste_positions, temple_archivist::resident_body_cells};
use crate::registry::Registries;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::world::ChunkCoord;

/// Validate the current physical body of every generated caste, including
/// implicit home posts. A missing supporting floor may strand a resident,
/// but neither solid terrain nor stationary infrastructure can occupy its body.
pub(in crate::kobolds) fn validate_temple_occupancy(
    registries: &Registries,
    state: &AppState,
) -> Result<(), &'static str> {
    // At trusted load, both cursor collections have already been validated.
    // Visit each generated sanctuary once, then consult the finite body cells
    // and the same rebuilt spatial indexes used by movement and placement.
    for chunk in state.world().chunks() {
        let Some(x) = chunk.coord().world_min_x().and_then(|x| x.checked_add(8)) else {
            continue;
        };
        let Some(z) = chunk.coord().world_min_z().and_then(|z| z.checked_add(8)) else {
            continue;
        };
        let observer = VoxelCoord::new(x, -19, z);
        let Some(homes) = crate::world::temple_residents_at(registries, state, observer) else {
            continue;
        };
        let [warden, archivist] = current_temple_caste_positions(registries, state, homes)
            .ok_or("temple caste lacks a representable current position")?;
        validate_body_clearance(
            registries,
            state,
            warden_body_cells(warden)
                .ok_or("kobold temple warden occupies obstructed or absent world geometry")?,
            "kobold temple warden occupies obstructed or absent world geometry",
        )?;
        validate_body_clearance(
            registries,
            state,
            resident_body_cells(archivist),
            "temple archivist is embedded in missing or solid world",
        )?;
    }
    Ok(())
}

fn validate_body_clearance(
    registries: &Registries,
    state: &AppState,
    cells: impl IntoIterator<Item = VoxelCoord>,
    terrain_error: &'static str,
) -> Result<(), &'static str> {
    for cell in cells {
        if state
            .world()
            .full_cell_collision_at(registries.world(), cell)
            != Some(false)
        {
            return Err(terrain_error);
        }
        if state.logistics().has_stationary_occupant_at(cell) {
            return Err("stationary infrastructure intersects an occupied temple caste cell");
        }
        if state.structures().has_materialized_member_at(cell) {
            return Err("structural member intersects an occupied temple caste cell");
        }
    }
    Ok(())
}

/// Resolves one occupied caste cell within an intended member's bounds.
/// Temple caste patrols are fully within their generated home chunk: authored
/// centres lie at local offsets 6..=10, the warden visits at most four cells
/// east, and the archivist reaches at most three cells west. A modest member
/// footprint therefore needs to inspect only intersected chunks, not all of
/// the world's generated sites. Broad bounds instead traverse existing chunks
/// to avoid unbounded memory or coordinate expansion.
pub(in crate::kobolds) fn first_temple_member_overlap(
    registries: &Registries,
    state: &AppState,
    bounds: VoxelBounds,
) -> Option<VoxelCoord> {
    const MAX_DIRECT_CHUNK_LOOKUPS: u64 = 64;
    let first = ChunkCoord::containing(bounds.min());
    let last = ChunkCoord::containing(VoxelCoord::new(
        bounds.max_exclusive().x() - 1,
        bounds.min().y(),
        bounds.max_exclusive().z() - 1,
    ));
    let direct_count = first.x().abs_diff(last.x()).checked_add(1).and_then(|x| {
        first
            .z()
            .abs_diff(last.z())
            .checked_add(1)
            .and_then(|z| x.checked_mul(z))
    });
    if direct_count.is_some_and(|count| count <= MAX_DIRECT_CHUNK_LOOKUPS) {
        for x in first.x()..=last.x() {
            for z in first.z()..=last.z() {
                if let Some(cell) =
                    temple_member_overlap_in_chunk(registries, state, ChunkCoord::new(x, z), bounds)
                {
                    return Some(cell);
                }
            }
        }
    } else {
        for chunk in state.world().chunks() {
            let coord = chunk.coord();
            if coord.x() < first.x()
                || coord.x() > last.x()
                || coord.z() < first.z()
                || coord.z() > last.z()
            {
                continue;
            }
            if let Some(cell) = temple_member_overlap_in_chunk(registries, state, coord, bounds) {
                return Some(cell);
            }
        }
    }
    None
}

fn temple_member_overlap_in_chunk(
    registries: &Registries,
    state: &AppState,
    chunk: ChunkCoord,
    bounds: VoxelBounds,
) -> Option<VoxelCoord> {
    let x = chunk.world_min_x()?.checked_add(8)?;
    let z = chunk.world_min_z()?.checked_add(8)?;
    let observer = VoxelCoord::new(x, -19, z);
    let homes = crate::world::temple_residents_at(registries, state, observer)?;
    let [warden, archivist] = current_temple_caste_positions(registries, state, homes)?;
    warden_body_cells(warden)?
        .into_iter()
        .chain(resident_body_cells(archivist))
        .find(|&cell| bounds.has_voxel(cell))
}
