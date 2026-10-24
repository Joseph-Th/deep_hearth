//! Nonlethal, unarmed first-person contact on the canonical 20 Hz player clock.
//! Kobolds own their reactions; combat owns the player's recovery history.

use crate::core::state::AppState;
use crate::core::time::ControlTick;
use crate::kobolds::{KoboldRole, close_contact_at, record_close_contact};
use crate::player::{PlayerActionGuard, PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::{ViewDirection, VoxelCoord};
use crate::world::{PlayerBlockTargetError, raycast_player_air_voxel};
use serde::{Deserialize, Serialize};

const SHOVE_RECOVERY_TICKS: u64 = 16;
const SHOVE_REACH_VOXELS: u64 = 2;

/// The actor's durable attack tempo; no client-controlled timer or virtual damage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CombatState {
    revision: u64,
    last_shove_at: Option<ControlTick>,
}
impl CombatState {
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            last_shove_at: None,
        }
    }
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }
    pub(crate) fn validate(&self, now: ControlTick) -> Result<(), &'static str> {
        if self.last_shove_at.is_some_and(|at| at > now)
            || (self.revision == 0) != self.last_shove_at.is_none()
        {
            return Err("combat recovery record conflicts with control history");
        }
        Ok(())
    }
    fn ready(&self, now: ControlTick) -> bool {
        self.last_shove_at.is_none_or(|last| {
            now.checked_steps_since(last)
                .is_some_and(|elapsed| elapsed >= SHOVE_RECOVERY_TICKS)
        })
    }
    fn apply_shove(&mut self, revision: u64, at: ControlTick) {
        assert_eq!(self.revision, revision);
        self.revision = revision.checked_add(1).expect("reserved combat revision");
        self.last_shove_at = Some(at);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveShoveError {
    PlayerNotAdmitted,
    PlayerDead,
    PlayerBusy,
    OccupiedHand,
    Recovering { last_at: ControlTick },
    RevisionExhausted,
    Target(PlayerBlockTargetError),
    AmbiguousContact { voxel: VoxelCoord },
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveShoveOutcome {
    Miss,
    Startled {
        resident: KoboldRole,
        position: VoxelCoord,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveShoveCommitError {
    StaleControlTick,
    StaleCombatRevision,
    StaleLogisticsRevision,
    StaleSurvivalRevision,
    TargetMoved,
}

#[must_use]
pub struct ValidatedPrimitiveShove {
    player_action: PlayerActionGuard,
    at: ControlTick,
    expected_combat_revision: u64,
    expected_logistics_revision: u64,
    outcome: PrimitiveShoveOutcome,
}
impl ValidatedPrimitiveShove {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<PrimitiveShoveOutcome, PrimitiveShoveCommitError> {
        if state.control_tick() != self.at {
            return Err(PrimitiveShoveCommitError::StaleControlTick);
        }
        if state.combat().revision() != self.expected_combat_revision {
            return Err(PrimitiveShoveCommitError::StaleCombatRevision);
        }
        if state.logistics().revision() != self.expected_logistics_revision {
            return Err(PrimitiveShoveCommitError::StaleLogisticsRevision);
        }
        self.player_action
            .precheck(state)
            .map_err(|_| PrimitiveShoveCommitError::StaleSurvivalRevision)?;
        if let PrimitiveShoveOutcome::Startled { resident, position } = self.outcome
            && close_contact_at(state, position) != Ok(Some(resident))
        {
            return Err(PrimitiveShoveCommitError::TargetMoved);
        }
        state
            .combat_state_mut()
            .apply_shove(self.expected_combat_revision, self.at);
        if let PrimitiveShoveOutcome::Startled { resident, .. } = self.outcome {
            record_close_contact(state, resident);
        }
        Ok(self.outcome)
    }
}

/// A shove uses the shared occluding player-eye ray. Misses consume tempo,
/// but hidden, remote or co-located ambiguous actors cannot be attacked.
pub fn validate_primitive_shove(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<ValidatedPrimitiveShove, PrimitiveShoveError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => PrimitiveShoveError::PlayerNotAdmitted,
        PlayerActionValidationError::PlayerDead => PrimitiveShoveError::PlayerDead,
    })?;
    if state.player_work().active().is_some() {
        return Err(PrimitiveShoveError::PlayerBusy);
    }
    if state.logistics().active_selection().is_some() {
        return Err(PrimitiveShoveError::OccupiedHand);
    }
    let at = state.control_tick();
    if !state.combat().ready(at) {
        return Err(PrimitiveShoveError::Recovering {
            last_at: state
                .combat()
                .last_shove_at
                .expect("recovery follows contact"),
        });
    }
    state
        .combat()
        .revision()
        .checked_add(1)
        .ok_or(PrimitiveShoveError::RevisionExhausted)?;
    let player_at = state
        .logistics()
        .player()
        .expect("admitted logistics")
        .position();
    let target = raycast_player_air_voxel(registries, state, direction, |voxel, _| {
        let distance = player_at
            .x()
            .abs_diff(voxel.x())
            .saturating_add(player_at.y().abs_diff(voxel.y()))
            .saturating_add(player_at.z().abs_diff(voxel.z()));
        if distance > SHOVE_REACH_VOXELS {
            return None;
        }
        match close_contact_at(state, voxel) {
            Ok(Some(role)) => Some(Ok((role, voxel))),
            Ok(None) => None,
            Err(()) => Some(Err(voxel)),
        }
    })
    .map_err(PrimitiveShoveError::Target)?;
    let outcome = match target {
        Some(Ok((resident, position))) => PrimitiveShoveOutcome::Startled { resident, position },
        Some(Err(voxel)) => return Err(PrimitiveShoveError::AmbiguousContact { voxel }),
        None => PrimitiveShoveOutcome::Miss,
    };
    Ok(ValidatedPrimitiveShove {
        player_action,
        at,
        expected_combat_revision: state.combat().revision(),
        expected_logistics_revision: state.logistics().revision(),
        outcome,
    })
}

#[cfg(test)]
#[path = "combat_tests.rs"]
mod tests;
