//! Mining completion planning and equipment-wear application for due work.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::EquipmentOperationConditionOutcome;

use super::super::MiningJobId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MiningTickError {
    MiningRevisionExhausted,
    EquipmentRevisionExhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MiningTickPlan {
    expected_revision: u64,
    next_revision: u64,
    ready_at: SimulationTick,
    equipment_outcomes: Vec<EquipmentOperationConditionOutcome>,
}

pub(crate) fn decide_mining_tick(
    state: &AppState,
    next_tick: SimulationTick,
) -> Result<Option<MiningTickPlan>, MiningTickError> {
    let Some(due_jobs) = state.mining().jobs_due_at(next_tick) else {
        return Ok(None);
    };
    let expected_revision = state.mining().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(MiningTickError::MiningRevisionExhausted)?;
    let mut equipment_outcomes = Vec::with_capacity(due_jobs.len());
    for &job in due_jobs {
        let record = state
            .mining()
            .get_job(job)
            .unwrap_or_else(|| panic!("runtime invariant broken: due mining job disappeared"));
        let equipment = state
            .equipment()
            .get_equipment(record.equipment())
            .unwrap_or_else(|| panic!("runtime invariant broken: mining equipment disappeared"));
        assert_eq!(
            equipment.condition(),
            record.equipment_condition_before(),
            "mining occupancy must prevent equipment condition mutation while work is active"
        );
        if record.equipment_condition_after() != record.equipment_condition_before() {
            equipment_outcomes.push(EquipmentOperationConditionOutcome::new(
                record.equipment(),
                record.equipment_condition_before(),
                record.equipment_condition_after(),
            ));
        }
    }
    if !equipment_outcomes.is_empty() {
        state
            .equipment()
            .revision()
            .checked_add(2)
            .ok_or(MiningTickError::EquipmentRevisionExhausted)?;
    }
    Ok(Some(MiningTickPlan {
        expected_revision,
        next_revision,
        ready_at: next_tick,
        equipment_outcomes,
    }))
}

pub(crate) fn apply_mining_tick(
    state: &mut AppState,
    plan: Option<MiningTickPlan>,
) -> Vec<MiningJobId> {
    let Some(plan) = plan else {
        return Vec::new();
    };
    if !plan.equipment_outcomes.is_empty() {
        let expected_equipment_revision = state.equipment().revision();
        let next_equipment_revision = expected_equipment_revision
            .checked_add(1)
            .unwrap_or_else(|| panic!("prevalidated mining equipment revision exhausted"));
        state
            .equipment_state_mut()
            .apply_operation_condition_outcomes(
                expected_equipment_revision,
                next_equipment_revision,
                &plan.equipment_outcomes,
            );
    }
    state.mining_state_mut().mark_due_jobs_ready(
        plan.expected_revision,
        plan.next_revision,
        plan.ready_at,
    )
}
