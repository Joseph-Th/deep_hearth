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
    let mass = Mass::from_milligrams(10);
    MiningJobRecord::new(
        MiningJobIdentity {
            id: MiningJobId::new(1),
            method: MiningMethodId::new(1),
            deposit: GeologicalDepositId::new(1),
        },
        MiningJobResources {
            destination: StockpileId::new(1),
            equipment_trace: EquipmentOperationTrace::new(
                EquipmentId::new(1),
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
fn scheduled_equipment_revision_demand_tracks_only_active_wear_buckets() {
    let completion_tick = SimulationTick::new(4);
    let worn = Condition::new(900_000)
        .unwrap_or_else(|error| panic!("mining index test condition failed: {error}"));
    let mut state = MiningState::new();
    state.insert_job(job_record(worn, completion_tick), 2, 1);

    assert_eq!(state.scheduled_equipment_revision_bucket_count(), 1);
    assert_eq!(
        state.mark_due_jobs_ready(1, 2, completion_tick),
        [MiningJobId::new(1)]
    );
    assert_eq!(state.jobs().count(), 1, "ready job remains durable history");
    assert_eq!(state.scheduled_equipment_revision_bucket_count(), 0);

    let mut no_wear = MiningState::new();
    no_wear.insert_job(job_record(Condition::PRISTINE, completion_tick), 2, 1);
    assert_eq!(no_wear.scheduled_equipment_revision_bucket_count(), 0);
}
