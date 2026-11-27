//! Shared admission and revision binding for actions that require the local player's attention.

use crate::core::state::AppState;
use crate::core::time::ControlTick;
use crate::registry::Registries;
use crate::spatial::HorizontalVelocity;
use crate::survival::Vitality;

use super::PlayerWork;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlayerAttentionError {
    SurvivalNotInitialized,
    PlayerDead,
    PlayerAirborne,
    PlayerMoving,
    Busy { active: Box<PlayerWork> },
    PreparedStrikeActive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerAttentionConflict {
    ControlTick {
        expected: ControlTick,
        actual: ControlTick,
    },
    WorkRevision {
        expected: u64,
        actual: u64,
    },
    CombatRevision {
        expected: u64,
        actual: u64,
    },
    SurvivalRevision {
        expected: u64,
        actual: u64,
    },
    LogisticsRevision {
        expected: u64,
        actual: u64,
    },
    WorldRevision {
        expected: u64,
        actual: u64,
    },
    StructureRevision {
        expected: u64,
        actual: u64,
    },
}

/// Every exclusive action depends on the same actor, clock, and physical support.
/// Capture those owners together so a retained token cannot outlive its admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AttentionSnapshot {
    control_tick: ControlTick,
    work: u64,
    combat: u64,
    survival: u64,
    logistics: u64,
    world: u64,
    structures: u64,
}

impl AttentionSnapshot {
    fn capture(state: &AppState) -> Self {
        Self {
            control_tick: state.control_tick(),
            work: state.player_work().revision(),
            combat: state.combat().revision(),
            survival: state.survival().revision(),
            logistics: state.logistics().revision(),
            world: state.world().revision(),
            structures: state.structures().revision(),
        }
    }

    fn precheck(self, state: &AppState) -> Result<(), PlayerAttentionConflict> {
        let actual = Self::capture(state);
        if self.control_tick.simulation_tick() == actual.control_tick.simulation_tick()
            && self.control_tick != actual.control_tick
        {
            return Err(PlayerAttentionConflict::ControlTick {
                expected: self.control_tick,
                actual: actual.control_tick,
            });
        }
        for (expected, actual, conflict) in [
            (
                self.work,
                actual.work,
                PlayerAttentionConflict::WorkRevision {
                    expected: self.work,
                    actual: actual.work,
                },
            ),
            (
                self.combat,
                actual.combat,
                PlayerAttentionConflict::CombatRevision {
                    expected: self.combat,
                    actual: actual.combat,
                },
            ),
            (
                self.survival,
                actual.survival,
                PlayerAttentionConflict::SurvivalRevision {
                    expected: self.survival,
                    actual: actual.survival,
                },
            ),
            (
                self.logistics,
                actual.logistics,
                PlayerAttentionConflict::LogisticsRevision {
                    expected: self.logistics,
                    actual: actual.logistics,
                },
            ),
            (
                self.world,
                actual.world,
                PlayerAttentionConflict::WorldRevision {
                    expected: self.world,
                    actual: actual.world,
                },
            ),
            (
                self.structures,
                actual.structures,
                PlayerAttentionConflict::StructureRevision {
                    expected: self.structures,
                    actual: actual.structures,
                },
            ),
        ] {
            if expected != actual {
                return Err(conflict);
            }
        }
        // Even a clock advance with no other changed owner cannot authorize
        // work scheduled relative to an earlier boundary.
        if self.control_tick != actual.control_tick {
            return Err(PlayerAttentionConflict::ControlTick {
                expected: self.control_tick,
                actual: actual.control_tick,
            });
        }
        Ok(())
    }
}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedPlayerAttention {
    snapshot: AttentionSnapshot,
}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedPlayerAttentionHold {
    snapshot: AttentionSnapshot,
    next_revision: u64,
    work: PlayerWork,
}

impl ValidatedPlayerAttention {
    pub(crate) fn precheck(&self, state: &AppState) -> Result<(), PlayerAttentionConflict> {
        self.snapshot.precheck(state)
    }

    pub(crate) fn hold(self, work: PlayerWork) -> Option<ValidatedPlayerAttentionHold> {
        // A delayed attention hold mutates player-work once when admitted and once when the
        // interval releases. Reserve both revisions before the first mutation.
        self.snapshot.work.checked_add(2)?;
        let next_revision = self.snapshot.work.checked_add(1).unwrap_or_else(|| {
            unreachable!("two-step attention revision budget includes admission")
        });
        Some(ValidatedPlayerAttentionHold {
            snapshot: self.snapshot,
            next_revision,
            work,
        })
    }
}

impl ValidatedPlayerAttentionHold {
    pub(super) const fn work(&self) -> PlayerWork {
        self.work
    }

    pub(crate) fn precheck(&self, state: &AppState) -> Result<(), PlayerAttentionConflict> {
        self.snapshot.precheck(state)
    }

    pub(crate) fn apply(self, state: &mut AppState) {
        state.player_work_state_mut().apply_start(
            self.snapshot.work,
            self.next_revision,
            self.work,
        );
    }
}

pub(crate) fn validate_player_attention(
    registries: &Registries,
    state: &AppState,
) -> Result<ValidatedPlayerAttention, PlayerAttentionError> {
    let Some(player) = state.survival().player() else {
        return Err(PlayerAttentionError::SurvivalNotInitialized);
    };
    if player.vitality() == Vitality::ZERO {
        return Err(PlayerAttentionError::PlayerDead);
    }
    if let Some(active) = state.player_work().active() {
        return Err(PlayerAttentionError::Busy {
            active: Box::new(active),
        });
    }
    // A raised weapon is already an exclusive actor commitment. All consumers
    // of attention (work, meals, drinks) must honor the same admission rule.
    if state.combat().has_prepared_strike() {
        return Err(PlayerAttentionError::PreparedStrikeActive);
    }
    // Every held attention interval needs a fixed floor in generated terrain.
    // A temple resident is solid to motion but may walk away before this work
    // completes. Trusted-load admission uses the same stable-support rule.
    if crate::logistics::player_body_uses_generated_world(
        state,
        registries.survival().physiology().locomotion(),
    ) {
        if !crate::logistics::player_body_has_fixed_floor(registries, state) {
            return Err(PlayerAttentionError::PlayerAirborne);
        }
        if state.logistics().player().is_some_and(|player| {
            player.body().horizontal_velocity() != HorizontalVelocity::ZERO
                || player.body().vertical_velocity().nanometers_per_second() != 0
        }) {
            return Err(PlayerAttentionError::PlayerMoving);
        }
    }
    Ok(ValidatedPlayerAttention {
        snapshot: AttentionSnapshot::capture(state),
    })
}
