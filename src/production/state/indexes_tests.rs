//! Focused proofs for production scheduling-index bucket semantics.

use super::*;

#[test]
fn scheduled_bucket_queries_deduplicate_ticks_and_respect_filtered_categories() {
    let shared_tick = SimulationTick::new(4);
    let other_tick = SimulationTick::new(5);
    let future_tick = SimulationTick::new(6);
    let first = ProductionJobId::new(1);
    let second = ProductionJobId::new(2);
    let unrelated = ProductionJobId::new(3);
    let mut indexes = ProductionIndexes::new();
    indexes.insert_due_job(first, shared_tick);
    indexes.insert_due_job(second, shared_tick);
    indexes.insert_due_job(unrelated, other_tick);

    assert_eq!(indexes.scheduled_bucket_count(), 2);
    assert_eq!(
        indexes.scheduled_bucket_count_where(|job| job == first || job == second),
        1
    );
    assert_eq!(
        indexes.scheduled_bucket_count_with_additional_tick_where(shared_tick, |job| {
            job == first || job == second
        }),
        1,
        "a new matching job on an already-matching tick must share the owner revision"
    );
    assert_eq!(
        indexes.scheduled_bucket_count_with_additional_tick_where(other_tick, |job| {
            job == first || job == second
        }),
        2,
        "a tick occupied only by another revision category is new for this category"
    );
    assert_eq!(
        indexes.scheduled_bucket_count_with_additional_tick_where(future_tick, |job| {
            job == first || job == second
        }),
        2
    );
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
