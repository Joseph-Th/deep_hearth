//! Focused contracts for pre-action woodworking investment policy.

use super::WoodworkingInvestmentPreference::{ConserveScarceCopper, ConserveTimber};
use super::WoodworkingInvestmentReason::*;
use super::WoodworkingTimberBalance::{Costlier, Neutral, Saving, Unavailable};
use super::{woodworking_investment_decision, woodworking_timber_balance};
use deep_hearth::core::quantity::Mass;

#[test]
fn preference_and_timber_balance_mapping_stay_semantic() {
    assert_eq!(
        super::WoodworkingInvestmentPreference::from_behavior_seed(2),
        ConserveScarceCopper
    );
    assert_eq!(
        super::WoodworkingInvestmentPreference::from_behavior_seed(3),
        ConserveTimber
    );
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
    for (preference, reserve, budget, timber, expected) in [
        (
            ConserveScarceCopper,
            true,
            false,
            Costlier,
            (false, SetupAttentionBudgetExceeded),
        ),
        (
            ConserveScarceCopper,
            true,
            true,
            Costlier,
            (true, SurplusCopperWithinSetupBudget),
        ),
        (
            ConserveScarceCopper,
            false,
            true,
            Saving,
            (false, CopperReserveProtected),
        ),
        (
            ConserveTimber,
            false,
            false,
            Saving,
            (true, PipelineNetTimberSaving),
        ),
        (
            ConserveTimber,
            false,
            false,
            Neutral,
            (false, PipelineTimberNeutralOutsideSetupBudget),
        ),
        (
            ConserveTimber,
            false,
            true,
            Neutral,
            (true, PipelineTimberNeutralWithinSetupBudget),
        ),
        (
            ConserveTimber,
            false,
            true,
            Costlier,
            (false, PipelineTimberCostNotRecovered),
        ),
        (
            ConserveTimber,
            true,
            true,
            Unavailable,
            (false, CopperSupplyLimited),
        ),
    ] {
        assert_eq!(
            woodworking_investment_decision(preference, reserve, budget, timber),
            expected
        );
    }
}
