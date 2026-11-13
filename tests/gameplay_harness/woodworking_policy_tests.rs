//! Focused contracts for the woodworking investment criterion.

use super::WoodworkingInvestmentReason::*;
use super::WoodworkingTimberBalance::{Costlier, Neutral, Saving, Unavailable};
use super::{woodworking_investment_decision, woodworking_timber_balance};
use deep_hearth::core::quantity::Mass;

#[test]
fn timber_balance_mapping_stays_semantic() {
    assert_eq!(
        woodworking_timber_balance(None, Mass::from_milligrams(10)),
        Unavailable
    );
    assert_eq!(
        woodworking_timber_balance(Some(Mass::from_milligrams(9)), Mass::from_milligrams(10)),
        Saving
    );
    assert_eq!(
        woodworking_timber_balance(Some(Mass::from_milligrams(10)), Mass::from_milligrams(10)),
        Neutral
    );
    assert_eq!(
        woodworking_timber_balance(Some(Mass::from_milligrams(11)), Mass::from_milligrams(10)),
        Costlier
    );
}

#[test]
fn prices_observed_budget_copper_and_timber_before_execution() {
    for (reserve, budget, timber, expected) in [
        (
            true,
            false,
            Costlier,
            (false, PipelineTimberCostNotRecovered),
        ),
        (
            true,
            true,
            Costlier,
            (false, PipelineTimberCostNotRecovered),
        ),
        (false, true, Saving, (false, CopperReserveProtected)),
        (true, false, Saving, (false, SetupAttentionBudgetExceeded)),
        (
            true,
            false,
            Neutral,
            (false, PipelineTimberNeutralOutsideSetupBudget),
        ),
        (
            true,
            true,
            Neutral,
            (true, PipelineTimberNeutralWithinSetupBudget),
        ),
        (true, true, Saving, (true, PipelineNetTimberSaving)),
        (true, true, Unavailable, (false, CopperSupplyLimited)),
    ] {
        assert_eq!(
            woodworking_investment_decision(reserve, budget, timber),
            expected
        );
    }
}
