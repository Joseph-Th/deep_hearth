//! Read-only pointed structural resolution, independent of action admission and mutation.

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::{CubeFace, ViewDirection, VoxelCoord};
use crate::structural::StructuralElementId;
use crate::world::raycast_player_air_voxel;

pub(super) enum DirectStructuralTargetError {
    TargetUnavailable,
    NoTarget,
    Ambiguous { count: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DirectStructuralFaceTarget {
    pub(super) element: StructuralElementId,
    pub(super) voxel: VoxelCoord,
    pub(super) face: CubeFace,
}

/// Select a unique reachable embodied member before any unbuilt plan.
/// A plan is only selectable when no physical member obstructs the ray.
pub(super) fn resolve_direct_structural_target(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<StructuralElementId, DirectStructuralTargetError> {
    resolve_direct_structural_face_target(registries, state, direction)?
        .map(|target| target.element)
        .ok_or(DirectStructuralTargetError::NoTarget)
}

/// Keep the exact struck face: supported timber placement needs the actual
/// adjacent voxel, while dismantling only consumes the selected element.
pub(super) fn resolve_direct_structural_face_target(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<Option<DirectStructuralFaceTarget>, DirectStructuralTargetError> {
    let mut first_plan = None;
    let traced = raycast_player_air_voxel(registries, state, direction, |voxel, face| {
        let mut embodied = (None, 0_usize);
        let mut planned = (None, 0_usize);
        for record in state.structures().elements_covering(voxel) {
            let group = if record.embodied_material().is_empty() {
                &mut planned
            } else {
                &mut embodied
            };
            group.0.get_or_insert(record.id());
            group.1 += 1;
        }
        if embodied.1 != 0 {
            return direct_structural_face_candidate(embodied, voxel, face);
        }
        if first_plan.is_none() {
            first_plan = direct_structural_face_candidate(planned, voxel, face);
        }
        None
    });
    let target = match traced {
        Ok(physical) => physical.or(first_plan),
        // Unknown terrain after a visible plan cannot erase that observed plan,
        // but it cannot authorize a physical target beyond the unknown cell.
        Err(_) if first_plan.is_some() => first_plan,
        Err(_) => return Err(DirectStructuralTargetError::TargetUnavailable),
    };
    target
        .transpose()
        .map_err(|count| DirectStructuralTargetError::Ambiguous { count })
}

fn direct_structural_face_candidate(
    (first, count): (Option<StructuralElementId>, usize),
    voxel: VoxelCoord,
    face: CubeFace,
) -> Option<Result<DirectStructuralFaceTarget, usize>> {
    first.map(|element| {
        if count == 1 {
            Ok(DirectStructuralFaceTarget {
                element,
                voxel,
                face,
            })
        } else {
            Err(count)
        }
    })
}
