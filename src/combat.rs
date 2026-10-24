//! Nonlethal first-person melee contact on the canonical 20 Hz player clock.
//! Kobolds own their reactions; combat owns the player's recovery history.

use crate::core::state::AppState;
use crate::core::time::ControlTick;
use crate::equipment::{
    EquipmentId, EquipmentOperationConditionOutcome, MeleeWeaponProfile, equipment_occupancy,
};
use crate::kobolds::{
    KoboldRole, STAGGERED_BELOW_PPM, close_contact_at, opening_balance, record_close_contact,
};
use crate::logistics::PlayerActiveSelection;
use crate::maintenance::Condition;
use crate::player::{PlayerActionGuard, PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::{ViewDirection, VoxelCoord};
use crate::world::PlayerBlockTargetError;
use serde::{Deserialize, Serialize};

const SHOVE_RECOVERY_TICKS: u32 = 16;
const SHOVE_REACH_MILLIMETERS: u32 = 1_600;
const SHOVE_BALANCE_IMPULSE_PPM: u32 = 80_000;

/// The actor's durable attack tempo; no client-controlled timer or virtual damage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CombatState {
    revision: u64,
    last_attack_at: Option<ControlTick>,
    recovery_control_ticks: u32,
}
impl CombatState {
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            last_attack_at: None,
            recovery_control_ticks: 0,
        }
    }
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }
    /// A resident cannot remember a blow later than the last player attack.
    pub(crate) fn admits_contact_at(&self, at: ControlTick) -> bool {
        self.last_attack_at.is_some_and(|last| last >= at)
    }
    pub(crate) fn validate(&self, now: ControlTick) -> Result<(), &'static str> {
        if self.last_attack_at.is_some_and(|at| at > now)
            || (self.revision == 0) != self.last_attack_at.is_none()
            || (self.last_attack_at.is_none() && self.recovery_control_ticks != 0)
            || (self.last_attack_at.is_some() && !(8..=60).contains(&self.recovery_control_ticks))
        {
            return Err("combat recovery record conflicts with control history");
        }
        Ok(())
    }
    fn ready(&self, now: ControlTick) -> bool {
        self.last_attack_at.is_none_or(|last| {
            now.checked_steps_since(last)
                .is_some_and(|elapsed| elapsed >= u64::from(self.recovery_control_ticks))
        })
    }
    fn apply_attack(&mut self, revision: u64, at: ControlTick, recovery_control_ticks: u32) {
        assert_eq!(self.revision, revision);
        self.revision = revision.checked_add(1).expect("reserved combat revision");
        self.last_attack_at = Some(at);
        self.recovery_control_ticks = recovery_control_ticks;
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
    EquipmentRevisionExhausted,
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
    StaleEquipmentRevision,
    WeaponChanged,
    StaleSurvivalRevision,
    TargetMoved,
    StaleResidentBalance,
}

#[must_use]
pub struct ValidatedPrimitiveAttack {
    player_action: PlayerActionGuard,
    at: ControlTick,
    expected_combat_revision: u64,
    expected_logistics_revision: u64,
    outcome: PrimitiveAttackOutcome,
    recovery_control_ticks: u32,
    weapon_wear: Option<(EquipmentId, Condition, Condition, u64)>,
    balance_impulse_ppm: u32,
    expected_resident_balance: Option<u32>,
}
impl ValidatedPrimitiveAttack {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<PrimitiveAttackOutcome, PrimitiveAttackCommitError> {
        if state.control_tick() != self.at {
            return Err(PrimitiveAttackCommitError::StaleControlTick);
        }
        if state.combat().revision() != self.expected_combat_revision {
            return Err(PrimitiveAttackCommitError::StaleCombatRevision);
        }
        if state.logistics().revision() != self.expected_logistics_revision {
            return Err(PrimitiveAttackCommitError::StaleLogisticsRevision);
        }
        self.player_action
            .precheck(state)
            .map_err(|_| PrimitiveAttackCommitError::StaleSurvivalRevision)?;
        if let Some((equipment, before, _, expected_revision)) = self.weapon_wear {
            if state.equipment().revision() != expected_revision {
                return Err(PrimitiveAttackCommitError::StaleEquipmentRevision);
            }
            if state.logistics().active_selection()
                != Some(PlayerActiveSelection::Equipment(equipment))
                || !state.logistics().is_equipment_carried(equipment)
                || state
                    .equipment()
                    .get_equipment(equipment)
                    .is_none_or(|record| record.condition() != before)
                || equipment_occupancy(state, equipment).is_some()
            {
                return Err(PrimitiveAttackCommitError::WeaponChanged);
            }
        }
        let victim = match self.outcome {
            PrimitiveAttackOutcome::Miss => None,
            PrimitiveAttackOutcome::Startled { resident, position }
            | PrimitiveAttackOutcome::Staggered {
                resident, position, ..
            } => Some((resident, position)),
        };
        if let Some((resident, position)) = victim {
            if close_contact_at(state, position) != Ok(Some((resident, position))) {
                return Err(PrimitiveAttackCommitError::TargetMoved);
            }
            if Some(opening_balance(state, resident)) != self.expected_resident_balance {
                return Err(PrimitiveAttackCommitError::StaleResidentBalance);
            }
        }
        if let Some((equipment, before, after, expected_revision)) = self.weapon_wear {
            state
                .equipment_state_mut()
                .apply_operation_condition_outcomes(
                    expected_revision,
                    expected_revision + 1,
                    &[EquipmentOperationConditionOutcome::new(
                        equipment, before, after,
                    )],
                );
        }
        state.combat_state_mut().apply_attack(
            self.expected_combat_revision,
            self.at,
            self.recovery_control_ticks,
        );
        if let Some((resident, _)) = victim {
            record_close_contact(state, resident, self.balance_impulse_ppm);
        }
        Ok(self.outcome)
    }
}

/// Both primitive modes share physical observation, recovery and reaction.
fn validate_attack(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
    weapon_required: bool,
) -> Result<ValidatedPrimitiveAttack, PrimitiveAttackError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => PrimitiveAttackError::PlayerNotAdmitted,
        PlayerActionValidationError::PlayerDead => PrimitiveAttackError::PlayerDead,
    })?;
    if state.player_work().active().is_some() {
        return Err(PrimitiveAttackError::PlayerBusy);
    }
    let weapon = if weapon_required {
        let Some(PlayerActiveSelection::Equipment(equipment)) =
            state.logistics().active_selection()
        else {
            return Err(PrimitiveAttackError::NoHeldWeapon);
        };
        if !state.logistics().is_equipment_carried(equipment) {
            return Err(PrimitiveAttackError::WeaponNotCarried);
        }
        let record = state
            .equipment()
            .get_equipment(equipment)
            .ok_or(PrimitiveAttackError::WeaponNotCarried)?;
        let definition = registries
            .equipment()
            .get_equipment(record.definition())
            .ok_or(PrimitiveAttackError::UnsupportedWeapon)?;
        let profile = definition
            .melee_profile()
            .ok_or(PrimitiveAttackError::UnsupportedWeapon)?;
        if record.condition() == Condition::FAILED {
            return Err(PrimitiveAttackError::BrokenWeapon);
        }
        if equipment_occupancy(state, equipment).is_some() {
            return Err(PrimitiveAttackError::WeaponBusy);
        }
        let revision = state.equipment().revision();
        revision
            .checked_add(1)
            .ok_or(PrimitiveAttackError::EquipmentRevisionExhausted)?;
        let before = record.condition();
        let after = Condition::new(
            before
                .parts_per_million()
                .saturating_sub(profile.wear_ppm_per_swing()),
        )
        .expect("saturating equipment wear fits condition");
        Some((profile, (equipment, before, after, revision)))
    } else {
        if state.logistics().active_selection().is_some() {
            return Err(PrimitiveAttackError::OccupiedHand);
        }
        None
    };
    let at = state.control_tick();
    if !state.combat().ready(at) {
        return Err(PrimitiveAttackError::Recovering {
            last_at: state
                .combat()
                .last_attack_at
                .expect("recovery follows an attack"),
        });
    }
    state
        .combat()
        .revision()
        .checked_add(1)
        .ok_or(PrimitiveAttackError::RevisionExhausted)?;
    let reach = weapon.map_or(SHOVE_REACH_MILLIMETERS, |(profile, _)| {
        profile.reach_millimeters()
    });
    let mut outcome = resolve_contact(registries, state, direction, reach)?;
    // Material condition reduces delivered impact, independently of tempo and reach.
    // The transient posture metric is not health, blood, or unaccounted material.
    let balance_impulse_ppm =
        weapon.map_or(SHOVE_BALANCE_IMPULSE_PPM, |(profile, (_, before, _, _))| {
            let scaled = u64::from(profile.balance_impulse_ppm())
                * u64::from(before.parts_per_million())
                / 1_000_000;
            u32::try_from(scaled.max(1)).expect("bounded authored impact")
        });
    let expected_resident_balance = match outcome {
        PrimitiveAttackOutcome::Startled { resident, position } => {
            let before = opening_balance(state, resident);
            let after = before.saturating_sub(balance_impulse_ppm);
            if after < STAGGERED_BELOW_PPM {
                outcome = PrimitiveAttackOutcome::Staggered {
                    resident,
                    position,
                    remaining_balance_ppm: after,
                };
            }
            Some(before)
        }
        _ => None,
    };
    Ok(ValidatedPrimitiveAttack {
        player_action,
        at,
        expected_combat_revision: state.combat().revision(),
        expected_logistics_revision: state.logistics().revision(),
        outcome,
        recovery_control_ticks: weapon.map_or(SHOVE_RECOVERY_TICKS, |(profile, _)| {
            profile.recovery_control_ticks()
        }),
        weapon_wear: weapon.map(|(_, wear)| wear),
        balance_impulse_ppm,
        expected_resident_balance,
    })
}

/// Player-local, renderer-neutral attack readiness for an input adapter or HUD.
/// The projection reveals no remote actor identity or hidden world state.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerMeleeAssessment {
    weapon: Option<EquipmentId>,
    reach_millimeters: u32,
    recovery_control_ticks: u32,
    remaining_recovery_ticks: u32,
    condition: Option<Condition>,
}
impl PlayerMeleeAssessment {
    pub const fn weapon(self) -> Option<EquipmentId> {
        self.weapon
    }
    pub const fn reach_millimeters(self) -> u32 {
        self.reach_millimeters
    }
    pub const fn recovery_control_ticks(self) -> u32 {
        self.recovery_control_ticks
    }
    pub const fn remaining_recovery_ticks(self) -> u32 {
        self.remaining_recovery_ticks
    }
    pub const fn condition(self) -> Option<Condition> {
        self.condition
    }
    pub const fn ready(self) -> bool {
        self.remaining_recovery_ticks == 0
    }
}

/// Observe current combat readiness without initiating contact or consuming
/// equipment wear. Selected non-weapons and unavailable equipment fail closed.
pub fn assess_player_melee(
    registries: &Registries,
    state: &AppState,
) -> Result<PlayerMeleeAssessment, PrimitiveAttackError> {
    validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => PrimitiveAttackError::PlayerNotAdmitted,
        PlayerActionValidationError::PlayerDead => PrimitiveAttackError::PlayerDead,
    })?;
    if state.player_work().active().is_some() {
        return Err(PrimitiveAttackError::PlayerBusy);
    }
    let (weapon, profile, condition) = match state.logistics().active_selection() {
        None => (None, None, None),
        Some(PlayerActiveSelection::Material(_)) => {
            return Err(PrimitiveAttackError::UnsupportedWeapon);
        }
        Some(PlayerActiveSelection::Equipment(equipment)) => {
            if !state.logistics().is_equipment_carried(equipment) {
                return Err(PrimitiveAttackError::WeaponNotCarried);
            }
            let record = state
                .equipment()
                .get_equipment(equipment)
                .ok_or(PrimitiveAttackError::WeaponNotCarried)?;
            let definition = registries
                .equipment()
                .get_equipment(record.definition())
                .ok_or(PrimitiveAttackError::UnsupportedWeapon)?;
            let profile = definition
                .melee_profile()
                .ok_or(PrimitiveAttackError::UnsupportedWeapon)?;
            if record.condition() == Condition::FAILED {
                return Err(PrimitiveAttackError::BrokenWeapon);
            }
            if equipment_occupancy(state, equipment).is_some() {
                return Err(PrimitiveAttackError::WeaponBusy);
            }
            (Some(equipment), Some(profile), Some(record.condition()))
        }
    };
    let remaining_recovery_ticks = state.combat().last_attack_at.map_or(0, |last| {
        let elapsed = state
            .control_tick()
            .checked_steps_since(last)
            .expect("trusted combat history cannot be in the future");
        let remaining = u64::from(state.combat().recovery_control_ticks).saturating_sub(elapsed);
        u32::try_from(remaining).expect("combat recovery maximum is 60 ticks")
    });
    Ok(PlayerMeleeAssessment {
        weapon,
        reach_millimeters: profile.map_or(
            SHOVE_REACH_MILLIMETERS,
            MeleeWeaponProfile::reach_millimeters,
        ),
        recovery_control_ticks: profile.map_or(
            SHOVE_RECOVERY_TICKS,
            MeleeWeaponProfile::recovery_control_ticks,
        ),
        remaining_recovery_ticks,
        condition,
    })
}

/// Unarmed shove is available with an empty hand. It never damages a living resident.
pub fn validate_primitive_shove(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<ValidatedPrimitiveAttack, PrimitiveAttackError> {
    validate_attack(registries, state, direction, false)
}

/// A selected, physically carried weapon with an authored melee profile may
/// startle an opening resident. This slice deliberately does not deal injury.
pub fn validate_primitive_strike(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<ValidatedPrimitiveAttack, PrimitiveAttackError> {
    validate_attack(registries, state, direction, true)
}

fn resolve_contact(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
    reach_millimeters: u32,
) -> Result<PrimitiveAttackOutcome, PrimitiveAttackError> {
    let player_at = state
        .logistics()
        .player()
        .expect("admitted logistics")
        .position();
    let target = crate::world::raycast_player_air_voxel_with_reach(
        registries,
        state,
        direction,
        reach_millimeters,
        |voxel, _| match close_contact_at(state, voxel) {
            Ok(Some((role, anchor))) => Some(Ok((role, anchor))),
            Ok(None) => None,
            Err(()) => Some(Err(voxel)),
        },
    )
    .map_err(PrimitiveAttackError::Target)?;
    match target {
        Some(Ok((resident, position))) if player_at != position => {
            Ok(PrimitiveAttackOutcome::Startled { resident, position })
        }
        Some(Err(voxel)) => Err(PrimitiveAttackError::AmbiguousContact { voxel }),
        _ => Ok(PrimitiveAttackOutcome::Miss),
    }
}

#[cfg(test)]
#[path = "combat_tests.rs"]
mod tests;
