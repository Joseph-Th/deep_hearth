//! Deterministic player-eye block targeting over generated world truth.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::logistics::PlayerStance;
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::{CubeFace, ViewDirection, VoxelCoord, WorldPoint};

use super::BlockStateId;

mod traversal;
use traversal::{VoxelTraceStep, targetable_full_cube, trace_generated_voxels};

/// Observe a living trunk at landscape scale without granting interaction range.
///
/// A long tree sightline uses the same exact voxel traversal as block targeting:
/// rock, trunks, built walls, and missing chunks stop the ray. This is an
/// observation only; chopping continues to require standing beside the tree.
pub(crate) fn player_can_see_distant_tree_trunk(
    registries: &Registries,
    state: &AppState,
    root: VoxelCoord,
) -> bool {
    let Some(player) = state.logistics().player().copied() else {
        return false;
    };
    let Ok(eye) = player_eye_origin(
        player.body().foot_position(),
        player.body().stance(),
        registries.survival().physiology().locomotion(),
    ) else {
        return false;
    };
    if state.world().tree_at(root).is_none() {
        return false;
    }
    let Some(above) = root.y().checked_add(2) else {
        return false;
    };
    let illuminated = VoxelCoord::new(root.x(), above, root.z());
    if super::sky::assess_generated_skylight_at(registries, state, illuminated, state.tick()) == 0
        && crate::energy::assess_local_illumination(registries, state, illuminated).level() == 0
    {
        return false;
    }
    [root.y().checked_add(1), Some(root.y())]
        .into_iter()
        .flatten()
        .any(|y| {
            let target = VoxelCoord::new(root.x(), y, root.z());
            clear_landmark_ray(registries, state, eye, target)
        })
}

/// Observe a lit cell using the same physical eye ray as distant tree sightings.
pub(crate) fn player_can_see_distant_cell(
    registries: &Registries,
    state: &AppState,
    target: VoxelCoord,
) -> bool {
    let Some(player) = state.logistics().player().copied() else {
        return false;
    };
    let Ok(eye) = player_eye_origin(
        player.body().foot_position(),
        player.body().stance(),
        registries.survival().physiology().locomotion(),
    ) else {
        return false;
    };
    if super::sky::assess_generated_skylight_at(registries, state, target, state.tick()) == 0
        && crate::energy::assess_local_illumination(registries, state, target).level() == 0
    {
        return false;
    }
    clear_landmark_ray(registries, state, eye, target)
}

fn clear_landmark_ray(
    registries: &Registries,
    state: &AppState,
    eye: WorldPoint,
    target: VoxelCoord,
) -> bool {
    let Some((direction, length)) = direction_to_voxel_center(eye, target) else {
        return false;
    };
    let Some(reach) = i64::try_from(length)
        .ok()
        .and_then(|distance| distance.checked_add(1_000_000))
    else {
        return false;
    };
    trace_generated_voxels(
        registries,
        state,
        eye,
        direction,
        reach,
        |voxel, block, _| {
            if voxel == target {
                VoxelTraceStep::Hit(())
            } else if targetable_full_cube(registries, block)
                || state.structures().blocks_full_cell(voxel)
                || state.world().tree_trunk_at(voxel).is_some()
            {
                VoxelTraceStep::Blocked
            } else {
                VoxelTraceStep::Continue
            }
        },
    )
    .ok()
        == Some(Some(()))
}

/// Aim from the current physical eye toward a voxel centre. This is input
/// geometry only: the regular action ray still resolves the first hit and
/// enforces interaction reach, occlusion and all operation admission.
pub fn aim_player_at_voxel(
    registries: &Registries,
    state: &AppState,
    target: VoxelCoord,
) -> Result<ViewDirection, PlayerBlockTargetError> {
    let (eye, _) = player_view_origin_and_reach(registries, state)?;
    direction_to_voxel_center(eye, target)
        .map(|(direction, _)| direction)
        .ok_or(PlayerBlockTargetError::InvalidAimPoint)
}

fn direction_to_voxel_center(eye: WorldPoint, target: VoxelCoord) -> Option<(ViewDirection, u128)> {
    let floor = WorldPoint::centered_on_voxel_floor(target)?;
    let center_y = floor.y_nanometers().checked_add(500_000_000)?;
    let delta = [
        i128::from(floor.x_nanometers()) - i128::from(eye.x_nanometers()),
        i128::from(center_y) - i128::from(eye.y_nanometers()),
        i128::from(floor.z_nanometers()) - i128::from(eye.z_nanometers()),
    ];
    // Normalization must also be safe for manually supplied coordinates at
    // opposite ends of the represented continuous world.
    let squared = delta.iter().try_fold(0_u128, |sum, axis| {
        let magnitude = axis.unsigned_abs();
        sum.checked_add(magnitude.checked_mul(magnitude)?)
    })?;
    let length = squared.isqrt();
    if length == 0 {
        return None;
    }
    let [Some(x), Some(y), Some(z)] =
        delta.map(|axis| i32::try_from(axis * 1_000_000 / length as i128).ok())
    else {
        return None;
    };
    Some((ViewDirection::new(x, y, z).ok()?, length))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerBlockTargetError {
    PlayerNotAdmitted,
    PlayerDead,
    EyeCoordinateOverflow,
    InvalidAimPoint,
    WorldAuthorityUnavailable { voxel: VoxelCoord },
    EyeInsideSolid { voxel: VoxelCoord },
}

impl Display for PlayerBlockTargetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => {
                formatter.write_str("block targeting requires an admitted player")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot target the world"),
            Self::EyeCoordinateOverflow => formatter
                .write_str("player eye or interaction reach exceeded continuous world coordinates"),
            Self::InvalidAimPoint => {
                formatter.write_str("aim point cannot be represented from the current eye position")
            }
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
            if state.structures().blocks_full_cell(voxel)
                || state.world().tree_trunk_at(voxel).is_some()
            {
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
            let structure_blocks = state.structures().blocks_full_cell(voxel)
                || state.world().tree_trunk_at(voxel).is_some();
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
    validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => PlayerBlockTargetError::PlayerNotAdmitted,
        PlayerActionValidationError::PlayerDead => PlayerBlockTargetError::PlayerDead,
    })?;
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

/// The authoritative first-person eye cell for local observations.
///
/// Resident sightings use the same stance and continuous foot position as
/// block targeting. Neither occupied body cell is an independent viewpoint.
pub(crate) fn player_eye_voxel(registries: &Registries, state: &AppState) -> Option<VoxelCoord> {
    let body = state.logistics().player()?.body();
    player_eye_origin(
        body.foot_position(),
        body.stance(),
        registries.survival().physiology().locomotion(),
    )
    .ok()
    .map(WorldPoint::containing_voxel)
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
