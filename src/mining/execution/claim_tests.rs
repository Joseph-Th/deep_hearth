//! Focused proof for mining-claim inventory landing receipts.

use super::*;
use crate::content::{FORM_ORE, MATERIAL_COPPER, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, apply_clock_advance};
use crate::core::time::{SimulationTick, WorldSeed};
use crate::equipment::{EquipmentDefinitionId, EquipmentId, EquipmentOperationTrace};
use crate::geology::GeologicalDepositId;
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_lot_for_test, validate_inbound_reservation,
};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, MaterialLotSpec};
use crate::mining::state::{
    MiningJobIdentity, MiningJobPhase, MiningJobResources, MiningJobSchedule,
};
use crate::mining::{MiningJobRecord, MiningMethodId};

#[test]
fn mining_claim_returns_merge_aware_inventory_landing_identity() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC1A1_0001));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("claim receipt destination failed: {error}"));
    let completion_tick = SimulationTick::new(1);
    apply_clock_advance(&mut state, completion_tick);
    let commodity = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
    let temperature = Temperature::from_millikelvin(300_000);
    let existing = deposit_lot_for_test(
        &registries,
        &mut state,
        destination,
        commodity,
        Mass::from_milligrams(4),
        temperature,
    )
    .unwrap_or_else(|error| panic!("claim receipt existing lot failed: {error}"));
    let claimed_mass = Mass::from_milligrams(6);
    validate_inbound_reservation(state.inventory(), destination, claimed_mass)
        .unwrap_or_else(|error| panic!("claim receipt reservation failed: {error:?}"))
        .apply(state.inventory_state_mut());

    let job = MiningJobId::new(1);
    let record = MiningJobRecord::new(
        MiningJobIdentity {
            id: job,
            method: MiningMethodId::new(1),
            deposit: GeologicalDepositId::new(1),
        },
        MiningJobResources {
            destination,
            equipment_trace: EquipmentOperationTrace::new(
                EquipmentId::new(1),
                EquipmentDefinitionId::new(1),
                Condition::PRISTINE,
            ),
            deposit_mass_before: claimed_mass,
            output: MaterialLotSpec::new(commodity, claimed_mass, temperature),
            equipment_condition_after: Condition::PRISTINE,
        },
        MiningJobSchedule {
            started_at: SimulationTick::new(0),
            completes_at: completion_tick,
            phase: MiningJobPhase::Working,
        },
    );
    state.mining_state_mut().insert_job(record, 2, 1);
    let ready = state
        .mining_state_mut()
        .mark_due_jobs_ready(1, 2, completion_tick);
    assert_eq!(ready, [job]);

    let outcome = validate_claim_mining_output(&registries, &state, job)
        .unwrap_or_else(|error| panic!("claim receipt validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("claim receipt commit failed: {error}"));

    assert_eq!(outcome.job(), job);
    assert_eq!(outcome.destination(), destination);
    assert_eq!(outcome.output().commodity(), commodity);
    assert_eq!(outcome.output().mass(), claimed_mass);
    assert_eq!(outcome.landed_lot(), existing);
    assert_eq!(
        state.inventory().lot_ids(destination).collect::<Vec<_>>(),
        [existing]
    );
    assert_eq!(
        state.inventory().get_lot(existing).map(|lot| lot.mass()),
        Some(Mass::from_milligrams(10))
    );
    assert!(state.mining().get_job(job).is_none());
}
