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
        suspended: false,
        player_labor_suspended: false,
        requires_active_support: false,
        requires_energy_revision: false,
        requires_equipment_revision: false,
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
                requires_energy_revision: false,
                requires_equipment_revision: false,
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

#[test]
fn revision_requirement_buckets_track_shared_ticks_without_rescanning_jobs() {
    let shared_tick = SimulationTick::new(4);
    let other_tick = SimulationTick::new(5);
    let first = ProductionJobId::new(1);
    let second = ProductionJobId::new(2);
    let third = ProductionJobId::new(3);
    let mut indexes = ProductionIndexes::new();

    indexes.insert_due_job_with_requirements(first, shared_tick, true, true);
    indexes.insert_due_job_with_requirements(second, shared_tick, true, false);
    indexes.insert_due_job_with_requirements(third, other_tick, false, true);

    assert_eq!(indexes.scheduled_energy_revision_bucket_count(), 1);
    assert_eq!(indexes.scheduled_equipment_revision_bucket_count(), 2);
    assert_eq!(
        indexes.scheduled_energy_revision_bucket_count_with_tick(shared_tick),
        1
    );
    assert_eq!(
        indexes.scheduled_energy_revision_bucket_count_with_tick(other_tick),
        2
    );

    indexes.remove_due_job_with_requirements(first, shared_tick, true, true);
    assert_eq!(indexes.scheduled_energy_revision_bucket_count(), 1);
    assert_eq!(indexes.scheduled_equipment_revision_bucket_count(), 1);
    indexes.remove_due_job_with_requirements(second, shared_tick, true, false);
    assert_eq!(indexes.scheduled_energy_revision_bucket_count(), 0);
    indexes.remove_due_job_with_requirements(third, other_tick, false, true);
    assert_eq!(indexes.scheduled_equipment_revision_bucket_count(), 0);
}
