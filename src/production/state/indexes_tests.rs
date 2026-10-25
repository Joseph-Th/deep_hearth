//! Focused proofs for production derived-index semantics.

use super::*;

#[test]
fn due_schedule_indexes_remain_synchronized_across_bucket_mutations() {
    let mut indexes = ProductionIndexes::new();
    let early = SimulationTick::new(10);
    let late = SimulationTick::new(20);
    let first = ProductionJobId::new(1);
    let second = ProductionJobId::new(2);
    let third = ProductionJobId::new(3);

    indexes.insert_due_job(first, late);
    indexes.insert_due_job(second, early);
    indexes.insert_due_job(third, late);
    assert_eq!(indexes.earliest_due_tick(), Some(early));
    assert_eq!(indexes.jobs_due_at(late), BTreeSet::from([first, third]));
    for (id, due) in [(first, late), (second, early), (third, late)] {
        indexes.assert_due_job_present(id, due);
    }

    indexes.remove_due_job(second, early);
    indexes.assert_due_job_absent(second);
    assert_eq!(indexes.earliest_due_tick(), Some(late));
    indexes.remove_due_job(first, late);
    indexes.remove_due_job(third, late);
    assert_eq!(indexes.earliest_due_tick(), None);
    assert!(indexes.jobs_due_at(late).is_empty());
    for id in [first, second, third] {
        indexes.assert_due_job_absent(id);
    }
}

#[test]
fn a_job_cannot_enter_two_different_due_buckets() {
    let mut indexes = ProductionIndexes::new();
    let id = ProductionJobId::new(1);
    let original = SimulationTick::new(10);
    indexes.insert_due_job(id, original);
    let before = indexes.clone();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            indexes.insert_due_job(id, SimulationTick::new(20));
        }))
        .is_err()
    );
    assert_eq!(indexes, before);
}

#[test]
fn future_material_lot_identity_demand_tracks_job_insert_and_remove() {
    let job = ProductionJobId::new(1);
    let projection = ProductionJobIndexProjection {
        due_tick: None,
        consumed_energy_store: None,
        released_energy_store: None,
        equipment: None,
        output_stockpiles: std::collections::BTreeSet::new(),
        future_material_lot_id_demand: 3,
        suspended: false,
        player_labor_suspended: false,
        requires_active_support: false,
    };
    let mut indexes = ProductionIndexes::new();

    indexes.insert_job(job, &projection);
    assert_eq!(indexes.future_material_lot_id_demand(), 3);

    indexes.remove_job(job, &projection);
    assert_eq!(indexes.future_material_lot_id_demand(), 0);

    indexes.future_material_lot_id_demand = 1;
    assert_eq!(
        indexes.future_material_lot_id_demand_mismatch(std::iter::empty()),
        Some((1, 0)),
        "derived demand drift must be detectable against durable jobs"
    );
}

#[test]
fn physical_availability_candidates_inspect_only_active_output_destinations() {
    let suspended = ProductionJobId::new(1);
    let support_required = ProductionJobId::new(2);
    let supported_output = ProductionJobId::new(3);
    let unsupported_output = ProductionJobId::new(4);
    let overlapping = ProductionJobId::new(5);
    let supported_stockpile = StockpileId::new(10);
    let unsupported_stockpile = StockpileId::new(11);
    let projection =
        |output_stockpiles: BTreeSet<StockpileId>, suspended, requires_active_support| {
            ProductionJobIndexProjection {
                due_tick: None,
                consumed_energy_store: None,
                released_energy_store: None,
                equipment: None,
                output_stockpiles,
                future_material_lot_id_demand: 0,
                suspended,
                player_labor_suspended: false,
                requires_active_support,
            }
        };
    let mut indexes = ProductionIndexes::new();
    indexes.insert_job(suspended, &projection(BTreeSet::new(), true, false));
    indexes.insert_job(support_required, &projection(BTreeSet::new(), false, true));
    indexes.insert_job(
        supported_output,
        &projection(BTreeSet::from([supported_stockpile]), false, false),
    );
    indexes.insert_job(
        unsupported_output,
        &projection(BTreeSet::from([unsupported_stockpile]), false, false),
    );
    indexes.insert_job(
        overlapping,
        &projection(BTreeSet::from([supported_stockpile]), true, false),
    );

    let mut inspected = Vec::new();
    let candidates = indexes.physical_availability_candidate_jobs(|stockpile| {
        inspected.push(stockpile);
        stockpile == supported_stockpile
    });

    assert_eq!(inspected, vec![supported_stockpile, unsupported_stockpile]);
    assert_eq!(
        candidates,
        vec![suspended, support_required, supported_output, overlapping],
        "availability candidates must remain sorted and deduplicated"
    );
}
