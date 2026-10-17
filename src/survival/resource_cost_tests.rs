//! Owner contracts for canonical per-tick survival resource costs.

use super::*;
use crate::core::quantity::Mass;
use crate::core::time::TickSpan;
use crate::survival::{
    BodyKinematicsDefinition, DirectConsumptionDefinition, HydrationDefinition,
    LocomotionDefinition, MetabolismDefinition, NutritionDefinition, SurvivalStressDefinition,
    ThermalComfortDefinition,
};

fn physiology(
    basal_energy: Energy,
    hydration_loss: Volume,
    maximum_energy: Energy,
    maximum_hydration: Volume,
) -> PhysiologyDefinition {
    PhysiologyDefinition::new(
        MetabolismDefinition::new(maximum_energy, Energy::ZERO, basal_energy),
        HydrationDefinition::new(maximum_hydration, Volume::ZERO, hydration_loss),
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
            BodyKinematicsDefinition::new(
                crate::core::quantity::Length::from_micrometers(600_000),
                crate::survival::BodyStanceDefinition::new(
                    crate::core::quantity::Length::from_micrometers(1_800_000),
                    crate::core::quantity::Length::from_micrometers(1_500_000),
                ),
                crate::survival::BodyViewDefinition::new(
                    crate::core::quantity::Length::from_micrometers(1_620_000),
                    crate::core::quantity::Length::from_micrometers(1_270_000),
                    crate::core::quantity::Length::from_micrometers(4_500_000),
                ),
                crate::survival::LocomotionSpeedDefinition::new(
                    crate::core::quantity::Speed::from_nanometers_per_second(1_300_000_000),
                    crate::core::quantity::Speed::from_nanometers_per_second(4_300_000_000),
                    crate::core::quantity::Speed::from_nanometers_per_second(5_600_000_000),
                    crate::core::quantity::Speed::from_nanometers_per_second(5_000_000_000),
                    crate::core::quantity::Speed::from_nanometers_per_second(4_600_000_000),
                    crate::core::quantity::Speed::from_nanometers_per_second(8_000_000_000),
                    crate::core::quantity::Speed::from_nanometers_per_second(55_000_000_000),
                ),
                crate::core::quantity::Acceleration::from_micrometers_per_second_squared(
                    20_000_000,
                ),
            ),
        ),
        Mass::from_milligrams(1),
        SurvivalStressDefinition::new(
            1,
            1,
            ThermalComfortDefinition::new(1, 1, 1, 1, Energy::from_nanojoules(1), 1),
        ),
    )
}

#[test]
fn control_exertion_splits_preserve_exact_authored_total() {
    let exertion =
        SurvivalExertion::new(Energy::from_nanojoules(101), Volume::from_microliters(73));
    let first = control_exertion_between(exertion, 0, 37)
        .unwrap_or_else(|| panic!("first control exertion slice failed"));
    let middle = control_exertion_between(exertion, 37, 109)
        .unwrap_or_else(|| panic!("middle control exertion slice failed"));
    let last = control_exertion_between(exertion, 109, 144)
        .unwrap_or_else(|| panic!("last control exertion slice failed"));

    let energy = first
        .energy_cost_per_tick()
        .checked_add(middle.energy_cost_per_tick())
        .and_then(|value| value.checked_add(last.energy_cost_per_tick()))
        .unwrap_or_else(|| panic!("control exertion energy sum overflowed"));
    let hydration = first
        .hydration_loss_per_tick()
        .checked_add(middle.hydration_loss_per_tick())
        .and_then(|value| value.checked_add(last.hydration_loss_per_tick()))
        .unwrap_or_else(|| panic!("control exertion hydration sum overflowed"));

    assert_eq!(energy, Energy::from_nanojoules(202));
    assert_eq!(hydration, Volume::from_microliters(146));
    assert_eq!(control_exertion_between(exertion, 4, 3), None);
}

#[test]
fn duration_projection_matches_exact_tick_cost_multiplication() {
    let definition = physiology(
        Energy::from_nanojoules(10),
        Volume::from_microliters(2),
        Energy::from_nanojoules(10_000),
        Volume::from_microliters(10_000),
    );
    let exertion = SurvivalExertion::new(Energy::from_nanojoules(30), Volume::from_microliters(3));

    let budget = project_survival_resource_budget(definition, exertion, TickSpan::new(7))
        .unwrap_or_else(|error| panic!("survival duration projection failed: {error:?}"));

    assert_eq!(budget.metabolic_energy(), Energy::from_nanojoules(280));
    assert_eq!(budget.hydration(), Volume::from_microliters(35));
    assert_eq!(
        budget.checked_add(SurvivalResourceBudget::ZERO),
        Some(budget)
    );
}

#[test]
fn duration_projection_reports_multiplication_overflow() {
    let definition = physiology(
        Energy::from_nanojoules(u128::MAX / 2 + 1),
        Volume::from_microliters(1),
        Energy::from_nanojoules(u128::MAX),
        Volume::from_microliters(10),
    );

    assert_eq!(
        project_survival_resource_budget(definition, SurvivalExertion::REST, TickSpan::new(2)),
        Err(SurvivalResourceProjectionError::EnergyOverflow)
    );
}

#[test]
fn tick_resource_cost_combines_basal_and_incremental_work_costs() {
    let definition = physiology(
        Energy::from_nanojoules(10),
        Volume::from_microliters(2),
        Energy::from_nanojoules(1_000),
        Volume::from_microliters(1_000),
    );
    let exertion = SurvivalExertion::new(Energy::from_nanojoules(30), Volume::from_microliters(3));

    let cost = resolve_survival_tick_resource_cost(definition, exertion)
        .unwrap_or_else(|error| panic!("canonical survival cost failed: {error:?}"));

    assert_eq!(cost.metabolic_energy(), Energy::from_nanojoules(40));
    assert_eq!(cost.hydration(), Volume::from_microliters(5));
}

#[test]
fn tick_resource_cost_reports_energy_overflow_before_execution() {
    let definition = physiology(
        Energy::from_nanojoules(u128::MAX),
        Volume::from_microliters(1),
        Energy::from_nanojoules(u128::MAX),
        Volume::from_microliters(10),
    );

    assert_eq!(
        resolve_survival_tick_resource_cost(
            definition,
            SurvivalExertion::new(Energy::from_nanojoules(1), Volume::ZERO),
        ),
        Err(SurvivalTickResourceCostError::EnergyOverflow)
    );
}

#[test]
fn tick_resource_cost_reports_hydration_overflow_before_execution() {
    let definition = physiology(
        Energy::from_nanojoules(1),
        Volume::from_microliters(u64::MAX),
        Energy::from_nanojoules(10),
        Volume::from_microliters(u64::MAX),
    );

    assert_eq!(
        resolve_survival_tick_resource_cost(
            definition,
            SurvivalExertion::new(Energy::ZERO, Volume::from_microliters(1)),
        ),
        Err(SurvivalTickResourceCostError::HydrationOverflow)
    );
}
