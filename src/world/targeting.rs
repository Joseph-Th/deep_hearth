//! Deterministic player-eye block targeting over generated world truth.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::logistics::PlayerStance;
use crate::player::{PlayerAdmissionState, player_admission_state};
use crate::registry::Registries;
use crate::spatial::{
    Axis, CubeFace, NANOMETERS_PER_VOXEL, VIEW_DIRECTION_PARTS_PER_MILLION, ViewDirection,
    VoxelCoord, WorldPoint,
};

use super::{BlockCollisionShape, BlockStateId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerBlockTargetError {
    PlayerNotAdmitted,
    EyeCoordinateOverflow,
    WorldAuthorityUnavailable { voxel: VoxelCoord },
    EyeInsideSolid { voxel: VoxelCoord },
}

impl Display for PlayerBlockTargetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => {
                formatter.write_str("block targeting requires an admitted player")
            }
            Self::EyeCoordinateOverflow => formatter
                .write_str("player eye or interaction reach exceeded continuous world coordinates"),
            Self::WorldAuthorityUnavailable { voxel } => write!(
                formatter,
                "block targeting reached unavailable world truth at voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::EyeInsideSolid { voxel } => write!(
                formatter,
                "player eye is inside solid world voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
        }
    }
}

impl Error for PlayerBlockTargetError {}

/// First targetable full-cube block intersected by the admitted player's bounded view ray.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerBlockTarget {
    voxel: VoxelCoord,
    state: BlockStateId,
    face: CubeFace,
}

impl PlayerBlockTarget {
    #[must_use]
    pub const fn voxel(self) -> VoxelCoord {
        self.voxel
    }

    #[must_use]
    pub const fn state(self) -> BlockStateId {
        self.state
    }

    #[must_use]
    pub const fn face(self) -> CubeFace {
        self.face
    }
}

/// Resolves the first full-cube block under the admitted player's authored interaction reach.
///
/// Missing chunks and unsupported vertical world space are not transparent. Materialized
/// full-footprint structures also occlude terrain targeting. The query fails closed at the first
/// unavailable voxel so interaction can never infer or act through absent authority.
pub fn raycast_player_block(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<Option<PlayerBlockTarget>, PlayerBlockTargetError> {
    let (origin, reach_nanometers) = player_view_origin_and_reach(registries, state)?;
    trace_generated_voxels(
        registries,
        state,
        origin,
        direction,
        reach_nanometers,
        |voxel, block_state, face| {
            let Some(face) = face else {
                // The eye's own air cell has no entered face and cannot be a block hit.
                return VoxelTraceStep::Continue;
            };
            if state.structures().blocks_full_cell(voxel) {
                VoxelTraceStep::Blocked
            } else if targetable_full_cube(registries, block_state) {
                VoxelTraceStep::Hit(PlayerBlockTarget {
                    voxel,
                    state: block_state,
                    face,
                })
            } else {
                VoxelTraceStep::Continue
            }
        },
    )
}

/// Visits visible non-solid voxels under the same admitted-player eye ray and reach as block
/// targeting, stopping at the first full-cube terrain or materialized structural blocker.
///
/// This is deliberately crate-private. World owns sight-line authority and missing-chunk behavior;
/// higher-level interaction owners may inspect only the traversed air voxels for their own targets.
pub(crate) fn raycast_player_air_voxel<T>(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
    mut select: impl FnMut(VoxelCoord, CubeFace) -> Option<T>,
) -> Result<Option<T>, PlayerBlockTargetError> {
    trace_player_air_voxels(
        registries,
        state,
        direction,
        u32::MAX,
        AirRayMode::StructuralTarget,
        |voxel, face| face.and_then(|face| select(voxel, face)),
    )
}

/// Uses the same opaque-world ray with a shorter, authored weapon reach.
/// The originating eye cell is inspected with no entry face, allowing contact
/// with an actor already sharing that cell. Missing chunks beyond the reach
/// must never affect a close-range miss. An embodied full-cell structure blocks
/// contact even if an actor occupies the structure's cell.
pub(crate) fn raycast_player_air_voxel_with_reach<T>(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
    reach_millimeters: u32,
    select: impl FnMut(VoxelCoord, Option<CubeFace>) -> Option<T>,
) -> Result<Option<T>, PlayerBlockTargetError> {
    trace_player_air_voxels(
        registries,
        state,
        direction,
        reach_millimeters,
        AirRayMode::UnobstructedContact,
        select,
    )
}

/// Structural pointing may select the front panel; contact requires empty physical space.
enum AirRayMode {
    StructuralTarget,
    UnobstructedContact,
}

fn trace_player_air_voxels<T>(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
    reach_millimeters: u32,
    mode: AirRayMode,
    mut select: impl FnMut(VoxelCoord, Option<CubeFace>) -> Option<T>,
) -> Result<Option<T>, PlayerBlockTargetError> {
    let (origin, interaction_reach) = player_view_origin_and_reach(registries, state)?;
    let reach_nanometers = interaction_reach.min(i64::from(reach_millimeters) * 1_000_000);
    trace_generated_voxels(
        registries,
        state,
        origin,
        direction,
        reach_nanometers,
        |voxel, block_state, face| {
            let structure_blocks = state.structures().blocks_full_cell(voxel);
            if targetable_full_cube(registries, block_state)
                || (structure_blocks && matches!(mode, AirRayMode::UnobstructedContact))
            {
                return VoxelTraceStep::Blocked;
            }
            if let Some(target) = select(voxel, face) {
                return VoxelTraceStep::Hit(target);
            }
            if structure_blocks {
                VoxelTraceStep::Blocked
            } else {
                VoxelTraceStep::Continue
            }
        },
    )
}

fn player_view_origin_and_reach(
    registries: &Registries,
    state: &AppState,
) -> Result<(WorldPoint, i64), PlayerBlockTargetError> {
    if player_admission_state(state) != PlayerAdmissionState::Admitted {
        return Err(PlayerBlockTargetError::PlayerNotAdmitted);
    }
    let player = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| unreachable!("admitted player has logistics state"));
    let locomotion = registries.survival().physiology().locomotion();
    let origin = player_eye_origin(
        player.body().foot_position(),
        player.body().stance(),
        locomotion,
    )?;
    let reach_nanometers = i64::try_from(
        u128::from(locomotion.interaction_reach().micrometers())
            .checked_mul(1_000)
            .ok_or(PlayerBlockTargetError::EyeCoordinateOverflow)?,
    )
    .map_err(|_| PlayerBlockTargetError::EyeCoordinateOverflow)?;
    Ok((origin, reach_nanometers))
}

fn player_eye_origin(
    foot: WorldPoint,
    stance: PlayerStance,
    locomotion: crate::survival::LocomotionDefinition,
) -> Result<WorldPoint, PlayerBlockTargetError> {
    let eye_height = match stance {
        PlayerStance::Standing => locomotion.standing_eye_height(),
        PlayerStance::Crouching => locomotion.crouching_eye_height(),
    };
    let eye_height_nanometers = i64::try_from(
        u128::from(eye_height.micrometers())
            .checked_mul(1_000)
            .ok_or(PlayerBlockTargetError::EyeCoordinateOverflow)?,
    )
    .map_err(|_| PlayerBlockTargetError::EyeCoordinateOverflow)?;
    let y = foot
        .y_nanometers()
        .checked_add(eye_height_nanometers)
        .ok_or(PlayerBlockTargetError::EyeCoordinateOverflow)?;
    Ok(WorldPoint::new(foot.x_nanometers(), y, foot.z_nanometers()))
}

enum VoxelTraceStep<T> {
    Continue,
    Hit(T),
    Blocked,
}

fn trace_generated_voxels<T>(
    registries: &Registries,
    state: &AppState,
    origin: WorldPoint,
    direction: ViewDirection,
    reach_nanometers: i64,
    mut inspect: impl FnMut(VoxelCoord, BlockStateId, Option<CubeFace>) -> VoxelTraceStep<T>,
) -> Result<Option<T>, PlayerBlockTargetError> {
    let mut voxel = origin.containing_voxel();
    let start_state = state
        .world()
        .block_at(voxel)
        .ok_or(PlayerBlockTargetError::WorldAuthorityUnavailable { voxel })?;
    if targetable_full_cube(registries, start_state) {
        return Err(PlayerBlockTargetError::EyeInsideSolid { voxel });
    }
    match inspect(voxel, start_state, None) {
        VoxelTraceStep::Continue => {}
        VoxelTraceStep::Hit(target) => return Ok(Some(target)),
        VoxelTraceStep::Blocked => return Ok(None),
    }

    let mut cursors = [
        AxisCursor::new(
            Axis::X,
            origin.x_nanometers(),
            voxel.x(),
            direction.x_parts_per_million(),
        ),
        AxisCursor::new(
            Axis::Y,
            origin.y_nanometers(),
            voxel.y(),
            direction.y_parts_per_million(),
        ),
        AxisCursor::new(
            Axis::Z,
            origin.z_nanometers(),
            voxel.z(),
            direction.z_parts_per_million(),
        ),
    ];
    loop {
        let Some(minimum_index) = minimum_cursor_index(&cursors) else {
            unreachable!("validated view direction always has a nonzero component")
        };
        let minimum = cursors[minimum_index]
            .unwrap_or_else(|| unreachable!("minimum cursor index references a cursor"));
        if !minimum.within_reach(reach_nanometers) {
            return Ok(None);
        }

        match advance_ray_boundary(
            registries,
            state,
            &mut voxel,
            &mut cursors,
            minimum,
            &mut inspect,
        )? {
            VoxelTraceStep::Continue => {}
            VoxelTraceStep::Hit(target) => return Ok(Some(target)),
            VoxelTraceStep::Blocked => return Ok(None),
        }
    }
}

type RayCrossingCell = (VoxelCoord, CubeFace, BlockStateId);

/// Include every face-adjacent cell touched on a tied boundary, not only the diagonal.
/// Missing world truth fails closed before any interaction at that same distance.
fn boundary_cells(
    state: &AppState,
    voxel: VoxelCoord,
    cursors: &[Option<AxisCursor>; 3],
    minimum: AxisCursor,
) -> Result<([Option<RayCrossingCell>; 7], usize, [bool; 3]), PlayerBlockTargetError> {
    let tied = std::array::from_fn(|index| {
        cursors[index].is_some_and(|cursor| cursor.same_distance(minimum))
    });
    let tied_mask = tied.iter().enumerate().fold(0_u8, |mask, (index, tied)| {
        mask | (u8::from(*tied) << index)
    });
    let mut touched = [None; 7];
    let mut count = 0;
    for axis_count in 1..=tied_mask.count_ones() {
        for mask in 1..=tied_mask {
            if mask & tied_mask != mask || mask.count_ones() != axis_count {
                continue;
            }
            let mut candidate = voxel;
            let mut face = None;
            for (index, cursor) in cursors.iter().enumerate() {
                if mask & (1 << index) == 0 {
                    continue;
                }
                let cursor = cursor.unwrap_or_else(|| unreachable!("tied axis has a cursor"));
                candidate = cursor.step_voxel(candidate)?;
                face.get_or_insert(cursor.face);
            }
            let block_state = state
                .world()
                .block_at(candidate)
                .ok_or(PlayerBlockTargetError::WorldAuthorityUnavailable { voxel: candidate })?;
            touched[count] = Some((
                candidate,
                face.unwrap_or_else(|| unreachable!("crossed boundary has an entry face")),
                block_state,
            ));
            count += 1;
        }
    }
    Ok((touched, count, tied))
}

fn advance_ray_boundary<T>(
    registries: &Registries,
    state: &AppState,
    voxel: &mut VoxelCoord,
    cursors: &mut [Option<AxisCursor>; 3],
    minimum: AxisCursor,
    inspect: &mut impl FnMut(VoxelCoord, BlockStateId, Option<CubeFace>) -> VoxelTraceStep<T>,
) -> Result<VoxelTraceStep<T>, PlayerBlockTargetError> {
    let (touched, count, tied) = boundary_cells(state, *voxel, cursors, minimum)?;
    // Physical occlusion takes precedence over air-space selection at tied intersections.
    for solid in [true, false] {
        for &(candidate, face, block_state) in touched[..count].iter().flatten() {
            if (targetable_full_cube(registries, block_state)
                || state.structures().blocks_full_cell(candidate))
                != solid
            {
                continue;
            }
            match inspect(candidate, block_state, Some(face)) {
                VoxelTraceStep::Continue => {}
                outcome => return Ok(outcome),
            }
        }
    }
    for (index, is_tied) in tied.into_iter().enumerate() {
        if is_tied {
            let cursor = cursors[index]
                .as_mut()
                .unwrap_or_else(|| unreachable!("tied axis has a cursor"));
            *voxel = cursor.step_voxel(*voxel)?;
            cursor.advance();
        }
    }
    Ok(VoxelTraceStep::Continue)
}

fn targetable_full_cube(registries: &Registries, state: BlockStateId) -> bool {
    registries
        .world()
        .block_for_state(state)
        .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
}

#[derive(Clone, Copy)]
struct AxisCursor {
    axis: Axis,
    step: i64,
    face: CubeFace,
    next_distance_numerator: i128,
    direction_denominator: i128,
    voxel_distance_numerator: i128,
}

impl AxisCursor {
    fn new(axis: Axis, origin_nanometers: i64, voxel: i64, component: i32) -> Option<Self> {
        if component == 0 {
            return None;
        }
        let positive = component > 0;
        let boundary_voxel = if positive {
            i128::from(voxel) + 1
        } else {
            i128::from(voxel)
        };
        let boundary_nanometers = boundary_voxel * i128::from(NANOMETERS_PER_VOXEL);
        let origin_nanometers = i128::from(origin_nanometers);
        let distance = if positive {
            boundary_nanometers - origin_nanometers
        } else {
            origin_nanometers - boundary_nanometers
        };
        debug_assert!(distance >= 0);
        let scale = i128::from(VIEW_DIRECTION_PARTS_PER_MILLION);
        let face = match (axis, positive) {
            (Axis::X, true) => CubeFace::West,
            (Axis::X, false) => CubeFace::East,
            (Axis::Y, true) => CubeFace::Bottom,
            (Axis::Y, false) => CubeFace::Top,
            (Axis::Z, true) => CubeFace::North,
            (Axis::Z, false) => CubeFace::South,
        };
        Some(Self {
            axis,
            step: if positive { 1 } else { -1 },
            face,
            next_distance_numerator: distance * scale,
            direction_denominator: i128::from(component).abs(),
            voxel_distance_numerator: i128::from(NANOMETERS_PER_VOXEL) * scale,
        })
    }

    fn compare_distance(self, other: Self) -> Ordering {
        (self.next_distance_numerator * other.direction_denominator)
            .cmp(&(other.next_distance_numerator * self.direction_denominator))
    }

    fn same_distance(self, other: Self) -> bool {
        self.compare_distance(other) == Ordering::Equal
    }

    fn within_reach(self, reach_nanometers: i64) -> bool {
        self.next_distance_numerator <= i128::from(reach_nanometers) * self.direction_denominator
    }

    fn step_voxel(self, voxel: VoxelCoord) -> Result<VoxelCoord, PlayerBlockTargetError> {
        let checked = |value: i64| {
            value
                .checked_add(self.step)
                .ok_or(PlayerBlockTargetError::EyeCoordinateOverflow)
        };
        Ok(match self.axis {
            Axis::X => VoxelCoord::new(checked(voxel.x())?, voxel.y(), voxel.z()),
            Axis::Y => VoxelCoord::new(voxel.x(), checked(voxel.y())?, voxel.z()),
            Axis::Z => VoxelCoord::new(voxel.x(), voxel.y(), checked(voxel.z())?),
        })
    }

    fn advance(&mut self) {
        self.next_distance_numerator += self.voxel_distance_numerator;
    }
}

fn minimum_cursor_index(cursors: &[Option<AxisCursor>; 3]) -> Option<usize> {
    let mut minimum: Option<usize> = None;
    for (index, cursor) in cursors.iter().enumerate() {
        let Some(cursor) = cursor else {
            continue;
        };
        if minimum.is_none_or(|minimum_index| {
            let current = cursors[minimum_index]
                .unwrap_or_else(|| unreachable!("minimum cursor index is populated"));
            cursor.compare_distance(current) == Ordering::Less
        }) {
            minimum = Some(index);
        }
    }
    minimum
}
