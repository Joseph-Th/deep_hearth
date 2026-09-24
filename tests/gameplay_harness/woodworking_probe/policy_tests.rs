//! Focused contracts for pre-action woodworking investment policy.

use super::WoodworkingInvestmentPreference::{ConserveScarceCopper, ConserveTimber};
use super::WoodworkingInvestmentReason::*;
use super::WoodworkingTimberBalance::{Costlier, Neutral, Saving, Unavailable};
use super::woodworking_investment_decision;

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
