//! Heavy temple wardens physically challenge intruders on a short, supported hall lane.
//!
//! The architect's original post is implicit in generated geometry. Only a
//! resident actually walking a challenge or local inspection needs persistent state.
//! Local sight uses the full two-high body and respects opaque stone. A guardian
//! advances defensively only while its next hall cell can still see the intruder;
//! it also makes brief, collision-checked quiet gallery inspections on a local clock.
//! It never pursues beyond its post. Distant temples are not scanned; there is no combat.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::survival::Vitality;
use crate::world::{
    BlockCollisionShape, BlockStateId, ChunkCoord, WORLD_MIN_Y, temple_warden_route_at,
};

use super::{
    SCOUT_STEP_TICKS,
    visibility::{scout_visible_from, within_manhattan_reach},
};

/// One inspection of the inner gallery every 2.4 simulated hours. Unvisited
/// sanctuaries stay at their standing post and consume no offscreen AI work.
const TEMPLE_INSPECTION_CYCLE_TICKS: u64 = 120;
const TEMPLE_INSPECTION_LOCAL_REACH: u64 = 8;
/// Every third quiet circuit checks the existing dormant drive bay.
const TEMPLE_ENGINE_INSPECTION_INTERVAL_CYCLES: u64 = 3;

/// Resolve a durable cursor to the actual face-adjacent temple corridor.
/// The returning half repeats cells; no cursor enters the installed copper drive.
pub(super) fn warden_position(route: [VoxelCoord; 3], step: u8) -> Option<VoxelCoord> {
    match step {
        0..=2 => Some(route[usize::from(step)]),
        4..=12 => {
            let dx = match step {
                4 | 10 => 1,
                5 | 9 => 2,
                6 | 8 => 3,
                7 => 4,
                11 | 12 => 0,
                _ => unreachable!("bounded engine gallery cursor"),
            };
            Some(VoxelCoord::new(
                route[2].x().checked_add(dx)?,
                route[2].y(),
                if step == 12 {
                    route[1].z()
                } else {
                    route[2].z()
                },
            ))
        }
        _ => None,
    }
}

fn retreat_step(step: u8) -> u8 {
    match step {
        0 => 0,
        4 => 2,
        1 | 2 | 5..=7 => step - 1,
        8..=11 => step + 1,
        12 => 0,
        _ => unreachable!("trusted temple cursor must name a real patrol cell"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TempleWardenRecord {
    pub(super) chunk: ChunkCoord,
    pub(super) step: u8,
}

fn clear_body_cell(state: &AppState, position: VoxelCoord) -> bool {
    state.world().block_at(position) == Some(BlockStateId::AIR)
        && state.world().block_at(VoxelCoord::new(
            position.x(),
            position.y() + 1,
            position.z(),
        )) == Some(BlockStateId::AIR)
}

fn clear_standing_cell(registries: &Registries, state: &AppState, position: VoxelCoord) -> bool {
    let Some(top_y) = position.y().checked_add(2) else {
        return false;
    };
    let Some(max_x) = position.x().checked_add(1) else {
        return false;
    };
    let Some(max_z) = position.z().checked_add(1) else {
        return false;
    };
    let body_bounds =
        crate::spatial::VoxelBounds::new(position, VoxelCoord::new(max_x, top_y, max_z))
            .expect("positive one-cell warden footprint");
    position.y() > WORLD_MIN_Y
        && clear_body_cell(state, position)
        && !state.structures().has_embodied_member_intersecting(body_bounds)
        // World air alone does not mean free space: built infrastructure has
        // separate durable custody and blocks both occupied warden cells.
        && [position, VoxelCoord::new(position.x(), position.y() + 1, position.z())]
            .into_iter()
            .all(|cell| !state.logistics().has_stationary_occupant_at(cell))
        && state
            .world()
            .block_at(VoxelCoord::new(
                position.x(),
                position.y() - 1,
                position.z(),
            ))
            .and_then(|block| registries.world().block_for_state(block))
            .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
}

/// The heavy guardian watches with its complete two-cell body, rather than
/// seeing only along the floor. The player's occupied head cell is similarly
/// observable. Each of the four local rays checks authoritative opacity and
/// range, so this never reveals a distant or hidden actor.
fn warden_sees_intruder(
    registries: &Registries,
    state: &AppState,
    warden: VoxelCoord,
    player: VoxelCoord,
) -> bool {
    let Some(warden_head_y) = warden.y().checked_add(1) else {
        return false;
    };
    let Some(player_head_y) = player.y().checked_add(1) else {
        return false;
    };
    let warden_head = VoxelCoord::new(warden.x(), warden_head_y, warden.z());
    let player_head = VoxelCoord::new(player.x(), player_head_y, player.z());
    [warden, warden_head].into_iter().any(|eye| {
        [player, player_head]
            .into_iter()
            .any(|target| scout_visible_from(registries, state, eye, target))
    })
}

/// Only a living intruder on the sanctuary side of the ancestral post can
/// trigger a challenge. The same rule owns visible defensive posture.
pub(super) fn warden_is_challenging(
    registries: &Registries,
    state: &AppState,
    post: VoxelCoord,
    current: VoxelCoord,
    player: VoxelCoord,
) -> bool {
    player.z() <= post.z()
        && state
            .survival()
            .player()
            .is_some_and(|actor| actor.vitality() != Vitality::ZERO)
        && warden_sees_intruder(registries, state, current, player)
}

fn inspection_step(at: SimulationTick, step: u8, player_near: bool, seen: bool) -> Option<u8> {
    if !player_near || seen {
        return None;
    }
    let phase = at.value() % TEMPLE_INSPECTION_CYCLE_TICKS;
    let engine_round = (at.value() / TEMPLE_INSPECTION_CYCLE_TICKS)
        .is_multiple_of(TEMPLE_ENGINE_INSPECTION_INTERVAL_CYCLES);
    match (step, phase) {
        (0, 0) => Some(1),
        (1, SCOUT_STEP_TICKS) => Some(2),
        (2, p) if p == 2 * SCOUT_STEP_TICKS && engine_round => Some(4),
        (2, p) if p == 2 * SCOUT_STEP_TICKS => Some(1),
        (1, p) if p == 3 * SCOUT_STEP_TICKS => Some(0),
        (4..=11, p) if p == (u64::from(step) - 1) * SCOUT_STEP_TICKS => Some(step + 1),
        (12, p) if p == 11 * SCOUT_STEP_TICKS => Some(0),
        _ => None,
    }
}

/// A warden can inherit a caste warning only by directly seeing the
/// archivist at its actual breached-archive post, not by knowing remote blocks.
pub(super) fn warden_sees_archive_vigil(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> bool {
    super::temple_archivist::archive_vigil_visible_to_warden(registries, state, position)
}

/// The drive watch uses its actual adjacent copper housing, not a remotely
/// flagged machine. Only an empty authored drive cell is an observable breach;
/// a full-cube replacement or ungenerated terrain does not disclose what is
/// behind it. The resident must physically reach the end of the engine patrol.
pub(super) fn warden_sees_missing_drive(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> bool {
    let Some(route) = temple_warden_route_at(registries, state, position) else {
        return false;
    };
    if warden_position(route, 7) != Some(position) {
        return false;
    }
    let Some(x) = position.x().checked_add(1) else {
        return false;
    };
    let housing = VoxelCoord::new(x, position.y(), position.z());
    state.world().block_at(housing) == Some(BlockStateId::AIR)
        && !state.structures().blocks_full_cell(housing)
}

pub(super) fn warden_has_damage_warning(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> bool {
    warden_sees_archive_vigil(registries, state, position)
        || warden_sees_missing_drive(registries, state, position)
}

/// Choose between a real intruder challenge, a local inspection and retreat.
fn plan_warden_step(
    registries: &Registries,
    state: &AppState,
    route: [VoxelCoord; 3],
    step: u8,
    player: VoxelCoord,
    at: SimulationTick,
) -> u8 {
    let current = warden_position(route, step).expect("trusted patrol has a valid cursor");
    let alive = state
        .survival()
        .player()
        .is_some_and(|actor| actor.vitality() != Vitality::ZERO);
    // Home activates an implicit resident; once it travels, its actual body
    // remains locally relevant even when the player leaves the home radius.
    let nearby = alive
        && (within_manhattan_reach(route[0], player, TEMPLE_INSPECTION_LOCAL_REACH)
            || within_manhattan_reach(current, player, TEMPLE_INSPECTION_LOCAL_REACH));
    let seen = alive && warden_sees_intruder(registries, state, current, player);
    // The ancestral post determines the guarded boundary even if a side
    // intruder moves across the actual sentry's present position.
    let aware = seen && player.z() <= route[0].z();
    // Never advance into a blind spot merely because the previous cell saw
    // the same intruder.
    if aware
        && step < 2
        && warden_sees_intruder(registries, state, route[usize::from(step + 1)], player)
    {
        return step + 1;
    }
    // When the inner-hall sentry can actually see an archivist holding a
    // damaged archive, it keeps the threshold rather than wandering east
    // on routine machinery rounds. That posture vanishes when the visual
    // link is broken; no extra alarm entity or hidden knowledge is created.
    if nearby
        && ((step == 2 && warden_sees_archive_vigil(registries, state, current))
            || (step == 7 && warden_sees_missing_drive(registries, state, current)))
    {
        return step;
    }
    if let Some(round) = inspection_step(at, step, nearby, seen) {
        return round;
    }
    if !aware && step > 0 {
        retreat_step(step)
    } else {
        step
    }
}

/// Advance one resident over at most one real face-adjacent hall cell on the
/// existing sparse scout clock. A living, visible intruder draws the guardian
/// forward; otherwise a locally simulated, unseen resident may make a brief
/// scheduled inspection, or withdraw one cell toward its ancestral post.
pub(super) fn apply_temple_warden_tick(
    registries: &Registries,
    state: &mut AppState,
    at: SimulationTick,
) {
    if !at.value().is_multiple_of(SCOUT_STEP_TICKS) {
        return;
    }
    let Some(player) = state.logistics().player().map(|actor| actor.position()) else {
        return;
    };
    // Ordinary homebound defenders remain implicit. Only an actually displaced
    // defender continues its return after the player leaves its chunk. Iterating
    // the sparse saved cursors does not discover or animate dormant remote sites.
    let mut active = state
        .kobold_state()
        .temple_wardens
        .iter()
        .filter_map(|record| {
            let x = record.chunk.world_min_x()?.checked_add(8)?;
            let z = record.chunk.world_min_z()?.checked_add(8)?;
            temple_warden_route_at(registries, state, VoxelCoord::new(x, -19, z))
                .map(|route| (record.chunk, route))
        })
        .collect::<Vec<_>>();
    for [post, _] in crate::world::nearby_temple_residents(registries, state, player) {
        let chunk = ChunkCoord::containing(post);
        if !active.iter().any(|(present, _)| *present == chunk) {
            let route = temple_warden_route_at(registries, state, post)
                .expect("generated temple resident retains its patrol route");
            active.push((chunk, route));
        }
    }
    for (chunk, route) in active {
        let step = state.kobold_state().temple_warden_step(chunk);
        let current = warden_position(route, step).expect("trusted patrol has a valid cursor");
        if !clear_standing_cell(registries, state, current) {
            continue;
        }
        let next_step = plan_warden_step(registries, state, route, step, player, at);
        if next_step == step {
            continue;
        }
        let destination = warden_position(route, next_step).expect("patrol plan names a real cell");
        // A warden fills two vertical cells. An airborne intruder may occupy
        // only the upper one, even when the floor cell is free.
        let intersects_player =
            crate::logistics::player_body_occupies_voxel(registries, state, destination)
                || destination.y().checked_add(1).is_none_or(|head_y| {
                    crate::logistics::player_body_occupies_voxel(
                        registries,
                        state,
                        VoxelCoord::new(destination.x(), head_y, destination.z()),
                    )
                });
        if intersects_player || !clear_standing_cell(registries, state, destination) {
            continue;
        }
        let displaced = &mut state.kobold_state_mut().temple_wardens;
        match displaced.binary_search_by_key(&chunk, |record| record.chunk) {
            Ok(index) if next_step == 0 => {
                displaced.remove(index);
            }
            Ok(index) => displaced[index].step = next_step,
            Err(index) => displaced.insert(
                index,
                TempleWardenRecord {
                    chunk,
                    step: next_step,
                },
            ),
        }
    }
}

/// A saved displacement is only a possible step along one real generated
/// temple lane. Reject arbitrary invented guardians, duplicate keys, absent
/// columns, obstructed locations and out-of-range cursors on trusted load.
pub(super) fn validate_temple_wardens(
    registries: &Registries,
    state: &AppState,
) -> Result<(), &'static str> {
    let mut previous = None;
    for &TempleWardenRecord { chunk, step } in &state.kobold_state().temple_wardens {
        if previous.is_some_and(|before| before >= chunk) {
            return Err("kobold temple warden records must have unique sorted homes");
        }
        previous = Some(chunk);
        if !matches!(step, 1..=2 | 4..=12) {
            return Err("kobold temple warden has an invalid off-post cursor");
        }
        if step >= 4 {
            // A forged future gallery position cannot appear before the first
            // scheduled engine tour could physically have reached that cell.
            let earliest = TEMPLE_INSPECTION_CYCLE_TICKS * TEMPLE_ENGINE_INSPECTION_INTERVAL_CYCLES
                + (u64::from(step) - 2) * SCOUT_STEP_TICKS;
            if state.tick().value() < earliest {
                return Err("kobold temple warden precedes its first physical engine tour");
            }
        }
        let Some(x) = chunk.world_min_x().and_then(|min| min.checked_add(8)) else {
            return Err("kobold temple warden has an overflowing home chunk");
        };
        let Some(z) = chunk.world_min_z().and_then(|min| min.checked_add(8)) else {
            return Err("kobold temple warden has an overflowing home chunk");
        };
        let Some(route) = temple_warden_route_at(registries, state, VoxelCoord::new(x, -19, z))
        else {
            return Err("kobold temple warden lacks an authored generated sanctuary");
        };
        // A player may mine away the supporting floor *after* the warden
        // occupies this cell. Preserve that stranded location across saves;
        // movement stays suspended until a real supported step is available.
        if !warden_position(route, step).is_some_and(|position| clear_body_cell(state, position)) {
            return Err("kobold temple warden occupies obstructed or absent world geometry");
        }
    }
    Ok(())
}

/// Loaded constructed custody must not intersect either generated temple caste.
/// A forged enclosure location cannot become valid merely because ordinary
/// pointed placement already rejects that position at runtime.
pub(super) fn validate_temple_infrastructure(
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
            && super::temple_caste_occupies_voxel(registries, state, position)
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
    let Some(archivist) = super::temple_archivist::archivist_position(
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
    super::temple_caste_occupies_voxel(registries, state, cell)
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
pub(super) fn first_temple_member_overlap(
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
    let archivist = super::temple_archivist::archivist_position(
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

#[cfg(test)]
#[path = "temple_guard_tests.rs"]
mod tests;
