//! Deterministic control-rate player-body integration against authoritative block cells.

mod collision;
mod cover;
mod exertion;
mod ground_control;
mod horizontal;
mod vertical;
mod walk_projection;

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Speed};
use crate::core::state::AppState;
use crate::core::time::CONTROL_TICK_DURATION;
use crate::registry::Registries;
use crate::spatial::WorldPoint;
use crate::survival::{LocomotionDefinition, SurvivalExertion, Vitality};

use super::state::LocomotionExertionAccumulator;
use super::{
    CONTROL_INPUT_PARTS_PER_MILLION, HorizontalControlInput, PlayerBodyState, PlayerControlInput,
    PlayerLogisticsRecord, PlayerStance, assess_player_carrying,
};
use collision::{
    body_height, horizontal_chunks_generated, horizontal_voxel_bounds, length_nanometers,
    player_cell_blocks_motion, player_cell_has_fixed_support,
};
pub(crate) use collision::{
    first_player_body_collision, first_player_body_collision_in_chunk,
    first_player_body_overlap_with_bounds, first_player_structure_collision,
    first_player_temple_caste_collision, player_body_fits_continuous_world,
    player_body_floor_structures, player_body_footprint_intersects_chunk, player_body_is_grounded,
    player_body_is_grounded_at, player_body_occupies_voxel, player_body_stands_above_structure,
    player_body_stands_above_temple_caste, player_body_uses_generated_world,
};
use exertion::resolve_control_exertion;
use ground_control::resolve_horizontal_control;
pub(crate) use vertical::{player_body_has_fixed_floor, player_placement_has_fixed_floor};
use vertical::{resolve_vertical_control, supporting_floor_y};

const MICROSECONDS_PER_SECOND: i128 = 1_000_000;
const NANOMETERS_PER_MICROMETER: i128 = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerBodyMotionError {
    CoordinateOverflow,
    VelocityOverflow,
    ExertionOverflow,
    LogisticsRevisionExhausted,
}

impl Display for PlayerBodyMotionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CoordinateOverflow => {
                formatter.write_str("player body motion exceeded continuous world coordinates")
            }
            Self::VelocityOverflow => {
                formatter.write_str("player body velocity exceeded continuous motion range")
            }
            Self::ExertionOverflow => {
                formatter.write_str("control-rate locomotion exertion exceeded supported range")
            }
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted by player body motion")
            }
        }
    }
}

impl Error for PlayerBodyMotionError {}

pub use cover::{PlayerOverheadCoverAssessment, assess_player_overhead_cover};
pub use walk_projection::{
    PlayerWalkProjection, PlayerWalkProjectionError, project_player_adjacent_walk,
    project_player_walk_path, project_player_walk_path_for_load,
};

/// Observable continuous-body consequence of one control tick.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerBodyMotionOutcome {
    before: PlayerBodyState,
    after: PlayerBodyState,
    grounded: bool,
    fall_impact: Option<PlayerFallImpactOutcome>,
}

impl PlayerBodyMotionOutcome {
    #[must_use]
    pub const fn before(self) -> PlayerBodyState {
        self.before
    }

    #[must_use]
    pub const fn after(self) -> PlayerBodyState {
        self.after
    }

    #[must_use]
    pub const fn grounded(self) -> bool {
        self.grounded
    }

    #[must_use]
    pub const fn fall_impact(self) -> Option<PlayerFallImpactOutcome> {
        self.fall_impact
    }
}

/// Observable downward landing consequence captured by the continuous body solver.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerFallImpactOutcome {
    impact_speed: Speed,
    vitality_loss_ppm: u32,
}

impl PlayerFallImpactOutcome {
    #[must_use]
    pub const fn impact_speed(self) -> Speed {
        self.impact_speed
    }

    #[must_use]
    pub const fn vitality_loss_ppm(self) -> u32 {
        self.vitality_loss_ppm
    }
}

pub(crate) struct PlayerBodyMotionPlan {
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
    expected_body: PlayerBodyState,
    body: PlayerBodyState,
    locomotion_exertion: LocomotionExertionAccumulator,
    settled_locomotion_exertion: SurvivalExertion,
    pending_fall_vitality_loss_ppm: u32,
    settled_fall_vitality_loss_ppm: u32,
    outcome: Option<PlayerBodyMotionOutcome>,
}

impl PlayerBodyMotionPlan {
    pub(crate) const fn settled_locomotion_exertion(&self) -> SurvivalExertion {
        self.settled_locomotion_exertion
    }

    pub(crate) const fn settled_fall_vitality_loss_ppm(&self) -> u32 {
        self.settled_fall_vitality_loss_ppm
    }
}

#[derive(Clone, Copy)]
struct PlayerControlAvailability {
    alive: bool,
    busy: bool,
    grounded: bool,
}

impl PlayerControlAvailability {
    const fn accepts_ground_control(self) -> bool {
        self.alive && self.grounded && !self.busy
    }
}

#[derive(Clone, Copy)]
struct ResolvedPlayerBodyTick {
    after: PlayerBodyState,
    availability: PlayerControlAvailability,
    grounded: bool,
    fall_impact: Option<PlayerFallImpactOutcome>,
}

/// One physical control frame, shared by authoritative movement and read-only walk replay.
/// A hypothetical load changes speed, never world, footing, work, or actor authority.
fn resolve_player_body_tick(
    registries: &Registries,
    state: &AppState,
    before: PlayerBodyState,
    input: PlayerControlInput,
    projected_load: Option<(Mass, Mass)>,
) -> Result<Option<ResolvedPlayerBodyTick>, PlayerBodyMotionError> {
    let locomotion = registries.survival().physiology().locomotion();
    let bounds = horizontal_voxel_bounds(before, locomotion)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    if !horizontal_chunks_generated(state.world(), bounds) {
        return Ok(None);
    }
    let availability = PlayerControlAvailability {
        alive: state
            .survival()
            .player()
            .is_some_and(|survival| survival.vitality() != Vitality::ZERO),
        busy: state.player_work().active().is_some(),
        grounded: supporting_floor_y(registries, state, before, bounds)
            == Some(before.foot_position().y_nanometers()),
    };
    let horizontal_body = resolve_horizontal_control(
        registries,
        state,
        before,
        locomotion,
        availability,
        input,
        projected_load,
    )?;
    let (after, grounded, fall_impact) = resolve_vertical_control(
        registries,
        state,
        horizontal_body,
        locomotion,
        availability,
        input,
    )?;
    Ok(Some(ResolvedPlayerBodyTick {
        after,
        availability,
        grounded,
        fall_impact,
    }))
}

pub(crate) fn decide_player_body_motion(
    registries: &Registries,
    state: &AppState,
    input: PlayerControlInput,
    settle_pending_exertion: bool,
) -> Result<Option<PlayerBodyMotionPlan>, PlayerBodyMotionError> {
    let Some(player) = state.logistics().player().copied() else {
        return Ok(None);
    };
    let before = player.body();
    let Some(ResolvedPlayerBodyTick {
        after,
        availability,
        grounded,
        fall_impact,
    }) = resolve_player_body_tick(registries, state, before, input, None)?
    else {
        return Ok(None);
    };
    let locomotion = registries.survival().physiology().locomotion();
    let exertion_before = player.locomotion_exertion();
    let pending_fall_before = player.pending_fall_vitality_loss_ppm();
    let (locomotion_exertion, settled_locomotion_exertion) = resolve_control_exertion(
        state,
        before,
        after,
        locomotion,
        availability,
        input,
        settle_pending_exertion,
    )?;
    let settled_fall_vitality_loss_ppm = if settle_pending_exertion {
        pending_fall_before
    } else {
        0
    };
    let pending_fall_base = if settle_pending_exertion {
        0
    } else {
        pending_fall_before
    };
    let pending_fall_vitality_loss_ppm = pending_fall_base
        .saturating_add(fall_impact.map_or(0, PlayerFallImpactOutcome::vitality_loss_ppm))
        .min(crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION);
    if after == before
        && locomotion_exertion == exertion_before
        && pending_fall_vitality_loss_ppm == pending_fall_before
    {
        return Ok(None);
    }
    if !state.can_spend_logistics_revisions(1) {
        return Err(PlayerBodyMotionError::LogisticsRevisionExhausted);
    }
    let expected_logistics_revision = state.logistics().revision();
    let next_logistics_revision = expected_logistics_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("logistics headroom check includes player body motion"));
    Ok(Some(PlayerBodyMotionPlan {
        expected_logistics_revision,
        next_logistics_revision,
        expected_body: before,
        body: after,
        locomotion_exertion,
        settled_locomotion_exertion,
        pending_fall_vitality_loss_ppm,
        settled_fall_vitality_loss_ppm,
        outcome: (after != before).then_some(PlayerBodyMotionOutcome {
            before,
            after,
            grounded,
            fall_impact,
        }),
    }))
}

pub(crate) fn apply_player_body_motion(
    state: &mut AppState,
    plan: Option<PlayerBodyMotionPlan>,
) -> Option<PlayerBodyMotionOutcome> {
    let plan = plan?;
    state.logistics_state_mut().apply_player_body_motion(
        plan.expected_logistics_revision,
        plan.next_logistics_revision,
        plan.expected_body,
        plan.body,
        plan.locomotion_exertion,
        plan.pending_fall_vitality_loss_ppm,
    );
    plan.outcome
}

fn integrate_velocity(velocity_nanometers_per_second: i64) -> Result<i64, PlayerBodyMotionError> {
    let displacement = i128::from(velocity_nanometers_per_second)
        .checked_mul(i128::from(CONTROL_TICK_DURATION.microseconds()))
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?
        / MICROSECONDS_PER_SECOND;
    i64::try_from(displacement).map_err(|_| PlayerBodyMotionError::CoordinateOverflow)
}
