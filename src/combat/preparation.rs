//! Optional committed nonlethal windup using the existing actor, equipment and combat owners.

use super::*;

pub(super) const PREPARATION_TICKS: u64 = 10;
pub(super) const PREPARED_RECOVERY_BONUS_TICKS: u32 = 12;
const MIN_FACING_DOT: i64 = 800_000_000_000;

/// Saved control-rate preparation. A new strike still traces live world truth on release.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PreparedStrike {
    pub(super) weapon: EquipmentId,
    pub(super) started_at: ControlTick,
    pub(super) aim: [i32; 3],
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitivePreparationOutcome {
    Prepared { ready_at: ControlTick },
    Cancelled,
}

#[must_use]
pub struct ValidatedPrimitivePreparation {
    at: ControlTick,
    expected_combat_revision: u64,
    expected_logistics_revision: u64,
    expected_player_work_revision: u64,
    expected_equipment_revision: u64,
    player_action: PlayerActionGuard,
    next: Option<PreparedStrike>,
    outcome: PrimitivePreparationOutcome,
}

impl ValidatedPrimitivePreparation {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<PrimitivePreparationOutcome, PrimitiveAttackCommitError> {
        if state.control_tick() != self.at {
            return Err(PrimitiveAttackCommitError::StaleControlTick);
        }
        if state.combat().revision() != self.expected_combat_revision {
            return Err(PrimitiveAttackCommitError::StaleCombatRevision);
        }
        if state.logistics().revision() != self.expected_logistics_revision {
            return Err(PrimitiveAttackCommitError::StaleLogisticsRevision);
        }
        if state.player_work().revision() != self.expected_player_work_revision {
            return Err(PrimitiveAttackCommitError::StalePlayerWorkRevision);
        }
        if state.equipment().revision() != self.expected_equipment_revision {
            return Err(PrimitiveAttackCommitError::StaleEquipmentRevision);
        }
        self.player_action
            .precheck(state)
            .map_err(|_| PrimitiveAttackCommitError::StaleSurvivalRevision)?;
        if let Some(next) = self.next
            && (state.logistics().active_selection()
                != Some(PlayerActiveSelection::Equipment(next.weapon))
                || !state.logistics().is_equipment_carried(next.weapon)
                || state
                    .equipment()
                    .get_equipment(next.weapon)
                    .is_none_or(|r| r.condition() == Condition::FAILED)
                || equipment_occupancy(state, next.weapon).is_some())
        {
            return Err(PrimitiveAttackCommitError::WeaponChanged);
        }
        let combat = state.combat_state_mut();
        combat.revision = combat
            .revision
            .checked_add(1)
            .expect("reserved preparation revision");
        combat.prepared = self.next;
        Ok(self.outcome)
    }
}

fn preflight(state: &AppState) -> Result<(PlayerActionGuard, u64), PrimitiveAttackError> {
    let action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => PrimitiveAttackError::PlayerNotAdmitted,
        PlayerActionValidationError::PlayerDead => PrimitiveAttackError::PlayerDead,
    })?;
    if state.player_work().active().is_some() {
        return Err(PrimitiveAttackError::PlayerBusy);
    }
    let revision = state.combat().revision();
    revision
        .checked_add(1)
        .ok_or(PrimitiveAttackError::RevisionExhausted)?;
    Ok((action, revision))
}

/// Raise the currently carried melee weapon for a stronger but slower blow.
/// Merely preparing spends no wear, contact, matter or health.
pub fn validate_prepare_primitive_strike(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<ValidatedPrimitivePreparation, PrimitiveAttackError> {
    let (player_action, revision) = preflight(state)?;
    revision
        .checked_add(2)
        .ok_or(PrimitiveAttackError::RevisionExhausted)?;
    if state.combat().prepared.is_some() {
        return Err(PrimitiveAttackError::AlreadyPreparing);
    }
    let assessment = assess_player_melee(registries, state)?;
    let weapon = assessment
        .weapon()
        .ok_or(PrimitiveAttackError::NoHeldWeapon)?;
    if !state.combat().ready(state.control_tick()) {
        return Err(PrimitiveAttackError::Recovering {
            last_at: state
                .combat()
                .last_attack_at
                .expect("unready means previous attack"),
        });
    }
    let at = state.control_tick();
    let ready_at = at
        .checked_add_steps(PREPARATION_TICKS)
        .ok_or(PrimitiveAttackError::RevisionExhausted)?;
    Ok(ValidatedPrimitivePreparation {
        at,
        expected_combat_revision: revision,
        expected_logistics_revision: state.logistics().revision(),
        expected_player_work_revision: state.player_work().revision(),
        expected_equipment_revision: state.equipment().revision(),
        player_action,
        next: Some(PreparedStrike {
            weapon,
            started_at: at,
            aim: [
                direction.x_parts_per_million(),
                direction.y_parts_per_million(),
                direction.z_parts_per_million(),
            ],
        }),
        outcome: PrimitivePreparationOutcome::Prepared { ready_at },
    })
}

/// Release within roughly 37 degrees of the committed facing, after ten 20 Hz ticks.
/// A target must still be unobstructed and physically present at release.
pub fn validate_release_prepared_strike(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<ValidatedPrimitiveAttack, PrimitiveAttackError> {
    let prepared = state
        .combat()
        .prepared
        .ok_or(PrimitiveAttackError::NotPreparing)?;
    let dot = i64::from(prepared.aim[0]) * i64::from(direction.x_parts_per_million())
        + i64::from(prepared.aim[1]) * i64::from(direction.y_parts_per_million())
        + i64::from(prepared.aim[2]) * i64::from(direction.z_parts_per_million());
    if dot < MIN_FACING_DOT {
        return Err(PrimitiveAttackError::PreparationAimChanged);
    }
    validate_attack(registries, state, direction, true, true)
}

/// Deliberately lower the prepared weapon without producing an attack.
/// Normal recovery history remains unchanged; cancel does not reset earlier attack tempo.
pub fn validate_cancel_primitive_preparation(
    state: &AppState,
) -> Result<ValidatedPrimitivePreparation, PrimitiveAttackError> {
    let (player_action, revision) = preflight(state)?;
    if state.combat().prepared.is_none() {
        return Err(PrimitiveAttackError::NotPreparing);
    }
    Ok(ValidatedPrimitivePreparation {
        at: state.control_tick(),
        expected_combat_revision: revision,
        expected_logistics_revision: state.logistics().revision(),
        expected_player_work_revision: state.player_work().revision(),
        expected_equipment_revision: state.equipment().revision(),
        player_action,
        next: None,
        outcome: PrimitivePreparationOutcome::Cancelled,
    })
}

/// Cross-owner save admission: combat history requires an admitted actor, and
/// a prepared strike must reference a real melee-capable weapon. Losing carrying
/// custody or changing quick slots during preparation is legal; release then
/// rejects until the player restores or cancels the stance.
pub(crate) fn validate_loaded_combat_references(
    equipment_registry: &crate::equipment::EquipmentRegistry,
    state: &AppState,
) -> Result<(), &'static str> {
    if state.combat().revision() != 0 && state.logistics().player().is_none() {
        return Err("combat history requires an admitted player");
    }
    let Some(prepared) = state.combat().prepared else {
        return Ok(());
    };
    if state
        .survival()
        .player()
        .is_none_or(|player| player.vitality() == crate::survival::Vitality::ZERO)
    {
        return Err("dead or absent actor cannot retain a prepared strike");
    }
    if state.player_work().active().is_some() {
        return Err("prepared strike cannot coexist with exclusive player work");
    }
    let weapon = state
        .equipment()
        .get_equipment(prepared.weapon)
        .ok_or("prepared melee weapon has no equipment identity")?;
    if !equipment_registry
        .get_equipment(weapon.definition())
        .is_some_and(|definition| definition.melee_profile().is_some())
    {
        return Err("prepared strike references equipment without a melee profile");
    }
    Ok(())
}
