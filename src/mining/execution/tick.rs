//! Mining completion planning and equipment-wear application for due work.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::EquipmentOperationConditionOutcome;
use crate::geology::GeologicalDepositId;

use super::super::MiningJobId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MiningTickError {
    Geology,
    Mining,
    Equipment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GeologicalExtraction {
    deposit: GeologicalDepositId,
    remaining_after: Mass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MiningTickPlan {
    expected_revision: u64,
    next_revision: u64,
    expected_geology_revision: u64,
    next_geology_revision: u64,
    completion_tick: SimulationTick,
    extraction: GeologicalExtraction,
    equipment_outcomes: Vec<EquipmentOperationConditionOutcome>,
}

impl MiningTickPlan {
    pub(crate) fn equipment_revision_steps(&self) -> u64 {
        u64::from(!self.equipment_outcomes.is_empty())
    }
}

pub(crate) fn decide_mining_tick(
    state: &AppState,
    next_tick: SimulationTick,
) -> Result<Option<MiningTickPlan>, MiningTickError> {
    let Some(due_jobs) = state.mining().jobs_due_at(next_tick) else {
        return Ok(None);
    };
    assert_eq!(
        due_jobs.len(),
        1,
        "runtime invariant broken: exclusive player labor permits only one due mining job"
    );
    let expected_revision = state.mining().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(MiningTickError::Mining)?;
    let expected_geology_revision = state.geology().revision();
    let next_geology_revision = expected_geology_revision
        .checked_add(1)
        .ok_or(MiningTickError::Geology)?;
    let mut equipment_outcomes = Vec::with_capacity(1);
    let mut extraction = None;
    for &job in due_jobs {
        let record = state
            .mining()
            .get_job(job)
            .unwrap_or_else(|| panic!("runtime invariant broken: due mining job disappeared"));
        let deposit = state
            .geology()
            .get_deposit(record.deposit())
            .unwrap_or_else(|| panic!("runtime invariant broken: mining deposit disappeared"));
        assert_eq!(
            deposit.remaining_mass(),
            record.deposit_mass_before(),
            "runtime invariant broken: working mining source mass changed before completion"
        );
        let remaining_after = record
            .deposit_mass_before()
            .checked_sub(record.output().mass())
            .unwrap_or_else(|| {
                panic!("runtime invariant broken: mining output exceeds source trace")
            });
        extraction = Some(GeologicalExtraction {
            deposit: record.deposit(),
            remaining_after,
        });
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
            .checked_add(1)
            .ok_or(MiningTickError::Equipment)?;
    }
    Ok(Some(MiningTickPlan {
        expected_revision,
        next_revision,
        expected_geology_revision,
        next_geology_revision,
        completion_tick: next_tick,
        extraction: extraction.unwrap_or_else(|| {
            panic!("runtime invariant broken: due mining job produced no extraction")
        }),
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
    assert_eq!(state.geology().revision(), plan.expected_geology_revision);
    state.geology_state_mut().apply_extraction(
        plan.extraction.deposit,
        plan.extraction.remaining_after,
        plan.next_geology_revision,
    );
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
        plan.completion_tick,
    )
}
