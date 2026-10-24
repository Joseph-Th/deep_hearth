//! Local archive custodians attend a real inherited copper shell, not an active engine.
//!
//! The massive archivist keeps its authored chamber post until someone is
//! physically nearby. Its slow inspection follows a supported two-by-two-wide
//! passage toward the dormant archive; only a displaced body needs save state.
//! A visibly breached shell causes an attending archivist to guard the approach,
//! without restoring metal, reading unknown records, or summoning reinforcements.

use crate::content::BLOCK_STATE_TEMPLE_ARCHIVE;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::survival::Vitality;
use crate::world::{
    BlockCollisionShape, BlockStateId, ChunkCoord, WORLD_MIN_Y, temple_residents_at,
};

use super::{SCOUT_STEP_TICKS, visibility::scout_visible_from};

const ARCHIVIST_STEP_TICKS: u64 = SCOUT_STEP_TICKS * 2;
const ARCHIVIST_INSPECTION_CYCLE_TICKS: u64 = 240;
const ARCHIVIST_LOCAL_REACH: u64 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TempleArchivistRecord {
    pub(super) chunk: ChunkCoord,
    pub(super) step: u8,
}

/// The original two-by-two-by-three-cell body shifts one face-connected
/// horizontal cell per cursor; steps 6..=9 retrace the outward floor cells.
pub(super) fn archivist_position(home: VoxelCoord, step: u8) -> Option<VoxelCoord> {
    let (west, south) = match step {
        0 => (0, 0),
        1 | 9 => (1, 0),
        2 | 8 => (2, 0),
        3 | 7 => (3, 0),
        4 | 6 => (3, 1),
        5 => (3, 2),
        _ => return None,
    };
    Some(VoxelCoord::new(
        home.x().checked_sub(west)?,
        home.y(),
        home.z().checked_add(south)?,
    ))
}

fn resident_body_cells(anchor: VoxelCoord) -> impl Iterator<Item = VoxelCoord> {
    (0..=1).flat_map(move |dx| {
        (0..=1).flat_map(move |dz| {
            (0..=2)
                .map(move |dy| VoxelCoord::new(anchor.x() + dx, anchor.y() + dy, anchor.z() - dz))
        })
    })
}

fn clear_body_cells(state: &AppState, anchor: VoxelCoord) -> bool {
    resident_body_cells(anchor)
        .all(|voxel| state.world().block_at(voxel) == Some(BlockStateId::AIR))
}

fn supported_and_clear(registries: &Registries, state: &AppState, anchor: VoxelCoord) -> bool {
    anchor.y() > WORLD_MIN_Y
        && clear_body_cells(state, anchor)
        && resident_body_cells(anchor).all(|voxel| {
            !state.logistics().has_stationary_occupant_at(voxel)
                && !state.structures().elements().any(|member| {
                    !member.embodied_material().is_empty() && member.bounds().has_voxel(voxel)
                })
                && !crate::logistics::player_body_occupies_voxel(registries, state, voxel)
        })
        && (0..=1).all(|dx| {
            (0..=1).all(|dz| {
                let floor = VoxelCoord::new(anchor.x() + dx, anchor.y() - 1, anchor.z() - dz);
                state
                    .world()
                    .block_at(floor)
                    .and_then(|block| registries.world().block_for_state(block))
                    .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
            })
        })
}

fn locally_observed(state: &AppState, home: VoxelCoord, player: VoxelCoord) -> bool {
    state
        .survival()
        .player()
        .is_some_and(|actor| actor.vitality() != Vitality::ZERO)
        && ChunkCoord::containing(home) == ChunkCoord::containing(player)
        && home
            .x()
            .abs_diff(player.x())
            .saturating_add(home.y().abs_diff(player.y()))
            .saturating_add(home.z().abs_diff(player.z()))
            <= ARCHIVIST_LOCAL_REACH
}

/// Only the archivist at the end of its *physical* path can check the archive
/// across the exposed one-cell maintenance throat. Rock or built full-cell
/// obstructions prevent it from inferring what happened behind a sealed wall.
fn archive_shell_breached(state: &AppState, home: VoxelCoord, step: u8) -> bool {
    if step != 5 {
        return false;
    }
    let Some(archive_x) = home.x().checked_sub(5) else {
        return false;
    };
    let Some(near_x) = home.x().checked_sub(4) else {
        return false;
    };
    let Some(archive_z) = home.z().checked_add(2) else {
        return false;
    };
    let mouth = VoxelCoord::new(near_x, home.y(), archive_z);
    let archive = VoxelCoord::new(archive_x, home.y(), archive_z);
    state.world().block_at(mouth) == Some(BlockStateId::AIR)
        && !state.structures().blocks_full_cell(mouth)
        && state
            .world()
            .block_at(archive)
            .is_some_and(|block| block != BLOCK_STATE_TEMPLE_ARCHIVE)
}

/// A massive resident is observable through any exposed part of its true
/// two-by-two-by-three body, not only the lower-left anchor cell.
pub(super) fn archivist_visible_from(
    registries: &Registries,
    state: &AppState,
    anchor: VoxelCoord,
    observer: VoxelCoord,
) -> bool {
    let Some(head_y) = observer.y().checked_add(1) else {
        return false;
    };
    let observer_head = VoxelCoord::new(observer.x(), head_y, observer.z());
    resident_body_cells(anchor).any(|eye| {
        [observer, observer_head]
            .into_iter()
            .any(|target| scout_visible_from(registries, state, eye, target))
    })
}

pub(super) fn archivist_is_guarding_breach(
    registries: &Registries,
    state: &AppState,
    home: VoxelCoord,
    step: u8,
    player: VoxelCoord,
) -> bool {
    let Some(at) = archivist_position(home, step) else {
        return false;
    };
    archive_shell_breached(state, home, step)
        && state
            .survival()
            .player()
            .is_some_and(|actor| actor.vitality() != Vitality::ZERO)
        && archivist_visible_from(registries, state, at, player)
}

fn return_step(step: u8) -> u8 {
    match step {
        0 => 0,
        1..=5 => step - 1,
        6..=8 => step + 1,
        9 => 0,
        _ => unreachable!("trusted archive cursor fits its real route"),
    }
}

fn scheduled_step(at: SimulationTick, step: u8) -> Option<u8> {
    let phase = at.value() % ARCHIVIST_INSPECTION_CYCLE_TICKS;
    if phase == u64::from(step) * ARCHIVIST_STEP_TICKS {
        match step {
            0..=8 => Some(step + 1),
            9 => Some(0),
            _ => None,
        }
    } else {
        None
    }
}

/// Only a locally relevant resident is advanced; an unfinished excursion is
/// saved and must retreat over real cells even if the observer leaves.
pub(super) fn apply_temple_archivist_tick(
    registries: &Registries,
    state: &mut AppState,
    at: SimulationTick,
) {
    if !at.value().is_multiple_of(ARCHIVIST_STEP_TICKS) {
        return;
    }
    let Some(player) = state.logistics().player().map(|actor| actor.position()) else {
        return;
    };
    let mut active = state
        .kobold_state()
        .temple_archivists
        .iter()
        .filter_map(|record| {
            let x = record.chunk.world_min_x()?.checked_add(8)?;
            let z = record.chunk.world_min_z()?.checked_add(8)?;
            temple_residents_at(registries, state, VoxelCoord::new(x, -19, z))
                .map(|[_, home]| (record.chunk, home))
        })
        .collect::<Vec<_>>();
    if let Some([_, home]) = temple_residents_at(registries, state, player) {
        let chunk = ChunkCoord::containing(home);
        if !active.iter().any(|(present, _)| *present == chunk) {
            active.push((chunk, home));
        }
    }
    for (chunk, home) in active {
        let step = state.kobold_state().temple_archivist_step(chunk);
        let Some(current) = archivist_position(home, step) else {
            continue;
        };
        if !supported_and_clear_without_player(state, current) {
            continue;
        }
        let nearby = locally_observed(state, home, player);
        let next = if archive_shell_breached(state, home, step) && nearby {
            step
        } else if nearby {
            scheduled_step(at, step).unwrap_or_else(|| return_step(step))
        } else {
            return_step(step)
        };
        if next == step {
            continue;
        }
        let Some(destination) = archivist_position(home, next) else {
            continue;
        };
        if !supported_and_clear(registries, state, destination) {
            continue;
        }
        let displaced = &mut state.kobold_state_mut().temple_archivists;
        match displaced.binary_search_by_key(&chunk, |record| record.chunk) {
            Ok(index) if next == 0 => {
                displaced.remove(index);
            }
            Ok(index) => displaced[index].step = next,
            Err(index) => displaced.insert(index, TempleArchivistRecord { chunk, step: next }),
        }
    }
}

/// A resident already standing on a floor may have had that floor mined away.
/// Do not remove the resident from the physical world simply because support
/// was lost later, but prohibit additional movement until it is restored.
fn supported_and_clear_without_player(state: &AppState, anchor: VoxelCoord) -> bool {
    clear_body_cells(state, anchor)
}

/// A loaded cursor must name a real locally generated sanctuary, valid
/// excursion step, and clear three-high body. Floor excavation may strand it.
pub(super) fn validate_archivists(
    registries: &Registries,
    state: &AppState,
) -> Result<(), &'static str> {
    let mut previous = None;
    for &TempleArchivistRecord { chunk, step } in &state.kobold_state().temple_archivists {
        if previous.is_some_and(|prior| prior >= chunk) {
            return Err("temple archivist records must be unique and sorted by sanctuary");
        }
        previous = Some(chunk);
        if !(1..=9).contains(&step) {
            return Err("temple archivist has an invalid excursion cursor");
        }
        let earliest =
            ARCHIVIST_INSPECTION_CYCLE_TICKS + (u64::from(step) - 1) * ARCHIVIST_STEP_TICKS;
        if state.tick().value() < earliest {
            return Err("temple archivist excursion predates any physical inspection");
        }
        let Some(x) = chunk.world_min_x().and_then(|v| v.checked_add(8)) else {
            return Err("temple archivist home x exceeds world coordinates");
        };
        let Some(z) = chunk.world_min_z().and_then(|v| v.checked_add(8)) else {
            return Err("temple archivist home z exceeds world coordinates");
        };
        let Some([_, home]) = temple_residents_at(registries, state, VoxelCoord::new(x, -19, z))
        else {
            return Err("temple archivist lacks a generated ancestral chamber");
        };
        if !archivist_position(home, step).is_some_and(|anchor| clear_body_cells(state, anchor)) {
            return Err("temple archivist is embedded in missing or solid world");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "temple_archivist_tests.rs"]
mod tests;
