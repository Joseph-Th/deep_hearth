//! Nonlethal first-person melee contact on the canonical 20 Hz player clock.
//! Kobolds own their reactions; combat owns the player's recovery history.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::ControlTick;
use crate::equipment::{EquipmentId, EquipmentOperationConditionOutcome, equipment_occupancy};
use crate::kobolds::{
    KoboldRole, STAGGERED_BELOW_PPM, close_contact_at, opening_balance, record_close_contact,
};
use crate::logistics::PlayerActiveSelection;
use crate::maintenance::Condition;
use crate::player::{PlayerActionGuard, PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::{ViewDirection, VoxelCoord};
use crate::survival::{
    MeleeExertionError, SurvivalExertion, SurvivalTickPlan, apply_survival_tick,
    decide_melee_exertion,
};
use crate::world::PlayerBlockTargetError;
use serde::{Deserialize, Serialize};

mod assessment;
mod attack;
mod preparation;
pub use assessment::{PlayerMeleeAssessment, assess_player_melee};
use attack::{prepared_attack_exertion, validate_attack};
pub use attack::{validate_primitive_shove, validate_primitive_strike};
pub(crate) use preparation::validate_loaded_combat_references;
pub use preparation::{
    PrimitivePreparationOutcome, ValidatedPrimitivePreparation,
    validate_cancel_primitive_preparation, validate_prepare_primitive_strike,
    validate_release_prepared_strike,
};

const SHOVE_RECOVERY_TICKS: u32 = 16;
const SHOVE_REACH_MILLIMETERS: u32 = 1_600;
const SHOVE_BALANCE_IMPULSE_PPM: u32 = 80_000;
const SHOVE_EXERTION: SurvivalExertion = SurvivalExertion::new(
    Energy::from_nanojoules(90_000_000_000),
    Volume::from_microliters(6),
);

/// The actor's durable attack tempo; no client-controlled timer or virtual damage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CombatState {
    revision: u64,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    last_attack_at: Option<ControlTick>,
    recovery_control_ticks: u32,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    prepared: Option<preparation::PreparedStrike>,
}
impl CombatState {
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            last_attack_at: None,
            recovery_control_ticks: 0,
            prepared: None,
        }
    }
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }
    pub(crate) fn preparing_weapon(&self, equipment: EquipmentId) -> bool {
        self.prepared
            .is_some_and(|strike| strike.weapon == equipment)
    }
    pub(crate) const fn has_prepared_strike(&self) -> bool {
        self.prepared.is_some()
    }
    /// A resident cannot remember a blow later than the last player attack.
    pub(crate) fn admits_contact_at(&self, at: ControlTick) -> bool {
        self.last_attack_at.is_some_and(|last| last >= at)
    }
    pub(crate) fn validate(&self, now: ControlTick) -> Result<(), &'static str> {
        // An active windup always owes one more combat revision, whether the
        // player deliberately cancels, releases the strike, or dies. A forged
        // saved revision at u64::MAX would panic during fatal cleanup.
        if self.prepared.is_some() && self.revision == u64::MAX {
            return Err("prepared combat stance has no revision space for cancellation");
        }
        if self.last_attack_at.is_some_and(|at| at > now)
            || (self.revision == 0 && (self.last_attack_at.is_some() || self.prepared.is_some()))
            || (self.last_attack_at.is_none() && self.recovery_control_ticks != 0)
            // Before the first attack, preparation/cancellation is the only revision path.
            || (self.last_attack_at.is_none()
                && (self.revision % 2 == 1) != self.prepared.is_some())
            || (self.last_attack_at.is_some()
                && !(8..=60 + preparation::PREPARED_RECOVERY_BONUS_TICKS)
                    .contains(&self.recovery_control_ticks))
        {
            return Err("combat recovery record conflicts with control history");
        }
        if let Some(prepared) = self.prepared
            && (prepared.started_at > now
                || self.revision == 0
                || ViewDirection::new(prepared.aim[0], prepared.aim[1], prepared.aim[2]).is_err()
                || prepared
                    .started_at
                    .checked_add_steps(preparation::PREPARATION_TICKS)
                    .is_none()
                || self.last_attack_at.is_some_and(|at| {
                    prepared.started_at <= at
                        || prepared
                            .started_at
                            .checked_steps_since(at)
                            .is_none_or(|elapsed| elapsed < u64::from(self.recovery_control_ticks))
                }))
        {
            return Err("prepared combat stance contradicts actor clock or facing");
        }
        Ok(())
    }
    fn ready(&self, now: ControlTick) -> bool {
        self.last_attack_at.is_none_or(|last| {
            now.checked_steps_since(last)
                .is_some_and(|elapsed| elapsed >= u64::from(self.recovery_control_ticks))
        })
    }
    /// A fatal survival transition drops a raised weapon without registering an attack.
    pub(crate) fn clear_preparation_on_death(&mut self) {
        if self.prepared.take().is_some() {
            self.revision = self
                .revision
                .checked_add(1)
                .expect("preparation reserved a cancellation revision");
        }
    }

    fn apply_attack(&mut self, revision: u64, at: ControlTick, recovery_control_ticks: u32) {
        assert_eq!(self.revision, revision);
        self.revision = revision.checked_add(1).expect("reserved combat revision");
        self.last_attack_at = Some(at);
        self.recovery_control_ticks = recovery_control_ticks;
        self.prepared = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveAttackError {
    PlayerNotAdmitted,
    PlayerDead,
    PlayerBusy,
    OccupiedHand,
    NoHeldWeapon,
    WeaponNotCarried,
    UnsupportedWeapon,
    BrokenWeapon,
    WeaponBusy,
    AlreadyPreparing,
    NotPreparing,
    PreparationNotReady { ready_at: ControlTick },
    PreparationWeaponChanged,
    PreparationAimChanged,
    EquipmentRevisionExhausted,
    InsufficientMetabolicEnergy,
    InsufficientHydration,
    ExertionOverflow,
    SurvivalRevisionExhausted,
    Recovering { last_at: ControlTick },
    RevisionExhausted,
    Target(PlayerBlockTargetError),
    AmbiguousContact { voxel: VoxelCoord },
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveAttackOutcome {
    Miss,
    Startled {
        resident: KoboldRole,
        position: VoxelCoord,
    },
    /// Loss of footing, not lasting tissue damage or an instant knockback.
    Staggered {
        resident: KoboldRole,
        position: VoxelCoord,
        remaining_balance_ppm: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveAttackCommitError {
    StaleControlTick,
    StaleCombatRevision,
    StaleLogisticsRevision,
    StaleWorldRevision,
    StaleStructureRevision,
    StaleEquipmentRevision,
    WeaponChanged,
    StaleSurvivalRevision,
    StalePlayerWorkRevision,
    TargetMoved,
    StaleResidentBalance,
}

#[must_use]
pub struct ValidatedPrimitiveAttack {
    player_action: PlayerActionGuard,
    at: ControlTick,
    expected_combat_revision: u64,
    expected_logistics_revision: u64,
    expected_player_work_revision: u64,
    expected_world_revision: u64,
    expected_structure_revision: u64,
    outcome: PrimitiveAttackOutcome,
    recovery_control_ticks: u32,
    weapon_wear: Option<(EquipmentId, Condition, Condition, u64)>,
    balance_impulse_ppm: u32,
    expected_resident_balance: Option<u32>,
    expected_preparation: Option<preparation::PreparedStrike>,
    exertion_plan: SurvivalTickPlan,
}

#[cfg(test)]
#[path = "combat_tests.rs"]
mod tests;
