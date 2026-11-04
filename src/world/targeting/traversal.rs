//! Checked voxel-ray traversal and tied-boundary occlusion for player targeting.

use std::cmp::Ordering;

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::{
    Axis, CubeFace, NANOMETERS_PER_VOXEL, VIEW_DIRECTION_PARTS_PER_MILLION, ViewDirection,
    VoxelCoord, WorldPoint,
};
use crate::world::BlockCollisionShape;

use super::{BlockStateId, PlayerBlockTargetError};

pub(super) enum VoxelTraceStep<T> {
    Continue,
    Hit(T),
    Blocked,
}

pub(super) fn trace_generated_voxels<T>(
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
struct RayBoundaryCells {
    touched: [Option<RayCrossingCell>; 7],
    count: usize,
    tied_axes: [bool; 3],
}

fn boundary_cells(
    state: &AppState,
    voxel: VoxelCoord,
    cursors: &[Option<AxisCursor>; 3],
    minimum: AxisCursor,
) -> Result<RayBoundaryCells, PlayerBlockTargetError> {
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
    Ok(RayBoundaryCells {
        touched,
        count,
        tied_axes: tied,
    })
}

fn advance_ray_boundary<T>(
    registries: &Registries,
    state: &AppState,
    voxel: &mut VoxelCoord,
    cursors: &mut [Option<AxisCursor>; 3],
    minimum: AxisCursor,
    inspect: &mut impl FnMut(VoxelCoord, BlockStateId, Option<CubeFace>) -> VoxelTraceStep<T>,
) -> Result<VoxelTraceStep<T>, PlayerBlockTargetError> {
    let RayBoundaryCells {
        touched,
        count,
        tied_axes,
    } = boundary_cells(state, *voxel, cursors, minimum)?;
    // Physical occlusion takes precedence over air-space selection at tied intersections.
    // Trees are world-owned solid trunks, even though their underlying block state is air.
    for solid in [true, false] {
        for &(candidate, face, block_state) in touched[..count].iter().flatten() {
            if (targetable_full_cube(registries, block_state)
                || state.structures().blocks_full_cell(candidate)
                || state.world().tree_trunk_at(candidate).is_some())
                != solid
            {
                continue;
            }
            match inspect(candidate, block_state, Some(face)) {
                VoxelTraceStep::Continue => {}
                outcome @ (VoxelTraceStep::Hit(_) | VoxelTraceStep::Blocked) => {
                    return Ok(outcome);
                }
            }
        }
    }
    for (index, is_tied) in tied_axes.into_iter().enumerate() {
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

pub(super) fn targetable_full_cube(registries: &Registries, state: BlockStateId) -> bool {
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
