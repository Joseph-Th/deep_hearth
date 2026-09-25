//! Mining completion planning and equipment-wear application for due work.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;
use crate::geology::GeologicalDepositId;
use crate::maintenance::Condition;

use super::super::MiningJobId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GeologicalExtraction {
    deposit: GeologicalDepositId,
    mass: Mass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EquipmentConditionChange {
    equipment: EquipmentId,
    before: Condition,
    after: Condition,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MiningTickPlan {
    expected_revision: u64,
    expected_geology_revision: u64,
    completion_tick: SimulationTick,
    extraction: GeologicalExtraction,
    equipment_condition_change: Option<EquipmentConditionChange>,
}

impl MiningTickPlan {
    pub(crate) const fn mining_revision_steps(&self) -> u64 {
        1
    }

    pub(crate) const fn geology_revision_steps(&self) -> u64 {
        1
    }

    pub(crate) fn equipment_revision_steps(&self) -> u64 {
        u64::from(self.equipment_condition_change.is_some())
    }
}

pub(crate) fn decide_mining_tick(
    state: &AppState,
    next_tick: SimulationTick,
) -> Option<MiningTickPlan> {
    let due_jobs = state.mining().jobs_due_at(next_tick)?;
    assert_eq!(
        due_jobs.len(),
        1,
        "runtime invariant broken: exclusive player labor permits only one due mining job"
    );
    let job = *due_jobs
        .first()
        .unwrap_or_else(|| unreachable!("nonempty due mining bucket has a first job"));
    let expected_revision = state.mining().revision();
    let expected_geology_revision = state.geology().revision();
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
    let extraction = GeologicalExtraction {
        deposit: record.deposit(),
        mass: record.output().mass(),
    };
    let equipment = state
        .equipment()
        .get_equipment(record.equipment())
        .unwrap_or_else(|| panic!("runtime invariant broken: mining equipment disappeared"));
    assert_eq!(
        equipment.condition(),
        record.equipment_condition_before(),
        "mining occupancy must prevent equipment condition mutation while work is active"
    );
    let equipment_condition_change = (record.equipment_condition_after()
        != record.equipment_condition_before())
    .then_some(EquipmentConditionChange {
        equipment: record.equipment(),
        before: record.equipment_condition_before(),
        after: record.equipment_condition_after(),
    });
    Some(MiningTickPlan {
        expected_revision,
        expected_geology_revision,
        completion_tick: next_tick,
        extraction,
        equipment_condition_change,
    })
}

pub(crate) fn apply_mining_tick(
    state: &mut AppState,
    plan: Option<MiningTickPlan>,
) -> Option<MiningJobId> {
    let plan = plan?;
    assert_eq!(state.geology().revision(), plan.expected_geology_revision);
    let next_mining_revision = plan
        .expected_revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("prebudgeted mining revision exhausted"));
    let next_geology_revision = plan
        .expected_geology_revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("prebudgeted geology revision exhausted"));
    state.mining().assert_due_job_ready_available(
        plan.expected_revision,
        next_mining_revision,
        plan.completion_tick,
    );
    let equipment_revision = plan.equipment_condition_change.map(|change| {
        let expected = state.equipment().revision();
        let next = expected
            .checked_add(1)
            .unwrap_or_else(|| panic!("prebudgeted mining equipment revision exhausted"));
        state
            .equipment()
            .assert_condition_change_available(change.equipment, change.before, next);
        assert!(
            change.after <= change.before,
            "completed mining operation cannot improve equipment condition"
        );
        (change, next)
    });
    state.geology_state_mut().apply_extraction(
        plan.extraction.deposit,
        plan.extraction.mass,
        next_geology_revision,
    );
    if let Some((change, next_equipment_revision)) = equipment_revision {
        state.equipment_state_mut().apply_condition_change(
            change.equipment,
            change.before,
            change.after,
            next_equipment_revision,
        );
    }
    Some(state.mining_state_mut().mark_due_job_ready(
        plan.expected_revision,
        next_mining_revision,
        plan.completion_tick,
    ))
}
