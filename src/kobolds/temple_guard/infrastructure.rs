//! Physical overlap and trusted constructed-custody validation for generated temple castes.

use super::warden_position;
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::world::ChunkCoord;

/// Loaded constructed custody must not intersect either generated temple caste.
/// A forged enclosure location cannot become valid merely because ordinary
/// pointed placement already rejects that position at runtime.
pub(in crate::kobolds) fn validate_temple_infrastructure(
    registries: &Registries,
    state: &AppState,
) -> Result<(), &'static str> {
    let logistics = state.logistics();
    let mut stationary = logistics
        .stockpile_locations()
        .map(|(_, position)| position)
        .chain(
            logistics
                .equipment_locations()
                .map(|(_, position)| position),
        )
        .chain(
            logistics
                .energy_store_locations()
                .map(|(_, position)| position),
        )
        .chain(
            logistics
                .fluid_store_locations()
                .map(|(_, position)| position),
        );
    if stationary.any(|position| {
        state
            .world()
            .get_chunk(ChunkCoord::containing(position))
            .is_some()
            && super::super::temple_caste_occupies_voxel(registries, state, position)
    }) {
        return Err("stationary infrastructure intersects an occupied temple caste cell");
    }
    if !state
        .structures()
        .elements()
        .any(|element| !element.embodied_material().is_empty())
    {
        return Ok(());
    }
    // Structural bounds may cover more than one voxel. Inspect the finite body
    // cells of generated temples rather than enumerate potentially immense,
    // untrusted structure bounds. Occupancy itself remains the caste owner's
    // canonical predicate.
    for chunk in state.world().chunks() {
        let Some(x) = chunk.coord().world_min_x().and_then(|x| x.checked_add(8)) else {
            continue;
        };
        let Some(z) = chunk.coord().world_min_z().and_then(|z| z.checked_add(8)) else {
            continue;
        };
        let observer = VoxelCoord::new(x, -19, z);
        let Some([post, archivist]) =
            crate::world::temple_residents_at(registries, state, observer)
        else {
            continue;
        };
        validate_structural_clearance_at_temple(registries, state, post, archivist)?;
    }
    Ok(())
}

fn validate_structural_clearance_at_temple(
    registries: &Registries,
    state: &AppState,
    post: VoxelCoord,
    archivist: VoxelCoord,
) -> Result<(), &'static str> {
    let step = state
        .kobold_state()
        .temple_warden_step(ChunkCoord::containing(post));
    let Some(warden) = crate::world::temple_warden_route_at(registries, state, post)
        .and_then(|route| warden_position(route, step))
    else {
        return Err("temple warden cursor lacks an actual generated corridor");
    };
    for dy in 0..=1 {
        let cell = VoxelCoord::new(warden.x(), warden.y() + dy, warden.z());
        if structure_intersects_temple_caste_cell(registries, state, cell) {
            return Err("structural member intersects an occupied temple caste cell");
        }
    }
    let Some(archivist) = super::super::temple_archivist::archivist_position(
        archivist,
        state
            .kobold_state()
            .temple_archivist_step(ChunkCoord::containing(post)),
    ) else {
        return Err("temple archivist has an invalid physical excursion");
    };
    for dx in 0..=1 {
        for dy in 0..=2 {
            for dz in 0..=1 {
                let Some(x) = archivist.x().checked_add(dx) else {
                    return Err("temple archivist extent exceeds representable coordinates");
                };
                let Some(z) = archivist.z().checked_sub(dz) else {
                    return Err("temple archivist extent exceeds representable coordinates");
                };
                let cell = VoxelCoord::new(x, archivist.y() + dy, z);
                if structure_intersects_temple_caste_cell(registries, state, cell) {
                    return Err("structural member intersects an occupied temple caste cell");
                }
            }
        }
    }
    Ok(())
}

fn structure_intersects_temple_caste_cell(
    registries: &Registries,
    state: &AppState,
    cell: VoxelCoord,
) -> bool {
    super::super::temple_caste_occupies_voxel(registries, state, cell)
        && state
            .structures()
            .elements_covering(cell)
            .any(|element| !element.embodied_material().is_empty())
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
    let [post, archivist] = crate::world::temple_residents_at(registries, state, observer)?;
    let step = state.kobold_state().temple_warden_step(chunk);
    let warden = crate::world::temple_warden_route_at(registries, state, post)
        .and_then(|route| warden_position(route, step))?;
    for dy in 0..=1 {
        let cell = VoxelCoord::new(warden.x(), warden.y().checked_add(dy)?, warden.z());
        if bounds.has_voxel(cell) {
            return Some(cell);
        }
    }
    let archivist = super::super::temple_archivist::archivist_position(
        archivist,
        state.kobold_state().temple_archivist_step(chunk),
    )?;
    for dx in 0..=1 {
        for dy in 0..=2 {
            for dz in 0..=1 {
                let cell = VoxelCoord::new(
                    archivist.x().checked_add(dx)?,
                    archivist.y().checked_add(dy)?,
                    archivist.z().checked_sub(dz)?,
                );
                if bounds.has_voxel(cell) {
                    return Some(cell);
                }
            }
        }
    }
    None
}
