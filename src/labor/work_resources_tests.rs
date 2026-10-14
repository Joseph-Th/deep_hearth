//! Contract tests for player-work physiological budgeting.

use super::*;
use crate::core::quantity::Mass;
use crate::survival::{
    DirectConsumptionDefinition, HydrationDefinition, LocomotionDefinition, MetabolismDefinition,
    NutritionDefinition, SurvivalStressDefinition,
};

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
        DirectConsumptionDefinition::new(
            Mass::from_milligrams(1),
            Mass::from_milligrams(1),
            TickSpan::new(1),
            Volume::from_microliters(1),
            Volume::from_microliters(1),
            TickSpan::new(1),
        ),
        LocomotionDefinition::new(
            TickSpan::new(1),
            SurvivalExertion::new(Energy::from_nanojoules(1), Volume::ZERO),
            SurvivalExertion::new(Energy::from_nanojoules(1), Volume::ZERO),
            crate::core::quantity::Length::from_micrometers(600_000),
            crate::core::quantity::Length::from_micrometers(1_800_000),
            crate::core::quantity::Speed::from_nanometers_per_second(55_000_000_000),
        ),
        Mass::from_milligrams(1),
        SurvivalStressDefinition::new(
            1,
            1,
            crate::survival::ThermalComfortDefinition::new(1, 1, 1, Energy::from_nanojoules(1), 1),
        ),
    );
    let exertion = SurvivalExertion::new(Energy::from_nanojoules(30), Volume::from_microliters(3));

    let budget = calculate_player_work_resource_budget(physiology, exertion, TickSpan::new(4))
        .unwrap_or_else(|error| panic!("player-work budget fixture failed: {error:?}"));

    assert_eq!(budget.metabolic_energy(), Energy::from_nanojoules(160));
    assert_eq!(budget.hydration(), Volume::from_microliters(20));
}
