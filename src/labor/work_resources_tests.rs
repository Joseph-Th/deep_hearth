//! Tests for the sibling work resources module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::survival::{HydrationDefinition, MetabolismDefinition, NutritionDefinition};

#[test]
fn budget_includes_basal_and_incremental_work_costs() {
    let physiology = PhysiologyDefinition::new(
        MetabolismDefinition::new(
            Energy::from_nanojoules(1_000),
            Energy::from_nanojoules(100),
            Energy::from_nanojoules(10),
        ),
        HydrationDefinition::new(
            Volume::from_microliters(1_000),
            Volume::from_microliters(100),
            Volume::from_microliters(2),
        ),
        NutritionDefinition::new(1, 1),
        1,
        1,
    );
    let exertion = SurvivalExertion::new(Energy::from_nanojoules(30), Volume::from_microliters(3));

    let budget = calculate_player_work_resource_budget(physiology, exertion, TickSpan::new(4))
        .unwrap_or_else(|error| panic!("player-work budget fixture failed: {error:?}"));

    assert_eq!(budget.metabolic_energy(), Energy::from_nanojoules(160));
    assert_eq!(budget.hydration(), Volume::from_microliters(20));
}
