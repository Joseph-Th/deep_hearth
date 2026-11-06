//! Player-controlled melee contact: revision-bound attack execution and physical validation.

use super::*;

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
        if state.player_work().revision() != self.expected_player_work_revision {
            return Err(PrimitiveAttackCommitError::StalePlayerWorkRevision);
        }
        if state.world().revision() != self.expected_world_revision {
            return Err(PrimitiveAttackCommitError::StaleWorldRevision);
        }
        if state.structures().revision() != self.expected_structure_revision {
            return Err(PrimitiveAttackCommitError::StaleStructureRevision);
        }
        if state.combat().prepared != self.expected_preparation {
            return Err(PrimitiveAttackCommitError::StaleCombatRevision);
        }
        self.player_action
            .precheck(state)
            .map_err(|_| PrimitiveAttackCommitError::StaleSurvivalRevision)?;
        if let Some((equipment, before, _, expected_revision)) = self.weapon_wear {
            if state.equipment().revision() != expected_revision
                || !state.can_spend_equipment_revisions(1)
            {
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
        let _ = apply_survival_tick(state, Some(self.exertion_plan));
        Ok(self.outcome)
    }
}

fn validate_attack_timing(
    state: &AppState,
    weapon: Option<EquipmentId>,
    powered: bool,
) -> Result<(), PrimitiveAttackError> {
    let at = state.control_tick();
    if powered {
        let prepared = state
            .combat()
            .prepared
            .ok_or(PrimitiveAttackError::NotPreparing)?;
        if weapon != Some(prepared.weapon) {
            return Err(PrimitiveAttackError::PreparationWeaponChanged);
        }
        let ready_at = prepared
            .started_at
            .checked_add_steps(preparation::PREPARATION_TICKS)
            .ok_or(PrimitiveAttackError::RevisionExhausted)?;
        if at < ready_at {
            return Err(PrimitiveAttackError::PreparationNotReady { ready_at });
        }
    } else if state.combat().prepared.is_some() {
        return Err(PrimitiveAttackError::AlreadyPreparing);
    }
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
    Ok(())
}

/// Both primitive modes share physical observation, recovery and reaction.
pub(super) fn validate_attack(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
    weapon_required: bool,
    powered: bool,
) -> Result<ValidatedPrimitiveAttack, PrimitiveAttackError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => PrimitiveAttackError::PlayerNotAdmitted,
        PlayerActionValidationError::PlayerDead => PrimitiveAttackError::PlayerDead,
    })?;
    if state.player_work().active().is_some() {
        return Err(PrimitiveAttackError::PlayerBusy);
    }
    if powered
        && state.combat().prepared.is_some_and(|prepared| {
            state.logistics().active_selection()
                != Some(PlayerActiveSelection::Equipment(prepared.weapon))
        })
    {
        return Err(PrimitiveAttackError::PreparationWeaponChanged);
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
        // A background process or mine may have already reserved the last
        // equipment revision for its future wear settlement. Melee cannot
        // spend that promise even when the selected weapon is otherwise idle.
        if !state.can_spend_equipment_revisions(1) {
            return Err(PrimitiveAttackError::EquipmentRevisionExhausted);
        }
        let revision = state.equipment().revision();
        let before = record.condition();
        let wear = profile.wear_ppm_per_swing() * if powered { 2 } else { 1 };
        let after = Condition::new(before.parts_per_million().saturating_sub(wear))
            .expect("saturating equipment wear fits condition");
        Some((profile, (equipment, before, after, revision)))
    } else {
        if state.logistics().active_selection().is_some() {
            return Err(PrimitiveAttackError::OccupiedHand);
        }
        None
    };
    let at = state.control_tick();
    validate_attack_timing(state, weapon.map(|(_, (id, _, _, _))| id), powered)?;
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
            let scaled = if powered { scaled * 3 / 2 } else { scaled };
            u32::try_from(scaled.max(1)).expect("bounded authored impact")
        });
    let cost = weapon.map_or(SHOVE_EXERTION, |(profile, _)| profile.exertion_per_swing());
    let cost = if powered {
        prepared_attack_exertion(cost)?
    } else {
        cost
    };
    let exertion_plan =
        decide_melee_exertion(registries, state, cost).map_err(|error| match error {
            MeleeExertionError::InsufficientMetabolicEnergy => {
                PrimitiveAttackError::InsufficientMetabolicEnergy
            }
            MeleeExertionError::InsufficientHydration => {
                PrimitiveAttackError::InsufficientHydration
            }
            MeleeExertionError::RevisionExhausted => {
                PrimitiveAttackError::SurvivalRevisionExhausted
            }
        })?;
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
        PrimitiveAttackOutcome::Miss | PrimitiveAttackOutcome::Staggered { .. } => None,
    };
    Ok(ValidatedPrimitiveAttack {
        player_action,
        at,
        expected_combat_revision: state.combat().revision(),
        expected_logistics_revision: state.logistics().revision(),
        expected_player_work_revision: state.player_work().revision(),
        expected_world_revision: state.world().revision(),
        expected_structure_revision: state.structures().revision(),
        outcome,
        recovery_control_ticks: weapon.map_or(SHOVE_RECOVERY_TICKS, |(profile, _)| {
            profile.recovery_control_ticks()
                + if powered {
                    preparation::PREPARED_RECOVERY_BONUS_TICKS
                } else {
                    0
                }
        }),
        weapon_wear: weapon.map(|(_, wear)| wear),
        balance_impulse_ppm,
        expected_resident_balance,
        expected_preparation: state.combat().prepared,
        exertion_plan,
    })
}

pub(super) fn prepared_attack_exertion(
    cost: SurvivalExertion,
) -> Result<SurvivalExertion, PrimitiveAttackError> {
    Ok(SurvivalExertion::new(
        Energy::from_nanojoules(
            cost.energy_cost_per_tick()
                .nanojoules()
                .checked_mul(2)
                .ok_or(PrimitiveAttackError::ExertionOverflow)?,
        ),
        Volume::from_microliters(
            cost.hydration_loss_per_tick()
                .microliters()
                .checked_mul(2)
                .ok_or(PrimitiveAttackError::ExertionOverflow)?,
        ),
    ))
}

/// Unarmed shove is available with an empty hand. It never damages a living resident.
pub fn validate_primitive_shove(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<ValidatedPrimitiveAttack, PrimitiveAttackError> {
    validate_attack(registries, state, direction, false, false)
}

/// A selected, physically carried weapon with an authored melee profile may
/// startle an opening resident. This slice deliberately does not deal injury.
pub fn validate_primitive_strike(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<ValidatedPrimitiveAttack, PrimitiveAttackError> {
    validate_attack(registries, state, direction, true, false)
}

fn resolve_contact(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
    reach_millimeters: u32,
) -> Result<PrimitiveAttackOutcome, PrimitiveAttackError> {
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
        Some(Ok((resident, position))) => {
            Ok(PrimitiveAttackOutcome::Startled { resident, position })
        }
        Some(Err(voxel)) => Err(PrimitiveAttackError::AmbiguousContact { voxel }),
        _ => Ok(PrimitiveAttackOutcome::Miss),
    }
}
