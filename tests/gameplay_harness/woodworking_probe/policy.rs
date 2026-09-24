//! Pre-action woodworking investment policy and its observable decision vocabulary.

use deep_hearth::core::quantity::Mass;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WoodworkingInvestmentPreference {
    ConserveScarceCopper,
    ConserveTimber,
}

impl WoodworkingInvestmentPreference {
    pub(super) const fn from_behavior_seed(seed: u64) -> Self {
        if seed.is_multiple_of(2) {
            Self::ConserveScarceCopper
        } else {
            Self::ConserveTimber
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::ConserveScarceCopper => "conserve-scarce-copper",
            Self::ConserveTimber => "conserve-timber",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WoodworkingInvestmentReason {
    BareHandsAvoidsInvestmentCost,
    CopperSupplyLimited,
    CopperReserveProtected,
    SetupAttentionBudgetExceeded,
    SurplusCopperWithinSetupBudget,
    PipelineTimberCostNotRecovered,
    PipelineTimberNeutralOutsideSetupBudget,
    PipelineTimberNeutralWithinSetupBudget,
    PipelineNetTimberSaving,
}

impl WoodworkingInvestmentReason {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::BareHandsAvoidsInvestmentCost => "bare-hands-avoids-investment-cost",
            Self::CopperSupplyLimited => "copper-supply-limited",
            Self::CopperReserveProtected => "copper-reserve-protected",
            Self::SetupAttentionBudgetExceeded => "setup-attention-budget-exceeded",
            Self::SurplusCopperWithinSetupBudget => "surplus-copper-within-setup-budget",
            Self::PipelineTimberCostNotRecovered => "pipeline-timber-cost-not-recovered",
            Self::PipelineTimberNeutralOutsideSetupBudget => {
                "pipeline-timber-neutral-outside-setup-budget"
            }
            Self::PipelineTimberNeutralWithinSetupBudget => {
                "pipeline-timber-neutral-within-setup-budget"
            }
            Self::PipelineNetTimberSaving => "pipeline-net-timber-saving",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WoodworkingTimberBalance {
    Unavailable,
    Saving,
    Neutral,
    Costlier,
}

impl WoodworkingTimberBalance {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Saving => "saving",
            Self::Neutral => "neutral",
            Self::Costlier => "costlier",
        }
    }
}

pub(super) fn woodworking_timber_balance(
    saw: Option<Mass>,
    adze: Mass,
) -> WoodworkingTimberBalance {
    match saw.map(|mass| mass.cmp(&adze)) {
        None => WoodworkingTimberBalance::Unavailable,
        Some(std::cmp::Ordering::Less) => WoodworkingTimberBalance::Saving,
        Some(std::cmp::Ordering::Equal) => WoodworkingTimberBalance::Neutral,
        Some(std::cmp::Ordering::Greater) => WoodworkingTimberBalance::Costlier,
    }
}

pub(super) fn woodworking_investment_decision(
    preference: WoodworkingInvestmentPreference,
    reserve_safe: bool,
    setup_attention_budget_met: bool,
    timber_balance: WoodworkingTimberBalance,
) -> (bool, WoodworkingInvestmentReason) {
    if timber_balance == WoodworkingTimberBalance::Unavailable {
        return (false, WoodworkingInvestmentReason::CopperSupplyLimited);
    }
    match preference {
        WoodworkingInvestmentPreference::ConserveScarceCopper => {
            conserve_copper_decision(reserve_safe, setup_attention_budget_met)
        }
        WoodworkingInvestmentPreference::ConserveTimber => {
            conserve_timber_decision(timber_balance, setup_attention_budget_met)
        }
    }
}

fn conserve_copper_decision(
    reserve_safe: bool,
    setup_attention_budget_met: bool,
) -> (bool, WoodworkingInvestmentReason) {
    if !reserve_safe {
        return (false, WoodworkingInvestmentReason::CopperReserveProtected);
    }
    if !setup_attention_budget_met {
        return (
            false,
            WoodworkingInvestmentReason::SetupAttentionBudgetExceeded,
        );
    }
    (
        true,
        WoodworkingInvestmentReason::SurplusCopperWithinSetupBudget,
    )
}

fn conserve_timber_decision(
    timber_balance: WoodworkingTimberBalance,
    setup_attention_budget_met: bool,
) -> (bool, WoodworkingInvestmentReason) {
    match timber_balance {
        WoodworkingTimberBalance::Costlier => (
            false,
            WoodworkingInvestmentReason::PipelineTimberCostNotRecovered,
        ),
        WoodworkingTimberBalance::Neutral if !setup_attention_budget_met => (
            false,
            WoodworkingInvestmentReason::PipelineTimberNeutralOutsideSetupBudget,
        ),
        WoodworkingTimberBalance::Neutral => (
            true,
            WoodworkingInvestmentReason::PipelineTimberNeutralWithinSetupBudget,
        ),
        WoodworkingTimberBalance::Saving => {
            (true, WoodworkingInvestmentReason::PipelineNetTimberSaving)
        }
        WoodworkingTimberBalance::Unavailable => {
            unreachable!("unavailable timber balance is handled before preference dispatch")
        }
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
