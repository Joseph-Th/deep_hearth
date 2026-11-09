//! Focused proofs for mining derived-index scheduling semantics.

use super::*;
use crate::core::quantity::{Mass, Temperature};
use crate::equipment::{EquipmentDefinitionId, EquipmentId, EquipmentOperationTrace};
use crate::geology::GeologicalDepositId;
use crate::inventory::StockpileId;
use crate::maintenance::Condition;
use crate::material::{CommodityKey, FormId, MaterialId, MaterialLotSpec};
use crate::mining::MiningMethodId;
use crate::mining::state::{
    MiningJobIdentity, MiningJobPhase, MiningJobRecord, MiningJobResources, MiningJobSchedule,
};

fn job_record(condition_after: Condition, completes_at: SimulationTick) -> MiningJobRecord {
    job_record_for(1, condition_after, completes_at)
}

fn job_record_for(
    id: u32,
    condition_after: Condition,
    completes_at: SimulationTick,
) -> MiningJobRecord {
    let mass = Mass::from_milligrams(10);
    MiningJobRecord::new(
        MiningJobIdentity {
            id: MiningJobId::new(u64::from(id)),
            method: MiningMethodId::new(1),
            deposit: GeologicalDepositId::new(id),
        },
        MiningJobResources {
            destination: StockpileId::new(id),
            equipment_trace: EquipmentOperationTrace::new(
                EquipmentId::new(id),
                EquipmentDefinitionId::new(1),
                Condition::PRISTINE,
            ),
            deposit_mass_before: mass,
            requested_mass: mass,
            output: MaterialLotSpec::new(
                CommodityKey::new(MaterialId::new(1), FormId::new(1)),
                mass,
                Temperature::from_millikelvin(300_000),
            ),
            equipment_condition_after: condition_after,
        },
        MiningJobSchedule {
            started_at: SimulationTick::ZERO,
            completes_at,
            phase: MiningJobPhase::Working,
        },
    )
}

#[test]
fn settlement_index_tracks_completion_retirement_and_rebuild_without_future_jobs() {
    let first_due = SimulationTick::new(4);
    let future_due = SimulationTick::new(12);
    let first_id = MiningJobId::new(1);
    let second_id = MiningJobId::new(2);
    let mut state = MiningState::new();
    let candidates = |state: &MiningState, at| {
        state
            .structurally_settling_jobs_at(at)
            .map(MiningJobRecord::id)
            .collect::<BTreeSet<_>>()
    };

    state.insert_job(job_record(Condition::PRISTINE, first_due), 2, 1);
    assert_eq!(candidates(&state, first_due), BTreeSet::from([first_id]));
    assert!(candidates(&state, future_due).is_empty());
    assert_eq!(
        state.mark_due_job_awaiting_settlement(1, 2, first_due),
        first_id
    );
    assert_eq!(candidates(&state, first_due), BTreeSet::from([first_id]));
    assert_eq!(candidates(&state, future_due), BTreeSet::from([first_id]));

    // A later, still working job cannot be selected before its own completion.
    // A previously completed parcel must remain visible until retirement.
    state.insert_job(job_record_for(2, Condition::PRISTINE, future_due), 3, 3);
    assert_eq!(candidates(&state, first_due), BTreeSet::from([first_id]));
    assert_eq!(
        candidates(&state, future_due),
        BTreeSet::from([first_id, second_id])
    );
    let mut rebuilt = state.clone();
    rebuilt.rebuild_derived_indexes();
    assert_eq!(rebuilt, state, "trusted-load index rebuild must be exact");

    let retired = state.remove_awaiting_settlement_job(first_id, 3, 4);
    assert_eq!(retired.id(), first_id);
    assert!(candidates(&state, first_due).is_empty());
    assert_eq!(candidates(&state, future_due), BTreeSet::from([second_id]));
    state.cancel_working_job(second_id, 4, 5);
    assert!(candidates(&state, future_due).is_empty());
    let mut rebuilt = state.clone();
    rebuilt.rebuild_derived_indexes();
    assert_eq!(rebuilt, state);
}

#[test]
fn scheduled_equipment_revision_demand_tracks_only_active_wear_buckets() {
    let completion_tick = SimulationTick::new(4);
    let worn = Condition::new(900_000)
        .unwrap_or_else(|error| panic!("mining index test condition failed: {error}"));
    let mut state = MiningState::new();
    state.insert_job(job_record(worn, completion_tick), 2, 1);

    assert_eq!(state.scheduled_equipment_revision_bucket_count(), 1);
    assert_eq!(
        state.mark_due_job_awaiting_settlement(1, 2, completion_tick),
        MiningJobId::new(1)
    );
    assert_eq!(
        state.jobs().count(),
        1,
        "awaiting-settlement job remains durable history"
    );
    assert_eq!(state.scheduled_equipment_revision_bucket_count(), 0);

    let mut no_wear = MiningState::new();
    no_wear.insert_job(job_record(Condition::PRISTINE, completion_tick), 2, 1);
    assert_eq!(no_wear.scheduled_equipment_revision_bucket_count(), 0);
}
