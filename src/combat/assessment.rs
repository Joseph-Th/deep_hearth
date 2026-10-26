//! Read-only player melee HUD projection; combat retains attack mutation ownership.

use crate::core::state::AppState;
use crate::equipment::{EquipmentId, MeleeWeaponProfile, equipment_occupancy};
use crate::logistics::PlayerActiveSelection;
use crate::maintenance::Condition;
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;

use super::{PrimitiveAttackError, SHOVE_REACH_MILLIMETERS, SHOVE_RECOVERY_TICKS, preparation};

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
    preparation_remaining_ticks: Option<u32>,
    prepared_weapon_matches_selection: bool,
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
    pub const fn preparation_remaining_ticks(self) -> Option<u32> {
        self.preparation_remaining_ticks
    }
    /// Whether an ordinary shove or strike can be started; a wound-up strike is separate.
    pub const fn ready(self) -> bool {
        self.remaining_recovery_ticks == 0 && self.preparation_remaining_ticks.is_none()
    }

    /// Whether a prepared blow has completed windup, pending live weapon and aim checks.
    pub const fn prepared_release_ready(self) -> bool {
        self.prepared_weapon_matches_selection
            && self.remaining_recovery_ticks == 0
            && matches!(self.preparation_remaining_ticks, Some(0))
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
        u32::try_from(remaining).expect("combat recovery is bounded by authored strike timing")
    });
    let selected_recovery = profile.map_or(
        SHOVE_RECOVERY_TICKS,
        MeleeWeaponProfile::recovery_control_ticks,
    );
    let recovery_control_ticks = if remaining_recovery_ticks > 0 {
        // Switching equipment does not shorten an already committed attack.
        state.combat().recovery_control_ticks
    } else if state
        .combat()
        .prepared
        .is_some_and(|prepared| Some(prepared.weapon) == weapon)
    {
        selected_recovery + preparation::PREPARED_RECOVERY_BONUS_TICKS
    } else {
        selected_recovery
    };
    Ok(PlayerMeleeAssessment {
        weapon,
        reach_millimeters: profile.map_or(
            SHOVE_REACH_MILLIMETERS,
            MeleeWeaponProfile::reach_millimeters,
        ),
        recovery_control_ticks,
        remaining_recovery_ticks,
        condition,
        prepared_weapon_matches_selection: state
            .combat()
            .prepared
            .is_some_and(|prepared| Some(prepared.weapon) == weapon),
        preparation_remaining_ticks: state.combat().prepared.map(|prepared| {
            let elapsed = state
                .control_tick()
                .checked_steps_since(prepared.started_at)
                .expect("trusted preparation cannot be in the future");
            u32::try_from(preparation::PREPARATION_TICKS.saturating_sub(elapsed))
                .expect("authored windup fits u32")
        }),
    })
}
