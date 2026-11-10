//! Mixed-output inventory planning retains exact physical and storage cohorts.

use super::*;
use crate::content::{FORM_LOG, MATERIAL_WOOD};
use crate::core::quantity::Temperature;
use crate::material::{CommodityKey, MaterialComposition};

fn wood_profile(temperature_mk: u32) -> MaterialLotProfile {
    MaterialLotProfile {
        commodity: CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        temperature: Temperature::from_millikelvin(temperature_mk),
        composition: MaterialComposition::pure(MATERIAL_WOOD),
        particle_size: None,
    }
}

#[test]
fn interleaved_arrivals_use_the_first_matching_profile_and_freshness_cohort() {
    let state = InventoryState::new();
    let mut planner = LotIdentityPlanner::new(&state, []);
    let cold = wood_profile(290_000);
    let warm = wood_profile(300_000);
    let a = StockpileId::new(1);
    let b = StockpileId::new(2);
    let at = SimulationTick::new(10);
    let old = MaterialStorageHistory::new(SimulationTick::new(2));
    let recent = MaterialStorageHistory::new(SimulationTick::new(8));
    let preservation = 1_000_000;

    planner.note_preserved_arrival(MaterialLotId::new(90), b, &warm, old);
    planner.note_preserved_arrival(MaterialLotId::new(80), a, &cold, old);
    planner.note_preserved_arrival(MaterialLotId::new(70), a, &warm, old);
    planner.note_preserved_arrival(MaterialLotId::new(60), a, &warm, recent);

    assert_eq!(
        planner.plan(
            a,
            &warm,
            old,
            at,
            preservation,
            LotMergePolicy::ExactStorageExposure
        ),
        Some(MaterialLotId::new(70)),
        "arrival order, not numeric lot identity, decides a compatible merge"
    );
    assert_eq!(
        planner.plan(
            a,
            &warm,
            recent,
            at,
            preservation,
            LotMergePolicy::ExactStorageExposure
        ),
        Some(MaterialLotId::new(60)),
        "distinct food-age cohorts must stay separate"
    );
    assert_eq!(
        planner.plan(
            a,
            &warm,
            recent,
            at,
            preservation,
            LotMergePolicy::OldestStorageExposure
        ),
        Some(MaterialLotId::new(70)),
        "nonperishable merging may choose the first physically compatible arrival"
    );
    assert_eq!(planner.next_lot_id(), state.next_lot_id());
    assert!(!planner.allocated_any());
}

#[test]
fn new_profiles_and_cohorts_allocate_once_with_no_cross_destination_reuse() {
    let state = InventoryState::new();
    let mut planner = LotIdentityPlanner::new(&state, []);
    let warm = wood_profile(300_000);
    let cold = wood_profile(290_000);
    let a = StockpileId::new(1);
    let b = StockpileId::new(2);
    let at = SimulationTick::new(10);
    let old = MaterialStorageHistory::new(SimulationTick::new(2));
    let recent = MaterialStorageHistory::new(SimulationTick::new(8));

    let mut plan = |destination, profile, history| {
        planner.plan(
            destination,
            profile,
            history,
            at,
            1_000_000,
            LotMergePolicy::ExactStorageExposure,
        )
    };
    assert_eq!(plan(a, &warm, old), Some(MaterialLotId::new(1)));
    assert_eq!(plan(b, &warm, old), Some(MaterialLotId::new(2)));
    assert_eq!(plan(a, &cold, old), Some(MaterialLotId::new(3)));
    assert_eq!(plan(a, &warm, recent), Some(MaterialLotId::new(4)));
    assert_eq!(plan(a, &warm, old), Some(MaterialLotId::new(1)));
    assert_eq!(plan(a, &warm, recent), Some(MaterialLotId::new(4)));
    assert_eq!(planner.next_lot_id(), 5);
    assert!(planner.allocated_any());
}
